use academy_auth_contracts::MockAuthService;
use academy_core_user_contracts::{
    UserFeatureService, UserUpdateError, UserUpdateRequest, UserUpdateUserRequest,
    update::MockUserUpdateService,
};
use academy_demo::{
    UUID1,
    session::ADMIN_1,
    user::{ADMIN, FOO},
};
use academy_models::user::{User, UserComposite, UserIdOrSelf};
use academy_persistence_contracts::{MockDatabase, user::MockUserRepository};
use academy_utils::assert_matches;

use crate::{UserFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn update_admin() {
    let admin2 = UserComposite {
        user: User {
            id: UUID1.into(),
            ..ADMIN.user.clone()
        },
        ..ADMIN.clone()
    };

    for (admin, user_composite) in [(false, &admin2), (true, &*FOO)] {
        // Arrange
        let expected = UserComposite {
            user: User {
                admin,
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
            MockUserUpdateService::new().with_update_admin(user_composite.user.id, admin, true);

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
                        admin: admin.into(),
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
async fn demote_self() {
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
                    admin: false.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await;

    // Assert
    assert_matches!(result, Err(UserUpdateError::CannotDemoteSelf));
}
