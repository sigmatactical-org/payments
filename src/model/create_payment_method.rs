//! [`CreatePaymentMethod`].

use super::PaymentMethodType;

/// Fields accepted when creating a payment method. `is_default` is
/// deliberately absent: the caller never chooses it. `PaymentMethodStore::create`
/// makes the user's first payment method their default automatically; any
/// later promotion goes through the clear-then-set transaction in
/// `PaymentMethodStore::set_default`, which the UI calls as a separate action.
#[derive(Debug, Clone)]
pub struct CreatePaymentMethod {
    pub method_type: PaymentMethodType,
    pub billing_address_id: String,
    pub label: Option<String>,
    pub brand: Option<String>,
    pub last4: String,
    pub cardholder_name: Option<String>,
    pub expiry_month: Option<u8>,
    pub expiry_year: Option<u16>,
}
