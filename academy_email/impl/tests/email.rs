use std::time::{Duration, Instant};

use academy_email_contracts::{ContentType, Email, EmailService};
use academy_email_impl::EmailServiceImpl;
use academy_models::{email_address::EmailAddressWithName, url::Url};
use anyhow::Context;
use serde::Deserialize;
use uuid::Uuid;

#[tokio::test]
async fn send_email() {
    let client = setup().await;

    let result = client
        .email
        .send(Email {
            recipient: "recipient@example.com".parse().unwrap(),
            subject: "The Subject".into(),
            body: "<h1>Hello World!</h1>".into(),
            content_type: ContentType::Html,
            reply_to: Some("replyto@example.com".parse().unwrap()),
            attachments: Vec::new(),
        })
        .await
        .unwrap();

    assert!(result);

    let mail = client.wait_for_mail().await;
    assert_eq!(mail.from, client.from.0.email.as_ref());
    assert_eq!(mail.to, ["recipient@example.com"]);
    assert_eq!(mail.subject, "The Subject");

    let details = client.fetch_email_details(mail.id).await;
    assert!(!details.plain_text);
    let reply_to = details
        .headers
        .into_iter()
        .find(|h| h.name == "Reply-To")
        .unwrap();
    assert_eq!(reply_to.value, "replyto@example.com");

    let source = client.fetch_email_source(mail.id).await;
    assert_eq!(source, "<h1>Hello World!</h1>");
}

struct TestClient {
    email: EmailServiceImpl,
    from: EmailAddressWithName,
    smtp4dev_url: Url,
}

impl TestClient {
    async fn reset(&self) {
        reqwest::Client::new()
            .delete(self.smtp4dev_url.join("api/Messages/*").unwrap())
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
    }

    async fn wait_for_mail(&self) -> EmailSummary {
        let now = Instant::now();
        while now.elapsed() < Duration::from_secs(5) {
            let mut mailbox = self.fetch_mailbox().await;
            if let Some(mail) = mailbox.pop() {
                return mail;
            }
        }
        panic!("No email received");
    }

    async fn fetch_mailbox(&self) -> Vec<EmailSummary> {
        reqwest::Client::new()
            .get(self.smtp4dev_url.join("api/Messages").unwrap())
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json::<PaginationResponse<_>>()
            .await
            .unwrap()
            .results
    }

    async fn fetch_email_details(&self, id: Uuid) -> EmailDetails {
        reqwest::Client::new()
            .get(
                self.smtp4dev_url
                    .join(&format!("api/Messages/{id}"))
                    .unwrap(),
            )
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    async fn fetch_email_source(&self, id: Uuid) -> String {
        reqwest::Client::new()
            .get(
                self.smtp4dev_url
                    .join(&format!("api/Messages/{id}/source"))
                    .unwrap(),
            )
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .text()
            .await
            .unwrap()
    }
}

async fn setup() -> TestClient {
    let config = academy_config::load().unwrap();

    let email = EmailServiceImpl::new(&config.email.smtp_url, config.email.from.clone())
        .await
        .unwrap();

    let smtp4dev_url = std::env::var("SMTP4DEV_URL")
        .context("Failed to read SMTP4DEV_URL environment variable")
        .unwrap()
        .parse()
        .context("Failed to parse SMTP4DEV_URL environment variable")
        .unwrap();

    let client = TestClient {
        email,
        from: config.email.from,
        smtp4dev_url,
    };

    client.reset().await;

    client
}

#[derive(Debug, Deserialize)]
struct PaginationResponse<T> {
    results: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct EmailSummary {
    id: Uuid,
    from: String,
    to: Vec<String>,
    subject: String,
}

#[derive(Debug, Deserialize)]
struct EmailDetails {
    headers: Vec<EmailHeader>,
    #[serde(rename = "hasPlainTextBody")]
    plain_text: bool,
}

#[derive(Debug, Deserialize)]
struct EmailHeader {
    name: String,
    value: String,
}
