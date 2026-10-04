use academy_di::Build;
use academy_models::mfa::{TotpCode, TotpSecret, TotpSecretLength, TotpSetup};
use academy_shared_contracts::{
    secret::SecretService,
    time::TimeService,
    totp::{TotpCheckError, TotpService},
};
use academy_utils::trace_instrument;
use totp_rs::{Rfc6238, TOTP};

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct TotpServiceImpl<Secret, Time> {
    secret: Secret,
    time: Time,
    config: TotpServiceConfig,
}

#[derive(Debug, Clone)]
pub struct TotpServiceConfig {
    pub secret_length: TotpSecretLength,
}

impl<Secret, Time> TotpService for TotpServiceImpl<Secret, Time>
where
    Secret: SecretService,
    Time: TimeService,
{
    #[tracing::instrument(skip(self))]
    fn generate_secret(&self) -> (TotpSecret, TotpSetup) {
        tracing::trace!("call");
        let secret = self.secret.generate_bytes(*self.config.secret_length).0;
        let totp = TOTP::from_rfc6238(Rfc6238::with_defaults(secret).unwrap()).unwrap();
        let setup = TotpSetup {
            secret: totp.get_secret_base32().into(),
        };
        (TotpSecret::try_new(totp.secret).unwrap(), setup)
    }

    #[trace_instrument(skip(self, code, secret))]
    async fn check(&self, code: &TotpCode, secret: TotpSecret) -> Result<i64, TotpCheckError> {
        let now =
            u64::try_from(self.time.now().timestamp()).map_err(|_| TotpCheckError::InvalidCode)?;
        let totp =
            TOTP::from_rfc6238(Rfc6238::with_defaults(secret.into_inner()).unwrap()).unwrap();
        let current_step = now / totp.step;
        // Validate each candidate through the library's constant-time comparison.
        // Choose the newest matching step if truncated codes happen to collide.
        let mut exact = totp.clone();
        exact.skew = 0;
        for offset in (-1..=1).rev() {
            if let Some(step) = current_step.checked_add_signed(offset)
                && exact.check(code, step * totp.step)
            {
                return Ok(step as i64);
            }
        }
        Err(TotpCheckError::InvalidCode)
    }
}

#[cfg(test)]
mod tests {
    use academy_shared_contracts::{secret::MockSecretService, time::MockTimeService};
    use academy_utils::assert_matches;
    use chrono::DateTime;

    use super::*;

    type Sut = TotpServiceImpl<MockSecretService, MockTimeService>;

    #[test]
    fn generate_secret() {
        let expected_secret = b"XSSYkVp8pDsOnT1jB5eN0CB8".to_vec();
        let secret = MockSecretService::new().with_generate_bytes(24, expected_secret.clone());
        let sut = Sut {
            secret,
            ..Sut::default()
        };
        let (secret, setup) = sut.generate_secret();
        assert_eq!(secret.into_inner(), expected_secret);
        assert_eq!(
            setup.secret.as_str(),
            "LBJVGWLLKZYDQ4CEONHW4VBRNJBDKZKOGBBUEOA"
        );
    }

    #[tokio::test]
    async fn matching_step_is_stable_across_the_entire_skew_window() {
        let secret = TotpSecret::try_new(b"XSSYkVp8pDsOnT1jB5eN0CB8".to_vec()).unwrap();
        let code: TotpCode = "960546".try_into().unwrap();
        let step = 1724949831 / 30;
        for (timestamp, valid) in [
            ((step - 1) * 30 - 1, false),
            ((step - 1) * 30, true),
            (step * 30 - 1, true),
            (step * 30, true),
            ((step + 1) * 30, true),
            ((step + 2) * 30 - 1, true),
            ((step + 2) * 30, false),
            (-1, false),
        ] {
            let sut = Sut {
                time: MockTimeService::new()
                    .with_now(DateTime::from_timestamp(timestamp, 0).unwrap()),
                ..Sut::default()
            };
            let result = sut.check(&code, secret.clone()).await;
            if valid {
                assert_eq!(result.unwrap(), step, "timestamp {timestamp}");
            } else {
                assert_matches!(result, Err(TotpCheckError::InvalidCode));
            }
        }
    }

    #[tokio::test]
    async fn invalid_code() {
        let sut = Sut {
            time: MockTimeService::new().with_now(DateTime::from_timestamp(1724949831, 0).unwrap()),
            ..Sut::default()
        };
        let secret = TotpSecret::try_new(b"XSSYkVp8pDsOnT1jB5eN0CB8".to_vec()).unwrap();
        let result = sut.check(&"384957".try_into().unwrap(), secret).await;
        assert_matches!(result, Err(TotpCheckError::InvalidCode));
    }

    impl Default for TotpServiceConfig {
        fn default() -> Self {
            Self {
                secret_length: 24.try_into().unwrap(),
            }
        }
    }
}
