use academy_email_contracts::{AttachmentContentType, ContentType, Email, EmailService};
use academy_models::email_address::EmailAddressWithName;
use academy_utils::{trace_instrument, Apply};
use anyhow::{anyhow, Context};
use lettre::{
    message::{header, Attachment, MessageBuilder, MultiPart, SinglePart},
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

pub mod template;

#[derive(Debug, Clone)]
pub struct EmailServiceImpl {
    from: EmailAddressWithName,
    transport: AsyncSmtpTransport<Tokio1Executor>,
}

impl EmailServiceImpl {
    pub async fn new(url: &str, from: EmailAddressWithName) -> anyhow::Result<Self> {
        let transport = AsyncSmtpTransport::<Tokio1Executor>::from_url(url)?.build();

        Ok(Self { from, transport })
    }

    #[cfg(feature = "dummy")]
    pub async fn dummy() -> Self {
        Self::new("smtp://dummy", "dummy@example.com".parse().unwrap())
            .await
            .unwrap()
    }
}

impl EmailService for EmailServiceImpl {
    #[trace_instrument(skip(self))]
    async fn send(&self, email: Email) -> anyhow::Result<bool> {
        let body = SinglePart::builder()
            .header(match email.content_type {
                ContentType::Text => header::ContentType::TEXT_PLAIN,
                ContentType::Html => header::ContentType::TEXT_HTML,
            })
            .body(email.body);

        let multipart = email.attachments.into_iter().fold(
            MultiPart::mixed().singlepart(body),
            |acc, attachment| {
                acc.singlepart(Attachment::new(attachment.filename).body(
                    attachment.content,
                    match attachment.content_type {
                        AttachmentContentType::Pdf => "application/pdf".parse().unwrap(),
                    },
                ))
            },
        );

        let message = Message::builder()
            .from(self.from.0.clone())
            .to(email.recipient.0)
            .apply_map(email.reply_to.map(|x| x.0), MessageBuilder::reply_to)
            .subject(email.subject)
            .multipart(multipart)
            .context("Failed to build email message")?;

        self.transport
            .send(message)
            .await
            .map(|response| response.is_positive())
            .context("Failed to send email")
    }

    #[trace_instrument(skip(self))]
    async fn ping(&self) -> anyhow::Result<()> {
        self.transport
            .test_connection()
            .await
            .context("Failed to ping smtp server")?
            .then_some(())
            .ok_or_else(|| anyhow!("Failed to ping smtp server"))
    }
}
