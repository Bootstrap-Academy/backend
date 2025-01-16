use academy_auth_contracts::{access_token::AuthAccessTokenService, Authentication};
use academy_cache_contracts::CacheService;
use academy_di::Build;
use academy_models::{
    auth::AccessToken,
    session::{SessionId, SessionRefreshTokenHash},
    user::{User, UserId},
};
use academy_shared_contracts::jwt::JwtService;
use academy_utils::trace_instrument;
use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::AuthServiceConfig;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct AuthAccessTokenServiceImpl<Jwt, Cache> {
    jwt: Jwt,
    cache: Cache,
    config: AuthServiceConfig,
}

impl<Jwt, Cache> AuthAccessTokenService for AuthAccessTokenServiceImpl<Jwt, Cache>
where
    Jwt: JwtService,
    Cache: CacheService,
{
    #[trace_instrument(skip(self))]
    fn issue(
        &self,
        user: &User,
        session_id: SessionId,
        refresh_token_hash: SessionRefreshTokenHash,
    ) -> anyhow::Result<AccessToken> {
        let auth = Authentication {
            user_id: user.id,
            session_id,
            refresh_token_hash,
            admin: user.admin,
            email_verified: user.email_verified,
        };

        self.jwt
            .sign(Token::from(auth), self.config.access_token_ttl)
            .map(Into::into)
            .context("Failed to sign JWT")
    }

    #[trace_instrument(skip(self))]
    fn verify(&self, access_token: &AccessToken) -> Option<Authentication> {
        self.jwt.verify(access_token).map(Token::into).ok()
    }

    #[trace_instrument(skip(self))]
    async fn invalidate(&self, refresh_token_hash: SessionRefreshTokenHash) -> anyhow::Result<()> {
        self.cache
            .set(
                &access_token_invalidated_key(refresh_token_hash),
                &(),
                Some(self.config.access_token_ttl),
            )
            .await
            .with_context(|| {
                format!(
                    "Failed to invalidate access token by refresh token hash {refresh_token_hash}"
                )
            })
    }

    #[trace_instrument(skip(self))]
    async fn is_invalidated(
        &self,
        refresh_token_hash: SessionRefreshTokenHash,
    ) -> anyhow::Result<bool> {
        self.cache
            .get::<()>(&access_token_invalidated_key(refresh_token_hash))
            .await
            .map(|x| x.is_some())
            .with_context(|| {
                format!(
                    "Failed to check whether the access token of refresh token hash \
                     {refresh_token_hash} has been invalidated"
                )
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Token {
    uid: UserId,
    sid: SessionId,
    rt: SessionRefreshTokenHash,
    data: TokenData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct TokenData {
    admin: bool,
    email_verified: bool,
}

impl From<Token> for Authentication {
    fn from(value: Token) -> Self {
        Self {
            user_id: value.uid,
            session_id: value.sid,
            refresh_token_hash: value.rt,
            admin: value.data.admin,
            email_verified: value.data.email_verified,
        }
    }
}

impl From<Authentication> for Token {
    fn from(value: Authentication) -> Self {
        Self {
            uid: value.user_id,
            sid: value.session_id,
            rt: value.refresh_token_hash,
            data: TokenData {
                admin: value.admin,
                email_verified: value.email_verified,
            },
        }
    }
}

fn access_token_invalidated_key(refresh_token_hash: SessionRefreshTokenHash) -> String {
    format!(
        "access_token_invalidated:{}",
        hex::encode(refresh_token_hash.0)
    )
}

#[cfg(test)]
mod tests {
    use academy_cache_contracts::MockCacheService;
    use academy_demo::{user::FOO, SHA256HASH1, SHA256HASH1_HEX, UUID1};
    use academy_shared_contracts::jwt::{MockJwtService, VerifyJwtError};

    use super::*;

    type Sut = AuthAccessTokenServiceImpl<MockJwtService, MockCacheService>;

    #[test]
    fn issue() {
        // Arrange
        let config = AuthServiceConfig::default();

        let expected = "the access token";

        let auth = Authentication {
            user_id: FOO.user.id,
            session_id: UUID1.into(),
            refresh_token_hash: (*SHA256HASH1).into(),
            admin: FOO.user.admin,
            email_verified: FOO.user.email_verified,
        };

        let jwt = MockJwtService::new().with_sign(
            Token::from(auth),
            config.access_token_ttl,
            Ok(expected.into()),
        );

        let sut = AuthAccessTokenServiceImpl {
            jwt,
            ..Sut::default()
        };

        // Act
        let result = sut.issue(&FOO.user, UUID1.into(), (*SHA256HASH1).into());

        // Assert
        assert_eq!(result.unwrap().into_inner(), expected);
    }

    #[test]
    fn verify_ok() {
        // Arrange
        let token = "the access token";

        let expected = Authentication {
            user_id: FOO.user.id,
            session_id: UUID1.into(),
            refresh_token_hash: (*SHA256HASH1).into(),
            admin: FOO.user.admin,
            email_verified: FOO.user.email_verified,
        };

        let jwt = MockJwtService::new().with_verify(token.into(), Ok(Token::from(expected)));

        let sut = AuthAccessTokenServiceImpl {
            jwt,
            ..Sut::default()
        };

        // Act
        let result = sut.verify(&token.into());

        // Assert
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn verify_invalid() {
        // Arrange
        let token = "the access token";

        let jwt =
            MockJwtService::new().with_verify(token.into(), Err(VerifyJwtError::<Token>::Invalid));

        let sut = AuthAccessTokenServiceImpl {
            jwt,
            ..Sut::default()
        };

        // Act
        let result = sut.verify(&token.into());

        // Assert
        assert_eq!(result, None);
    }
    #[test]
    fn verify_expired() {
        // Arrange
        let token = "the access token";

        let auth = Authentication {
            user_id: FOO.user.id,
            session_id: UUID1.into(),
            refresh_token_hash: (*SHA256HASH1).into(),
            admin: FOO.user.admin,
            email_verified: FOO.user.email_verified,
        };

        let jwt = MockJwtService::new().with_verify(
            token.into(),
            Err(VerifyJwtError::Expired(Token::from(auth))),
        );

        let sut = AuthAccessTokenServiceImpl {
            jwt,
            ..Sut::default()
        };

        // Act
        let result = sut.verify(&token.into());

        // Assert
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn invalidate() {
        // Arrange
        let config = AuthServiceConfig::default();

        let cache = MockCacheService::new().with_set(
            format!("access_token_invalidated:{SHA256HASH1_HEX}"),
            (),
            Some(config.access_token_ttl),
        );

        let sut = AuthAccessTokenServiceImpl {
            cache,
            ..Sut::default()
        };

        // Act
        let result = sut.invalidate((*SHA256HASH1).into()).await;

        // Assert
        result.unwrap();
    }

    #[tokio::test]
    async fn is_invalidated_true() {
        // Arrange
        let cache = MockCacheService::new().with_get(
            format!("access_token_invalidated:{SHA256HASH1_HEX}"),
            Some(()),
        );

        let sut = AuthAccessTokenServiceImpl {
            cache,
            ..Sut::default()
        };

        // Act
        let result = sut.is_invalidated((*SHA256HASH1).into()).await;

        // Assert
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn is_invalidated_false() {
        // Arrange
        let cache = MockCacheService::new().with_get(
            format!("access_token_invalidated:{SHA256HASH1_HEX}"),
            None::<()>,
        );

        let sut = AuthAccessTokenServiceImpl {
            cache,
            ..Sut::default()
        };

        // Act
        let result = sut.is_invalidated((*SHA256HASH1).into()).await;

        // Assert
        assert!(!result.unwrap());
    }
}
