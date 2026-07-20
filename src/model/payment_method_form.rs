//! [`PaymentMethodForm`].

use serde::Deserialize;
use sigma_pg::form::{empty_to_none, required};

use super::{
    CreatePaymentMethod, PaymentMethodType, UpdatePaymentMethod, detect_card_brand, normalize_pan,
    parse_expiry, validate_cvv, validate_last4, validate_pan_for_brand,
};

/// Raw `application/x-www-form-urlencoded` body for the create/edit web form.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PaymentMethodForm {
    #[serde(default)]
    pub method_type: String,
    #[serde(default)]
    pub billing_address_id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub brand: String,
    #[serde(default)]
    pub card_number: String,
    #[serde(default)]
    pub cardholder_name: String,
    #[serde(default)]
    pub cvv: String,
    #[serde(default)]
    pub last4: String,
    #[serde(default)]
    pub expiry_month: String,
    #[serde(default)]
    pub expiry_year: String,
}
impl PaymentMethodForm {
    /// Validate the form into a create request.
    pub fn into_create(
        self,
        method_type: PaymentMethodType,
    ) -> Result<CreatePaymentMethod, String> {
        let (brand, last4, cardholder_name) = match method_type {
            PaymentMethodType::CreditCard => {
                let digits = normalize_pan(&self.card_number)?;
                let brand = detect_card_brand(&digits)
                    .ok_or_else(|| "unrecognized card number — check the digits".to_string())?;
                validate_pan_for_brand(&digits, brand)?;
                validate_cvv(&self.cvv, brand)?;
                let cardholder_name = required(self.cardholder_name, "cardholder_name")?;
                let last4 = digits[digits.len() - 4..].to_string();
                (Some(brand.to_string()), last4, Some(cardholder_name))
            }
            PaymentMethodType::BankAccount => {
                let last4 = validate_last4(&self.last4)?;
                let brand = empty_to_none(self.brand);
                (brand, last4, None)
            }
        };
        let (expiry_month, expiry_year) =
            parse_expiry(method_type, &self.expiry_month, &self.expiry_year)?;
        Ok(CreatePaymentMethod {
            method_type,
            billing_address_id: required(self.billing_address_id, "billing_address_id")?,
            label: empty_to_none(self.label),
            brand,
            last4,
            cardholder_name,
            expiry_month,
            expiry_year,
        })
    }

    /// Validate the form into an update request.
    pub fn into_update(
        self,
        method_type: PaymentMethodType,
        existing_last4: &str,
        existing_brand: Option<&str>,
    ) -> Result<UpdatePaymentMethod, String> {
        let (brand, last4, cardholder_name) = match method_type {
            PaymentMethodType::CreditCard => {
                let cardholder_name = required(self.cardholder_name, "cardholder_name")?;
                let pan = self.card_number.trim();
                if pan.is_empty() {
                    // Keep stored last4/brand when the card number is left blank on edit.
                    let brand = existing_brand
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .map(str::to_string)
                        .ok_or_else(|| {
                            "brand is missing — re-enter the full card number".to_string()
                        })?;
                    let last4 = validate_last4(existing_last4)?;
                    (Some(brand), last4, Some(cardholder_name))
                } else {
                    let digits = normalize_pan(&self.card_number)?;
                    let brand = detect_card_brand(&digits)
                        .ok_or_else(|| "unrecognized card number — check the digits".to_string())?;
                    validate_pan_for_brand(&digits, brand)?;
                    validate_cvv(&self.cvv, brand)?;
                    let last4 = digits[digits.len() - 4..].to_string();
                    (Some(brand.to_string()), last4, Some(cardholder_name))
                }
            }
            PaymentMethodType::BankAccount => {
                let last4 = validate_last4(&self.last4)?;
                let brand = empty_to_none(self.brand);
                (brand, last4, None)
            }
        };
        let (expiry_month, expiry_year) =
            parse_expiry(method_type, &self.expiry_month, &self.expiry_year)?;
        Ok(UpdatePaymentMethod {
            billing_address_id: required(self.billing_address_id, "billing_address_id")?,
            label: empty_to_none(self.label),
            brand,
            last4,
            cardholder_name,
            expiry_month,
            expiry_year,
        })
    }
}
