//! [`CartLineDetail`].

#[allow(unused_imports)]
use super::*;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CartLineDetail {
    pub(crate) line: CartLine,
}
