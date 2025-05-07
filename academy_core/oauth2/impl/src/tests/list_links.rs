use academy_auth_contracts::MockAuthService;
use academy_core_oauth2_contracts::{OAuth2FeatureService, OAuth2ListLinksError};
use academy_demo::{
    UUID1,
    oauth2::FOO_OAUTH2_LINK_1,
    session::{ADMIN_1, BAR_1, FOO_1},
    user::{ADMIN, BAR, FOO},
};
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    oauth2::OAuth2Link,
};
use academy_persistence_contracts::{
    MockDatabase, oauth2::MockOAuth2Repository, user::MockUserRepository,
};
use academy_utils::assert_matches;

use crate::{OAuth2FeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn ok() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let oauth2_repo = MockOAuth2Repository::new().with_list_links_by_user(
        FOO.user.id,
        vec![
            FOO_OAUTH2_LINK_1.clone(),
            OAuth2Link {
                id: UUID1.into(),
                provider_id: "unknown-provider".into(),
                ..FOO_OAUTH2_LINK_1.clone()
            },
        ],
    );

    let sut = OAuth2FeatureServiceImpl {
        db,
        auth,
        user_repo,
        oauth2_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.list_links(&"token".into(), FOO.user.id.into()).await;

    // Assert
    assert_eq!(result.unwrap(), [FOO_OAUTH2_LINK_1.clone()]);
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(None);

    let sut = OAuth2FeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.list_links(&"token".into(), FOO.user.id.into()).await;

    // Assert
    assert_matches!(
        result,
        Err(OAuth2ListLinksError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

#[tokio::test]
async fn unauthorized() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((BAR.user.clone(), BAR_1.clone())));

    let sut = OAuth2FeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.list_links(&"token".into(), FOO.user.id.into()).await;

    // Assert
    assert_matches!(
        result,
        Err(OAuth2ListLinksError::Auth(AuthError::Authorize(
            AuthorizeError::Admin
        )))
    );
}

#[tokio::test]
async fn not_found() {
    // Arrange
    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, false);

    let sut = OAuth2FeatureServiceImpl {
        db,
        auth,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.list_links(&"token".into(), FOO.user.id.into()).await;

    // Assert
    assert_matches!(result, Err(OAuth2ListLinksError::NotFound));
}
