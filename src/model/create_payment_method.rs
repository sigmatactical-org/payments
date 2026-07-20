//! [`CreatePaymentMethod`].

use super::PaymentMethodType;

/// Fields accepted when creating a payment method. `is_default` is
/// deliberately absent: new payment methods are never created as the
/// default directly. Promoting a payment method to default requires the
/// clear-then-set transaction implemented by
/// `PaymentMethodStore::set_default`, which the UI calls as a separate
/// action after creation.
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
