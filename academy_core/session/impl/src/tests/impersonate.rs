use academy_auth_contracts::MockAuthService;
use academy_core_session_contracts::{
    SessionFeatureService, SessionImpersonateError, session::MockSessionService,
};
use academy_demo::{
    session::{ADMIN_1, BAR_1, FOO_1},
    user::{ADMIN, BAR, FOO},
};
use academy_models::auth::{AuthError, AuthenticateError, AuthorizeError, Login};
use academy_persistence_contracts::{MockDatabase, user::MockUserRepository};
use academy_utils::assert_matches;

use crate::{SessionFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn ok() {
    // Arrange
    let expected = Login {
        user_composite: FOO.clone(),
        session: FOO_1.clone(),
        access_token: "access token".into(),
        refresh_token: "refresh token".into(),
    };

    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let session =
        MockSessionService::new().with_create(FOO.clone(), None, false, false, expected.clone());

    let sut = SessionFeatureServiceImpl {
        auth,
        db,
        user_repo,
        session,
        ..Sut::default()
    };

    // Act
    let result = sut.impersonate(&"token".into(), FOO.user.id).await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(None);

    let sut = SessionFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.impersonate(&"token".into(), FOO.user.id).await;

    // Assert
    assert_matches!(
        result,
        Err(SessionImpersonateError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

#[tokio::test]
async fn unauthorized() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((BAR.user.clone(), BAR_1.clone())));

    let sut = SessionFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.impersonate(&"token".into(), FOO.user.id).await;

    // Assert
    assert_matches!(
        result,
        Err(SessionImpersonateError::Auth(AuthError::Authorize(
            AuthorizeError::Admin
        )))
    );
}

#[tokio::test]
async fn user_not_found() {
    // Arrange
    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, None);

    let sut = SessionFeatureServiceImpl {
        auth,
        db,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.impersonate(&"token".into(), FOO.user.id).await;

    // Assert
    assert_matches!(result, Err(SessionImpersonateError::NotFound));
}
