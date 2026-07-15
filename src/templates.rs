mod billing_address_option_row;
mod form_template;
mod index_template;
mod payment_method_form_values;
mod payment_method_row;
pub use billing_address_option_row::BillingAddressOptionRow;
pub(crate) use form_template::FormTemplate;
pub(crate) use index_template::IndexTemplate;
pub use payment_method_form_values::PaymentMethodFormValues;
pub use payment_method_row::PaymentMethodRow;

use askama::Template;

use crate::addresses_client::AddressSummary;
use crate::config;
use crate::model::{PaymentMethod, PaymentMethodType};
use sigma_theme::copyright_years;
use sigma_theme::nav::{SiteHeader, site_menu};
use sigma_theme::site_nav::{AppSiteNav, render_app_site_nav};

fn page_header() -> SiteHeader {
    SiteHeader::new("Payments").with_menu(site_menu(None))
}

fn site_nav(return_path: &str) -> Result<String, askama::Error> {
    render_app_site_nav(&AppSiteNav {
        identity_base: &config::identity_public_base_url(),
        app_base: &config::public_base_url(),
        contact_base: &config::contact_public_base_url(),
        cart_url: &config::cart_public_base_url(),
        cart_count: 0,
        return_path,
        show_cart: true,
        show_contact_us: false,
        leading_html: "",
    })
}

/// Mask all but the last 4 digits for display, e.g. `"•••• 4242"`. This is
/// purely a display helper — the underlying `last4` field never holds more
/// than 4 digits in the first place.
#[must_use]
pub fn mask_last4(last4: &str) -> String {
    format!("•••• {last4}")
}

fn method_type_label(method_type: PaymentMethodType) -> &'static str {
    match method_type {
        PaymentMethodType::CreditCard => "Credit card",
        PaymentMethodType::BankAccount => "Bank account",
    }
}

fn payment_method_row(
    payment_method: &PaymentMethod,
    billing_addresses: &std::collections::HashMap<String, AddressSummary>,
) -> PaymentMethodRow {
    let billing_address_summary = billing_addresses
        .get(&payment_method.billing_address_id)
        .map(AddressSummary::short_summary)
        .unwrap_or_else(|| "Unknown address".to_string());
    let expiry = match (payment_method.expiry_month, payment_method.expiry_year) {
        (Some(month), Some(year)) => format!("{month:02}/{year}"),
        _ => String::new(),
    };
    PaymentMethodRow {
        method_type_label: method_type_label(payment_method.method_type).to_string(),
        label: payment_method.label.clone().unwrap_or_default(),
        brand: payment_method.brand.clone().unwrap_or_default(),
        cardholder_name: payment_method.cardholder_name.clone().unwrap_or_default(),
        last4_masked: mask_last4(&payment_method.last4),
        expiry,
        billing_address_summary,
        is_default: payment_method.is_default,
        edit_url: format!("/{}/edit", payment_method.id),
        delete_url: format!("/{}/delete", payment_method.id),
        default_url: format!("/{}/default", payment_method.id),
    }
}

/// # Errors
///
/// Returns [`askama::Error`] when template rendering fails.
pub fn render_index_html(
    payment_methods: Vec<PaymentMethod>,
    billing_addresses: &std::collections::HashMap<String, AddressSummary>,
    message: Option<String>,
) -> Result<String, askama::Error> {
    let rows = payment_methods
        .iter()
        .map(|pm| payment_method_row(pm, billing_addresses))
        .collect();
    IndexTemplate {
        rows,
        message,
        site_header: page_header(),
        site_nav: site_nav("/")?,
        copyright_years: copyright_years(),
    }
    .render()
}

fn billing_address_options(
    billing_addresses: &[AddressSummary],
    selected_id: &str,
) -> Vec<BillingAddressOptionRow> {
    billing_addresses
        .iter()
        .map(|a| BillingAddressOptionRow {
            id: a.id.clone(),
            summary: a.short_summary(),
            selected: a.id == selected_id,
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn render_form(
    payment_method: Option<&PaymentMethod>,
    method_type: PaymentMethodType,
    billing_addresses: &[AddressSummary],
    error: Option<String>,
    values: PaymentMethodFormValues,
) -> Result<String, askama::Error> {
    let is_edit = payment_method.is_some();
    let payment_method_id = payment_method.map(|pm| pm.id.clone()).unwrap_or_default();
    let return_path = match payment_method {
        Some(pm) => format!("/{}/edit", pm.id),
        None => "/new".to_string(),
    };
    FormTemplate {
        is_edit,
        payment_method_id,
        method_type: method_type.as_str().to_string(),
        method_type_label: method_type_label(method_type).to_string(),
        billing_address_id: values.billing_address_id.clone(),
        billing_address_options: billing_address_options(
            billing_addresses,
            &values.billing_address_id,
        ),
        has_billing_addresses: !billing_addresses.is_empty(),
        addresses_public_url: config::addresses_public_base_url(),
        label: values.label,
        brand: values.brand,
        card_number: values.card_number,
        cardholder_name: values.cardholder_name,
        last4: values.last4,
        expiry_month: values.expiry_month,
        expiry_year: values.expiry_year,
        error,
        site_header: page_header(),
        site_nav: site_nav(&return_path)?,
        copyright_years: copyright_years(),
    }
    .render()
}

/// # Errors
///
/// Returns [`askama::Error`] when template rendering fails.
pub fn render_form_html(
    payment_method: Option<PaymentMethod>,
    method_type: PaymentMethodType,
    billing_addresses: &[AddressSummary],
    error: Option<String>,
) -> Result<String, askama::Error> {
    let values = payment_method
        .as_ref()
        .map(PaymentMethodFormValues::from_payment_method)
        .unwrap_or_default();
    render_form(
        payment_method.as_ref(),
        method_type,
        billing_addresses,
        error,
        values,
    )
}

/// # Errors
///
/// Returns [`askama::Error`] when template rendering fails.
pub fn render_form_html_with_values(
    payment_method: Option<PaymentMethod>,
    method_type: PaymentMethodType,
    billing_addresses: &[AddressSummary],
    error: Option<String>,
    values: PaymentMethodFormValues,
) -> Result<String, askama::Error> {
    render_form(
        payment_method.as_ref(),
        method_type,
        billing_addresses,
        error,
        values,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_last4_hides_all_but_last_4() {
        assert_eq!(mask_last4("4242"), "•••• 4242");
    }
}
