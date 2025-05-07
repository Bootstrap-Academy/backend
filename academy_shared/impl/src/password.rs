use std::sync::Arc;

use academy_di::Build;
use academy_models::Sensitive;
use academy_shared_contracts::password::{PasswordService, PasswordVerifyError};
use academy_utils::trace_instrument;
use anyhow::{Context, anyhow};
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{self, SaltString, rand_core::OsRng},
};

#[derive(Debug, Clone, Default, Build)]
pub struct PasswordServiceImpl {
    #[di(default)]
    argon2: Arc<Argon2<'static>>,
}

impl PasswordService for PasswordServiceImpl {
    #[trace_instrument(skip(self))]
    async fn hash(&self, password: Sensitive<String>) -> anyhow::Result<String> {
        let argon2 = Arc::clone(&self.argon2);
        let salt = SaltString::generate(&mut OsRng);
        tokio::task::spawn_blocking(move || {
            argon2
                .hash_password(password.0.as_bytes(), &salt)
                .map(|hash| hash.to_string())
        })
        .await?
        .context("Failed to hash password")
    }

    #[trace_instrument(skip(self))]
    async fn verify(
        &self,
        password: Sensitive<String>,
        hash: String,
    ) -> Result<(), PasswordVerifyError> {
        let argon2 = Arc::clone(&self.argon2);
        tokio::task::spawn_blocking(move || {
            let hash =
                PasswordHash::new(&hash).map_err(|err| PasswordVerifyError::Other(err.into()))?;
            argon2
                .verify_password(password.0.as_bytes(), &hash)
                .map_err(|err| match err {
                    password_hash::Error::Password => PasswordVerifyError::InvalidPassword,
                    err => anyhow!(err).context("Failed to verify password").into(),
                })
        })
        .await
        .map_err(|err| anyhow!(err).context("Failed to verify password"))?
    }
}

#[cfg(test)]
mod tests {
    use academy_utils::assert_matches;

    use super::*;

    #[tokio::test]
    async fn hash_verify_ok() {
        // Arrange
        let password = "some user password";

        let sut = PasswordServiceImpl::default();

        // Act
        let hash = sut.hash(password.to_owned().into()).await.unwrap();
        let result = sut.verify(password.to_owned().into(), hash).await;

        // Assert
        result.unwrap();
    }

    #[tokio::test]
    async fn hash_verify_invalid_password() {
        // Arrange
        let password = "some user password";
        let password2 = "other password";

        let sut = PasswordServiceImpl::default();

        // Act
        let hash = sut.hash(password.to_owned().into()).await.unwrap();
        let result = sut.verify(password2.to_owned().into(), hash).await;

        // Assert
        assert_matches!(result, Err(PasswordVerifyError::InvalidPassword));
    }
}
