//! [`PaymentMethodRow`].

#[allow(unused_imports)]
use super::*;

/// A row in the payment method list.
pub struct PaymentMethodRow {
    pub method_type_label: String,
    pub label: String,
    pub brand: String,
    pub cardholder_name: String,
    pub last4_masked: String,
    pub expiry: String,
    pub billing_address_summary: String,
    pub is_default: bool,
    pub edit_url: String,
    pub delete_url: String,
    pub default_url: String,
}
