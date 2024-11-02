use academy_config::Config;
use academy_email_contracts::{ContentType, Email, EmailService};
use academy_email_impl::EmailServiceImpl;
use academy_models::email_address::EmailAddressWithName;
use anyhow::{anyhow, Context};
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum EmailCommand {
    /// Test email deliverability by sending a test email
    Test {
        /// The address to which the test email should be sent
        recipient: EmailAddressWithName,
    },
}

impl EmailCommand {
    pub async fn invoke(self, config: Config) -> anyhow::Result<()> {
        match self {
            EmailCommand::Test { recipient } => test(config, recipient).await,
        }
    }
}

async fn test(config: Config, recipient: EmailAddressWithName) -> anyhow::Result<()> {
    let email_service = EmailServiceImpl::new(&config.email.smtp_url, config.email.from).await?;

    email_service
        .send(Email {
            recipient,
            subject: "Email Deliverability Test".into(),
            body: "Email deliverability seems to be working!".into(),
            content_type: ContentType::Text,
            reply_to: None,
            attachments: Vec::new(),
        })
        .await
        .and_then(|r| {
            r.then_some(())
                .ok_or_else(|| anyhow!("SMTP server returned a negative response"))
        })
        .context("Failed to send email")?;

    Ok(())
}
