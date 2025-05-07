use std::future::Future;

use academy_models::{RecaptchaResponse, contact::ContactMessage};
use thiserror::Error;

pub trait ContactFeatureService: Send + Sync + 'static {
    /// Send a message to the support team.
    fn send_message(
        &self,
        message: ContactMessage,
        recaptcha_response: Option<RecaptchaResponse>,
    ) -> impl Future<Output = Result<(), ContactSendMessageError>> + Send;
}

#[derive(Debug, Error)]
pub enum ContactSendMessageError {
    #[error("Invalid recaptcha response")]
    Recaptcha,
    #[error("Failed to send message.")]
    Send,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
