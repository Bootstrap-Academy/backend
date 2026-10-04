use academy_auth_contracts::MockAuthService;
use academy_core_user_contracts::{UserFeatureService, UserUpdateError, UserUpdateRequest};
use academy_demo::{
    UUID1,
    session::FOO_1,
    user::{ADMIN, BAR, FOO},
};
use academy_models::{
    session::{Session, SessionOrigin},
    user::{UserComposite, UserIdOrSelf, UserProfile, UserProfilePatch},
};
use academy_persistence_contracts::{
    MockDatabase, session::MockSessionRepository, user::MockUserRepository,
};
use academy_utils::{
    assert_matches,
    patch::{Patch, PatchValue},
};

use crate::{UserFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn update_profile() {
    // Arrange
    let expected = UserComposite {
        profile: BAR.profile.clone(),
        ..FOO.clone()
    };

    let auth = MockAuthService::new()
        .with_authenticate(Some((FOO.user.clone(), FOO_1.clone())))
        .with_authenticate_in_transaction(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    // `leaderboard_opt_out` is the same for both users, so it is minimized away
    let expected_patch = UserProfilePatch {
        leaderboard_opt_out: PatchValue::Unchanged,
        ..expected.profile.clone().into_patch()
    };

    let user_repo = MockUserRepository::new()
        .with_lock_account(FOO.user.id, true)
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
    let auth = MockAuthService::new()
        .with_authenticate(Some((FOO.user.clone(), FOO_1.clone())))
        .with_authenticate_in_transaction(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new()
        .with_lock_account(FOO.user.id, true)
        .with_get_composite(FOO.user.id, Some(FOO.clone()));

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

    let auth = MockAuthService::new()
        .with_authenticate(Some((FOO.user.clone(), FOO_1.clone())))
        .with_authenticate_in_transaction(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new()
        .with_lock_account(FOO.user.id, true)
        .with_get_composite(FOO.user.id, Some(FOO.clone()))
        .with_update_profile(
            FOO.user.id,
            UserProfilePatch {
                leaderboard_opt_out: PatchValue::Update(true),
                ..Default::default()
            },
            true,
        );
    let session_repo = MockSessionRepository::new().with_get(FOO_1.id, Some(FOO_1.clone()));

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        session_repo,
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

/// The old opt-out flag shares or withdraws the profile, so a session an
/// administrator opened in the account cannot change it.
#[tokio::test]
async fn update_leaderboard_opt_out_from_impersonation() {
    // Arrange
    let session = Session {
        id: UUID1.into(),
        device_name: None,
        origin: SessionOrigin::Impersonation {
            admin: Some(ADMIN.user.id),
        },
        ..FOO_1.clone()
    };

    let auth = MockAuthService::new()
        .with_authenticate(Some((FOO.user.clone(), session.clone())))
        .with_authenticate_in_transaction(Some((FOO.user.clone(), session.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new()
        .with_lock_account(FOO.user.id, true)
        .with_get_composite(FOO.user.id, Some(FOO.clone()));
    let session_repo = MockSessionRepository::new().with_get(session.id, Some(session));

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        session_repo,
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
    assert_matches!(result, Err(UserUpdateError::NotOwnerSignIn));
}
