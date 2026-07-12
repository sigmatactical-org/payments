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

/// A saved payment method. This is a demo payment-method registry, **not** a
/// PCI-compliant payment processor integration: it deliberately has no room
/// for a full card number (PAN), CVV/CVC, or full bank account/routing
/// number anywhere in this struct — only `brand`, `last4`, and (credit cards
/// only) `expiry_month`/`expiry_year`. Never add a field that could hold a
/// full card or account number.
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
    pub expiry_month: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry_year: Option<u16>,
    pub is_default: bool,
    pub updated_at: String,
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
        let last4 = validate_last4(&self.last4)?;
        let (expiry_month, expiry_year) =
            parse_expiry(method_type, &self.expiry_month, &self.expiry_year)?;
        Ok(CreatePaymentMethod {
            method_type,
            billing_address_id: required(self.billing_address_id, "billing_address_id")?,
            label: empty_to_none(self.label),
            brand: empty_to_none(self.brand),
            last4,
            expiry_month,
            expiry_year,
        })
    }

    pub fn into_update(
        self,
        method_type: PaymentMethodType,
    ) -> Result<UpdatePaymentMethod, String> {
        let last4 = validate_last4(&self.last4)?;
        let (expiry_month, expiry_year) =
            parse_expiry(method_type, &self.expiry_month, &self.expiry_year)?;
        Ok(UpdatePaymentMethod {
            billing_address_id: required(self.billing_address_id, "billing_address_id")?,
            label: empty_to_none(self.label),
            brand: empty_to_none(self.brand),
            last4,
            expiry_month,
            expiry_year,
        })
    }
}

/// Validate that `last4` is exactly 4 ASCII digits. This is the only place a
/// card/account number fragment is ever accepted — never widen this to more
/// than 4 characters.
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
        assert!(validate_last4("42-4").is_err());
    }

    #[test]
    fn last4_accepts_exactly_4_digits() {
        assert_eq!(validate_last4("4242").unwrap(), "4242");
        assert_eq!(validate_last4(" 0000 ").unwrap(), "0000");
    }

    #[test]
    fn credit_card_requires_valid_month_and_year() {
        assert!(validate_expiry(PaymentMethodType::CreditCard, None, None).is_err());
        assert!(validate_expiry(PaymentMethodType::CreditCard, Some(0), Some(2099)).is_err());
        assert!(validate_expiry(PaymentMethodType::CreditCard, Some(13), Some(2099)).is_err());
        assert!(validate_expiry(PaymentMethodType::CreditCard, Some(1), Some(2000)).is_err());
        assert!(validate_expiry(PaymentMethodType::CreditCard, Some(12), Some(2099)).is_ok());
    }

    #[test]
    fn bank_account_rejects_expiry_fields() {
        assert!(validate_expiry(PaymentMethodType::BankAccount, Some(1), None).is_err());
        assert!(validate_expiry(PaymentMethodType::BankAccount, None, Some(2099)).is_err());
        assert!(validate_expiry(PaymentMethodType::BankAccount, None, None).is_ok());
    }

    #[test]
    fn form_into_create_rejects_bad_last4() {
        let form = PaymentMethodForm {
            billing_address_id: "addr-1".to_string(),
            last4: "42".to_string(),
            ..Default::default()
        };
        assert!(form.into_create(PaymentMethodType::BankAccount).is_err());
    }

    #[test]
    fn form_into_create_credit_card_requires_expiry() {
        let form = PaymentMethodForm {
            billing_address_id: "addr-1".to_string(),
            last4: "4242".to_string(),
            ..Default::default()
        };
        assert!(form.into_create(PaymentMethodType::CreditCard).is_err());
    }
}
