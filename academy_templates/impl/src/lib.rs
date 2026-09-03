use std::{fmt::Debug, sync::Arc};

use academy_assets::templates;
use academy_di::Build;
use academy_templates_contracts::{TEMPLATES, Template, TemplateService};
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
        ContractCancellationConfirmationTemplate, ContractWithdrawalConfirmationTemplate,
        InvoiceTemplate, PurchaseConfirmationTemplate, ResetPasswordTemplate, VerifyEmailTemplate,
        WithdrawalConsentConfirmation,
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
    fn purchase_confirmation() {
        test_template(PurchaseConfirmationTemplate {
            coins: 4207,
            vat_percent: 19.into(),
            vat_total: 7.into(),
            gross_total: 49.into(),
            withdrawal_consent: None,
        });
    }

    #[test]
    fn purchase_confirmation_with_withdrawal_consent() {
        // Arrange
        let template = PurchaseConfirmationTemplate {
            coins: 4207,
            vat_percent: 19.into(),
            vat_total: 7.into(),
            gross_total: 49.into(),
            withdrawal_consent: Some(WithdrawalConsentConfirmation {
                text: "Ich stimme ausdrücklich zu, ...".into(),
                version: "2026-09".into(),
                timestamp: "03.09.2026, 14:23 Uhr (UTC)".into(),
            }),
        };

        let sut = TemplateServiceImpl {
            state: Default::default(),
        };

        // Act
        let result = sut.render(&template).unwrap();

        // Assert
        assert!(result.contains("Ich stimme ausdrücklich zu, ..."));
        assert!(result.contains("2026-09"));
        assert!(result.contains("03.09.2026, 14:23 Uhr (UTC)"));
        assert!(result.contains("https://bootstrap.academy/docs/right-of-withdrawal"));
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

    #[test]
    fn contract_cancellation_confirmation() {
        let rendered = render_template(ContractCancellationConfirmationTemplate {
            received_at: "03.09.2026 um 14:00:00 Uhr".into(),
            name: "Max Mustermann".into(),
            email: "max.mustermann@example.de".into(),
            contract: "Premium-Mitgliedschaft".into(),
            cancellation_type: "ordentliche Kündigung".into(),
            details: Some("Zu teuer".into()),
            requested_end: Some("31.12.2026".into()),
            effective_end: Some("01.10.2026".into()),
        });
        assert!(rendered.contains("Wir bestätigen den Eingang Ihrer Kündigungserklärung."));
        assert!(rendered.contains("Ihr Vertrag endet zum 01.10.2026."));
        assert!(rendered.contains("Begründung: Zu teuer"));
        assert!(rendered.contains("Diese Bestätigung erfolgt nach § 312k Abs. 4 BGB."));
    }

    #[test]
    fn contract_cancellation_confirmation_without_contract() {
        let rendered = render_template(ContractCancellationConfirmationTemplate {
            received_at: "03.09.2026 um 14:00:00 Uhr".into(),
            name: "Max Mustermann".into(),
            email: "max.mustermann@example.de".into(),
            contract: "Sonstiger Vertrag".into(),
            cancellation_type: "außerordentliche Kündigung".into(),
            details: None,
            requested_end: None,
            effective_end: None,
        });
        assert!(rendered.contains("zum nächstmöglichen Zeitpunkt"));
        assert!(!rendered.contains("Begründung:"));
        assert!(
            rendered.contains("teilen Ihnen den Beendigungszeitpunkt gesondert in Textform mit.")
        );
    }

    #[test]
    fn contract_withdrawal_confirmation() {
        let rendered = render_template(ContractWithdrawalConfirmationTemplate {
            received_at: "03.09.2026 um 14:00:00 Uhr".into(),
            name: "Max Mustermann".into(),
            email: "max.mustermann@example.de".into(),
            contract: "MorphCoins-Kauf".into(),
            details: None,
        });
        assert!(rendered.contains("Wir bestätigen den Eingang Ihrer Widerrufserklärung."));
        assert!(rendered.contains(
            "Wir erstatten den gezahlten Betrag innerhalb von 14 Tagen über das ursprüngliche \
             Zahlungsmittel."
        ));
        assert!(rendered.contains("Diese Bestätigung erfolgt nach § 356a BGB."));
    }

    fn render_template<T: Template + 'static>(template: T) -> String {
        let sut = TemplateServiceImpl {
            state: Default::default(),
        };

        sut.render(&template).unwrap()
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
