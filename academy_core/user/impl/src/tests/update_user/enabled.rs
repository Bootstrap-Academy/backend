use academy_auth_contracts::MockAuthService;
use academy_core_user_contracts::{
    UserFeatureService, UserUpdateError, UserUpdateRequest, UserUpdateUserRequest,
    update::MockUserUpdateService,
};
use academy_demo::{
    session::ADMIN_1,
    user::{ADMIN, BAR, FOO},
};
use academy_models::user::{User, UserComposite, UserIdOrSelf};
use academy_persistence_contracts::{MockDatabase, user::MockUserRepository};
use academy_utils::assert_matches;

use crate::{UserFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn update_enabled() {
    for (enabled, user_composite) in [(false, &*FOO), (true, &*BAR)] {
        // Arrange
        let expected = UserComposite {
            user: User {
                enabled,
                ..user_composite.user.clone()
            },
            ..user_composite.clone()
        };

        let auth =
            MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

        let db = MockDatabase::build(true);

        let user_repo = MockUserRepository::new()
            .with_get_composite(user_composite.user.id, Some(user_composite.clone()));

        let user_update =
            MockUserUpdateService::new().with_update_enabled(user_composite.user.id, enabled, true);

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
                UserIdOrSelf::UserId(user_composite.user.id),
                UserUpdateRequest {
                    user: UserUpdateUserRequest {
                        enabled: enabled.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }
}

#[tokio::test]
async fn disable_self() {
    // Arrange
    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo =
        MockUserRepository::new().with_get_composite(ADMIN.user.id, Some(ADMIN.clone()));

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
                    enabled: false.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await;

    // Assert
    assert_matches!(result, Err(UserUpdateError::CannotDisableSelf));
}
