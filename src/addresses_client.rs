//! Client for the addresses service's internal-token-gated JSON API
//! (`GET /api/users/{user_id}/addresses` and
//! `GET /api/users/{user_id}/addresses/{id}`), used to populate the
//! billing-address dropdown on the create/edit form and to validate that a
//! submitted `billing_address_id` actually belongs to the caller and is a
//! billing-category address. Addresses and payments are independently owned
//! services communicating over HTTP+JSON only — this module defines its own
//! minimal `AddressSummary` rather than depending on the addresses crate.

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone, Deserialize)]
pub struct AddressSummary {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    pub line1: String,
    pub city: String,
    #[serde(default)]
    pub region: Option<String>,
    pub postal_code: String,
    pub country: String,
    pub category: String,
}

impl AddressSummary {
    /// Short one-line summary for the billing-address dropdown, e.g.
    /// "123 Main St, Springfield".
    #[must_use]
    pub fn short_summary(&self) -> String {
        format!("{}, {}", self.line1, self.city)
    }

    #[must_use]
    pub fn is_billing(&self) -> bool {
        self.category == "billing"
    }
}

#[derive(Debug, Error)]
pub enum AddressesClientError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("addresses request failed: {0}")]
    Request(String),
}

fn build_addresses_url(base: &str, path: &str) -> String {
    format!("{base}{}", path.trim_start_matches('/'))
}

fn addresses_url(path: &str) -> String {
    build_addresses_url(&crate::config::addresses_internal_base_url(), path)
}

/// List `user_id`'s billing addresses, for the create/edit form dropdown.
pub async fn list_billing_addresses(
    user_id: &str,
) -> Result<Vec<AddressSummary>, AddressesClientError> {
    let url = addresses_url(&format!("api/users/{user_id}/addresses?category=billing"));
    let response =
        sigma_pg::clients::http::with_internal_auth(sigma_pg::clients::http::client().get(url))
            .send()
            .await?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AddressesClientError::Request(format!("{status}: {body}")));
    }
    let addresses: Vec<AddressSummary> = response.json().await?;
    Ok(addresses)
}

/// Fetch one address scoped to `user_id`, returning `None` if it doesn't
/// exist, doesn't belong to `user_id`, or isn't a billing-category address.
pub async fn get_billing_address(
    user_id: &str,
    id: &str,
) -> Result<Option<AddressSummary>, AddressesClientError> {
    let url = addresses_url(&format!("api/users/{user_id}/addresses/{id}"));
    let response =
        sigma_pg::clients::http::with_internal_auth(sigma_pg::clients::http::client().get(url))
            .send()
            .await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AddressesClientError::Request(format!("{status}: {body}")));
    }
    let address: AddressSummary = response.json().await?;
    if !address.is_billing() {
        return Ok(None);
    }
    Ok(Some(address))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_addresses_url_joins_base_and_path() {
        assert_eq!(
            build_addresses_url(
                "http://addresses.internal:8089/",
                "api/users/user-1/addresses"
            ),
            "http://addresses.internal:8089/api/users/user-1/addresses"
        );
    }

    #[test]
    fn build_addresses_url_strips_leading_slash_from_path() {
        assert_eq!(
            build_addresses_url(
                "http://127.0.0.1:8089/",
                "/api/users/user-1/addresses/addr-1"
            ),
            "http://127.0.0.1:8089/api/users/user-1/addresses/addr-1"
        );
    }

    #[test]
    fn short_summary_combines_line1_and_city() {
        let address = AddressSummary {
            id: "addr-1".to_string(),
            label: None,
            line1: "123 Main St".to_string(),
            city: "Springfield".to_string(),
            region: None,
            postal_code: "62704".to_string(),
            country: "US".to_string(),
            category: "billing".to_string(),
        };
        assert_eq!(address.short_summary(), "123 Main St, Springfield");
        assert!(address.is_billing());
    }

    #[test]
    fn is_billing_rejects_shipping_category() {
        let address = AddressSummary {
            id: "addr-1".to_string(),
            label: None,
            line1: "123 Main St".to_string(),
            city: "Springfield".to_string(),
            region: None,
            postal_code: "62704".to_string(),
            country: "US".to_string(),
            category: "shipping".to_string(),
        };
        assert!(!address.is_billing());
    }

    #[test]
    fn address_summary_deserializes_from_addresses_api_json() {
        let json = r#"{
            "id": "addr-1",
            "user_id": "user-1",
            "category": "billing",
            "line1": "123 Main St",
            "city": "Springfield",
            "postal_code": "62704",
            "country": "US",
            "is_default": false,
            "updated_at": "2026-01-01T00:00:00Z"
        }"#;
        let address: AddressSummary = serde_json::from_str(json).unwrap();
        assert_eq!(address.id, "addr-1");
        assert_eq!(address.line1, "123 Main St");
        assert!(address.is_billing());
    }
}
