//! [`StoreError`].

#[allow(unused_imports)]
use super::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("payment method not found")]
    NotFound,
    #[error("{0}")]
    InvalidInput(String),
    #[error("database error: {0}")]
    Database(#[from] anyhow::Error),
}
impl From<sqlx::Error> for StoreError {
    fn from(err: sqlx::Error) -> Self {
        Self::Database(err.into())
    }
}
