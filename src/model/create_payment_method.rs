//! [`CreatePaymentMethod`].

#[allow(unused_imports)]
use super::*;
use serde::Deserialize;

/// Fields accepted when creating a payment method. `is_default` is
/// deliberately absent: new payment methods are never created as the
/// default directly. Promoting a payment method to default requires the
/// clear-then-set transaction implemented by
/// `PaymentMethodStore::set_default`, which the UI calls as a separate
/// action after creation.
#[derive(Debug, Clone, Deserialize)]
pub struct CreatePaymentMethod {
    pub method_type: PaymentMethodType,
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
