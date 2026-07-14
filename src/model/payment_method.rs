//! [`PaymentMethod`].

#[allow(unused_imports)]
use super::*;
use serde::{Deserialize, Serialize};

/// A saved payment method. Full PAN and CVV are accepted on the create/edit
/// form for validation only and are **never** persisted — only `brand`,
/// `last4`, expiry, and (credit cards) `cardholder_name` are stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentMethod {
    pub id: String,
    pub user_id: String,
    pub method_type: PaymentMethodType,
    pub billing_address_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    pub last4: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cardholder_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry_month: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry_year: Option<u16>,
    pub is_default: bool,
    pub updated_at: String,
}
impl PaymentMethod {
    #[must_use]
    pub fn new(user_id: &str, input: CreatePaymentMethod) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.trim().to_string(),
            method_type: input.method_type,
            billing_address_id: input.billing_address_id,
            label: input.label,
            brand: input.brand,
            last4: input.last4,
            cardholder_name: input.cardholder_name,
            expiry_month: input.expiry_month,
            expiry_year: input.expiry_year,
            is_default: false,
            updated_at: now,
        }
    }

    /// Apply a partial update in place.
    pub fn apply_update(&mut self, input: UpdatePaymentMethod) {
        self.billing_address_id = input.billing_address_id;
        self.label = input.label;
        self.brand = input.brand;
        self.last4 = input.last4;
        self.cardholder_name = input.cardholder_name;
        self.expiry_month = input.expiry_month;
        self.expiry_year = input.expiry_year;
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }
}
