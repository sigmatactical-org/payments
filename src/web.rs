use std::collections::HashMap;
use std::convert::Infallible;

use sigma_pg::clients::addresses::{self, AddressSummary};
use sigma_pg::clients::cart;
use warp::http::StatusCode;
use warp::reply::Response;
use warp::{Filter, Rejection, Reply};

use crate::SharedStore;
use crate::config;
use crate::model::{PaymentMethod, PaymentMethodForm, PaymentMethodType};
use crate::store::StoreError;
use crate::templates::{self, PaymentMethodFormValues};

/// Build this module's routes.
pub fn routes(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    index(store.clone())
        .or(new_payment_method())
        .or(create_payment_method(store.clone()))
        .or(edit_payment_method(store.clone()))
        .or(update_payment_method(store.clone()))
        .or(delete_payment_method(store.clone()))
        .or(set_default_payment_method(store))
}

// ---------------------------------------------------------------------------
// Session gate + redirect helpers
// ---------------------------------------------------------------------------

/// Resolve the signed-in identity user id from the session cookie, or `Err`
/// with a 303 redirect to identity sign-in (returning to `return_path` after
/// login). Every route in this service is gated on this — payment methods
/// are strictly per-user, and there is no anonymous or admin-visible view.
async fn require_user(cookie: Option<&str>, return_path: &str) -> Result<String, Response> {
    let status = sigma_pg::clients::session::fetch_identity_status(
        &config::identity_internal_base_url(),
        cookie,
    )
    .await;
    let user_id = match status {
        Ok(Some(status)) => status.user_id.filter(|id| !id.trim().is_empty()),
        Ok(None) => None,
        Err(error) => {
            tracing::error!("web: fetch_identity_status failed: {error:?}");
            None
        }
    };
    user_id.ok_or_else(|| sign_in_redirect(return_path))
}

fn sign_in_redirect(return_path: &str) -> Response {
    let links = sigma_identity_nav::auth_links(
        &config::identity_public_base_url(),
        &config::public_base_url(),
        return_path,
    );
    redirect(&links.sign_in_url)
}

/// 303 redirect (PRG pattern for form POSTs, also used for the sign-in bounce).
// `to_string` is required, not redundant: `Uri::from_maybe_shared` needs an
// owned buffer it can turn into `Bytes` without borrowing past this call.
fn redirect(location: &str) -> Response {
    match warp::http::Uri::from_maybe_shared(location.to_string()) {
        Ok(uri) => warp::redirect::see_other(uri).into_response(),
        Err(_) => internal_error(),
    }
}

fn cookie_filter() -> impl Filter<Extract = (Option<String>,), Error = Rejection> + Clone {
    warp::header::optional::<String>("cookie")
}

/// Themed 500 response for unexpected failures (database or template render
/// errors) — distinct from [`StoreError::NotFound`], which stays a 404.
fn internal_error() -> Response {
    warp::reply::with_status(
        warp::reply::html(sigma_theme::errors::internal_server_error_html()),
        StatusCode::INTERNAL_SERVER_ERROR,
    )
    .into_response()
}

/// Live cart item count for the navbar badge (0 when unconfigured).
async fn nav_cart_count(cookie: Option<&str>) -> u32 {
    cart::nav_cart_count(config::cart_base_url().as_deref(), cookie).await
}

/// List `user_id`'s billing addresses for the form dropdown / index lookup.
/// A network failure or the addresses service being down is treated as "no
/// billing addresses available" rather than a 500 — the page still renders,
/// just with an empty dropdown and (on the create form) a message pointing
/// the visitor at the addresses service.
async fn fetch_billing_addresses_or_empty(user_id: &str) -> Vec<AddressSummary> {
    match addresses::list_addresses(
        Some(&config::addresses_internal_base_url()),
        user_id,
        "billing",
    )
    .await
    {
        Ok(addresses) => addresses,
        Err(error) => {
            tracing::error!("web: list_addresses failed for {user_id}: {error:?}");
            Vec::new()
        }
    }
}

/// Validate a submitted `billing_address_id` against the user's fetched
/// billing-address list (already scoped to the session user and the billing
/// category, so membership is ownership + category in one check). A failed
/// fetch yields an empty list, so this still fails closed: an unverifiable
/// id is refused rather than saved.
fn billing_address_is_valid(
    billing_addresses: &[AddressSummary],
    billing_address_id: &str,
) -> bool {
    billing_addresses.iter().any(|a| a.id == billing_address_id)
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

fn index(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path::end()
        .and(warp::get())
        .and(cookie_filter())
        .and(store)
        .and_then(|cookie: Option<String>, store: SharedStore| async move {
            let user_id = match require_user(cookie.as_deref(), "/").await {
                Ok(user_id) => user_id,
                Err(resp) => return Ok::<_, Rejection>(resp),
            };
            let (payment_methods, billing_addresses, cart_count) = tokio::join!(
                store.list_for_user(&user_id),
                fetch_billing_addresses_or_empty(&user_id),
                nav_cart_count(cookie.as_deref()),
            );
            let payment_methods = match payment_methods {
                Ok(payment_methods) => payment_methods,
                Err(error) => {
                    tracing::error!("web: list_for_user failed for {user_id}: {error:?}");
                    return Ok(internal_error());
                }
            };
            let lookup: HashMap<String, AddressSummary> = billing_addresses
                .into_iter()
                .map(|a| (a.id.clone(), a))
                .collect();
            match templates::render_index_html(payment_methods, &lookup, None, cart_count) {
                Ok(html) => Ok(warp::reply::html(html).into_response()),
                Err(error) => {
                    tracing::error!("web: index render failed: {error:?}");
                    Ok(internal_error())
                }
            }
        })
}

fn new_payment_method()
-> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path("new")
        .and(warp::path::end())
        .and(warp::get())
        .and(cookie_filter())
        .and_then(|cookie: Option<String>| async move {
            let user_id = match require_user(cookie.as_deref(), "/new").await {
                Ok(user_id) => user_id,
                Err(resp) => return Ok::<_, Rejection>(resp),
            };
            let (billing_addresses, cart_count) = tokio::join!(
                fetch_billing_addresses_or_empty(&user_id),
                nav_cart_count(cookie.as_deref()),
            );
            match templates::render_form_html(
                None,
                PaymentMethodType::CreditCard,
                &billing_addresses,
                None,
                cart_count,
            ) {
                Ok(html) => Ok(warp::reply::html(html).into_response()),
                Err(error) => {
                    tracing::error!("web: new form render failed: {error:?}");
                    Ok(internal_error())
                }
            }
        })
}

fn create_payment_method(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path::end()
        .and(warp::post())
        .and(cookie_filter())
        .and(warp::body::form())
        .and(store)
        .and_then(
            |cookie: Option<String>, form: PaymentMethodForm, store: SharedStore| async move {
                let user_id = match require_user(cookie.as_deref(), "/new").await {
                    Ok(user_id) => user_id,
                    Err(resp) => return Ok::<_, Rejection>(resp),
                };
                let billing_addresses = fetch_billing_addresses_or_empty(&user_id).await;
                let values = PaymentMethodFormValues::from_form(&form);

                let method_type = match PaymentMethodType::parse(&form.method_type) {
                    Ok(method_type) => method_type,
                    Err(e) => {
                        return Ok(render_form_error(
                            None,
                            PaymentMethodType::CreditCard,
                            &billing_addresses,
                            values,
                            StoreError::InvalidInput(e),
                            cookie.as_deref(),
                        )
                        .await);
                    }
                };

                if !billing_address_is_valid(&billing_addresses, &form.billing_address_id) {
                    return Ok(render_form_error(
                        None,
                        method_type,
                        &billing_addresses,
                        values,
                        StoreError::InvalidInput("Invalid billing address".to_string()),
                        cookie.as_deref(),
                    )
                    .await);
                }

                let input = match form.into_create(method_type) {
                    Ok(input) => input,
                    Err(e) => {
                        return Ok(render_form_error(
                            None,
                            method_type,
                            &billing_addresses,
                            values,
                            StoreError::InvalidInput(e),
                            cookie.as_deref(),
                        )
                        .await);
                    }
                };
                let response = match store.create(&user_id, input).await {
                    Ok(_) => redirect("/"),
                    Err(e) => {
                        render_form_error(
                            None,
                            method_type,
                            &billing_addresses,
                            values,
                            e,
                            cookie.as_deref(),
                        )
                        .await
                    }
                };
                Ok(response)
            },
        )
}

fn edit_payment_method(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path::param::<String>()
        .and(warp::path("edit"))
        .and(warp::path::end())
        .and(warp::get())
        .and(cookie_filter())
        .and(store)
        .and_then(
            |id: String, cookie: Option<String>, store: SharedStore| async move {
                let return_path = format!("/{id}/edit");
                let user_id = match require_user(cookie.as_deref(), &return_path).await {
                    Ok(user_id) => user_id,
                    Err(resp) => return Ok::<_, Rejection>(resp),
                };
                let payment_method = match store.get_for_user(&user_id, &id).await {
                    Ok(payment_method) => payment_method,
                    Err(StoreError::NotFound(_)) => return Err(warp::reject::not_found()),
                    Err(error) => {
                        tracing::error!("web: get_for_user failed for {user_id}/{id}: {error:?}");
                        return Ok(internal_error());
                    }
                };
                let (billing_addresses, cart_count) = tokio::join!(
                    fetch_billing_addresses_or_empty(&user_id),
                    nav_cart_count(cookie.as_deref()),
                );
                match templates::render_form_html(
                    Some(&payment_method),
                    payment_method.method_type,
                    &billing_addresses,
                    None,
                    cart_count,
                ) {
                    Ok(html) => Ok(warp::reply::html(html).into_response()),
                    Err(error) => {
                        tracing::error!("web: edit form render failed: {error:?}");
                        Ok(internal_error())
                    }
                }
            },
        )
}

fn update_payment_method(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path::param::<String>()
        .and(warp::path::end())
        .and(warp::post())
        .and(cookie_filter())
        .and(warp::body::form())
        .and(store)
        .and_then(
            |id: String,
             cookie: Option<String>,
             form: PaymentMethodForm,
             store: SharedStore| async move {
                let return_path = format!("/{id}/edit");
                let user_id = match require_user(cookie.as_deref(), &return_path).await {
                    Ok(user_id) => user_id,
                    Err(resp) => return Ok::<_, Rejection>(resp),
                };
                let existing: PaymentMethod = match store.get_for_user(&user_id, &id).await {
                    Ok(payment_method) => payment_method,
                    Err(StoreError::NotFound(_)) => return Err(warp::reject::not_found()),
                    Err(error) => {
                        tracing::error!("web: get_for_user failed for {user_id}/{id}: {error:?}");
                        return Ok(internal_error());
                    }
                };
                let method_type = existing.method_type;
                let billing_addresses = fetch_billing_addresses_or_empty(&user_id).await;
                let values = PaymentMethodFormValues::from_form(&form);

                if !billing_address_is_valid(&billing_addresses, &form.billing_address_id) {
                    return Ok(render_form_error(
                        Some(&existing),
                        method_type,
                        &billing_addresses,
                        values,
                        StoreError::InvalidInput("Invalid billing address".to_string()),
                        cookie.as_deref(),
                    )
                    .await);
                }

                let input = match form.into_update(
                    method_type,
                    &existing.last4,
                    existing.brand.as_deref(),
                ) {
                    Ok(input) => input,
                    Err(e) => {
                        return Ok(render_form_error(
                            Some(&existing),
                            method_type,
                            &billing_addresses,
                            values,
                            StoreError::InvalidInput(e),
                            cookie.as_deref(),
                        )
                        .await);
                    }
                };
                let response = match store.update(&user_id, &id, input).await {
                    Ok(_) => redirect("/"),
                    Err(e) => {
                        render_form_error(
                            Some(&existing),
                            method_type,
                            &billing_addresses,
                            values,
                            e,
                            cookie.as_deref(),
                        )
                        .await
                    }
                };
                Ok(response)
            },
        )
}

fn delete_payment_method(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path::param::<String>()
        .and(warp::path("delete"))
        .and(warp::path::end())
        .and(warp::post())
        .and(cookie_filter())
        .and(store)
        .and_then(
            |id: String, cookie: Option<String>, store: SharedStore| async move {
                let user_id = match require_user(cookie.as_deref(), "/").await {
                    Ok(user_id) => user_id,
                    Err(resp) => return Ok::<_, Rejection>(resp),
                };
                match store.delete(&user_id, &id).await {
                    Ok(()) => Ok(redirect("/")),
                    Err(StoreError::NotFound(_)) => Err(warp::reject::not_found()),
                    Err(error) => {
                        tracing::error!("web: delete failed for {user_id}/{id}: {error:?}");
                        Ok(internal_error())
                    }
                }
            },
        )
}

fn set_default_payment_method(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path::param::<String>()
        .and(warp::path("default"))
        .and(warp::path::end())
        .and(warp::post())
        .and(cookie_filter())
        .and(store)
        .and_then(
            |id: String, cookie: Option<String>, store: SharedStore| async move {
                let user_id = match require_user(cookie.as_deref(), "/").await {
                    Ok(user_id) => user_id,
                    Err(resp) => return Ok::<_, Rejection>(resp),
                };
                match store.set_default(&user_id, &id).await {
                    Ok(()) => Ok(redirect("/")),
                    Err(StoreError::NotFound(_)) => Err(warp::reject::not_found()),
                    Err(error) => {
                        tracing::error!("web: set_default failed for {user_id}/{id}: {error:?}");
                        Ok(internal_error())
                    }
                }
            },
        )
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Re-render the form with the visitor's input preserved and `err` shown,
/// fetching the nav cart count only now that we know HTML will be rendered
/// (successful POSTs redirect and never need it).
async fn render_form_error(
    payment_method: Option<&PaymentMethod>,
    method_type: PaymentMethodType,
    billing_addresses: &[AddressSummary],
    values: PaymentMethodFormValues,
    err: StoreError,
    cookie: Option<&str>,
) -> Response {
    let cart_count = nav_cart_count(cookie).await;
    let message = err.to_string();
    match templates::render_form_html_with_values(
        payment_method,
        method_type,
        billing_addresses,
        Some(message),
        values,
        cart_count,
    ) {
        Ok(html) => warp::reply::with_status(warp::reply::html(html), StatusCode::BAD_REQUEST)
            .into_response(),
        Err(_) => internal_error(),
    }
}
