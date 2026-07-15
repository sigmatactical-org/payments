//! [`CartDetail`].

#[allow(unused_imports)]
use super::*;
use serde::Deserialize;

/// Enriched cart returned by the cart service (`{ cart, user, lines }`).
#[derive(Debug, Clone, Deserialize)]
pub struct CartDetail {
    #[serde(default)]
    pub(crate) lines: Vec<CartLineDetail>,
}
impl CartDetail {
    /// Total number of items (summed line quantities) for the nav badge.
    #[must_use]
    pub fn item_count(&self) -> u32 {
        self.lines.iter().map(|l| l.line.quantity).sum()
    }
}
