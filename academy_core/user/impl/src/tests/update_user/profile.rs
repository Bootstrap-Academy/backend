use academy_auth_contracts::MockAuthService;
use academy_core_user_contracts::{UserFeatureService, UserUpdateRequest};
use academy_demo::{
    session::FOO_1,
    user::{BAR, FOO},
};
use academy_models::user::{UserComposite, UserIdOrSelf, UserProfile, UserProfilePatch};
use academy_persistence_contracts::{MockDatabase, user::MockUserRepository};
use academy_utils::patch::{Patch, PatchValue};

use crate::{UserFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn update_profile() {
    // Arrange
    let expected = UserComposite {
        profile: BAR.profile.clone(),
        ..FOO.clone()
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    // `leaderboard_opt_out` is the same for both users, so it is minimized away
    let expected_patch = UserProfilePatch {
        leaderboard_opt_out: PatchValue::Unchanged,
        ..expected.profile.clone().into_patch()
    };

    let user_repo = MockUserRepository::new()
        .with_get_composite(FOO.user.id, Some(FOO.clone()))
        .with_update_profile(FOO.user.id, expected_patch, true);

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
                profile: expected.profile.clone().into_patch(),
                ..Default::default()
            },
        )
        .await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn update_profile_no_changes() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

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
                profile: FOO.profile.clone().into_patch(),
                ..Default::default()
            },
        )
        .await;

    // Assert
    assert_eq!(result.unwrap(), *FOO);
}

#[tokio::test]
async fn update_leaderboard_opt_out() {
    // Arrange
    let expected = UserComposite {
        profile: UserProfile {
            leaderboard_opt_out: true,
            ..FOO.profile.clone()
        },
        ..FOO.clone()
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new()
        .with_get_composite(FOO.user.id, Some(FOO.clone()))
        .with_update_profile(
            FOO.user.id,
            UserProfilePatch {
                leaderboard_opt_out: PatchValue::Update(true),
                ..Default::default()
            },
            true,
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
                profile: UserProfilePatch {
                    leaderboard_opt_out: PatchValue::Update(true),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}
