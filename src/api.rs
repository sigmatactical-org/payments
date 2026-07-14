//! Internal-token-gated JSON API for payment methods and charges.

use std::convert::Infallible;

use warp::http::StatusCode;
use warp::reply::Response;
use warp::{Filter, Rejection, Reply};

use crate::SharedStore;
use crate::model::CreateCharge;
use crate::store::StoreError;

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn json_error(status: StatusCode, message: impl Into<String>) -> Response {
    warp::reply::with_status(
        warp::reply::json(&ErrorBody {
            error: message.into(),
        }),
        status,
    )
    .into_response()
}

fn store_error_status(err: &StoreError) -> StatusCode {
    match err {
        StoreError::NotFound => StatusCode::NOT_FOUND,
        StoreError::InvalidInput(_) => StatusCode::BAD_REQUEST,
        StoreError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn internal_auth() -> impl Filter<Extract = (), Error = Rejection> + Clone {
    warp::header::optional::<String>("authorization")
        .and(warp::header::optional::<String>("x-sigma-internal-token"))
        .and_then(
            |authorization: Option<String>, internal_token: Option<String>| async move {
                if sigma_pg::clients::internal::authorize_internal(
                    authorization.as_deref(),
                    internal_token.as_deref(),
                ) {
                    Ok::<_, Rejection>(())
                } else {
                    Err(warp::reject::not_found())
                }
            },
        )
        .untuple_one()
}

/// Mounted under `/api` by [`crate::routes`].
pub fn routes(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    list_user_payment_methods(store.clone()).or(create_charge(store))
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
                    let status = if charge.status.as_str() == "succeeded" {
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
