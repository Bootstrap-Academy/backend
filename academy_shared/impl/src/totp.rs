use std::time::Duration;

use academy_cache_contracts::CacheService;
use academy_di::Build;
use academy_models::mfa::{TotpCode, TotpSecret, TotpSecretLength, TotpSetup};
use academy_shared_contracts::{
    hash::HashService,
    secret::SecretService,
    time::TimeService,
    totp::{TotpCheckError, TotpService},
};
use academy_utils::trace_instrument;
use anyhow::Context;
use totp_rs::{Rfc6238, TOTP};

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct TotpServiceImpl<Secret, Time, Hash, Cache> {
    secret: Secret,
    time: Time,
    hash: Hash,
    cache: Cache,
    config: TotpServiceConfig,
}

#[derive(Debug, Clone)]
pub struct TotpServiceConfig {
    pub secret_length: TotpSecretLength,
}

impl<Secret, Time, Hash, Cache> TotpService for TotpServiceImpl<Secret, Time, Hash, Cache>
where
    Secret: SecretService,
    Time: TimeService,
    Hash: HashService,
    Cache: CacheService,
{
    #[trace_instrument(skip(self))]
    fn generate_secret(&self) -> (TotpSecret, TotpSetup) {
        let secret = self.secret.generate_bytes(*self.config.secret_length).0;

        let totp = TOTP::from_rfc6238(Rfc6238::with_defaults(secret).unwrap()).unwrap();
        let setup = TotpSetup {
            secret: totp.get_secret_base32().into(),
        };

        (TotpSecret::try_new(totp.secret).unwrap(), setup)
    }

    #[trace_instrument(skip(self))]
    async fn check(&self, code: &TotpCode, secret: TotpSecret) -> Result<(), TotpCheckError> {
        let now = self.time.now().timestamp();
        let secret_hash = self.hash.sha256(&*secret);

        let totp =
            TOTP::from_rfc6238(Rfc6238::with_defaults(secret.into_inner()).unwrap()).unwrap();

        if !totp.check(code, now as _) {
            return Err(TotpCheckError::InvalidCode);
        }

        // temporarily cache used totp codes to prevent replay attacks
        let cache_key = format!("totp_code_used:{}:{}", hex::encode(secret_hash.0), **code);
        if self
            .cache
            .get::<()>(&cache_key)
            .await
            .context("Failed to check whether totp code has recently been used")?
            .is_some()
        {
            return Err(TotpCheckError::RecentlyUsed);
        }

        // Each code is valid for 30 seconds and we also accept the window before and
        // after the current one. So after 30 + 30 + 30 = 90 seconds the code should
        // have expired and can be removed from the cache.
        self.cache
            .set(&cache_key, &(), Some(Duration::from_secs(90)))
            .await
            .context("Failed to cache used totp code")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use academy_cache_contracts::MockCacheService;
    use academy_demo::{SHA256HASH1, SHA256HASH1_HEX};
    use academy_shared_contracts::{
        hash::MockHashService, secret::MockSecretService, time::MockTimeService,
    };
    use academy_utils::assert_matches;
    use chrono::DateTime;

    use super::*;

    type Sut =
        TotpServiceImpl<MockSecretService, MockTimeService, MockHashService, MockCacheService>;

    #[test]
    fn generate_secret() {
        // Arrange
        let expected_secret = "XSSYkVp8pDsOnT1jB5eN0CB8".to_owned().into_bytes();
        let expected_totp_setup = TotpSetup {
            secret: "LBJVGWLLKZYDQ4CEONHW4VBRNJBDKZKOGBBUEOA".into(),
        };

        let secret = MockSecretService::new().with_generate_bytes(24, expected_secret.clone());

        let sut = TotpServiceImpl {
            secret,
            ..Sut::default()
        };

        // Act
        let (secret, setup) = sut.generate_secret();

        // Assert
        assert_eq!(secret.into_inner(), expected_secret);
        assert_eq!(setup, expected_totp_setup);
    }

    #[tokio::test]
    async fn check_ok() {
        // Arrange
        let code = "960546";
        let secret =
            TotpSecret::try_new("XSSYkVp8pDsOnT1jB5eN0CB8".to_owned().into_bytes()).unwrap();

        let time =
            MockTimeService::new().with_now(DateTime::from_timestamp(1724949831, 0).unwrap());
        let hash = MockHashService::new().with_sha256(secret.clone().into_inner(), *SHA256HASH1);

        let cache_key = format!("totp_code_used:{SHA256HASH1_HEX}:{code}");
        let cache = MockCacheService::new()
            .with_get(cache_key.clone(), None::<()>)
            .with_set(cache_key, (), Some(Duration::from_secs(90)));

        let sut = TotpServiceImpl {
            time,
            cache,
            hash,
            ..Sut::default()
        };

        // Act
        let result = sut.check(&code.try_into().unwrap(), secret.clone()).await;

        // Assert
        result.unwrap();
    }

    #[tokio::test]
    async fn check_invalid() {
        // Arrange
        let code = "384957";
        let secret =
            TotpSecret::try_new("XSSYkVp8pDsOnT1jB5eN0CB8".to_owned().into_bytes()).unwrap();

        let time =
            MockTimeService::new().with_now(DateTime::from_timestamp(1724949831, 0).unwrap());
        let hash = MockHashService::new().with_sha256(secret.clone().into_inner(), *SHA256HASH1);

        let sut = TotpServiceImpl {
            time,
            hash,
            ..Sut::default()
        };

        // Act
        let result = sut.check(&code.try_into().unwrap(), secret.clone()).await;

        // Assert
        assert_matches!(result, Err(TotpCheckError::InvalidCode));
    }

    #[tokio::test]
    async fn check_recently_used() {
        // Arrange
        let code = "960546";
        let secret =
            TotpSecret::try_new("XSSYkVp8pDsOnT1jB5eN0CB8".to_owned().into_bytes()).unwrap();

        let time =
            MockTimeService::new().with_now(DateTime::from_timestamp(1724949831, 0).unwrap());
        let hash = MockHashService::new().with_sha256(secret.clone().into_inner(), *SHA256HASH1);

        let cache_key = format!("totp_code_used:{SHA256HASH1_HEX}:{code}");
        let cache = MockCacheService::new().with_get(cache_key.clone(), Some(()));

        let sut = TotpServiceImpl {
            time,
            cache,
            hash,
            ..Sut::default()
        };

        // Act
        let result = sut.check(&code.try_into().unwrap(), secret.clone()).await;

        // Assert
        assert_matches!(result, Err(TotpCheckError::RecentlyUsed));
    }

    impl Default for TotpServiceConfig {
        fn default() -> Self {
            Self {
                secret_length: 24.try_into().unwrap(),
            }
        }
    }
}
