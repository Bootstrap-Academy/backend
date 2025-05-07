use academy_auth_contracts::MockAuthService;
use academy_core_user_contracts::{
    UserFeatureService, UserListError,
    user::{MockUserService, UserListQuery, UserListResult},
};
use academy_demo::{
    session::{ADMIN_1, FOO_1},
    user::{ADMIN, ALL_USERS, FOO},
};
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    pagination::PaginationSlice,
    user::UserFilter,
};
use academy_persistence_contracts::MockDatabase;
use academy_utils::assert_matches;

use crate::{UserFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn ok() {
    // Arrange
    let query = build_query();

    let expected = UserListResult {
        total: 42,
        user_composites: ALL_USERS.iter().copied().cloned().collect(),
    };

    let db = MockDatabase::build(false);
    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let user = MockUserService::new().with_list(query.clone(), expected.clone());

    let sut = UserFeatureServiceImpl {
        db,
        auth,
        user,
        ..Sut::default()
    };

    // Act
    let result = sut.list_users(&"token".into(), query).await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let query = build_query();

    let auth = MockAuthService::new().with_authenticate(None);

    let sut = UserFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.list_users(&"token".into(), query).await;

    // Assert
    assert_matches!(
        result,
        Err(UserListError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

#[tokio::test]
async fn unauthorized() {
    // Arrange
    let query = build_query();

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let sut = UserFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.list_users(&"token".into(), query).await;

    // Assert
    assert_matches!(
        result,
        Err(UserListError::Auth(AuthError::Authorize(
            AuthorizeError::Admin
        )))
    );
}

fn build_query() -> UserListQuery {
    UserListQuery {
        pagination: PaginationSlice {
            limit: 42.try_into().unwrap(),
            offset: 7,
        },
        filter: UserFilter {
            name: Some("the name".try_into().unwrap()),
            email: Some("the email".try_into().unwrap()),
            enabled: Some(true),
            admin: Some(false),
            mfa_enabled: None,
            email_verified: Some(true),
            newsletter: Some(false),
        },
    }
}
