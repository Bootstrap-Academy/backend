use academy_auth_contracts::MockAuthService;
use academy_core_session_contracts::{
    SessionDeleteCurrentError, SessionFeatureService, session::MockSessionService,
};
use academy_demo::{session::FOO_1, user::FOO};
use academy_models::auth::{AuthError, AuthenticateError};
use academy_persistence_contracts::MockDatabase;
use academy_utils::assert_matches;

use crate::{SessionFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn ok() {
    // Arrange
    let db = MockDatabase::build(true);

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let session = MockSessionService::new().with_delete(FOO_1.id, true);

    let sut = SessionFeatureServiceImpl {
        db,
        auth,
        session,
        ..Sut::default()
    };

    // Act
    let result = sut.delete_current_session(&"token".into()).await;

    // Assert
    result.unwrap();
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
    let result = sut.delete_current_session(&"token".into()).await;

    // Assert
    assert_matches!(
        result,
        Err(SessionDeleteCurrentError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}
