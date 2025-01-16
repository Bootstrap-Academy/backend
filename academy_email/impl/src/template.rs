use academy_assets::email::{ALLGEMEINE_GESCHAEFTSBEDINGUNGEN_PDF, WIDERRUFSBELEHRUNG_PDF};
use academy_di::Build;
use academy_email_contracts::{
    template::TemplateEmailService, AttachmentContentType, ContentType, Email, EmailAttachment,
    EmailService,
};
use academy_models::email_address::EmailAddressWithName;
use academy_templates_contracts::{
    PurchaseConfirmationTemplate, ResetPasswordTemplate, SubscribeNewsletterTemplate, Template,
    TemplateService, VerifyEmailTemplate,
};
use academy_utils::trace_instrument;

#[derive(Debug, Clone, Build)]
pub struct TemplateEmailServiceImpl<Email, Template> {
    email: Email,
    template: Template,
}

impl<EmailS, Template> TemplateEmailService for TemplateEmailServiceImpl<EmailS, Template>
where
    EmailS: EmailService,
    Template: TemplateService,
{
    #[trace_instrument(skip(self))]
    async fn send_reset_password_email(
        &self,
        recipient: EmailAddressWithName,
        data: &ResetPasswordTemplate,
    ) -> anyhow::Result<bool> {
        self.send_email(
            recipient,
            data,
            "Passwort zurücksetzen - Bootstrap Academy",
            Vec::new(),
        )
        .await
    }

    #[trace_instrument(skip(self))]
    async fn send_subscribe_newsletter_email(
        &self,
        recipient: EmailAddressWithName,
        data: &SubscribeNewsletterTemplate,
    ) -> anyhow::Result<bool> {
        self.send_email(
            recipient,
            data,
            "Newsletter abonnieren - Bootstrap Academy",
            Vec::new(),
        )
        .await
    }

    #[trace_instrument(skip(self))]
    async fn send_verification_email(
        &self,
        recipient: EmailAddressWithName,
        data: &VerifyEmailTemplate,
    ) -> anyhow::Result<bool> {
        self.send_email(
            recipient,
            data,
            "Willkommen bei der Bootstrap Academy!",
            Vec::new(),
        )
        .await
    }

    #[trace_instrument(skip(self, invoice))]
    async fn send_purchase_confirmation_email(
        &self,
        recipient: EmailAddressWithName,
        data: &PurchaseConfirmationTemplate,
        invoice: Vec<u8>,
    ) -> anyhow::Result<bool> {
        let invoice = EmailAttachment {
            filename: "rechnung.pdf".into(),
            content_type: AttachmentContentType::Pdf,
            content: invoice,
        };

        let terms = EmailAttachment {
            filename: "allgemeine_geschaeftsbedingungen.pdf".into(),
            content_type: AttachmentContentType::Pdf,
            content: ALLGEMEINE_GESCHAEFTSBEDINGUNGEN_PDF.into(),
        };

        let revocation_policy = EmailAttachment {
            filename: "widerrufsbelehrung.pdf".into(),
            content_type: AttachmentContentType::Pdf,
            content: WIDERRUFSBELEHRUNG_PDF.into(),
        };

        self.send_email(
            recipient,
            data,
            "Kaufbestätigung - Bootstrap Academy",
            vec![invoice, terms, revocation_policy],
        )
        .await
    }
}

impl<EmailS, TemplateS> TemplateEmailServiceImpl<EmailS, TemplateS>
where
    EmailS: EmailService,
    TemplateS: TemplateService,
{
    async fn send_email<T: Template + 'static>(
        &self,
        recipient: EmailAddressWithName,
        data: &T,
        subject: impl Into<String>,
        attachments: Vec<EmailAttachment>,
    ) -> anyhow::Result<bool> {
        self.email
            .send(Email {
                recipient,
                subject: subject.into(),
                body: self.template.render(data)?,
                content_type: ContentType::Html,
                reply_to: None,
                attachments,
            })
            .await
    }
}
