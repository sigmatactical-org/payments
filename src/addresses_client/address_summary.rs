//! [`AddressSummary`].

#[allow(unused_imports)]
use super::*;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AddressSummary {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    pub line1: String,
    pub city: String,
    #[serde(default)]
    pub region: Option<String>,
    pub postal_code: String,
    pub country: String,
    pub category: String,
}
impl AddressSummary {
    /// Short one-line summary for the billing-address dropdown, e.g.
    /// "123 Main St, Springfield".
    #[must_use]
    pub fn short_summary(&self) -> String {
        format!("{}, {}", self.line1, self.city)
    }

    #[must_use]
    pub fn is_billing(&self) -> bool {
        self.category == "billing"
    }
}
