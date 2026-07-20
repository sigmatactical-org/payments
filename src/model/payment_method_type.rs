//! [`PaymentMethodType`].

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

    /// Parse from its stored/wire string.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_lowercase().as_str() {
            "credit_card" => Ok(PaymentMethodType::CreditCard),
            "bank_account" => Ok(PaymentMethodType::BankAccount),
            _ => Err("method_type must be credit_card or bank_account".to_string()),
        }
    }
}
