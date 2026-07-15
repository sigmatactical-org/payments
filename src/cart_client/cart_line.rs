//! [`CartLine`].

#[allow(unused_imports)]
use super::*;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CartLine {
    pub(crate) quantity: u32,
}
