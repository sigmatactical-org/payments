fn normalize_base_url(url: &str) -> String {
    let mut url = url.trim().to_string();
    if !url.ends_with('/') {
        url.push('/');
    }
    url
}

/// PostgreSQL connection URL (shared Sigma database).
#[must_use]
pub fn database_url() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| sigma_pg::service_database_url("payments"))
}

/// Canonical public URL of this payments service, for sign-in return links
/// (e.g. `http://127.0.0.1:8090/`).
#[must_use]
pub fn public_base_url() -> String {
    std::env::var("PAYMENTS_PUBLIC_BASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| normalize_base_url(&s))
        .unwrap_or_else(|| "http://127.0.0.1:8090/".to_string())
}

/// Public base URL of the identity BFF (e.g. `http://127.0.0.1:3000/`).
#[must_use]
pub fn identity_public_base_url() -> String {
    std::env::var("PAYMENTS_IDENTITY_PUBLIC_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| normalize_base_url(&s))
        .unwrap_or_else(|| "http://127.0.0.1:3000/".to_string())
}

/// Browser origin of the identity BFF for CSP `connect-src` (no trailing slash).
#[must_use]
pub fn identity_public_origin() -> String {
    identity_public_base_url().trim_end_matches('/').to_string()
}

/// Base URL for server-to-server calls to the identity BFF (session status
/// checks gating every payments page). Must be reachable from this pod,
/// unlike `identity_public_base_url`, which is the browser-facing ingress
/// host and does not resolve back to identity from inside the cluster
/// network. Falls back to the public URL for non-cluster local dev.
#[must_use]
pub fn identity_internal_base_url() -> String {
    std::env::var("PAYMENTS_IDENTITY_INTERNAL_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| normalize_base_url(&s))
        .unwrap_or_else(identity_public_base_url)
}

/// Public base URL of the contact service for the navbar link.
#[must_use]
pub fn contact_public_base_url() -> String {
    std::env::var("PAYMENTS_CONTACT_PUBLIC_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| normalize_base_url(&s))
        .unwrap_or_else(|| "http://127.0.0.1:8083/".to_string())
}

/// Public base URL of the cart service for the navbar link.
#[must_use]
pub fn cart_public_base_url() -> String {
    std::env::var("PAYMENTS_CART_PUBLIC_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| normalize_base_url(&s))
        .unwrap_or_else(|| "http://127.0.0.1:8084/".to_string())
}

/// Base URL of the cart service over the mesh, used server-side to read the
/// live item count for the navbar badge (e.g. `http://127.0.0.1:8084/`).
#[must_use]
pub fn cart_base_url() -> Option<String> {
    std::env::var("PAYMENTS_CART_BASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| normalize_base_url(&s))
}

/// Whether cart integration is configured.
#[must_use]
pub fn cart_configured() -> bool {
    cart_base_url().is_some()
}

/// Base URL for server-to-server calls to the addresses service's internal
/// JSON API (validating a submitted `billing_address_id` and listing a
/// user's billing addresses for the create/edit form dropdown). Must be
/// reachable from this pod, same cluster-internal-vs-public distinction as
/// `identity_internal_base_url`. Falls back to the addresses service's own
/// default local port, matching `addresses_public_base_url`'s default.
#[must_use]
pub fn addresses_internal_base_url() -> String {
    std::env::var("PAYMENTS_ADDRESSES_INTERNAL_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| normalize_base_url(&s))
        .unwrap_or_else(|| "http://127.0.0.1:8089/".to_string())
}

/// Public base URL of the addresses service, for the "add a billing address
/// first" link shown when a user has none yet.
#[must_use]
pub fn addresses_public_base_url() -> String {
    std::env::var("PAYMENTS_ADDRESSES_PUBLIC_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| normalize_base_url(&s))
        .unwrap_or_else(|| "http://127.0.0.1:8089/".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_base_url_adds_trailing_slash() {
        assert_eq!(
            normalize_base_url("http://example.com"),
            "http://example.com/"
        );
    }

    #[test]
    fn normalize_base_url_keeps_existing_trailing_slash() {
        assert_eq!(
            normalize_base_url("http://example.com/"),
            "http://example.com/"
        );
    }

    #[test]
    fn normalize_base_url_trims_whitespace() {
        assert_eq!(
            normalize_base_url("  http://example.com  "),
            "http://example.com/"
        );
    }
}
