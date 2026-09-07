use std::{fmt::Debug, sync::Arc};

use academy_assets::templates;
use academy_di::Build;
use academy_templates_contracts::{LOGO_BASE64, TEMPLATES, Template, TemplateService};
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
        let mut context = tera::Context::from_serialize(template)
            .with_context(|| format!("Failed to build tera context for template {}", T::NAME))?;

        // Every template embeds the logo as base64 instead of loading it from a
        // remote host, so opening a mail never causes a request.
        context.insert("logo_base64", LOGO_BASE64.as_str());

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
        FinalStatementTemplate, InvoiceItem, InvoiceTemplate, PurchaseConfirmationTemplate,
        ResetPasswordTemplate, VerifyEmailTemplate, WithdrawalConsentConfirmation,
    };
    use chrono::{TimeZone, Utc};
    use rust_decimal_macros::dec;

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
        let rendered = render_template(PurchaseConfirmationTemplate {
            coins: 4207,
            vat_percent: 19.into(),
            vat_total: dec!(7.9832),
            gross_total: 49.into(),
            withdrawal_consent: None,
        });

        // Every number is printed the way the checkout prints it.
        assert!(rendered.contains(
            "Du hast erfolgreich 4.207 MorphCoins gekauft! Das entspricht 49,00 € inklusive 19 % \
             MwSt. von 7,98 €."
        ));

        // The attached documents are the version in force at the time of the
        // order, so the mail also has to point at the current online version.
        assert!(rendered.contains("https://bootstrap.academy/docs/terms-and-conditions"));
        assert!(rendered.contains("https://bootstrap.academy/docs/right-of-withdrawal"));
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
        let rendered = render_template(InvoiceTemplate {
            title: "Rechnung",
            customer_details: ["foo", "bar", "baz"].into_iter().map(Into::into).collect(),
            timestamp: Default::default(),
            invoice_number: "R1234".into(),
            items: vec![InvoiceItem {
                description: "MorphCoins".into(),
                net_unit: dec!(0.0084033613445378151260504202),
                count: 123456,
                net_total: dec!(1037.478991596638655462184874),
            }],
            vat_percent: 19.into(),
            net_total: dec!(1037.478991596638655462184874),
            vat_total: dec!(197.1210084033613445378151261),
            gross_total: dec!(1234.56),
        });

        // Amounts, percentages and quantities are printed the way a German
        // invoice prints them, and the net unit price keeps enough decimal
        // places to multiply out to the net total of the line.
        assert!(rendered.contains("0,0084 €"));
        assert!(rendered.contains("123.456"));
        assert!(rendered.contains("1.037,48 €"));
        assert!(rendered.contains("zzgl. 19 % MwSt."));
        assert!(rendered.contains("197,12 €"));
        assert!(rendered.contains("1.234,56 €"));
        assert!(!rendered.contains("EUR"));
    }

    #[test]
    fn final_statement() {
        let rendered = render_template(FinalStatementTemplate {
            title: "Schlussabrechnung",
            customer_details: ["Max Mustermann", "max@example.de"]
                .into_iter()
                .map(Into::into)
                .collect(),
            timestamp: Default::default(),
            statement_number: "S1337".into(),
            purchased_coins: 1500,
            balance_coins: 1200,
            unused_coins: 1200,
            coins_per_euro: 100,
            refund_amount: 12.into(),
        });

        // The name and the email address are what makes a later refund
        // possible, and the refundable amount has to be readable.
        assert!(rendered.contains("Max Mustermann"));
        assert!(rendered.contains("max@example.de"));
        assert!(rendered.contains("S1337"));
        assert!(rendered.contains("1.500"));
        assert!(rendered.contains("1.200"));
        assert!(rendered.contains("12,00 €"));
        assert!(rendered.contains("Ziffer 6.7"));
        assert!(!rendered.contains("EUR"));
    }

    /// The documents are issued by a German company and carry a German date,
    /// so the printed date is the one in `Europe/Berlin`. Half an hour before
    /// midnight UTC on New Year's Eve it is already the next year there.
    #[test]
    fn document_dates_are_printed_in_berlin_time() {
        let new_years_eve = Utc.with_ymd_and_hms(2024, 12, 31, 23, 30, 0).unwrap();

        let rendered = render_template(InvoiceTemplate {
            title: "Rechnung",
            customer_details: vec!["foo".into()],
            timestamp: new_years_eve,
            invoice_number: "R1234".into(),
            items: vec![InvoiceItem {
                description: "MorphCoins".into(),
                net_unit: dec!(0.0084),
                count: 100,
                net_total: dec!(0.84),
            }],
            vat_percent: 19.into(),
            net_total: dec!(0.84),
            vat_total: dec!(0.16),
            gross_total: dec!(1.00),
        });
        assert!(rendered.contains("01.01.2025"), "{rendered}");
        assert!(!rendered.contains("31.12.2024"), "{rendered}");

        let rendered = render_template(FinalStatementTemplate {
            title: "Schlussabrechnung",
            customer_details: vec!["Max Mustermann".into()],
            timestamp: new_years_eve,
            statement_number: "S1337".into(),
            purchased_coins: 1500,
            balance_coins: 1200,
            unused_coins: 1200,
            coins_per_euro: 100,
            refund_amount: 12.into(),
        });
        assert!(rendered.contains("01.01.2025"), "{rendered}");
        assert!(!rendered.contains("31.12.2024"), "{rendered}");
    }

    #[test]
    fn contract_cancellation_confirmation() {
        let rendered = render_template(ContractCancellationConfirmationTemplate {
            received_at: "03.09.2026 um 14:00:00 Uhr".into(),
            name: "Max Mustermann".into(),
            email: "max.mustermann@example.de".into(),
            contract: "Premium-Mitgliedschaft".into(),
            contract_designation: Some("Premium-Abo, monatlich".into()),
            cancellation_type: "ordentliche Kündigung".into(),
            extraordinary: false,
            details: Some("Zu teuer".into()),
            requested_end: Some("31.12.2026".into()),
            effective_end: Some("01.10.2026".into()),
        });
        assert!(rendered.contains("Wir bestätigen den Eingang Ihrer Kündigungserklärung."));
        assert!(rendered.contains("Ihr Vertrag endet zum 01.10.2026."));
        assert!(rendered.contains("Ihre Bezeichnung des Vertrags: Premium-Abo, monatlich"));
        assert!(rendered.contains("Begründung: Zu teuer"));
        assert!(rendered.contains("Diese Bestätigung erfolgt nach § 312k Abs. 4 BGB."));
    }

    /// An extraordinary cancellation is not confirmed with the ordinary end
    /// date; it is examined and answered separately.
    #[test]
    fn contract_cancellation_confirmation_extraordinary() {
        let rendered = render_template(ContractCancellationConfirmationTemplate {
            received_at: "03.09.2026 um 14:00:00 Uhr".into(),
            name: "Max Mustermann".into(),
            email: "max.mustermann@example.de".into(),
            contract: "Premium-Mitgliedschaft".into(),
            contract_designation: None,
            cancellation_type: "außerordentliche Kündigung".into(),
            extraordinary: true,
            details: Some("Leistung nicht verfügbar".into()),
            requested_end: None,
            effective_end: None,
        });
        assert!(rendered.contains("Sie haben außerordentlich gekündigt."));
        assert!(rendered.contains("teilen Ihnen das Ergebnis der Prüfung und den"));
        assert!(rendered.contains("Beendigungszeitpunkt gesondert in Textform mit."));
        assert!(!rendered.contains("Ihr Vertrag endet zum"));
        assert!(!rendered.contains("Ihre Bezeichnung des Vertrags"));
    }

    #[test]
    fn contract_cancellation_confirmation_without_contract() {
        let rendered = render_template(ContractCancellationConfirmationTemplate {
            received_at: "03.09.2026 um 14:00:00 Uhr".into(),
            name: "Max Mustermann".into(),
            email: "max.mustermann@example.de".into(),
            contract: "Sonstiger Vertrag".into(),
            contract_designation: None,
            cancellation_type: "ordentliche Kündigung".into(),
            extraordinary: false,
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
            contract_designation: Some("Bestellung vom 01.09.2026".into()),
            details: None,
        });
        assert!(rendered.contains("Wir bestätigen den Eingang Ihrer Widerrufserklärung."));
        assert!(rendered.contains(
            "Wir erstatten den gezahlten Betrag innerhalb von 14 Tagen über das ursprüngliche \
             Zahlungsmittel."
        ));
        assert!(rendered.contains("Diese Bestätigung erfolgt nach § 356a BGB."));
        assert!(rendered.contains("Ihre Bezeichnung des Vertrags: Bestellung vom 01.09.2026"));
    }

    /// No template may reference a remote resource. The logo used to be loaded
    /// from a static host, which disclosed the recipient's IP address to
    /// whoever operates it as soon as the mail was opened.
    #[test]
    fn templates_do_not_reference_remote_resources() {
        for &(name, template) in std::iter::once(&("base", templates::BASE_HTML)).chain(TEMPLATES) {
            for needle in ["src=\"http", "url(http", "<link"] {
                assert!(
                    !template.contains(needle),
                    "template {name} contains `{needle}`"
                );
            }
        }
    }

    fn render_template<T: Template + 'static>(template: T) -> String {
        let sut = TemplateServiceImpl {
            state: Default::default(),
        };

        let rendered = sut.render(&template).unwrap();

        assert!(
            rendered.contains("src=\"data:image/png;base64,"),
            "template {} does not embed the logo",
            T::NAME
        );

        rendered
    }

    fn test_template<T: Template + 'static>(template: T) {
        render_template(template);
    }
}
