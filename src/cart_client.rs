//! Minimal client for the cart service, mirroring the store's: payments only
//! reads the live item count for its navbar badge, server-side, using the
//! shared guest-cart cookie (`sigma_cart`, shared across subdomains via the
//! cart service's cookie `Domain`).

mod cart_detail;
mod cart_error;
mod cart_line;
mod cart_line_detail;
pub use cart_detail::CartDetail;
pub use cart_error::CartError;
pub(crate) use cart_line::CartLine;
pub(crate) use cart_line_detail::CartLineDetail;

use crate::config;

const CART_COOKIE: &str = "sigma_cart";

fn base_url() -> Result<String, CartError> {
    config::cart_base_url().ok_or(CartError::NotConfigured)
}

/// Fetch a cart by id. Returns `Ok(None)` when the cart no longer exists.
pub async fn get_cart(cart_id: &str) -> Result<Option<CartDetail>, CartError> {
    let url = format!("{}carts/{cart_id}", base_url()?);
    let response = sigma_pg::clients::http::client().get(url).send().await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(CartError::Request(format!("{status}: {body}")));
    }
    Ok(Some(response.json().await?))
}

/// Extract the guest cart id from the request `Cookie` header, if present.
fn cart_id_from_cookie(cookie_header: Option<&str>) -> Option<String> {
    cookie_header?.split(';').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name.trim() == CART_COOKIE)
            .then(|| value.trim().to_string())
            .filter(|v| !v.is_empty())
    })
}

/// Total item count for the nav cart badge (0 when there is no live cart).
pub async fn nav_cart_count(cookie_header: Option<&str>) -> u32 {
    if !config::cart_configured() {
        return 0;
    }
    let Some(cart_id) = cart_id_from_cookie(cookie_header) else {
        return 0;
    };
    match get_cart(&cart_id).await {
        Ok(Some(detail)) => detail.item_count(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cart_id_from_cookie_finds_sigma_cart() {
        assert_eq!(
            cart_id_from_cookie(Some("a=b; sigma_cart=cart-1; c=d")),
            Some("cart-1".to_string())
        );
        assert_eq!(cart_id_from_cookie(Some("a=b")), None);
        assert_eq!(cart_id_from_cookie(None), None);
    }
}
