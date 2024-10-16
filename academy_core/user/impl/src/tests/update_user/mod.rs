use academy_core_auth_contracts::MockAuthService;
use academy_core_user_contracts::{
    UserService, UserUpdateError, UserUpdateRequest, UserUpdateUserRequest,
};
use academy_demo::{
    session::{ADMIN_1, BAR_1, FOO_1},
    user::{ADMIN, BAR, FOO},
};
use academy_models::auth::{AuthError, AuthenticateError, AuthorizeError};
use academy_persistence_contracts::{user::MockUserRepository, MockDatabase};
use academy_utils::assert_matches;

use crate::{tests::Sut, UserServiceImpl};

mod admin;
mod email;
mod enabled;
mod name;
mod newsletter;
mod no_op;
mod password;
mod profile;

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(None);

    let sut = UserServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut
        .update_user("token", FOO.user.id.into(), Default::default())
        .await;

    // Assert
    assert_matches!(
        result,
        Err(UserUpdateError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

#[tokio::test]
async fn unauthorized() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((BAR.user.clone(), BAR_1.clone())));

    let sut = UserServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut
        .update_user("token", FOO.user.id.into(), Default::default())
        .await;

    // Assert
    assert_matches!(
        result,
        Err(UserUpdateError::Auth(AuthError::Authorize(
            AuthorizeError::Admin
        )))
    );
}

#[tokio::test]
async fn unauthorized_admin() {
    let requests = [
        UserUpdateRequest {
            user: UserUpdateUserRequest {
                email_verified: false.into(),
                ..Default::default()
            },
            ..Default::default()
        },
        UserUpdateRequest {
            user: UserUpdateUserRequest {
                enabled: false.into(),
                ..Default::default()
            },
            ..Default::default()
        },
        UserUpdateRequest {
            user: UserUpdateUserRequest {
                admin: true.into(),
                ..Default::default()
            },
            ..Default::default()
        },
    ];

    for request in requests {
        eprintln!("request = {request:?}");

        // Arrange
        let auth =
            MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

        let db = MockDatabase::build(false);

        let user_repo =
            MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

        let sut = UserServiceImpl {
            auth,
            db,
            user_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.update_user("token", FOO.user.id.into(), request).await;

        // Assert
        assert_matches!(
            result,
            Err(UserUpdateError::Auth(AuthError::Authorize(
                AuthorizeError::Admin
            )))
        );
    }
}

#[tokio::test]
async fn not_found() {
    // Arrange
    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, None);

    let sut = UserServiceImpl {
        auth,
        db,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .update_user("token", FOO.user.id.into(), Default::default())
        .await;

    // Assert
    assert_matches!(result, Err(UserUpdateError::NotFound));
}
