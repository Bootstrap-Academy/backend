use std::{fmt::Debug, sync::Arc};

use academy_assets::templates;
use academy_di::Build;
use academy_templates_contracts::{Template, TemplateService, TEMPLATES};
use anyhow::Context;
use tera::Tera;
use tracing::instrument;

#[derive(Debug, Clone, Build)]
pub struct TemplateServiceImpl {
    #[di(default)]
    state: State,
}

#[derive(Debug, Clone)]
struct State(Arc<Tera>);

impl Default for State {
    fn default() -> Self {
        let mut tera = Tera::default();

        tera.add_raw_template("base", templates::BASE_HTML).unwrap();

        for &(name, template) in TEMPLATES {
            tera.add_raw_template(name, template).unwrap();
        }

        Self(tera.into())
    }
}

impl TemplateService for TemplateServiceImpl {
    #[instrument(skip(self))]
    fn render<T: Template>(&self, template: &T) -> anyhow::Result<String> {
        let context = tera::Context::from_serialize(template)
            .with_context(|| format!("Failed to build tera context for template {}", T::NAME))?;

        self.state
            .0
            .render(T::NAME, &context)
            .with_context(|| format!("Failed to render template {}", T::NAME))
    }
}

#[cfg(test)]
mod tests {
    use academy_templates_contracts::{
        InvoiceTemplate, PurchaseConfirmationTemplate, ResetPasswordTemplate,
        SubscribeNewsletterTemplate, VerifyEmailTemplate,
    };

    use super::*;

    #[test]
    fn reset_password() {
        test_template(ResetPasswordTemplate {
            code: "code".into(),
            url: "https://bootstrap.academy/".into(),
        });
    }

    #[test]
    fn verify_email() {
        test_template(VerifyEmailTemplate {
            code: "code".into(),
            url: "https://bootstrap.academy/".into(),
        });
    }

    #[test]
    fn subscribe_newsletter() {
        test_template(SubscribeNewsletterTemplate {
            code: "code".into(),
            url: "https://bootstrap.academy/".into(),
        });
    }

    #[test]
    fn purchase_confirmation() {
        test_template(PurchaseConfirmationTemplate {
            coins: 4207,
            vat_percent: 19.into(),
            vat_total: 7.into(),
            gross_total: 49.into(),
        });
    }

    #[test]
    fn invoice() {
        test_template(InvoiceTemplate {
            title: "Rechnung",
            customer_details: ["foo", "bar", "baz"].into_iter().map(Into::into).collect(),
            timestamp: Default::default(),
            invoice_number: "R1234".into(),
            items: vec![],
            vat_percent: 19.into(),
            net_total: 42.into(),
            vat_total: 7.into(),
            gross_total: 49.into(),
            _static: Default::default(),
        });
    }

    fn test_template<T: Template + 'static>(template: T) {
        // Arrange
        let sut = TemplateServiceImpl {
            state: Default::default(),
        };

        // Act
        let result = sut.render(&template);

        // Assert
        result.unwrap();
    }
}
