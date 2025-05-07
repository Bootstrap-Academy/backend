use academy_auth_contracts::MockAuthService;
use academy_core_user_contracts::{
    PasswordUpdate, UserFeatureService, UserUpdateError, UserUpdateRequest, UserUpdateUserRequest,
    update::MockUserUpdateService,
};
use academy_demo::{session::FOO_1, user::FOO};
use academy_models::user::{UserIdOrSelf, UserPassword};
use academy_persistence_contracts::{MockDatabase, user::MockUserRepository};
use academy_utils::{Apply, assert_matches, patch::PatchValue};

use crate::{UserFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn update_password() {
    // Arrange
    let new_password = UserPassword::try_new("the new password").unwrap();

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let user_update =
        MockUserUpdateService::new().with_update_password(FOO.user.id, new_password.clone());

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_update,
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
                    password: PatchValue::Update(PasswordUpdate::Change(new_password)),
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
async fn remove_password_oauth() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new()
        .with_get_composite(FOO.user.id, Some(FOO.clone()))
        .with_remove_password_hash(FOO.user.id, true);

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
                    password: PatchValue::Update(PasswordUpdate::Remove),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await;

    // Assert
    assert_eq!(
        result.unwrap(),
        FOO.clone().with(|u| u.details.password_login = false)
    );
}

#[tokio::test]
async fn remove_password_no_oauth() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(
        FOO.user.id,
        Some(FOO.clone().with(|u| u.details.oauth2_login = false)),
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
                    password: PatchValue::Update(PasswordUpdate::Remove),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await;

    // Assert
    assert_matches!(result, Err(UserUpdateError::CannotRemovePassword));
}
