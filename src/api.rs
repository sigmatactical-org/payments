//! Internal-token-gated JSON API for payment methods and charges.

use std::convert::Infallible;

use sigma_pg::api::{internal_auth, json_error, store_error_status};
use warp::http::StatusCode;
use warp::{Filter, Rejection, Reply};

use crate::SharedStore;
use crate::model::{ChargeStatus, CreateCharge, RefundCharge};

/// Mounted under `/api` by [`crate::routes`].
pub fn routes(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    list_user_payment_methods(store.clone())
        .or(list_charges(store.clone()))
        .or(create_charge(store.clone()))
        .or(refund_charge(store))
}

/// `POST /api/charges/{id}/refund` — reverse a charge in full.
///
/// The compensating action for a checkout that took payment but could not be
/// completed. Idempotent: retrying returns the original refund rather than
/// issuing a second credit, so a caller that timed out can safely repeat the
/// request.
fn refund_charge(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("charges" / String / "refund")
        .and(warp::path::end())
        .and(warp::post())
        .and(internal_auth())
        .and(warp::body::json())
        .and(store)
        .and_then(
            |charge_id: String, body: RefundCharge, store: SharedStore| async move {
                match store.refund_charge(&charge_id, &body.reason).await {
                    Ok(refund) => Ok::<_, Rejection>(
                        warp::reply::with_status(warp::reply::json(&refund), StatusCode::CREATED)
                            .into_response(),
                    ),
                    Err(e) => Ok(json_error(store_error_status(&e), e.to_string())),
                }
            },
        )
}

/// `GET /api/charges` — the whole charge log, for the accounting service's
/// receipt reconcile. Internal-token-gated like every route here, so it is
/// never reachable from a browser session.
fn list_charges(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path("charges")
        .and(warp::path::end())
        .and(warp::get())
        .and(internal_auth())
        .and(store)
        .and_then(|store: SharedStore| async move {
            match store.list_charges().await {
                Ok(charges) => Ok::<_, Rejection>(warp::reply::json(&charges).into_response()),
                Err(e) => Ok(json_error(store_error_status(&e), e.to_string())),
            }
        })
}

fn list_user_payment_methods(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("users" / String / "payment-methods")
        .and(warp::path::end())
        .and(warp::get())
        .and(internal_auth())
        .and(store)
        .and_then(|user_id: String, store: SharedStore| async move {
            match store.list_for_user(&user_id).await {
                Ok(methods) => Ok::<_, Rejection>(warp::reply::json(&methods).into_response()),
                Err(e) => Ok(json_error(store_error_status(&e), e.to_string())),
            }
        })
}

fn create_charge(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path("charges")
        .and(warp::path::end())
        .and(warp::post())
        .and(internal_auth())
        .and(warp::body::json())
        .and(store)
        .and_then(|body: CreateCharge, store: SharedStore| async move {
            let currency = body.currency.as_deref().unwrap_or("usd");
            match store
                .create_charge(
                    &body.user_id,
                    &body.payment_method_id,
                    body.amount_cents,
                    currency,
                    body.reference.as_deref(),
                )
                .await
            {
                Ok(charge) => {
                    let status = if charge.status == ChargeStatus::Succeeded {
                        StatusCode::CREATED
                    } else {
                        StatusCode::PAYMENT_REQUIRED
                    };
                    Ok::<_, Rejection>(
                        warp::reply::with_status(warp::reply::json(&charge), status)
                            .into_response(),
                    )
                }
                Err(e) => Ok(json_error(store_error_status(&e), e.to_string())),
            }
        })
}
