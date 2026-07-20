//! [`CreateCharge`].

use serde::Deserialize;

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
