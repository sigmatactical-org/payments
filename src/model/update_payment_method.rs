//! [`UpdatePaymentMethod`].

#[allow(unused_imports)]
use super::*;
use serde::Deserialize;

/// Fields accepted when updating a payment method. `method_type` cannot be
/// changed after creation — it is fixed at creation time via
/// `CreatePaymentMethod`.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdatePaymentMethod {
    pub billing_address_id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub brand: Option<String>,
    pub last4: String,
    #[serde(default)]
    pub cardholder_name: Option<String>,
    #[serde(default)]
    pub expiry_month: Option<u8>,
    #[serde(default)]
    pub expiry_year: Option<u16>,
}
