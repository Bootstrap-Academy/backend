use std::time::Duration;

use academy_auth_contracts::MockAuthService;
use academy_core_user_contracts::{
    UserFeatureService, UserUpdateError, UserUpdateRequest, UserUpdateUserRequest,
    update::{MockUserUpdateService, UserUpdateNameError, UserUpdateNameRateLimitPolicy},
};
use academy_demo::{
    session::{ADMIN_1, FOO_1},
    user::{ADMIN, BAR, FOO},
};
use academy_models::user::{User, UserComposite, UserIdOrSelf};
use academy_persistence_contracts::{MockDatabase, user::MockUserRepository};
use academy_utils::assert_matches;

use crate::{UserFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn update_name_self() {
    // Arrange
    let expected = UserComposite {
        user: User {
            name: BAR.user.name.clone(),
            last_name_change: Some(FOO.user.last_login.unwrap()),
            ..FOO.user.clone()
        },
        ..FOO.clone()
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let user_update = MockUserUpdateService::new().with_update_name(
        FOO.user.clone(),
        BAR.user.name.clone(),
        UserUpdateNameRateLimitPolicy::Enforce,
        Ok(expected.user.clone()),
    );

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
                    name: BAR.user.name.clone().into(),
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
async fn update_name_admin_no_rate_limit() {
    // Arrange
    let expected = UserComposite {
        user: User {
            name: BAR.user.name.clone(),
            last_name_change: Some(FOO.user.last_login.unwrap()),
            ..FOO.user.clone()
        },
        ..FOO.clone()
    };

    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let user_update = MockUserUpdateService::new().with_update_name(
        FOO.user.clone(),
        BAR.user.name.clone(),
        UserUpdateNameRateLimitPolicy::Bypass,
        Ok(expected.user.clone()),
    );

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
            UserIdOrSelf::UserId(FOO.user.id),
            UserUpdateRequest {
                user: UserUpdateUserRequest {
                    name: BAR.user.name.clone().into(),
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
async fn update_name_self_rate_limit() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let expected = FOO.user.last_name_change.unwrap() + Duration::from_secs(17);

    let user_update = MockUserUpdateService::new().with_update_name(
        FOO.user.clone(),
        BAR.user.name.clone(),
        UserUpdateNameRateLimitPolicy::Enforce,
        Err(UserUpdateNameError::RateLimit { until: expected }),
    );

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
                    name: BAR.user.name.clone().into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await;

    // Assert
    assert_matches!(result, Err(UserUpdateError::NameChangeRateLimit { until }) if *until == expected);
}

#[tokio::test]
async fn update_name_conflict() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let user_update = MockUserUpdateService::new().with_update_name(
        FOO.user.clone(),
        BAR.user.name.clone(),
        UserUpdateNameRateLimitPolicy::Enforce,
        Err(UserUpdateNameError::Conflict),
    );

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
                    name: BAR.user.name.clone().into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await;

    // Assert
    assert_matches!(result, Err(UserUpdateError::NameConflict));
}
