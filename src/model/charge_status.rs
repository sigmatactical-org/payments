//! [`ChargeStatus`].

use serde::{Deserialize, Serialize};

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

    /// Parse from the stored/wire value.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_lowercase().as_str() {
            "succeeded" => Ok(ChargeStatus::Succeeded),
            "failed" => Ok(ChargeStatus::Failed),
            _ => Err("status must be succeeded or failed".to_string()),
        }
    }
}
