//! [`RefundCharge`].

use serde::Deserialize;

/// Input for `POST /api/charges/{id}/refund`.
#[derive(Debug, Clone, Deserialize)]
pub struct RefundCharge {
    /// Why the charge is being reversed, recorded with the refund so the
    /// ledger explains itself without cross-referencing service logs.
    pub reason: String,
}
