//! Sigma Payments: payment methods (credit card, bank account) for identity
//! users, each tied to one of the user's billing addresses.

#![forbid(unsafe_code)]

mod api;
pub mod config;
mod model;
pub mod store;
mod templates;
mod web;

use std::convert::Infallible;
use std::sync::{Arc, OnceLock};

use warp::Filter;
use warp::Reply;

/// Shared payment method store handle (`PgPool` is internally concurrent).
pub type SharedStore = Arc<store::PaymentMethodStore>;

fn with_store(
    store: SharedStore,
) -> impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone {
    warp::any().map(move || store.clone())
}

/// Site routes: session-gated web UI, internal JSON API (`/api`), `/up`,
/// health routes, theme static assets, and themed error recovery, with the
/// shared security header set (CSP `connect-src` extended with the identity
/// BFF origin).
pub fn routes(
    store: store::PaymentMethodStore,
) -> impl Filter<Extract = (impl Reply,), Error = Infallible> + Clone + Send + Sync + 'static {
    let health_pool = Arc::new(store.pool().clone());
    let store = Arc::new(store);

    let site = sigma_theme::warp::site_routes(
        web::routes(with_store(store.clone())),
        sigma_pg::health::warp::health_routes("payments", Some(health_pool))
            .or(warp::path("api").and(api::routes(with_store(store)))),
    );
    sigma_theme::warp::security_headers(site, identity_origin())
}

/// The identity origin appended to the CSP `connect-src`, resolved once.
/// `security_headers` returns a `'static` filter, so its `connect_src_extra`
/// must outlive the call; caching here avoids leaking a fresh `String` per
/// `routes()` invocation.
fn identity_origin() -> &'static str {
    static ORIGIN: OnceLock<String> = OnceLock::new();
    ORIGIN.get_or_init(config::identity_public_origin)
}

#[cfg(test)]
mod tests {
    use warp::http::StatusCode;

    use super::routes;
    use crate::store;

    async fn test_store() -> store::PaymentMethodStore {
        sigma_pg::clients::internal::ensure_test_internal_token();
        store::PaymentMethodStore::connect_empty()
            .await
            .expect("PostgreSQL required for tests")
    }

    #[tokio::test]
    async fn up_returns_ok() {
        let res = warp::test::request()
            .method("GET")
            .path("/up")
            .reply(&routes(test_store().await))
            .await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn index_without_session_redirects_to_sign_in() {
        let res = warp::test::request()
            .method("GET")
            .path("/")
            .reply(&routes(test_store().await))
            .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let location = res
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert!(location.contains("/auth/login"));
    }
}
