//! [`PaymentMethodFormValues`].

use crate::model::{PaymentMethod, PaymentMethodForm};

/// Preserved (possibly invalid) form input, re-rendered on a validation error
/// so the visitor doesn't lose what they typed.
#[derive(Default, Clone)]
pub struct PaymentMethodFormValues {
    pub billing_address_id: String,
    pub label: String,
    pub brand: String,
    pub card_number: String,
    pub cardholder_name: String,
    pub last4: String,
    pub expiry_month: String,
    pub expiry_year: String,
}
impl PaymentMethodFormValues {
    #[must_use]
    pub fn from_form(form: &PaymentMethodForm) -> Self {
        Self {
            billing_address_id: form.billing_address_id.clone(),
            label: form.label.clone(),
            brand: form.brand.clone(),
            card_number: form.card_number.clone(),
            cardholder_name: form.cardholder_name.clone(),
            last4: form.last4.clone(),
            expiry_month: form.expiry_month.clone(),
            expiry_year: form.expiry_year.clone(),
        }
    }

    #[must_use]
    pub fn from_payment_method(payment_method: &PaymentMethod) -> Self {
        Self {
            billing_address_id: payment_method.billing_address_id.clone(),
            label: payment_method.label.clone().unwrap_or_default(),
            brand: payment_method.brand.clone().unwrap_or_default(),
            card_number: String::new(),
            cardholder_name: payment_method.cardholder_name.clone().unwrap_or_default(),
            last4: payment_method.last4.clone(),
            expiry_month: payment_method
                .expiry_month
                .map(|m| m.to_string())
                .unwrap_or_default(),
            expiry_year: payment_method
                .expiry_year
                .map(|y| y.to_string())
                .unwrap_or_default(),
        }
    }
}
