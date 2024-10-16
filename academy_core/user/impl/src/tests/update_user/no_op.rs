use academy_core_auth_contracts::MockAuthService;
use academy_core_user_contracts::UserService;
use academy_demo::{
    session::{ADMIN_1, FOO_1},
    user::{ADMIN, FOO},
};
use academy_models::user::UserIdOrSelf;
use academy_persistence_contracts::{user::MockUserRepository, MockDatabase};

use crate::{tests::Sut, UserServiceImpl};

#[tokio::test]
async fn no_op_self() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let sut = UserServiceImpl {
        auth,
        db,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .update_user("token", UserIdOrSelf::Slf, Default::default())
        .await;

    // Assert
    assert_eq!(result.unwrap(), *FOO);
}

#[tokio::test]
async fn no_op_admin() {
    // Arrange
    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

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
    assert_eq!(result.unwrap(), *FOO);
}
