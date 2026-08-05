//! [`IndexTemplate`].

use askama::Template;
use sigma_theme::nav::SiteHeader;

use super::PaymentMethodRow;

#[derive(Template)]
#[template(path = "index.html")]
pub(crate) struct IndexTemplate {
    pub(crate) rows: Vec<PaymentMethodRow>,
    pub(crate) message: Option<String>,
    pub(crate) is_admin: bool,
    pub(crate) site_header: SiteHeader,
    pub(crate) site_nav: String,
    pub(crate) copyright_years: String,
}
