//! [`Charge`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::ChargeStatus;

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
    pub created_at: DateTime<Utc>,
}
