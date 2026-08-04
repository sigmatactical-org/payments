//! [`Refund`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A full reversal of a [`Charge`](super::Charge).
///
/// Refunds are append-only and limited to one per charge, which is what lets
/// the cart retry a reversal after a timeout without risking a second credit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refund {
    pub id: String,
    pub charge_id: String,
    pub amount_cents: u64,
    pub reason: String,
    pub created_at: DateTime<Utc>,
}
