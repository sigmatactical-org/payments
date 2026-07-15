//! [`CartError`].

#[allow(unused_imports)]
use super::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CartError {
    #[error("cart integration is not configured (set PAYMENTS_CART_BASE_URL)")]
    NotConfigured,
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("cart request failed: {0}")]
    Request(String),
}
