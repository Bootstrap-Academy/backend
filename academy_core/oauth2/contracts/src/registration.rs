use std::future::Future;

use academy_models::oauth2::{OAuth2Registration, OAuth2RegistrationToken};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait OAuth2RegistrationService: Send + Sync + 'static {
    /// Save the [`OAuth2Registration`] and return an
    /// [`OAuth2RegistrationToken`] to access it later.
    fn save(
        &self,
        registration: &OAuth2Registration,
    ) -> impl Future<Output = anyhow::Result<OAuth2RegistrationToken>> + Send;

    /// Return the [`OAuth2Registration`] identified by the given token.
    fn get(
        &self,
        registration_token: &OAuth2RegistrationToken,
    ) -> impl Future<Output = anyhow::Result<Option<OAuth2Registration>>> + Send;

    /// Invalidate the given [`OAuth2RegistrationToken`] and report whether it
    /// was still valid.
    ///
    /// Reading and invalidating happen atomically, so of two concurrent
    /// requests redeeming the same token exactly one gets `true`.
    fn consume(
        &self,
        registration_token: &OAuth2RegistrationToken,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;
}

#[cfg(feature = "mock")]
impl MockOAuth2RegistrationService {
    pub fn with_save(
        mut self,
        registration: OAuth2Registration,
        token: OAuth2RegistrationToken,
    ) -> Self {
        self.expect_save()
            .once()
            .with(mockall::predicate::eq(registration))
            .return_once(|_| Box::pin(std::future::ready(Ok(token))));
        self
    }

    pub fn with_get(
        mut self,
        token: OAuth2RegistrationToken,
        result: Option<OAuth2Registration>,
    ) -> Self {
        self.expect_get()
            .once()
            .with(mockall::predicate::eq(token))
            .return_once(|_| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_consume(mut self, token: OAuth2RegistrationToken, result: bool) -> Self {
        self.expect_consume()
            .once()
            .with(mockall::predicate::eq(token))
            .return_once(move |_| Box::pin(std::future::ready(Ok(result))));
        self
    }
}
