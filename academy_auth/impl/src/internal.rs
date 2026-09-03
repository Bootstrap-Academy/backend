use academy_auth_contracts::internal::{AuthInternalAuthenticateError, AuthInternalService};
use academy_di::Build;
use academy_models::auth::InternalToken;
use academy_shared_contracts::jwt::JwtService;
use academy_utils::trace_instrument;
use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::AuthServiceConfig;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct AuthInternalServiceImpl<Jwt> {
    jwt: Jwt,
    config: AuthServiceConfig,
}

impl<Jwt> AuthInternalService for AuthInternalServiceImpl<Jwt>
where
    Jwt: JwtService,
{
    #[trace_instrument(skip(self))]
    fn issue_token(&self, audience: &str) -> anyhow::Result<InternalToken> {
        self.jwt
            .sign_with_key(
                audience,
                Token {
                    aud: audience.into(),
                },
                self.config.internal_token_ttl,
            )
            .map(Into::into)
            .with_context(|| {
                format!("Failed to issue internal access token for audience {audience}")
            })
    }

    #[trace_instrument(skip(self))]
    fn authenticate(
        &self,
        token: &InternalToken,
        audience: &str,
    ) -> Result<(), AuthInternalAuthenticateError> {
        self.jwt
            .verify_with_key::<Token>(audience, token)
            .ok()
            .filter(|data| data.aud == audience)
            .map(|_| ())
            .ok_or(AuthInternalAuthenticateError::InvalidToken)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Token {
    aud: String,
}

#[cfg(test)]
mod tests {
    use academy_shared_contracts::jwt::{MockJwtService, VerifyJwtError};
    use academy_utils::assert_matches;

    use super::*;

    type Sut = AuthInternalServiceImpl<MockJwtService>;

    #[test]
    fn issue() {
        // Arrange
        let config = AuthServiceConfig::default();

        let expected = "the internal auth token";

        let jwt = MockJwtService::new().with_sign_with_key(
            "test",
            Token { aud: "test".into() },
            config.internal_token_ttl,
            Ok(expected.into()),
        );

        let sut = AuthInternalServiceImpl {
            jwt,
            ..Sut::default()
        };

        // Act
        let result = sut.issue_token("test");

        // Assert
        assert_eq!(result.unwrap().into_inner(), expected);
    }

    #[test]
    fn authenticate_ok() {
        // Arrange
        let jwt = MockJwtService::new().with_verify_with_key(
            "auth",
            "token".into(),
            Ok(Token { aud: "auth".into() }),
        );

        let sut = AuthInternalServiceImpl {
            jwt,
            ..Sut::default()
        };

        // Act
        let result = sut.authenticate(&"token".into(), "auth");

        // Assert
        result.unwrap();
    }

    #[test]
    fn authenticate_invalid() {
        // Arrange
        let jwt = MockJwtService::new().with_verify_with_key(
            "auth",
            "token".into(),
            Err(VerifyJwtError::<Token>::Invalid),
        );

        let sut = AuthInternalServiceImpl {
            jwt,
            ..Sut::default()
        };

        // Act
        let result = sut.authenticate(&"token".into(), "auth");

        // Assert
        assert_matches!(result, Err(AuthInternalAuthenticateError::InvalidToken));
    }

    #[test]
    fn authenticate_expired() {
        // Arrange
        let jwt = MockJwtService::new().with_verify_with_key(
            "auth",
            "token".into(),
            Err(VerifyJwtError::Expired(Token { aud: "auth".into() })),
        );

        let sut = AuthInternalServiceImpl {
            jwt,
            ..Sut::default()
        };

        // Act
        let result = sut.authenticate(&"token".into(), "auth");

        // Assert
        assert_matches!(result, Err(AuthInternalAuthenticateError::InvalidToken));
    }
}
