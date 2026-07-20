//! [`UpdatePaymentMethod`].

/// Fields accepted when updating a payment method. `method_type` cannot be
/// changed after creation — it is fixed at creation time via
/// `CreatePaymentMethod`.
#[derive(Debug, Clone)]
pub struct UpdatePaymentMethod {
    pub billing_address_id: String,
    pub label: Option<String>,
    pub brand: Option<String>,
    pub last4: String,
    pub cardholder_name: Option<String>,
    pub expiry_month: Option<u8>,
    pub expiry_year: Option<u16>,
}
