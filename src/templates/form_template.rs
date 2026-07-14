//! [`FormTemplate`].

#[allow(unused_imports)]
use super::*;
use askama::Template;
use sigma_theme::nav::SiteHeader;

#[derive(Template)]
#[template(path = "form.html")]
pub(crate) struct FormTemplate {
    pub(crate) is_edit: bool,
    pub(crate) payment_method_id: String,
    pub(crate) method_type: String,
    pub(crate) method_type_label: String,
    pub(crate) billing_address_id: String,
    pub(crate) billing_address_options: Vec<BillingAddressOptionRow>,
    pub(crate) has_billing_addresses: bool,
    pub(crate) addresses_public_url: String,
    pub(crate) label: String,
    pub(crate) brand: String,
    pub(crate) card_number: String,
    pub(crate) cardholder_name: String,
    pub(crate) last4: String,
    pub(crate) expiry_month: String,
    pub(crate) expiry_year: String,
    pub(crate) error: Option<String>,
    pub(crate) site_header: SiteHeader,
    pub(crate) site_nav: String,
    pub(crate) copyright_years: String,
}
