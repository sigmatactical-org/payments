//! [`BillingAddressOptionRow`].

#[allow(unused_imports)]
use super::*;

/// One `<option>` in the billing-address dropdown.
pub struct BillingAddressOptionRow {
    pub id: String,
    pub summary: String,
    pub selected: bool,
}
