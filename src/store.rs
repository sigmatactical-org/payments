pub use sigma_pg::api::StoreError;

use sqlx::{PgPool, Row};

use crate::model::{
    Charge, ChargeStatus, CreatePaymentMethod, PaymentMethod, PaymentMethodType,
    UpdatePaymentMethod, validate_expiry,
};

/// Entity name used in [`StoreError::NotFound`] messages.
const ENTITY: &str = "payment method";

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
        sqlx::query("TRUNCATE payments.charges, payments.payment_methods CASCADE")
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
        let payment_method = PaymentMethod::new(user_id, input);
        sqlx::query(
            "INSERT INTO payments.payment_methods \
             (id, user_id, method_type, billing_address_id, label, brand, last4, \
              cardholder_name, expiry_month, expiry_year, is_default, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
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
        .bind(payment_method.is_default)
        .bind(payment_method.updated_at)
        .execute(&self.pool)
        .await?;
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
            reference: reference
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            status,
            failure_reason,
            created_at: chrono::Utc::now(),
        };
        sqlx::query(
            "INSERT INTO payments.charges \
             (id, user_id, payment_method_id, amount_cents, currency, reference, status, \
              failure_reason, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
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
        Ok(charge)
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
    async fn create_list_update_delete_round_trip() {
        let store = test_store().await;
        let payment_method = store
            .create("user-1", credit_card_input("4242"))
            .await
            .unwrap();
        assert_eq!(payment_method.user_id, "user-1");
        assert!(!payment_method.is_default);

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
