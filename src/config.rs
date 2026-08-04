//! Environment-driven configuration for the payments service.
//!
//! Required values are declared in the [`sigma_config::service!`] block and
//! checked by [`validate_with`] at startup; optional integrations return
//! `None` when they are not configured for this environment.

sigma_config::service! {
    prefix = "PAYMENTS";
    role = "payments";
    urls {
        /// Canonical public URL of this payments service, for sign-in return links.
        public_base_url = "PUBLIC_BASE_URL" => "http://127.0.0.1:8090/";
        /// Public base URL of the identity BFF.
        identity_public_base_url = "IDENTITY_PUBLIC_URL" => "http://127.0.0.1:3000/";
        /// Public base URL of the contact service for the navbar link.
        contact_public_base_url = "CONTACT_PUBLIC_URL" => "http://127.0.0.1:8083/";
        /// Public base URL of the cart service for the navbar link.
        cart_public_base_url = "CART_PUBLIC_URL" => "http://127.0.0.1:8084/";
        /// Base URL for server-to-server calls to the addresses service's internal JSON API.
        addresses_internal_base_url = "ADDRESSES_INTERNAL_URL" => "http://127.0.0.1:8089/";
        /// Public base URL of the addresses service, for the "add a billing address first" link.
        addresses_public_base_url = "ADDRESSES_PUBLIC_URL" => "http://127.0.0.1:8089/";
    }
}

/// Browser origin of the identity BFF for CSP `connect-src` (no trailing slash).
#[must_use]
pub fn identity_public_origin() -> String {
    sigma_config::origin_of(&identity_public_base_url())
}

/// Base URL for server-to-server calls to the identity BFF (session status
/// checks gating every payments page). Must be reachable from this pod,
/// unlike `identity_public_base_url`, which is the browser-facing ingress
/// host and does not resolve back to identity from inside the cluster
/// network. Falls back to the public URL for non-cluster local dev.
#[must_use]
pub fn identity_internal_base_url() -> String {
    SERVICE
        .opt_url("IDENTITY_INTERNAL_URL")
        .unwrap_or_else(identity_public_base_url)
}

/// Base URL of the cart service over the mesh, used server-side to read the
/// live item count for the navbar badge. `None` (unset) means cart integration
/// is not configured.
#[must_use]
pub fn cart_base_url() -> Option<String> {
    SERVICE.opt_url("CART_BASE_URL")
}

/// PostgreSQL connection URL (shared Sigma database).
#[must_use]
pub fn database_url() -> String {
    SERVICE.database_url()
}
