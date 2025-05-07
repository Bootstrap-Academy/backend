use std::time::Duration;

use academy_auth_contracts::{AuthenticateByRefreshTokenError, MockAuthService};
use academy_core_session_contracts::{
    SessionFeatureService, SessionRefreshError, session::MockSessionService,
};
use academy_demo::{session::FOO_1, user::FOO};
use academy_models::{auth::Login, session::Session};
use academy_persistence_contracts::MockDatabase;
use academy_utils::assert_matches;

use crate::{SessionFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn ok() {
    // Arrange
    let expected = Login {
        user_composite: FOO.clone(),
        session: Session {
            updated_at: FOO_1.updated_at + Duration::from_secs(1234),
            ..FOO_1.clone()
        },
        access_token: "the new access token".into(),
        refresh_token: "the new refresh token".into(),
    };

    let db = MockDatabase::build(true);

    let auth = MockAuthService::new()
        .with_authenticate_by_refresh_token("refresh token".into(), Ok(FOO_1.id));

    let session = MockSessionService::new().with_refresh(FOO_1.id, Ok(expected.clone()));

    let sut = SessionFeatureServiceImpl {
        db,
        auth,
        session,
        ..Sut::default()
    };

    // Act
    let result = sut.refresh_session(&"refresh token".into()).await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn invalid_token() {
    // Arrange
    let db = MockDatabase::build(false);

    let auth = MockAuthService::new().with_authenticate_by_refresh_token(
        "refresh token".into(),
        Err(AuthenticateByRefreshTokenError::Invalid),
    );

    let sut = SessionFeatureServiceImpl {
        db,
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.refresh_session(&"refresh token".into()).await;

    // Assert
    assert_matches!(result, Err(SessionRefreshError::InvalidRefreshToken));
}

#[tokio::test]
async fn expired() {
    // Arrange
    let db = MockDatabase::build(false);

    let auth = MockAuthService::new().with_authenticate_by_refresh_token(
        "refresh token".into(),
        Err(AuthenticateByRefreshTokenError::Expired(FOO_1.id)),
    );

    let session = MockSessionService::new().with_delete(FOO_1.id, true);

    let sut = SessionFeatureServiceImpl {
        db,
        auth,
        session,
        ..Sut::default()
    };

    // Act
    let result = sut.refresh_session(&"refresh token".into()).await;

    // Assert
    assert_matches!(result, Err(SessionRefreshError::InvalidRefreshToken));
}

#[tokio::test]
async fn not_found() {
    // Arrange
    let db = MockDatabase::build(false);

    let auth = MockAuthService::new()
        .with_authenticate_by_refresh_token("refresh token".into(), Ok(FOO_1.id));

    let session = MockSessionService::new().with_refresh(
        FOO_1.id,
        Err(academy_core_session_contracts::session::SessionRefreshError::NotFound),
    );

    let sut = SessionFeatureServiceImpl {
        db,
        auth,
        session,
        ..Sut::default()
    };

    // Act
    let result = sut.refresh_session(&"refresh token".into()).await;

    // Assert
    assert_matches!(result, Err(SessionRefreshError::InvalidRefreshToken));
}
