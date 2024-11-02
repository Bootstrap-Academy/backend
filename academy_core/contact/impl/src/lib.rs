use std::sync::Arc;

use academy_core_contact_contracts::{ContactFeatureService, ContactSendMessageError};
use academy_di::Build;
use academy_email_contracts::{ContentType, Email, EmailService};
use academy_models::{
    contact::ContactMessage, email_address::EmailAddressWithName, RecaptchaResponse,
};
use academy_shared_contracts::captcha::{CaptchaCheckError, CaptchaService};
use academy_utils::trace_instrument;
use anyhow::Context;
use tracing::{error, trace};

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct ContactFeatureServiceImpl<Captcha, Email> {
    captcha: Captcha,
    email: Email,
    config: ContactFeatureConfig,
}

#[derive(Debug, Clone)]
pub struct ContactFeatureConfig {
    pub email: Arc<EmailAddressWithName>,
}

impl<Captcha, EmailS> ContactFeatureService for ContactFeatureServiceImpl<Captcha, EmailS>
where
    Captcha: CaptchaService,
    EmailS: EmailService,
{
    #[trace_instrument(skip(self))]
    async fn send_message(
        &self,
        message: ContactMessage,
        recaptcha_response: Option<RecaptchaResponse>,
    ) -> Result<(), ContactSendMessageError> {
        trace!("check captcha");
        self.captcha
            .check(recaptcha_response.as_deref().map(String::as_str))
            .await
            .map_err(|err| match err {
                CaptchaCheckError::Failed => ContactSendMessageError::Recaptcha,
                CaptchaCheckError::Other(err) => err.context("Failed to check captcha").into(),
            })?;

        let email = Email {
            recipient: (*self.config.email).clone(),
            subject: format!("[Contact Form] {}", *message.subject),
            body: format!(
                "Message from {} ({}):\n\n{}",
                *message.author.name,
                message.author.email.as_str(),
                *message.content
            ),
            content_type: ContentType::Text,
            reply_to: Some(
                message
                    .author
                    .email
                    .with_name(message.author.name.into_inner()),
            ),
            attachments: Vec::new(),
        };

        trace!("send email");
        if !self
            .email
            .send(email)
            .await
            .context("Failed to send email")?
        {
            error!("Failed to send email");
            return Err(ContactSendMessageError::Send);
        }

        trace!("email sent");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use academy_email_contracts::MockEmailService;
    use academy_models::contact::ContactMessageAuthor;
    use academy_shared_contracts::captcha::{CaptchaCheckError, MockCaptchaService};
    use academy_utils::assert_matches;

    use super::*;

    type Sut = ContactFeatureServiceImpl<MockCaptchaService, MockEmailService>;

    #[tokio::test]
    async fn ok() {
        // Arrange
        let captcha = MockCaptchaService::new().with_check(Some("resp"), Ok(()));

        let email = MockEmailService::new().with_send(make_email(), true);

        let sut = ContactFeatureServiceImpl {
            captcha,
            email,
            ..Sut::default()
        };

        // Act
        let result = sut
            .send_message(make_contact_message(), Some("resp".try_into().unwrap()))
            .await;

        // Assert
        result.unwrap();
    }

    #[tokio::test]
    async fn error_invalid_recaptcha_response() {
        // Arrange
        let captcha =
            MockCaptchaService::new().with_check(Some("resp"), Err(CaptchaCheckError::Failed));

        let sut = ContactFeatureServiceImpl {
            captcha,
            ..Sut::default()
        };

        // Act
        let result = sut
            .send_message(make_contact_message(), Some("resp".try_into().unwrap()))
            .await;

        // Assert
        assert_matches!(result, Err(ContactSendMessageError::Recaptcha));
    }

    #[tokio::test]
    async fn error() {
        // Arrange
        let captcha = MockCaptchaService::new().with_check(None, Ok(()));

        let email = MockEmailService::new().with_send(make_email(), false);

        let sut = ContactFeatureServiceImpl {
            captcha,
            email,
            ..Sut::default()
        };

        // Act
        let result = sut.send_message(make_contact_message(), None).await;

        // Assert
        assert_matches!(result, Err(ContactSendMessageError::Send));
    }

    impl Default for ContactFeatureConfig {
        fn default() -> Self {
            ContactFeatureConfig {
                email: Arc::new("contact@example.com".parse().unwrap()),
            }
        }
    }

    fn make_contact_message() -> ContactMessage {
        ContactMessage {
            author: ContactMessageAuthor {
                name: "Max Mustermann".try_into().unwrap(),
                email: "max.mustermann@example.de".parse().unwrap(),
            },
            subject: "Test".try_into().unwrap(),
            content: "Hello World!".try_into().unwrap(),
        }
    }

    fn make_email() -> Email {
        Email {
            recipient: "contact@example.com".parse().unwrap(),
            subject: "[Contact Form] Test".into(),
            body: "Message from Max Mustermann (max.mustermann@example.de):\n\nHello World!".into(),
            content_type: ContentType::Text,
            reply_to: Some(
                "Max Mustermann <max.mustermann@example.de>"
                    .parse()
                    .unwrap(),
            ),
            attachments: Vec::new(),
        }
    }
}
