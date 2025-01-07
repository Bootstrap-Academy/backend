use std::future::Future;

use thiserror::Error;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait CaptchaService: Send + Sync + 'static {
    /// Return the public reCAPTCHA sitekey if reCAPTCHA is enabled.
    #[allow(clippy::needless_lifetimes, reason = "automock")]
    fn get_recaptcha_sitekey<'a>(&'a self) -> Option<&'a str>;

    /// Verify the given reCAPTCHA response.
    #[allow(clippy::needless_lifetimes, reason = "automock")]
    fn check<'a>(
        &self,
        response: Option<&'a str>,
    ) -> impl Future<Output = Result<(), CaptchaCheckError>> + Send;
}

#[derive(Debug, Error)]
pub enum CaptchaCheckError {
    #[error("The response is invalid or the user is probably not human.")]
    Failed,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[cfg(feature = "mock")]
impl MockCaptchaService {
    pub fn with_get_recaptcha_sitekey(mut self, sitekey: Option<&'static str>) -> Self {
        self.expect_get_recaptcha_sitekey()
            .once()
            .with()
            .return_once(move || sitekey);
        self
    }

    pub fn with_check(
        mut self,
        response: Option<&'static str>,
        result: Result<(), CaptchaCheckError>,
    ) -> Self {
        self.expect_check()
            .once()
            .withf(move |x| *x == response)
            .return_once(|_| Box::pin(std::future::ready(result)));
        self
    }
}
