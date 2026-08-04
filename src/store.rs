pub use sigma_pg::api::StoreError;

use sqlx::{PgPool, Row};

use crate::model::{
    Charge, ChargeStatus, CreatePaymentMethod, PaymentMethod, PaymentMethodType,
    UpdatePaymentMethod, validate_expiry,
};

/// Entity name used in [`StoreError::NotFound`] messages.
const ENTITY: &str = "payment method";

/// Entity name used in [`StoreError::NotFound`] messages for charges.
const CHARGE_ENTITY: &str = "charge";

/// Resolve an idempotent replay against the charge already recorded for a
/// reference: same amount replays, a different amount is a caller bug.
fn replayed_charge(existing: Charge, amount_cents: u64) -> Result<Charge, StoreError> {
    if existing.amount_cents == amount_cents {
        return Ok(existing);
    }
    Err(StoreError::InvalidInput(format!(
        "reference already charged {} cents; refusing to charge {amount_cents} cents against it",
        existing.amount_cents
    )))
}

#[derive(Debug, Clone)]
pub struct PaymentMethodStore {
    pool: PgPool,
}

impl PaymentMethodStore {
    pub async fn connect() -> Result<Self, StoreError> {
        let pool = sigma_pg::connect_as("payments").await?;
        Ok(Self { pool })
    }

    #[cfg(test)]
    pub async fn connect_empty() -> Result<Self, StoreError> {
        let store = Self::connect().await?;
        sigma_pg::assert_disposable_test_db(&store.pool).await;
        sqlx::query(
            "TRUNCATE payments.refunds, payments.charges, payments.payment_methods CASCADE",
        )
        .execute(&store.pool)
        .await?;
        Ok(store)
    }

    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// List every payment method owned by `user_id`. Every read in this
    /// service is scoped by the caller's verified session `user_id` — there
    /// is no "list all payment methods" endpoint.
    pub async fn list_for_user(&self, user_id: &str) -> Result<Vec<PaymentMethod>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, user_id, method_type, billing_address_id, label, brand, last4, \
             cardholder_name, expiry_month, expiry_year, is_default, updated_at \
             FROM payments.payment_methods WHERE user_id = $1 ORDER BY updated_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_payment_method).collect()
    }

    /// Fetch one payment method, scoped to `user_id`. Returns
    /// [`StoreError::NotFound`] both when the id doesn't exist and when it
    /// belongs to a different user — the two cases are indistinguishable to
    /// the caller so a user can't probe for the existence of another user's
    /// payment method ids.
    pub async fn get_for_user(&self, user_id: &str, id: &str) -> Result<PaymentMethod, StoreError> {
        let row = sqlx::query(
            "SELECT id, user_id, method_type, billing_address_id, label, brand, last4, \
             cardholder_name, expiry_month, expiry_year, is_default, updated_at \
             FROM payments.payment_methods WHERE id = $1 AND user_id = $2",
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(row) => row_to_payment_method(row),
            None => Err(StoreError::NotFound(ENTITY)),
        }
    }

    /// Insert a new payment method for `user_id`. When it is the user's first
    /// one it is made the default, so a user always has a default without a
    /// separate `set_default` step. `NOT EXISTS` is evaluated inside the INSERT
    /// so the check and the write are one atomic statement; the per-user partial
    /// unique index still backstops any concurrent double-insert. The persisted
    /// flag is read back via `RETURNING` so the returned struct matches the row.
    pub async fn create(
        &self,
        user_id: &str,
        input: CreatePaymentMethod,
    ) -> Result<PaymentMethod, StoreError> {
        validate_fields(
            input.method_type,
            &input.billing_address_id,
            input.brand.as_deref(),
            &input.last4,
            input.cardholder_name.as_deref(),
            input.expiry_month,
            input.expiry_year,
        )?;
        let mut payment_method = PaymentMethod::new(user_id, input);
        let row = sqlx::query(
            "INSERT INTO payments.payment_methods \
             (id, user_id, method_type, billing_address_id, label, brand, last4, \
              cardholder_name, expiry_month, expiry_year, is_default, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, \
              NOT EXISTS (SELECT 1 FROM payments.payment_methods WHERE user_id = $2), \
              $11) \
             RETURNING is_default",
        )
        .bind(&payment_method.id)
        .bind(&payment_method.user_id)
        .bind(payment_method.method_type.as_str())
        .bind(&payment_method.billing_address_id)
        .bind(&payment_method.label)
        .bind(&payment_method.brand)
        .bind(&payment_method.last4)
        .bind(&payment_method.cardholder_name)
        .bind(payment_method.expiry_month.map(i16::from))
        .bind(payment_method.expiry_year.map(|y| y as i16))
        .bind(payment_method.updated_at)
        .fetch_one(&self.pool)
        .await?;
        payment_method.is_default = row.get("is_default");
        Ok(payment_method)
    }

    pub async fn update(
        &self,
        user_id: &str,
        id: &str,
        input: UpdatePaymentMethod,
    ) -> Result<PaymentMethod, StoreError> {
        let mut payment_method = self.get_for_user(user_id, id).await?;
        validate_fields(
            payment_method.method_type,
            &input.billing_address_id,
            input.brand.as_deref(),
            &input.last4,
            input.cardholder_name.as_deref(),
            input.expiry_month,
            input.expiry_year,
        )?;
        payment_method.apply_update(input);
        sqlx::query(
            "UPDATE payments.payment_methods SET billing_address_id = $3, label = $4, \
             brand = $5, last4 = $6, cardholder_name = $7, expiry_month = $8, expiry_year = $9, \
             updated_at = $10 \
             WHERE id = $1 AND user_id = $2",
        )
        .bind(&payment_method.id)
        .bind(user_id)
        .bind(&payment_method.billing_address_id)
        .bind(&payment_method.label)
        .bind(&payment_method.brand)
        .bind(&payment_method.last4)
        .bind(&payment_method.cardholder_name)
        .bind(payment_method.expiry_month.map(i16::from))
        .bind(payment_method.expiry_year.map(|y| y as i16))
        .bind(payment_method.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(payment_method)
    }

    pub async fn delete(&self, user_id: &str, id: &str) -> Result<(), StoreError> {
        let result =
            sqlx::query("DELETE FROM payments.payment_methods WHERE id = $1 AND user_id = $2")
                .bind(id)
                .bind(user_id)
                .execute(&self.pool)
                .await?;
        if result.rows_affected() == 0 {
            return Err(StoreError::NotFound(ENTITY));
        }
        Ok(())
    }

    /// Promote `id` to the default payment method for `user_id`. The DB
    /// enforces at most one default per user via a partial unique index (no
    /// category dimension, unlike addresses — there is only one category of
    /// payment method), so any other default for the same user must be
    /// cleared in the same transaction before this row is set, or the
    /// second write would violate the index.
    pub async fn set_default(&self, user_id: &str, id: &str) -> Result<(), StoreError> {
        self.get_for_user(user_id, id).await?;
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "UPDATE payments.payment_methods SET is_default = false \
             WHERE user_id = $1 AND id != $2",
        )
        .bind(user_id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        let result = sqlx::query(
            "UPDATE payments.payment_methods SET is_default = true, updated_at = now() \
             WHERE id = $1 AND user_id = $2",
        )
        .bind(id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() == 0 {
            tx.rollback().await?;
            return Err(StoreError::NotFound(ENTITY));
        }
        tx.commit().await?;
        Ok(())
    }

    /// Charge `amount_cents` against a saved payment method owned by `user_id`.
    /// Demo processor: succeeds unless the method's `last4` is `0000`.
    ///
    /// **Idempotent on `reference`.** Checkout can be retried or
    /// double-submitted, and there is no transaction spanning the cart and this
    /// service, so a `reference` that already has a successful charge returns
    /// that charge rather than taking payment a second time. Checkout passes the
    /// order id, making the order the unit of idempotency.
    ///
    /// A replay for the same reference but a *different* amount is rejected as
    /// invalid input: the caller has changed the basket under a reference that
    /// is already paid, and guessing which amount was intended would either
    /// under-charge or double-charge.
    ///
    /// Failed charges are not deduplicated — a declined attempt must not stop
    /// the customer retrying with another method.
    pub async fn create_charge(
        &self,
        user_id: &str,
        payment_method_id: &str,
        amount_cents: u64,
        currency: &str,
        reference: Option<&str>,
    ) -> Result<crate::model::Charge, StoreError> {
        use crate::model::{Charge, ChargeStatus};

        if amount_cents == 0 {
            return Err(StoreError::InvalidInput(
                "amount_cents must be greater than zero".to_string(),
            ));
        }
        let currency = currency.trim().to_ascii_lowercase();
        if currency.is_empty() {
            return Err(StoreError::InvalidInput("currency is required".to_string()));
        }
        let reference = reference
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        if let Some(reference) = reference.as_deref()
            && let Some(existing) = self.succeeded_charge_by_reference(reference).await?
        {
            return replayed_charge(existing, amount_cents);
        }

        let method = self.get_for_user(user_id, payment_method_id).await?;
        let (status, failure_reason) = if method.last4 == "0000" {
            (
                ChargeStatus::Failed,
                Some("card declined (demo last4 0000)".to_string()),
            )
        } else {
            (ChargeStatus::Succeeded, None)
        };
        let charge = Charge {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.trim().to_string(),
            payment_method_id: payment_method_id.trim().to_string(),
            amount_cents,
            currency,
            reference,
            status,
            failure_reason,
            created_at: chrono::Utc::now(),
        };
        // `ON CONFLICT DO NOTHING` closes the race the check above cannot: two
        // concurrent submissions of the same cart can both find no existing
        // charge, and only one may insert. The loser reads the winner's row.
        let inserted = sqlx::query(
            "INSERT INTO payments.charges \
             (id, user_id, payment_method_id, amount_cents, currency, reference, status, \
              failure_reason, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
             ON CONFLICT DO NOTHING",
        )
        .bind(&charge.id)
        .bind(&charge.user_id)
        .bind(&charge.payment_method_id)
        .bind(charge.amount_cents as i64)
        .bind(&charge.currency)
        .bind(&charge.reference)
        .bind(charge.status.as_str())
        .bind(&charge.failure_reason)
        .bind(charge.created_at)
        .execute(&self.pool)
        .await?;

        if inserted.rows_affected() == 0 {
            let Some(reference) = charge.reference.as_deref() else {
                return Err(StoreError::Database(anyhow::anyhow!(
                    "charge insert conflicted without a reference to reconcile against"
                )));
            };
            let winner = self
                .succeeded_charge_by_reference(reference)
                .await?
                .ok_or_else(|| {
                    StoreError::Database(anyhow::anyhow!(
                        "charge insert for reference {reference} conflicted but no succeeded \
                         charge is present"
                    ))
                })?;
            return replayed_charge(winner, amount_cents);
        }

        Ok(charge)
    }

    /// The successful charge recorded against `reference`, if any.
    ///
    /// At most one can exist: `payments_charges_reference_succeeded` enforces
    /// it (migration 010).
    pub async fn succeeded_charge_by_reference(
        &self,
        reference: &str,
    ) -> Result<Option<Charge>, StoreError> {
        let row = sqlx::query(
            "SELECT id, user_id, payment_method_id, amount_cents, currency, reference, status, \
             failure_reason, created_at \
             FROM payments.charges WHERE reference = $1 AND status = 'succeeded'",
        )
        .bind(reference)
        .fetch_optional(&self.pool)
        .await?;
        row.map(row_to_charge).transpose()
    }

    /// Reverse `charge_id` in full, recording `reason`.
    ///
    /// **Idempotent.** One refund per charge is enforced by
    /// `payments_refunds_charge_id`, so a caller that times out and retries
    /// receives the original refund rather than issuing a second credit. This
    /// is the compensating action for a checkout that took payment but could
    /// not be completed.
    pub async fn refund_charge(
        &self,
        charge_id: &str,
        reason: &str,
    ) -> Result<crate::model::Refund, StoreError> {
        use crate::model::Refund;

        let reason = reason.trim();
        if reason.is_empty() {
            return Err(StoreError::InvalidInput(
                "refund reason is required".to_string(),
            ));
        }
        let charge = self.get_charge(charge_id).await?;
        if charge.status != crate::model::ChargeStatus::Succeeded {
            return Err(StoreError::InvalidInput(
                "only a succeeded charge can be refunded".to_string(),
            ));
        }

        let refund = Refund {
            id: uuid::Uuid::new_v4().to_string(),
            charge_id: charge.id.clone(),
            amount_cents: charge.amount_cents,
            reason: reason.to_string(),
            created_at: chrono::Utc::now(),
        };
        let inserted = sqlx::query(
            "INSERT INTO payments.refunds (id, charge_id, amount_cents, reason, created_at) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (charge_id) DO NOTHING",
        )
        .bind(&refund.id)
        .bind(&refund.charge_id)
        .bind(refund.amount_cents as i64)
        .bind(&refund.reason)
        .bind(refund.created_at)
        .execute(&self.pool)
        .await?;

        if inserted.rows_affected() == 0 {
            return self
                .refund_for_charge(&refund.charge_id)
                .await?
                .ok_or_else(|| {
                    StoreError::Database(anyhow::anyhow!(
                        "refund insert for charge {} conflicted but no refund is present",
                        refund.charge_id
                    ))
                });
        }
        Ok(refund)
    }

    /// The refund recorded against `charge_id`, if the charge was reversed.
    pub async fn refund_for_charge(
        &self,
        charge_id: &str,
    ) -> Result<Option<crate::model::Refund>, StoreError> {
        let row = sqlx::query(
            "SELECT id, charge_id, amount_cents, reason, created_at \
             FROM payments.refunds WHERE charge_id = $1",
        )
        .bind(charge_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| crate::model::Refund {
            id: row.get("id"),
            charge_id: row.get("charge_id"),
            amount_cents: row.get::<i64, _>("amount_cents").max(0) as u64,
            reason: row.get("reason"),
            created_at: row.get("created_at"),
        }))
    }

    /// Fetch one charge by id.
    ///
    /// Not user-scoped: like [`list_charges`](Self::list_charges) this backs
    /// internal-token-gated routes only.
    pub async fn get_charge(&self, id: &str) -> Result<Charge, StoreError> {
        let row = sqlx::query(
            "SELECT id, user_id, payment_method_id, amount_cents, currency, reference, status, \
             failure_reason, created_at \
             FROM payments.charges WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound(CHARGE_ENTITY))?;
        row_to_charge(row)
    }

    /// Every recorded charge, newest first.
    ///
    /// Unlike payment methods this is deliberately not user-scoped: it backs
    /// the accounting service's receipt reconcile, which needs the whole
    /// charge log to find charges that have no receipt yet. The route is
    /// internal-token-gated and never reachable by a browser session.
    pub async fn list_charges(&self) -> Result<Vec<Charge>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, user_id, payment_method_id, amount_cents, currency, reference, status, \
             failure_reason, created_at \
             FROM payments.charges ORDER BY created_at DESC, id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_charge).collect()
    }
}

fn row_to_charge(row: sqlx::postgres::PgRow) -> Result<Charge, StoreError> {
    let status_str: String = row.get("status");
    Ok(Charge {
        id: row.get("id"),
        user_id: row.get("user_id"),
        payment_method_id: row.get("payment_method_id"),
        amount_cents: row.get::<i64, _>("amount_cents").max(0) as u64,
        currency: row.get("currency"),
        reference: row.get("reference"),
        status: ChargeStatus::parse(&status_str).map_err(StoreError::InvalidInput)?,
        failure_reason: row.get("failure_reason"),
        created_at: row.get("created_at"),
    })
}

fn validate_fields(
    method_type: PaymentMethodType,
    billing_address_id: &str,
    brand: Option<&str>,
    last4: &str,
    cardholder_name: Option<&str>,
    expiry_month: Option<u8>,
    expiry_year: Option<u16>,
) -> Result<(), StoreError> {
    if billing_address_id.trim().is_empty() {
        return Err(StoreError::InvalidInput(
            "billing_address_id is required".to_string(),
        ));
    }
    if last4.len() != 4 || !last4.bytes().all(|b| b.is_ascii_digit()) {
        return Err(StoreError::InvalidInput(
            "last4 must be exactly 4 digits".to_string(),
        ));
    }
    if method_type == PaymentMethodType::CreditCard {
        if brand
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
        {
            return Err(StoreError::InvalidInput(
                "brand is required for credit cards".to_string(),
            ));
        }
        if cardholder_name
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
        {
            return Err(StoreError::InvalidInput(
                "cardholder_name is required for credit cards".to_string(),
            ));
        }
    }
    validate_expiry(method_type, expiry_month, expiry_year).map_err(StoreError::InvalidInput)?;
    Ok(())
}

fn row_to_payment_method(row: sqlx::postgres::PgRow) -> Result<PaymentMethod, StoreError> {
    let method_type_str: String = row.get("method_type");
    let expiry_month: Option<i16> = row.get("expiry_month");
    let expiry_year: Option<i16> = row.get("expiry_year");
    Ok(PaymentMethod {
        id: row.get("id"),
        user_id: row.get("user_id"),
        method_type: PaymentMethodType::parse(&method_type_str)
            .map_err(StoreError::InvalidInput)?,
        billing_address_id: row.get("billing_address_id"),
        label: row.get("label"),
        brand: row.get("brand"),
        last4: row.get("last4"),
        cardholder_name: row.get("cardholder_name"),
        expiry_month: expiry_month.map(|v| v as u8),
        expiry_year: expiry_year.map(|v| v as u16),
        is_default: row.get("is_default"),
        updated_at: row.get("updated_at"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_store() -> PaymentMethodStore {
        PaymentMethodStore::connect_empty()
            .await
            .expect("PostgreSQL required for tests")
    }

    fn credit_card_input(last4: &str) -> CreatePaymentMethod {
        CreatePaymentMethod {
            method_type: PaymentMethodType::CreditCard,
            billing_address_id: "addr-1".to_string(),
            label: Some("Personal Visa".to_string()),
            brand: Some("Visa".to_string()),
            last4: last4.to_string(),
            cardholder_name: Some("Jane Doe".to_string()),
            expiry_month: Some(12),
            expiry_year: Some(2099),
        }
    }

    fn bank_account_input(last4: &str) -> CreatePaymentMethod {
        CreatePaymentMethod {
            method_type: PaymentMethodType::BankAccount,
            billing_address_id: "addr-1".to_string(),
            label: Some("Checking".to_string()),
            brand: Some("First Sigma Bank".to_string()),
            last4: last4.to_string(),
            cardholder_name: None,
            expiry_month: None,
            expiry_year: None,
        }
    }

    #[tokio::test]
    async fn list_charges_returns_every_charge_regardless_of_outcome() {
        let store = test_store().await;
        let good = store
            .create("user-1", credit_card_input("4242"))
            .await
            .unwrap();
        // last4 0000 is the demo processor's decline trigger.
        let declined = store
            .create("user-2", credit_card_input("0000"))
            .await
            .unwrap();
        let charge = store
            .create_charge("user-1", &good.id, 5000, "usd", Some("cart-1"))
            .await
            .unwrap();
        store
            .create_charge("user-2", &declined.id, 900, "usd", Some("cart-2"))
            .await
            .unwrap();

        let listed = store.list_charges().await.unwrap();
        assert_eq!(listed.len(), 2);
        let recorded = listed.iter().find(|c| c.id == charge.id).unwrap();
        assert_eq!(recorded.status, ChargeStatus::Succeeded);
        assert_eq!(recorded.amount_cents, 5000);
        assert_eq!(recorded.reference.as_deref(), Some("cart-1"));
        // Reconcile filters on status, so failures must still be listed.
        assert!(listed.iter().any(|c| c.status == ChargeStatus::Failed));
    }

    #[tokio::test]
    async fn create_charge_replaying_a_reference_returns_the_original_charge() {
        let store = test_store().await;
        let method = store
            .create("user-1", credit_card_input("4242"))
            .await
            .unwrap();

        let first = store
            .create_charge("user-1", &method.id, 5000, "usd", Some("cart-1"))
            .await
            .unwrap();
        // A double-submitted checkout: same cart, same amount.
        let replay = store
            .create_charge("user-1", &method.id, 5000, "usd", Some("cart-1"))
            .await
            .unwrap();

        assert_eq!(
            replay.id, first.id,
            "replay must not create a second charge"
        );
        assert_eq!(store.list_charges().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn create_charge_rejects_a_different_amount_for_a_paid_reference() {
        let store = test_store().await;
        let method = store
            .create("user-1", credit_card_input("4242"))
            .await
            .unwrap();
        store
            .create_charge("user-1", &method.id, 5000, "usd", Some("cart-1"))
            .await
            .unwrap();

        let err = store
            .create_charge("user-1", &method.id, 7500, "usd", Some("cart-1"))
            .await
            .expect_err("a paid reference must not be charged a different amount");
        assert!(matches!(err, StoreError::InvalidInput(_)), "got {err:?}");
        assert_eq!(store.list_charges().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn create_charge_does_not_deduplicate_declines() {
        let store = test_store().await;
        // last4 0000 is the demo processor's decline trigger.
        let declining = store
            .create("user-1", credit_card_input("0000"))
            .await
            .unwrap();
        let good = store
            .create("user-1", credit_card_input("4242"))
            .await
            .unwrap();

        let first = store
            .create_charge("user-1", &declining.id, 5000, "usd", Some("cart-1"))
            .await
            .unwrap();
        assert_eq!(first.status, ChargeStatus::Failed);

        // Retrying the same cart with a working method must go through: a
        // decline is not a completed payment.
        let retry = store
            .create_charge("user-1", &good.id, 5000, "usd", Some("cart-1"))
            .await
            .unwrap();
        assert_eq!(retry.status, ChargeStatus::Succeeded);
        assert_ne!(retry.id, first.id);
    }

    #[tokio::test]
    async fn succeeded_charge_by_reference_finds_only_successful_charges() {
        let store = test_store().await;
        let declining = store
            .create("user-1", credit_card_input("0000"))
            .await
            .unwrap();
        store
            .create_charge("user-1", &declining.id, 5000, "usd", Some("cart-1"))
            .await
            .unwrap();

        assert!(
            store
                .succeeded_charge_by_reference("cart-1")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .succeeded_charge_by_reference("cart-missing")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn refund_charge_is_idempotent_and_reverses_the_full_amount() {
        let store = test_store().await;
        let method = store
            .create("user-1", credit_card_input("4242"))
            .await
            .unwrap();
        let charge = store
            .create_charge("user-1", &method.id, 5000, "usd", Some("cart-1"))
            .await
            .unwrap();

        let refund = store
            .refund_charge(&charge.id, "order creation failed")
            .await
            .unwrap();
        assert_eq!(refund.charge_id, charge.id);
        assert_eq!(refund.amount_cents, 5000);

        // The cart retries after a timeout; it must not issue a second credit.
        let replay = store
            .refund_charge(&charge.id, "order creation failed")
            .await
            .unwrap();
        assert_eq!(replay.id, refund.id);

        let found = store.refund_for_charge(&charge.id).await.unwrap().unwrap();
        assert_eq!(found.id, refund.id);
    }

    #[tokio::test]
    async fn refund_charge_rejects_declines_unknown_charges_and_blank_reasons() {
        let store = test_store().await;
        let declining = store
            .create("user-1", credit_card_input("0000"))
            .await
            .unwrap();
        let declined = store
            .create_charge("user-1", &declining.id, 5000, "usd", Some("cart-1"))
            .await
            .unwrap();

        let err = store
            .refund_charge(&declined.id, "nothing to reverse")
            .await
            .expect_err("a declined charge took no money");
        assert!(matches!(err, StoreError::InvalidInput(_)), "got {err:?}");

        let err = store
            .refund_charge("charge-missing", "reason")
            .await
            .expect_err("unknown charge");
        assert!(matches!(err, StoreError::NotFound(_)), "got {err:?}");

        let err = store
            .refund_charge(&declined.id, "   ")
            .await
            .expect_err("a reversal must record why");
        assert!(matches!(err, StoreError::InvalidInput(_)), "got {err:?}");

        assert!(
            store
                .refund_for_charge(&declined.id)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn create_list_update_delete_round_trip() {
        let store = test_store().await;
        let payment_method = store
            .create("user-1", credit_card_input("4242"))
            .await
            .unwrap();
        assert_eq!(payment_method.user_id, "user-1");
        // The user's first payment method is promoted to default automatically.
        assert!(payment_method.is_default);

        let listed = store.list_for_user("user-1").await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, payment_method.id);

        let updated = store
            .update(
                "user-1",
                &payment_method.id,
                UpdatePaymentMethod {
                    billing_address_id: "addr-2".to_string(),
                    label: Some("Work Visa".to_string()),
                    brand: Some("Visa".to_string()),
                    last4: "1111".to_string(),
                    cardholder_name: Some("Jane Doe".to_string()),
                    expiry_month: Some(1),
                    expiry_year: Some(2099),
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.billing_address_id, "addr-2");
        assert_eq!(updated.last4, "1111");
        assert_eq!(updated.label.as_deref(), Some("Work Visa"));

        store.delete("user-1", &payment_method.id).await.unwrap();
        let err = store
            .get_for_user("user-1", &payment_method.id)
            .await
            .unwrap_err();
        assert!(matches!(err, StoreError::NotFound(_)));
    }

    #[tokio::test]
    async fn bank_account_round_trip_has_no_expiry() {
        let store = test_store().await;
        let payment_method = store
            .create("user-1", bank_account_input("5678"))
            .await
            .unwrap();
        assert!(payment_method.expiry_month.is_none());
        assert!(payment_method.expiry_year.is_none());

        let fetched = store
            .get_for_user("user-1", &payment_method.id)
            .await
            .unwrap();
        assert!(fetched.expiry_month.is_none());
        assert!(fetched.expiry_year.is_none());
    }

    #[tokio::test]
    async fn get_for_user_hides_other_users_payment_methods() {
        let store = test_store().await;
        let payment_method = store
            .create("user-1", credit_card_input("4242"))
            .await
            .unwrap();
        let err = store
            .get_for_user("user-2", &payment_method.id)
            .await
            .unwrap_err();
        assert!(matches!(err, StoreError::NotFound(_)));
    }

    #[tokio::test]
    async fn delete_missing_payment_method_returns_not_found() {
        let store = test_store().await;
        let err = store.delete("user-1", "does-not-exist").await.unwrap_err();
        assert!(matches!(err, StoreError::NotFound(_)));
    }

    #[tokio::test]
    async fn create_rejects_bad_last4() {
        let store = test_store().await;
        let err = store
            .create("user-1", credit_card_input("42"))
            .await
            .unwrap_err();
        assert!(matches!(err, StoreError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn create_rejects_credit_card_missing_brand() {
        let store = test_store().await;
        let mut input = credit_card_input("4242");
        input.brand = None;
        let err = store.create("user-1", input).await.unwrap_err();
        assert!(matches!(err, StoreError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn create_rejects_credit_card_missing_expiry() {
        let store = test_store().await;
        let mut input = credit_card_input("4242");
        input.expiry_month = None;
        let err = store.create("user-1", input).await.unwrap_err();
        assert!(matches!(err, StoreError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn create_rejects_bank_account_with_expiry() {
        let store = test_store().await;
        let mut input = bank_account_input("4242");
        input.expiry_month = Some(1);
        let err = store.create("user-1", input).await.unwrap_err();
        assert!(matches!(err, StoreError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn first_payment_method_becomes_default() {
        let store = test_store().await;
        // The user's first payment method is the default; later ones are not.
        let first = store
            .create("user-1", credit_card_input("4242"))
            .await
            .unwrap();
        assert!(first.is_default);
        let second = store
            .create("user-1", bank_account_input("5678"))
            .await
            .unwrap();
        assert!(!second.is_default);

        // A different user's first payment method is likewise their own default.
        let other_user = store
            .create("user-2", credit_card_input("4242"))
            .await
            .unwrap();
        assert!(other_user.is_default);
    }

    #[tokio::test]
    async fn set_default_clears_previous_default() {
        let store = test_store().await;
        let a = store
            .create("user-1", credit_card_input("4242"))
            .await
            .unwrap();
        let b = store
            .create("user-1", bank_account_input("5678"))
            .await
            .unwrap();

        store.set_default("user-1", &a.id).await.unwrap();
        let listed = store.list_for_user("user-1").await.unwrap();
        assert!(listed.iter().find(|x| x.id == a.id).unwrap().is_default);
        assert!(!listed.iter().find(|x| x.id == b.id).unwrap().is_default);

        store.set_default("user-1", &b.id).await.unwrap();
        let listed = store.list_for_user("user-1").await.unwrap();
        assert!(!listed.iter().find(|x| x.id == a.id).unwrap().is_default);
        assert!(listed.iter().find(|x| x.id == b.id).unwrap().is_default);
    }

    #[tokio::test]
    async fn set_default_does_not_affect_other_users() {
        let store = test_store().await;
        let user1_method = store
            .create("user-1", credit_card_input("4242"))
            .await
            .unwrap();
        let user2_method = store
            .create("user-2", bank_account_input("5678"))
            .await
            .unwrap();

        store.set_default("user-2", &user2_method.id).await.unwrap();
        store.set_default("user-1", &user1_method.id).await.unwrap();

        let user1_listed = store.list_for_user("user-1").await.unwrap();
        let user2_listed = store.list_for_user("user-2").await.unwrap();
        assert!(
            user1_listed
                .iter()
                .find(|x| x.id == user1_method.id)
                .unwrap()
                .is_default
        );
        assert!(
            user2_listed
                .iter()
                .find(|x| x.id == user2_method.id)
                .unwrap()
                .is_default
        );
    }

    #[tokio::test]
    async fn set_default_missing_payment_method_returns_not_found() {
        let store = test_store().await;
        let err = store
            .set_default("user-1", "does-not-exist")
            .await
            .unwrap_err();
        assert!(matches!(err, StoreError::NotFound(_)));
    }
}
