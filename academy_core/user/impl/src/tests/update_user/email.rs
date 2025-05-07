use academy_auth_contracts::MockAuthService;
use academy_core_user_contracts::{
    UserFeatureService, UserUpdateError, UserUpdateRequest, UserUpdateUserRequest,
    update::{MockUserUpdateService, UserUpdateEmailError},
};
use academy_demo::{
    session::{ADMIN_1, FOO_1},
    user::{ADMIN, FOO},
};
use academy_models::user::{User, UserComposite, UserIdOrSelf};
use academy_persistence_contracts::{MockDatabase, user::MockUserRepository};
use academy_utils::assert_matches;

use crate::{UserFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn update_email_self() {
    // Arrange
    let expected = UserComposite {
        user: User {
            email: Some(ADMIN.user.email.clone().unwrap()),
            email_verified: false,
            ..FOO.user.clone()
        },
        ..FOO.clone()
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let user_update = MockUserUpdateService::new().with_update_email(
        FOO.user.id,
        expected.user.email.clone().unwrap(),
        expected.user.email_verified,
        Ok(true),
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
                    email: expected.user.email.clone().unwrap().into(),
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
async fn update_email_admin() {
    // Arrange
    let expected = UserComposite {
        user: User {
            email: Some(ADMIN.user.email.clone().unwrap()),
            email_verified: false,
            ..FOO.user.clone()
        },
        ..FOO.clone()
    };

    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let user_update = MockUserUpdateService::new().with_update_email(
        FOO.user.id,
        expected.user.email.clone().unwrap(),
        expected.user.email_verified,
        Ok(true),
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
            FOO.user.id.into(),
            UserUpdateRequest {
                user: UserUpdateUserRequest {
                    email: expected.user.email.clone().unwrap().into(),
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
async fn update_email_admin_verified() {
    // Arrange
    let expected = UserComposite {
        user: User {
            email: Some(ADMIN.user.email.clone().unwrap()),
            email_verified: true,
            ..FOO.user.clone()
        },
        ..FOO.clone()
    };

    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let user_update = MockUserUpdateService::new().with_update_email(
        FOO.user.id,
        expected.user.email.clone().unwrap(),
        expected.user.email_verified,
        Ok(true),
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
            FOO.user.id.into(),
            UserUpdateRequest {
                user: UserUpdateUserRequest {
                    email: expected.user.email.clone().unwrap().into(),
                    email_verified: true.into(),
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
async fn update_email_admin_unverified() {
    // Arrange
    let expected = UserComposite {
        user: User {
            email: Some(ADMIN.user.email.clone().unwrap()),
            email_verified: false,
            ..FOO.user.clone()
        },
        ..FOO.clone()
    };

    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let user_update = MockUserUpdateService::new().with_update_email(
        FOO.user.id,
        expected.user.email.clone().unwrap(),
        expected.user.email_verified,
        Ok(true),
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
            FOO.user.id.into(),
            UserUpdateRequest {
                user: UserUpdateUserRequest {
                    email: expected.user.email.clone().unwrap().into(),
                    email_verified: false.into(),
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
async fn update_set_email_verified() {
    // Arrange
    let expected = FOO.clone();

    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_get_composite(
        FOO.user.id,
        Some(UserComposite {
            user: User {
                email_verified: false,
                ..FOO.user.clone()
            },
            ..FOO.clone()
        }),
    );

    let user_update = MockUserUpdateService::new().with_update_email(
        FOO.user.id,
        expected.user.email.clone().unwrap(),
        expected.user.email_verified,
        Ok(true),
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
            FOO.user.id.into(),
            UserUpdateRequest {
                user: UserUpdateUserRequest {
                    email_verified: true.into(),
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
async fn update_set_email_unverified() {
    // Arrange
    let expected = UserComposite {
        user: User {
            email_verified: false,
            ..FOO.user.clone()
        },
        ..FOO.clone()
    };

    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let user_update = MockUserUpdateService::new().with_update_email(
        FOO.user.id,
        expected.user.email.clone().unwrap(),
        expected.user.email_verified,
        Ok(true),
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
            FOO.user.id.into(),
            UserUpdateRequest {
                user: UserUpdateUserRequest {
                    email_verified: false.into(),
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
async fn update_email_conflict() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let user_update = MockUserUpdateService::new().with_update_email(
        FOO.user.id,
        ADMIN.user.email.clone().unwrap(),
        false,
        Err(UserUpdateEmailError::Conflict),
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
                    email: ADMIN.user.email.clone().unwrap().into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await;

    // Assert
    assert_matches!(result, Err(UserUpdateError::EmailConflict));
}
