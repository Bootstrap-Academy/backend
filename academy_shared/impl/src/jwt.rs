use std::{collections::HashMap, fmt::Debug, sync::Arc, time::Duration};

use academy_di::Build;
use academy_shared_contracts::{
    jwt::{JwtService, VerifyJwtError},
    time::TimeService,
};
use academy_utils::trace_instrument;
use anyhow::Context;
use hmac::{Hmac, digest::KeyInit};
use jwt::{SignWithKey, VerifyWithKey};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::Sha256;

#[derive(Debug, Clone, Build)]
pub struct JwtServiceImpl<Time> {
    time: Time,
    config: JwtServiceConfig,
}

#[derive(Debug, Clone)]
pub struct JwtServiceConfig {
    jwt_secret: Arc<Hmac<Sha256>>,
    /// Additional secrets, addressed by a key. Used to keep the internal
    /// service tokens of the different audiences cryptographically separate
    /// from each other and from the user tokens.
    keys: Arc<HashMap<String, Arc<Hmac<Sha256>>>>,
}

impl JwtServiceConfig {
    pub fn new(jwt_secret: &str, keys: &HashMap<String, String>) -> anyhow::Result<Self> {
        Ok(Self {
            jwt_secret: Hmac::new_from_slice(jwt_secret.as_bytes())
                .context("Failed to load JWT secret")?
                .into(),
            keys: keys
                .iter()
                .filter(|(_, secret)| !secret.is_empty())
                .map(|(key, secret)| {
                    let secret = Hmac::new_from_slice(secret.as_bytes())
                        .with_context(|| format!("Failed to load JWT secret for key {key}"))?;
                    anyhow::Ok((key.clone(), Arc::new(secret)))
                })
                .collect::<anyhow::Result<HashMap<_, _>>>()?
                .into(),
        })
    }

    /// Return the secret that is configured for the given key, falling back to
    /// the default JWT secret.
    fn key(&self, key: &str) -> &Hmac<Sha256> {
        self.keys.get(key).unwrap_or(&self.jwt_secret)
    }
}

impl<Time> JwtServiceImpl<Time>
where
    Time: TimeService,
{
    fn sign_with_secret<T: Serialize + Debug + 'static>(
        &self,
        secret: &Hmac<Sha256>,
        data: T,
        ttl: Duration,
    ) -> anyhow::Result<String> {
        let now = self.time.now().timestamp() as u64;
        let exp = now + ttl.as_secs();

        JwtData { exp, data }
            .sign_with_key(secret)
            .context("Failed to sign JWT")
    }

    fn verify_with_secret<T: DeserializeOwned + Debug + 'static>(
        &self,
        secret: &Hmac<Sha256>,
        jwt: &str,
    ) -> Result<T, VerifyJwtError<T>> {
        let JwtData { exp, data } = jwt
            .verify_with_key(secret)
            .map_err(|_| VerifyJwtError::Invalid)?;

        let now = self.time.now().timestamp() as u64;
        if now < exp {
            Ok(data)
        } else {
            Err(VerifyJwtError::Expired(data))
        }
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
        self.sign_with_secret(&self.config.jwt_secret, data, ttl)
    }

    #[trace_instrument(skip(self))]
    fn verify<T: DeserializeOwned + Debug + 'static>(
        &self,
        jwt: &str,
    ) -> Result<T, VerifyJwtError<T>> {
        self.verify_with_secret(&self.config.jwt_secret, jwt)
    }

    #[trace_instrument(skip(self))]
    fn sign_with_key<T: Serialize + Debug + 'static>(
        &self,
        key: &str,
        data: T,
        ttl: Duration,
    ) -> anyhow::Result<String> {
        self.sign_with_secret(self.config.key(key), data, ttl)
    }

    #[trace_instrument(skip(self))]
    fn verify_with_key<T: DeserializeOwned + Debug + 'static>(
        &self,
        key: &str,
        jwt: &str,
    ) -> Result<T, VerifyJwtError<T>> {
        self.verify_with_secret(self.config.key(key), jwt)
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

        let config = JwtServiceConfig::new("the jwt secret", &HashMap::new()).unwrap();

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

        let config = JwtServiceConfig::new("the jwt secret", &HashMap::new()).unwrap();

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

        let config = JwtServiceConfig::new("the jwt secret", &HashMap::new()).unwrap();
        let config2 = JwtServiceConfig::new("other jwt secret", &HashMap::new()).unwrap();

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

    #[test]
    fn sign_verify_with_key() {
        // Arrange
        let data = Data {
            foo: 42,
            bar: "hello world".into(),
        };

        let config = JwtServiceConfig::new(
            "the jwt secret",
            &HashMap::from([("skills".into(), "the skills secret".into())]),
        )
        .unwrap();

        let now = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let then = now + Duration::from_secs(10);
        // the token signed with the skills secret does not even reach the
        // expiry check when it is verified with the default secret, so `now` is
        // only read twice
        let time = MockTimeService::new().with_now(now).with_now(then);

        let sut = JwtServiceImpl { time, config };

        // Act
        let jwt = sut
            .sign_with_key("skills", data.clone(), Duration::from_secs(20))
            .unwrap();

        // Assert
        assert_eq!(sut.verify_with_key::<Data>("skills", &jwt).unwrap(), data);
        // the same token is not valid for the default secret
        assert_matches!(sut.verify::<Data>(&jwt), Err(VerifyJwtError::Invalid));
    }

    #[test]
    fn sign_verify_with_unconfigured_key() {
        // Arrange
        let data = Data {
            foo: 42,
            bar: "hello world".into(),
        };

        let config = JwtServiceConfig::new(
            "the jwt secret",
            &HashMap::from([("skills".into(), String::new())]),
        )
        .unwrap();

        let now = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let then = now + Duration::from_secs(10);
        let time = MockTimeService::new().with_now(now).with_now(then);

        let sut = JwtServiceImpl { time, config };

        // Act
        let jwt = sut
            .sign_with_key("skills", data.clone(), Duration::from_secs(20))
            .unwrap();

        // Assert: an empty secret falls back to the default one
        assert_eq!(sut.verify::<Data>(&jwt).unwrap(), data);
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct Data {
        foo: i32,
        bar: String,
    }
}
