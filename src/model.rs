use chrono::Datelike;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethodType {
    CreditCard,
    BankAccount,
}

impl PaymentMethodType {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            PaymentMethodType::CreditCard => "credit_card",
            PaymentMethodType::BankAccount => "bank_account",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_lowercase().as_str() {
            "credit_card" => Ok(PaymentMethodType::CreditCard),
            "bank_account" => Ok(PaymentMethodType::BankAccount),
            _ => Err("method_type must be credit_card or bank_account".to_string()),
        }
    }
}

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

/// Outcome of a deposit/charge against a saved payment method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChargeStatus {
    Succeeded,
    Failed,
}

impl ChargeStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ChargeStatus::Succeeded => "succeeded",
            ChargeStatus::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim() {
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            _ => Err("status must be succeeded or failed".to_string()),
        }
    }
}

/// A recorded charge (demo processor — no card network).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Charge {
    pub id: String,
    pub user_id: String,
    pub payment_method_id: String,
    pub amount_cents: u64,
    pub currency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    pub status: ChargeStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    pub created_at: String,
}

/// Input for `POST /api/charges`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateCharge {
    pub user_id: String,
    pub payment_method_id: String,
    pub amount_cents: u64,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub reference: Option<String>,
}

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

/// Strip spaces/dashes; require digits only.
fn normalize_pan(value: &str) -> Result<String, String> {
    let digits: String = value
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect();
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err("card number must contain digits only".to_string());
    }
    if !(13..=19).contains(&digits.len()) {
        return Err("card number must be 13–19 digits".to_string());
    }
    if !luhn_ok(&digits) {
        return Err("card number failed checksum validation".to_string());
    }
    Ok(digits)
}

fn luhn_ok(digits: &str) -> bool {
    let mut sum = 0u32;
    let mut dbl = false;
    for b in digits.bytes().rev() {
        let mut d = u32::from(b - b'0');
        if dbl {
            d *= 2;
            if d > 9 {
                d -= 9;
            }
        }
        sum += d;
        dbl = !dbl;
    }
    sum.is_multiple_of(10)
}

/// Detect major network from IIN / leading digits.
#[must_use]
pub fn detect_card_brand(digits: &str) -> Option<&'static str> {
    let b = digits.as_bytes();
    if b.is_empty() {
        return None;
    }
    // American Express: 34 / 37
    if digits.starts_with("34") || digits.starts_with("37") {
        return Some("American Express");
    }
    // Visa: 4
    if b[0] == b'4' {
        return Some("Visa");
    }
    // Mastercard: 51–55 or 2221–2720
    if digits.len() >= 2 {
        let two: u16 = digits[..2].parse().unwrap_or(0);
        if (51..=55).contains(&two) {
            return Some("Mastercard");
        }
    }
    if digits.len() >= 4 {
        let four: u16 = digits[..4].parse().unwrap_or(0);
        if (2221..=2720).contains(&four) {
            return Some("Mastercard");
        }
    }
    // Discover: 6011, 65, 644–649
    if digits.starts_with("6011") || digits.starts_with("65") {
        return Some("Discover");
    }
    if digits.len() >= 3 {
        let three: u16 = digits[..3].parse().unwrap_or(0);
        if (644..=649).contains(&three) {
            return Some("Discover");
        }
    }
    // Diners Club: 36, 38, 300–305
    if digits.starts_with("36") || digits.starts_with("38") {
        return Some("Diners Club");
    }
    if digits.len() >= 3 {
        let three: u16 = digits[..3].parse().unwrap_or(0);
        if (300..=305).contains(&three) {
            return Some("Diners Club");
        }
    }
    // JCB: 35
    if digits.starts_with("35") {
        return Some("JCB");
    }
    None
}

fn validate_pan_for_brand(digits: &str, brand: &str) -> Result<(), String> {
    let len = digits.len();
    let ok = match brand {
        "American Express" => len == 15,
        "Diners Club" => (14..=19).contains(&len),
        "Visa" => (13..=19).contains(&len),
        "Mastercard" | "Discover" | "JCB" => len == 16,
        _ => (13..=19).contains(&len),
    };
    if ok {
        Ok(())
    } else {
        Err(format!("{brand} card number has an unexpected length"))
    }
}

fn validate_cvv(value: &str, brand: &str) -> Result<(), String> {
    let trimmed = value.trim();
    let expected = if brand == "American Express" { 4 } else { 3 };
    if trimmed.len() == expected && trimmed.bytes().all(|b| b.is_ascii_digit()) {
        Ok(())
    } else {
        Err(format!("CVV must be {expected} digits for {brand}"))
    }
}

/// Validate that `last4` is exactly 4 ASCII digits.
fn validate_last4(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.len() == 4 && trimmed.bytes().all(|b| b.is_ascii_digit()) {
        Ok(trimmed.to_string())
    } else {
        Err("last4 must be exactly 4 digits".to_string())
    }
}

/// Parse and validate the raw expiry form fields against `method_type`'s
/// rules: credit cards require a valid month (1-12) and a plausible
/// (not-in-the-past) year; bank accounts must not submit expiry fields at
/// all.
fn parse_expiry(
    method_type: PaymentMethodType,
    expiry_month: &str,
    expiry_year: &str,
) -> Result<(Option<u8>, Option<u16>), String> {
    let month = empty_to_none(expiry_month.to_string())
        .map(|s| {
            s.parse::<u8>()
                .map_err(|_| "expiry_month must be a number".to_string())
        })
        .transpose()?;
    let year = empty_to_none(expiry_year.to_string())
        .map(|s| {
            s.parse::<u16>()
                .map_err(|_| "expiry_year must be a number".to_string())
        })
        .transpose()?;
    validate_expiry(method_type, month, year)?;
    Ok((month, year))
}

/// # Errors
///
/// Returns a description of the violated rule when `expiry_month`/`expiry_year`
/// don't satisfy `method_type`'s requirements.
pub fn validate_expiry(
    method_type: PaymentMethodType,
    expiry_month: Option<u8>,
    expiry_year: Option<u16>,
) -> Result<(), String> {
    match method_type {
        PaymentMethodType::CreditCard => {
            let month = expiry_month.ok_or("expiry_month is required for credit cards")?;
            let year = expiry_year.ok_or("expiry_year is required for credit cards")?;
            if !(1..=12).contains(&month) {
                return Err("expiry_month must be between 1 and 12".to_string());
            }
            let current_year = u16::try_from(chrono::Utc::now().year()).unwrap_or(0);
            if year < current_year {
                return Err("expiry_year must not be in the past".to_string());
            }
            Ok(())
        }
        PaymentMethodType::BankAccount => {
            if expiry_month.is_some() || expiry_year.is_some() {
                return Err("expiry fields are not valid for bank accounts".to_string());
            }
            Ok(())
        }
    }
}

fn empty_to_none(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn required(value: String, field: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("{field} is required"))
    } else {
        Ok(trimmed.to_string())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_method_type_accepts_known_values() {
        assert_eq!(
            PaymentMethodType::parse("credit_card").unwrap(),
            PaymentMethodType::CreditCard
        );
        assert_eq!(
            PaymentMethodType::parse("Bank_Account").unwrap(),
            PaymentMethodType::BankAccount
        );
        assert!(PaymentMethodType::parse("bogus").is_err());
    }

    #[test]
    fn detect_visa_mastercard_amex() {
        assert_eq!(detect_card_brand("4111111111111111"), Some("Visa"));
        assert_eq!(detect_card_brand("5500000000000004"), Some("Mastercard"));
        assert_eq!(detect_card_brand("2223000048400011"), Some("Mastercard"));
        assert_eq!(
            detect_card_brand("378282246310005"),
            Some("American Express")
        );
    }

    #[test]
    fn luhn_accepts_stripe_test_visa() {
        assert!(luhn_ok("4242424242424242"));
        assert!(!luhn_ok("4242424242424243"));
    }

    #[test]
    fn form_into_create_credit_card_from_pan() {
        let form = PaymentMethodForm {
            billing_address_id: "addr-1".to_string(),
            card_number: "4242 4242 4242 4242".to_string(),
            cardholder_name: "Jane Doe".to_string(),
            cvv: "123".to_string(),
            expiry_month: "12".to_string(),
            expiry_year: "2099".to_string(),
            ..Default::default()
        };
        let created = form.into_create(PaymentMethodType::CreditCard).unwrap();
        assert_eq!(created.brand.as_deref(), Some("Visa"));
        assert_eq!(created.last4, "4242");
        assert_eq!(created.cardholder_name.as_deref(), Some("Jane Doe"));
        assert_eq!(created.expiry_month, Some(12));
    }

    #[test]
    fn form_into_create_credit_card_requires_cvv_and_name() {
        let form = PaymentMethodForm {
            billing_address_id: "addr-1".to_string(),
            card_number: "4242424242424242".to_string(),
            expiry_month: "12".to_string(),
            expiry_year: "2099".to_string(),
            ..Default::default()
        };
        assert!(form.into_create(PaymentMethodType::CreditCard).is_err());
    }

    #[test]
    fn form_into_create_requires_billing_address_id() {
        let form = PaymentMethodForm {
            last4: "4242".to_string(),
            ..Default::default()
        };
        assert!(form.into_create(PaymentMethodType::BankAccount).is_err());
    }

    #[test]
    fn last4_rejects_non_4_digit_values() {
        assert!(validate_last4("123").is_err());
        assert!(validate_last4("12345").is_err());
        assert!(validate_last4("abcd").is_err());
    }

    #[test]
    fn credit_card_requires_valid_month_and_year() {
        assert!(validate_expiry(PaymentMethodType::CreditCard, None, None).is_err());
        assert!(validate_expiry(PaymentMethodType::CreditCard, Some(0), Some(2099)).is_err());
        assert!(validate_expiry(PaymentMethodType::CreditCard, Some(12), Some(2099)).is_ok());
    }

    #[test]
    fn bank_account_rejects_expiry_fields() {
        assert!(validate_expiry(PaymentMethodType::BankAccount, Some(1), None).is_err());
        assert!(validate_expiry(PaymentMethodType::BankAccount, None, None).is_ok());
    }
}
