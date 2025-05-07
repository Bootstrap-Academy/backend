use academy_auth_contracts::MockAuthService;
use academy_core_user_contracts::{
    UserFeatureService, UserUpdateError, UserUpdateRequest, UserUpdateUserRequest,
    email_confirmation::MockUserEmailConfirmationService,
};
use academy_demo::{
    session::{ADMIN_1, BAR_1, FOO_1},
    user::{ADMIN, BAR, FOO},
};
use academy_models::user::{User, UserComposite, UserIdOrSelf, UserPatch};
use academy_persistence_contracts::{MockDatabase, user::MockUserRepository};
use academy_utils::assert_matches;

use crate::{UserFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn enable_self() {
    // Arrange
    let foo = UserComposite {
        user: User {
            newsletter: false,
            ..FOO.user.clone()
        },
        ..FOO.clone()
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(foo.clone()));

    let user_email_confirmation = MockUserEmailConfirmationService::new()
        .with_request_newsletter_subscription(
            FOO.user.id,
            FOO.user
                .email
                .clone()
                .unwrap()
                .with_name(FOO.profile.display_name.clone().into_inner()),
        );

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        user_email_confirmation,
        ..Sut::default()
    };

    // Act
    let result = sut
        .update_user(
            &"token".into(),
            UserIdOrSelf::Slf,
            UserUpdateRequest {
                user: UserUpdateUserRequest {
                    newsletter: true.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await;

    // Assert
    assert_eq!(result.unwrap(), foo);
}

#[tokio::test]
async fn enable_admin() {
    // Arrange
    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new()
        .with_get_composite(
            FOO.user.id,
            Some(UserComposite {
                user: User {
                    newsletter: false,
                    ..FOO.user.clone()
                },
                ..FOO.clone()
            }),
        )
        .with_update(
            FOO.user.id,
            UserPatch::new().update_newsletter(true),
            Ok(true),
        );

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .update_user(
            &"token".into(),
            FOO.user.id.into(),
            UserUpdateRequest {
                user: UserUpdateUserRequest {
                    newsletter: true.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await;

    // Assert
    assert_eq!(result.unwrap(), *FOO);
}

#[tokio::test]
async fn disable_self() {
    // Arrange
    let expected = UserComposite {
        user: User {
            newsletter: false,
            ..FOO.user.clone()
        },
        ..FOO.clone()
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new()
        .with_get_composite(FOO.user.id, Some(FOO.clone()))
        .with_update(
            FOO.user.id,
            UserPatch::new().update_newsletter(false),
            Ok(true),
        );

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .update_user(
            &"token".into(),
            UserIdOrSelf::Slf,
            UserUpdateRequest {
                user: UserUpdateUserRequest {
                    newsletter: false.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn enable_self_no_email() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((BAR.user.clone(), BAR_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(BAR.user.id, Some(BAR.clone()));

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .update_user(
            &"token".into(),
            UserIdOrSelf::Slf,
            UserUpdateRequest {
                user: UserUpdateUserRequest {
                    newsletter: true.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await;

    // Assert
    assert_matches!(result, Err(UserUpdateError::NoEmail));
}
