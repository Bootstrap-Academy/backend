use std::sync::Arc;

use academy_di::Build;
use academy_extern_contracts::recaptcha::RecaptchaApiService;
use academy_shared_contracts::captcha::{CaptchaCheckError, CaptchaService};
use academy_utils::trace_instrument;
use anyhow::Context;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct CaptchaServiceImpl<RecaptchaApi> {
    recaptcha_api: RecaptchaApi,
    config: CaptchaServiceConfig,
}

#[derive(Debug, Clone)]
pub enum CaptchaServiceConfig {
    Disabled,
    Recaptcha(RecaptchaCaptchaServiceConfig),
}

#[derive(Clone)]
pub struct RecaptchaCaptchaServiceConfig {
    pub sitekey: Arc<str>,
    pub secret: Arc<str>,
    pub min_score: f64,
}

impl std::fmt::Debug for RecaptchaCaptchaServiceConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecaptchaCaptchaServiceConfig")
            .field("sitekey", &self.sitekey)
            .field("secret", &"<redacted>")
            .field("min_score", &self.min_score)
            .finish()
    }
}

impl<RecaptchaApi> CaptchaService for CaptchaServiceImpl<RecaptchaApi>
where
    RecaptchaApi: RecaptchaApiService,
{
    #[trace_instrument(skip(self))]
    fn get_recaptcha_sitekey(&self) -> Option<&str> {
        match &self.config {
            CaptchaServiceConfig::Recaptcha(RecaptchaCaptchaServiceConfig { sitekey, .. }) => {
                Some(sitekey)
            }
            CaptchaServiceConfig::Disabled => None,
        }
    }

    #[trace_instrument(skip(self, response))]
    async fn check(&self, response: Option<&str>) -> Result<(), CaptchaCheckError> {
        let config = match &self.config {
            CaptchaServiceConfig::Recaptcha(config) => config,
            CaptchaServiceConfig::Disabled => return Ok(()),
        };

        let response = response.ok_or(CaptchaCheckError::Failed)?;
        let response = self
            .recaptcha_api
            .siteverify(response, &config.secret)
            .await
            .context("Failed to verify reCAPTCHA response")?;
        let ok = response.success && response.score.unwrap_or(0.0) >= config.min_score;
        ok.then_some(()).ok_or(CaptchaCheckError::Failed)
    }
}

#[cfg(test)]
mod tests {
    use academy_extern_contracts::recaptcha::{
        MockRecaptchaApiService, RecaptchaSiteverifyResponse,
    };
    use academy_utils::assert_matches;

    use super::*;

    type Sut = CaptchaServiceImpl<MockRecaptchaApiService>;

    #[test]
    fn get_recaptcha_sitekey_enabled() {
        // Arrange
        let sut = Sut::default();

        // Act
        let result = sut.get_recaptcha_sitekey();

        // Arrange
        assert_eq!(result, Some("sitekey"));
    }

    #[test]
    fn get_recaptcha_sitekey_disabled() {
        // Arrange
        let config = CaptchaServiceConfig::Disabled;

        let sut = CaptchaServiceImpl {
            config,
            ..Sut::default()
        };

        // Act
        let result = sut.get_recaptcha_sitekey();

        // Arrange
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn check_ok() {
        // Arrange
        let recaptcha_api = MockRecaptchaApiService::new().with_siteverify(
            "captcha response".into(),
            "secret".into(),
            RecaptchaSiteverifyResponse {
                success: true,
                score: Some(0.7),
            },
        );

        let sut = CaptchaServiceImpl {
            recaptcha_api,
            ..Sut::default()
        };

        // Act
        let result = sut.check(Some("captcha response")).await;

        // Assert
        result.unwrap();
    }

    #[tokio::test]
    async fn check_ok_no_score() {
        // Arrange
        let config = CaptchaServiceConfig::Recaptcha(RecaptchaCaptchaServiceConfig {
            min_score: 0.0,
            ..Default::default()
        });

        let recaptcha_api = MockRecaptchaApiService::new().with_siteverify(
            "captcha response".into(),
            "secret".into(),
            RecaptchaSiteverifyResponse {
                success: true,
                score: None,
            },
        );

        let sut = CaptchaServiceImpl {
            recaptcha_api,
            config,
        };

        // Act
        let result = sut.check(Some("captcha response")).await;

        // Assert
        result.unwrap();
    }

    #[tokio::test]
    async fn check_ok_disabled() {
        // Arrange
        let config = CaptchaServiceConfig::Disabled;

        let sut = CaptchaServiceImpl {
            config,
            ..Sut::default()
        };

        // Act
        let result = sut.check(Some("captcha response")).await;

        // Assert
        result.unwrap();
    }

    #[tokio::test]
    async fn check_ok_disabled_no_response() {
        // Arrange
        let config = CaptchaServiceConfig::Disabled;

        let sut = CaptchaServiceImpl {
            config,
            ..Sut::default()
        };

        // Act
        let result = sut.check(None).await;

        // Assert
        result.unwrap();
    }

    #[tokio::test]
    async fn check_failed_no_score() {
        // Arrange
        let recaptcha_api = MockRecaptchaApiService::new().with_siteverify(
            "captcha response".into(),
            "secret".into(),
            RecaptchaSiteverifyResponse {
                success: true,
                score: None,
            },
        );

        let sut = CaptchaServiceImpl {
            recaptcha_api,
            ..Sut::default()
        };

        // Act
        let result = sut.check(Some("captcha response")).await;

        // Assert
        assert_matches!(result, Err(CaptchaCheckError::Failed));
    }

    #[tokio::test]
    async fn check_failed_insufficient_score() {
        // Arrange
        let recaptcha_api = MockRecaptchaApiService::new().with_siteverify(
            "captcha response".into(),
            "secret".into(),
            RecaptchaSiteverifyResponse {
                success: true,
                score: Some(0.3),
            },
        );

        let sut = CaptchaServiceImpl {
            recaptcha_api,
            ..Sut::default()
        };

        // Act
        let result = sut.check(Some("captcha response")).await;

        // Assert
        assert_matches!(result, Err(CaptchaCheckError::Failed));
    }

    #[tokio::test]
    async fn check_failed_no_success() {
        // Arrange
        let recaptcha_api = MockRecaptchaApiService::new().with_siteverify(
            "captcha response".into(),
            "secret".into(),
            RecaptchaSiteverifyResponse {
                success: false,
                score: None,
            },
        );

        let sut = CaptchaServiceImpl {
            recaptcha_api,
            ..Sut::default()
        };

        // Act
        let result = sut.check(Some("captcha response")).await;

        // Assert
        assert_matches!(result, Err(CaptchaCheckError::Failed));
    }

    #[tokio::test]
    async fn check_failed_no_response() {
        // Arrange
        let sut = Sut::default();

        // Act
        let result = sut.check(None).await;

        // Assert
        assert_matches!(result, Err(CaptchaCheckError::Failed));
    }

    impl Default for CaptchaServiceConfig {
        fn default() -> Self {
            Self::Recaptcha(Default::default())
        }
    }

    impl Default for RecaptchaCaptchaServiceConfig {
        fn default() -> Self {
            Self {
                sitekey: "sitekey".into(),
                secret: "secret".into(),
                min_score: 0.5,
            }
        }
    }
}
