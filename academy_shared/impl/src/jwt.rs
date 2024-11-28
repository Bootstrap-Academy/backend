use std::{fmt::Debug, sync::Arc, time::Duration};

use academy_di::Build;
use academy_shared_contracts::{
    jwt::{JwtService, VerifyJwtError},
    time::TimeService,
};
use academy_utils::trace_instrument;
use anyhow::Context;
use hmac::{digest::KeyInit, Hmac};
use jwt::{SignWithKey, VerifyWithKey};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::Sha256;

#[derive(Debug, Clone, Build)]
pub struct JwtServiceImpl<Time> {
    time: Time,
    config: JwtServiceConfig,
}

#[derive(Debug, Clone)]
pub struct JwtServiceConfig {
    jwt_secret: Arc<Hmac<Sha256>>,
}

impl JwtServiceConfig {
    pub fn new(jwt_secret: &str) -> anyhow::Result<Self> {
        Ok(Self {
            jwt_secret: Hmac::new_from_slice(jwt_secret.as_bytes())
                .context("Failed to load JWT secret")?
                .into(),
        })
    }
}

impl<Time> JwtService for JwtServiceImpl<Time>
where
    Time: TimeService,
{
    #[trace_instrument(skip(self))]
    fn sign<T: Serialize + Debug + 'static>(
        &self,
        data: T,
        ttl: Duration,
    ) -> anyhow::Result<String> {
        let now = self.time.now().timestamp() as u64;
        let exp = now + ttl.as_secs();

        JwtData { exp, data }
            .sign_with_key(&*self.config.jwt_secret)
            .context("Failed to sign JWT")
    }

    #[trace_instrument(skip(self))]
    fn verify<T: DeserializeOwned + Debug + 'static>(
        &self,
        jwt: &str,
    ) -> Result<T, VerifyJwtError<T>> {
        let JwtData { exp, data } = jwt
            .verify_with_key(&*self.config.jwt_secret)
            .map_err(|_| VerifyJwtError::Invalid)?;

        let now = self.time.now().timestamp() as u64;
        if now < exp {
            Ok(data)
        } else {
            Err(VerifyJwtError::Expired(data))
        }
    }
}

#[derive(Serialize, Deserialize)]
struct JwtData<T> {
    exp: u64,
    #[serde(flatten)]
    data: T,
}

#[cfg(test)]
mod tests {
    use academy_shared_contracts::time::MockTimeService;
    use academy_utils::assert_matches;
    use chrono::{TimeZone, Utc};

    use super::*;

    #[test]
    fn sign_verify_valid() {
        // Arrange
        let data = Data {
            foo: 42,
            bar: "hello world".into(),
        };

        let config = JwtServiceConfig::new("the jwt secret").unwrap();

        let now = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let then = now + Duration::from_secs(10);
        let time = MockTimeService::new().with_now(now).with_now(then);

        let sut = JwtServiceImpl { time, config };

        // Act
        let jwt = sut.sign(data.clone(), Duration::from_secs(20)).unwrap();
        let verified = sut.verify::<Data>(&jwt);

        // Assert
        assert_eq!(verified.unwrap(), data);
    }

    #[test]
    fn sign_verify_expired() {
        // Arrange
        let data = Data {
            foo: 42,
            bar: "hello world".into(),
        };

        let config = JwtServiceConfig::new("the jwt secret").unwrap();

        let now = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let then = now + Duration::from_secs(20);
        let time = MockTimeService::new().with_now(now).with_now(then);

        let sut = JwtServiceImpl { time, config };

        // Act
        let jwt = sut.sign(data.clone(), Duration::from_secs(10)).unwrap();
        let verified = sut.verify::<Data>(&jwt);

        // Assert
        assert_matches!(verified, Err(VerifyJwtError::Expired(x)) if x == &data);
    }

    #[test]
    fn sign_verify_invalid() {
        // Arrange
        let data = Data {
            foo: 42,
            bar: "hello world".into(),
        };

        let config = JwtServiceConfig::new("the jwt secret").unwrap();
        let config2 = JwtServiceConfig::new("other jwt secret").unwrap();

        let now = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let time = MockTimeService::new().with_now(now);

        let sut = JwtServiceImpl { time, config };
        let sut2 = JwtServiceImpl {
            time: MockTimeService::new(),
            config: config2,
        };

        // Act
        let jwt = sut.sign(data.clone(), Duration::from_secs(10)).unwrap();
        let verified = sut2.verify::<Data>(&jwt);

        // Assert
        assert_matches!(verified, Err(VerifyJwtError::Invalid));
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct Data {
        foo: i32,
        bar: String,
    }
}
