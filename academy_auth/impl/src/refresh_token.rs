use academy_auth_contracts::refresh_token::AuthRefreshTokenService;
use academy_di::Build;
use academy_models::{auth::RefreshToken, session::SessionRefreshTokenHash};
use academy_shared_contracts::{hash::HashService, secret::SecretService};

use crate::AuthServiceConfig;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct AuthRefreshTokenServiceImpl<Secret, Hash> {
    secret: Secret,
    hash: Hash,
    config: AuthServiceConfig,
}

impl<Secret, Hash> AuthRefreshTokenService for AuthRefreshTokenServiceImpl<Secret, Hash>
where
    Secret: SecretService,
    Hash: HashService,
{
    #[tracing::instrument(skip(self))]
    fn issue(&self) -> RefreshToken {
        self.secret
            .generate(self.config.refresh_token_length)
            .0
            .into()
    }

    #[tracing::instrument(skip(self, refresh_token))]
    fn hash(&self, refresh_token: &RefreshToken) -> SessionRefreshTokenHash {
        self.hash.sha256(refresh_token).into()
    }
}

#[cfg(test)]
mod tests {
    use academy_demo::SHA256HASH1;
    use academy_shared_contracts::{hash::MockHashService, secret::MockSecretService};

    use super::*;

    type Sut = AuthRefreshTokenServiceImpl<MockSecretService, MockHashService>;

    #[test]
    fn issue() {
        // Arrange
        let config = AuthServiceConfig::default();

        let refresh_token = "the new refresh token";

        let secret = MockSecretService::new()
            .with_generate(config.refresh_token_length, refresh_token.into());

        let sut = AuthRefreshTokenServiceImpl {
            secret,
            config,
            ..Sut::default()
        };

        // Act
        let result = sut.issue();

        // Assert
        assert_eq!(result.into_inner(), refresh_token);
    }

    #[test]
    fn hash() {
        // Arrange
        let refresh_token = "the refresh token";

        let hash =
            MockHashService::new().with_sha256(RefreshToken::new(refresh_token), *SHA256HASH1);

        let sut = AuthRefreshTokenServiceImpl {
            hash,
            ..Sut::default()
        };

        // Act
        let result = sut.hash(&refresh_token.into());

        // Assert
        assert_eq!(result, (*SHA256HASH1).into());
    }
}
