use std::collections::HashMap;
use std::convert::Infallible;

use warp::http::StatusCode;
use warp::reply::Response;
use warp::{Filter, Rejection, Reply};

use crate::SharedStore;
use crate::addresses_client::{self, AddressSummary};
use crate::model::{PaymentMethod, PaymentMethodForm, PaymentMethodType};
use crate::store::StoreError;
use crate::templates::{self, PaymentMethodFormValues};

pub fn routes(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    index(store.clone())
        .or(new_payment_method(store.clone()))
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
async fn require_user(cookie: Option<String>, return_path: &str) -> Result<String, Response> {
    let status = sigma_pg::clients::session::fetch_identity_status(
        &crate::config::identity_internal_base_url(),
        cookie.as_deref(),
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
        &crate::config::identity_public_base_url(),
        &crate::config::public_base_url(),
        return_path,
    );
    redirect(&links.sign_in_url)
}

/// 303 redirect (PRG pattern for form POSTs, also used for the sign-in bounce).
// `to_string` is required, not redundant: `Uri::from_maybe_shared` needs an
// owned buffer it can turn into `Bytes` without borrowing past this call.
#[allow(clippy::unnecessary_to_owned)]
fn redirect(location: &str) -> Response {
    match warp::http::Uri::from_maybe_shared(location.to_string()) {
        Ok(uri) => warp::redirect::see_other(uri).into_response(),
        Err(_) => warp::reply::with_status(warp::reply(), StatusCode::INTERNAL_SERVER_ERROR)
            .into_response(),
    }
}

fn cookie_filter() -> impl Filter<Extract = (Option<String>,), Error = Rejection> + Clone {
    warp::header::optional::<String>("cookie")
}

/// List `user_id`'s billing addresses for the form dropdown / index lookup.
/// A network failure or the addresses service being down is treated as "no
/// billing addresses available" rather than a 500 — the page still renders,
/// just with an empty dropdown and (on the create form) a message pointing
/// the visitor at the addresses service.
async fn fetch_billing_addresses_or_empty(user_id: &str) -> Vec<AddressSummary> {
    match addresses_client::list_billing_addresses(user_id).await {
        Ok(addresses) => addresses,
        Err(error) => {
            tracing::error!("web: list_billing_addresses failed for {user_id}: {error:?}");
            Vec::new()
        }
    }
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
            let user_id = match require_user(cookie, "/").await {
                Ok(user_id) => user_id,
                Err(resp) => return Ok::<_, Rejection>(resp),
            };
            let payment_methods = store
                .list_for_user(&user_id)
                .await
                .map_err(|_| warp::reject::not_found())?;
            let billing_addresses = fetch_billing_addresses_or_empty(&user_id).await;
            let lookup: HashMap<String, AddressSummary> = billing_addresses
                .into_iter()
                .map(|a| (a.id.clone(), a))
                .collect();
            templates::render_index_html(payment_methods, &lookup, None)
                .map(|html| warp::reply::html(html).into_response())
                .map_err(|_| warp::reject::not_found())
        })
}

fn new_payment_method(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path("new")
        .and(warp::path::end())
        .and(warp::get())
        .and(cookie_filter())
        .and(store)
        .and_then(|cookie: Option<String>, _store: SharedStore| async move {
            let user_id = match require_user(cookie, "/new").await {
                Ok(user_id) => user_id,
                Err(resp) => return Ok::<_, Rejection>(resp),
            };
            let billing_addresses = fetch_billing_addresses_or_empty(&user_id).await;
            templates::render_form_html(
                None,
                PaymentMethodType::CreditCard,
                &billing_addresses,
                None,
            )
            .map(|html| warp::reply::html(html).into_response())
            .map_err(|_| warp::reject::not_found())
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
                let user_id = match require_user(cookie, "/new").await {
                    Ok(user_id) => user_id,
                    Err(resp) => return Ok::<_, Rejection>(resp),
                };
                let billing_addresses = fetch_billing_addresses_or_empty(&user_id).await;
                let render_method_type = PaymentMethodType::parse(&form.method_type)
                    .unwrap_or(PaymentMethodType::CreditCard);
                let values = PaymentMethodFormValues::from_form(&form);

                let method_type = match PaymentMethodType::parse(&form.method_type) {
                    Ok(method_type) => method_type,
                    Err(e) => {
                        return Ok(render_form_error(
                            None,
                            render_method_type,
                            &billing_addresses,
                            values,
                            StoreError::InvalidInput(e),
                        ));
                    }
                };

                if let Err(resp) = validate_billing_address(
                    &user_id,
                    &form.billing_address_id,
                    None,
                    method_type,
                    &billing_addresses,
                    &values,
                )
                .await
                {
                    return Ok(resp);
                }

                let response = match form.into_create(method_type) {
                    Ok(input) => match store.create(&user_id, input).await {
                        Ok(_) => redirect("/"),
                        Err(e) => {
                            render_form_error(None, method_type, &billing_addresses, values, e)
                        }
                    },
                    Err(e) => render_form_error(
                        None,
                        method_type,
                        &billing_addresses,
                        values,
                        StoreError::InvalidInput(e),
                    ),
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
                let user_id = match require_user(cookie, &return_path).await {
                    Ok(user_id) => user_id,
                    Err(resp) => return Ok::<_, Rejection>(resp),
                };
                match store.get_for_user(&user_id, &id).await {
                    Ok(payment_method) => {
                        let billing_addresses = fetch_billing_addresses_or_empty(&user_id).await;
                        templates::render_form_html(
                            Some(payment_method.clone()),
                            payment_method.method_type,
                            &billing_addresses,
                            None,
                        )
                        .map(|html| warp::reply::html(html).into_response())
                        .map_err(|_| warp::reject::not_found())
                    }
                    Err(StoreError::NotFound) => Err(warp::reject::not_found()),
                    Err(_) => Err(warp::reject::not_found()),
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
                let user_id = match require_user(cookie, &return_path).await {
                    Ok(user_id) => user_id,
                    Err(resp) => return Ok::<_, Rejection>(resp),
                };
                let existing: PaymentMethod = match store.get_for_user(&user_id, &id).await {
                    Ok(payment_method) => payment_method,
                    Err(StoreError::NotFound) => return Err(warp::reject::not_found()),
                    Err(_) => return Err(warp::reject::not_found()),
                };
                let method_type = existing.method_type;
                let billing_addresses = fetch_billing_addresses_or_empty(&user_id).await;
                let values = PaymentMethodFormValues::from_form(&form);

                if let Err(resp) = validate_billing_address(
                    &user_id,
                    &form.billing_address_id,
                    Some(&existing),
                    method_type,
                    &billing_addresses,
                    &values,
                )
                .await
                {
                    return Ok(resp);
                }

                let response = match form.into_update(
                    method_type,
                    &existing.last4,
                    existing.brand.as_deref(),
                ) {
                    Ok(input) => match store.update(&user_id, &id, input).await {
                        Ok(_) => redirect("/"),
                        Err(e) => render_form_error(
                            Some(existing),
                            method_type,
                            &billing_addresses,
                            values,
                            e,
                        ),
                    },
                    Err(e) => render_form_error(
                        Some(existing),
                        method_type,
                        &billing_addresses,
                        values,
                        StoreError::InvalidInput(e),
                    ),
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
                let user_id = match require_user(cookie, "/").await {
                    Ok(user_id) => user_id,
                    Err(resp) => return Ok::<_, Rejection>(resp),
                };
                match store.delete(&user_id, &id).await {
                    Ok(()) => Ok(redirect("/")),
                    Err(StoreError::NotFound) => Err(warp::reject::not_found()),
                    Err(_) => Err(warp::reject::not_found()),
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
                let user_id = match require_user(cookie, "/").await {
                    Ok(user_id) => user_id,
                    Err(resp) => return Ok::<_, Rejection>(resp),
                };
                match store.set_default(&user_id, &id).await {
                    Ok(()) => Ok(redirect("/")),
                    Err(StoreError::NotFound) => Err(warp::reject::not_found()),
                    Err(_) => Err(warp::reject::not_found()),
                }
            },
        )
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Validate that `billing_address_id` belongs to `user_id` and is a
/// billing-category address, over the addresses service's internal JSON
/// API. A network failure is treated the same as "not found" (fail closed —
/// we can't confirm ownership, so we refuse to save) rather than silently
/// accepting an unverified id.
async fn validate_billing_address(
    user_id: &str,
    billing_address_id: &str,
    existing: Option<&PaymentMethod>,
    method_type: PaymentMethodType,
    billing_addresses: &[AddressSummary],
    values: &PaymentMethodFormValues,
) -> Result<(), Response> {
    let found = match addresses_client::get_billing_address(user_id, billing_address_id).await {
        Ok(found) => found,
        Err(error) => {
            tracing::error!(
                "web: get_billing_address failed for {user_id}/{billing_address_id}: {error:?}"
            );
            None
        }
    };
    if found.is_some() {
        return Ok(());
    }
    Err(render_form_error(
        existing.cloned(),
        method_type,
        billing_addresses,
        values.clone(),
        StoreError::InvalidInput("Invalid billing address".to_string()),
    ))
}

fn render_form_error(
    payment_method: Option<PaymentMethod>,
    method_type: PaymentMethodType,
    billing_addresses: &[AddressSummary],
    values: PaymentMethodFormValues,
    err: StoreError,
) -> Response {
    let message = err.to_string();
    match templates::render_form_html_with_values(
        payment_method,
        method_type,
        billing_addresses,
        Some(message),
        values,
    ) {
        Ok(html) => warp::reply::with_status(warp::reply::html(html), StatusCode::BAD_REQUEST)
            .into_response(),
        Err(_) => warp::reply::with_status(warp::reply(), StatusCode::INTERNAL_SERVER_ERROR)
            .into_response(),
    }
}
