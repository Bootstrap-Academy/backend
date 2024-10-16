use std::sync::Arc;

use academy_core_contact_contracts::{ContactSendMessageError, ContactService};
use academy_di::Build;
use academy_email_contracts::{ContentType, Email, EmailService};
use academy_models::{
    contact::ContactMessage, email_address::EmailAddressWithName, RecaptchaResponse,
};
use academy_shared_contracts::captcha::{CaptchaCheckError, CaptchaService};

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct ContactServiceImpl<Captcha, Email> {
    captcha: Captcha,
    email: Email,
    config: ContactServiceConfig,
}

#[derive(Debug, Clone)]
pub struct ContactServiceConfig {
    pub email: Arc<EmailAddressWithName>,
}

impl<Captcha, EmailS> ContactService for ContactServiceImpl<Captcha, EmailS>
where
    Captcha: CaptchaService,
    EmailS: EmailService,
{
    async fn send_message(
        &self,
        message: ContactMessage,
        recaptcha_response: Option<RecaptchaResponse>,
    ) -> Result<(), ContactSendMessageError> {
        self.captcha
            .check(recaptcha_response.as_deref().map(String::as_str))
            .await
            .map_err(|err| match err {
                CaptchaCheckError::Failed => ContactSendMessageError::Recaptcha,
                CaptchaCheckError::Other(err) => err.into(),
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
        };

        if !self.email.send(email).await? {
            return Err(ContactSendMessageError::Send);
        }

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

    type Sut = ContactServiceImpl<MockCaptchaService, MockEmailService>;

    #[tokio::test]
    async fn ok() {
        // Arrange
        let captcha = MockCaptchaService::new().with_check(Some("resp"), Ok(()));

        let email = MockEmailService::new().with_send(make_email(), true);

        let sut = ContactServiceImpl {
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

        let sut = ContactServiceImpl {
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

        let sut = ContactServiceImpl {
            captcha,
            email,
            ..Sut::default()
        };

        // Act
        let result = sut.send_message(make_contact_message(), None).await;

        // Assert
        assert_matches!(result, Err(ContactSendMessageError::Send));
    }

    impl Default for ContactServiceConfig {
        fn default() -> Self {
            ContactServiceConfig {
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
        }
    }
}
