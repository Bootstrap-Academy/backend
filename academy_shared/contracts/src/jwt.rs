use std::{fmt::Debug, time::Duration};

use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait JwtService: Send + Sync + 'static {
    /// Sign a JWT with the given data and time to live.
    ///
    /// `data` must serialize to a map (JSON object), which may not contain the
    /// `exp` key.
    fn sign<T: Serialize + Debug + 'static>(
        &self,
        data: T,
        ttl: Duration,
    ) -> anyhow::Result<String>;

    /// Verify the signature of the given JWT, deserialize its payload and
    /// ensure the JWT has not expired yet.
    fn verify<T: DeserializeOwned + Debug + 'static>(
        &self,
        jwt: &str,
    ) -> Result<T, VerifyJwtError<T>>;

    /// Like [`JwtService::sign`], but signed with the secret that is configured
    /// for `key` instead of the default JWT secret.
    ///
    /// Keys without their own secret fall back to the default JWT secret.
    fn sign_with_key<T: Serialize + Debug + 'static>(
        &self,
        key: &str,
        data: T,
        ttl: Duration,
    ) -> anyhow::Result<String>;

    /// Like [`JwtService::verify`], but verified with the secret that is
    /// configured for `key` instead of the default JWT secret.
    ///
    /// Keys without their own secret fall back to the default JWT secret.
    fn verify_with_key<T: DeserializeOwned + Debug + 'static>(
        &self,
        key: &str,
        jwt: &str,
    ) -> Result<T, VerifyJwtError<T>>;
}

#[derive(Debug, Error)]
pub enum VerifyJwtError<T> {
    #[error("JWT has already expired (data: {0})")]
    Expired(T),
    #[error("Invalid JWT")]
    Invalid,
}

#[cfg(feature = "mock")]
impl MockJwtService {
    pub fn with_sign<T: Debug + PartialEq + Serialize + Send + 'static>(
        mut self,
        data: T,
        ttl: Duration,
        result: anyhow::Result<String>,
    ) -> Self {
        self.expect_sign()
            .once()
            .with(mockall::predicate::eq(data), mockall::predicate::eq(ttl))
            .return_once(|_, _| result);
        self
    }

    pub fn with_verify<T: DeserializeOwned + Debug + Send + 'static>(
        mut self,
        jwt: String,
        result: Result<T, VerifyJwtError<T>>,
    ) -> Self {
        self.expect_verify()
            .once()
            .with(mockall::predicate::eq(jwt))
            .return_once(|_| result);
        self
    }

    pub fn with_sign_with_key<T: Debug + PartialEq + Serialize + Send + 'static>(
        mut self,
        key: &'static str,
        data: T,
        ttl: Duration,
        result: anyhow::Result<String>,
    ) -> Self {
        self.expect_sign_with_key()
            .once()
            .with(
                mockall::predicate::eq(key),
                mockall::predicate::eq(data),
                mockall::predicate::eq(ttl),
            )
            .return_once(|_, _, _| result);
        self
    }

    pub fn with_verify_with_key<T: DeserializeOwned + Debug + Send + 'static>(
        mut self,
        key: &'static str,
        jwt: String,
        result: Result<T, VerifyJwtError<T>>,
    ) -> Self {
        self.expect_verify_with_key()
            .once()
            .with(mockall::predicate::eq(key), mockall::predicate::eq(jwt))
            .return_once(|_, _| result);
        self
    }
}
