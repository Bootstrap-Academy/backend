//! Isolated regression probe for the application's real telemetry and SMTP stack.
use academy::telemetry;
use academy_email_contracts::{ContentType, Email, EmailService};
use academy_email_impl::EmailServiceImpl;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let _guard = sentry::init((args[1].as_str(), telemetry::options()));
    telemetry::init_tracing();
    let mailer = EmailServiceImpl::new(&args[2], "probe@example.invalid".parse()?).await?;
    assert!(
        mailer
            .send(Email {
                sender: None,
                message_id: None,
                recipient: "recipient@example.invalid".parse()?,
                subject: "Owned SMTP regression".into(),
                body: "PRIVATE-MAIL-TEXT confirmation: ABCD-EFGH-IJKL-MNOP".into(),
                content_type: ContentType::Text,
                reply_to: None,
                attachments: Vec::new(),
            })
            .await?
    );
    tracing::debug!("DEBUG-PRIVATE-CODE");
    tracing::info!(
        password = "SYNTHETIC-SMTP-PASSWORD",
        mail_body = "PRIVATE-MAIL-TEXT",
        confirmation_code = "ABCD-EFGH-IJKL-MNOP",
        request_id = "owned-request",
        "safe-info-control"
    );
    tracing::error!(
        token = "PRIVATE-EVENT-TOKEN",
        request_id = "owned-request",
        "safe-error-control"
    );
    sentry::capture_event(sentry::protocol::Event {
        level: sentry::Level::Debug,
        message: Some("DIRECT-DEBUG-SECRET".into()),
        ..Default::default()
    });
    assert!(
        sentry::Hub::current()
            .client()
            .unwrap()
            .flush(Some(std::time::Duration::from_secs(10)))
    );
    Ok(())
}
