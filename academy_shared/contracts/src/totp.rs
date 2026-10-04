use std::future::Future;

use academy_models::mfa::{TotpCode, TotpSecret, TotpSetup};
use thiserror::Error;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait TotpService: Send + Sync + 'static {
    /// Generate a new random totp secret.
    fn generate_secret(&self) -> (TotpSecret, TotpSetup);

    /// Validate the code and return its matching 30-second time step.
    /// Callers must atomically consume this step in their database transaction
    /// before authorizing a request or confirming an authenticator.
    fn check(
        &self,
        code: &TotpCode,
        secret: TotpSecret,
    ) -> impl Future<Output = Result<i64, TotpCheckError>> + Send;
}

#[derive(Debug, Error)]
pub enum TotpCheckError {
    #[error("The code is incorrect.")]
    InvalidCode,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[cfg(feature = "mock")]
impl MockTotpService {
    pub fn with_generate_secret(mut self, secret: TotpSecret, setup: TotpSetup) -> Self {
        self.expect_generate_secret()
            .once()
            .with()
            .return_once(|| (secret, setup));
        self
    }

    pub fn with_check(
        mut self,
        code: TotpCode,
        secret: TotpSecret,
        result: Result<i64, TotpCheckError>,
    ) -> Self {
        self.expect_check()
            .once()
            .with(mockall::predicate::eq(code), mockall::predicate::eq(secret))
            .return_once(|_, _| Box::pin(std::future::ready(result)));
        self
    }
}
