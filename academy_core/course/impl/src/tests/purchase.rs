use academy_auth_contracts::MockAuthService;
use academy_core_coin_contracts::coin::{CoinAddCoinsError, MockCoinService};
use academy_core_course_contracts::{CourseFeatureService, CoursePurchaseError};
use academy_demo::{
    course::{COURSE1, COURSE2},
    session::{BAR_1, FOO_1},
    user::{BAR, FOO},
};
use academy_email_contracts::template::MockTemplateEmailService;
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    coin::Balance,
    course::{CourseUser, CourseUserPatch},
};
use academy_persistence_contracts::{
    MockDatabase, course::MockCourseRepository, user::MockUserRepository,
};
use academy_templates_contracts::CoursePurchaseConfirmationTemplate;
use academy_utils::{assert_matches, patch::PatchValue};

use crate::{
    CourseFeatureServiceImpl,
    tests::{Sut, course_data_repo},
};

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(None);

    let sut = CourseFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.purchase(&"token".into(), COURSE1.base.id.clone()).await;

    // Assert
    assert_matches!(
        result,
        Err(CoursePurchaseError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

#[tokio::test]
async fn email_not_verified() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((BAR.user.clone(), BAR_1.clone())));

    let sut = CourseFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.purchase(&"token".into(), COURSE1.base.id.clone()).await;

    // Assert
    assert_matches!(
        result,
        Err(CoursePurchaseError::Auth(AuthError::Authorize(
            AuthorizeError::EmailVerified
        )))
    );
}

#[tokio::test]
async fn course_not_found() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let sut = CourseFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.purchase(&"token".into(), COURSE1.base.id.clone()).await;

    // Assert
    assert_matches!(result, Err(CoursePurchaseError::CourseNotFound));
}

#[tokio::test]
async fn course_is_free() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let sut = CourseFeatureServiceImpl {
        auth,
        course_data_repo: course_data_repo(),
        ..Sut::default()
    };

    // Act
    let result = sut.purchase(&"token".into(), COURSE1.base.id.clone()).await;

    // Assert
    assert_matches!(result, Err(CoursePurchaseError::CourseIsFree));
}

#[tokio::test]
async fn already_purchased() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let course_repo = MockCourseRepository::new().with_get_course_user(
        COURSE2.base.id.clone(),
        FOO.user.id,
        CourseUser {
            course_id: COURSE2.base.id.clone(),
            user_id: FOO.user.id,
            purchased: true,
        },
    );

    let sut = CourseFeatureServiceImpl {
        auth,
        db,
        course_data_repo: course_data_repo(),
        course_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.purchase(&"token".into(), COURSE2.base.id.clone()).await;

    // Assert
    assert_matches!(result, Err(CoursePurchaseError::AlreadyPurchased));
}

#[tokio::test]
async fn not_enough_coins() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let course_repo = MockCourseRepository::new().with_get_course_user(
        COURSE2.base.id.clone(),
        FOO.user.id,
        CourseUser {
            course_id: COURSE2.base.id.clone(),
            user_id: FOO.user.id,
            purchased: false,
        },
    );

    let coin = MockCoinService::new().with_add_coins(
        FOO.user.id,
        -42,
        false,
        Some("Course \"Course 2\"".try_into().unwrap()),
        false,
        Err(CoinAddCoinsError::NotEnoughCoins),
    );

    let sut = CourseFeatureServiceImpl {
        auth,
        db,
        coin,
        course_data_repo: course_data_repo(),
        course_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.purchase(&"token".into(), COURSE2.base.id.clone()).await;

    // Assert
    assert_matches!(result, Err(CoursePurchaseError::NotEnoughCoins));
}

#[tokio::test]
async fn ok() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let course_repo = MockCourseRepository::new()
        .with_get_course_user(
            COURSE2.base.id.clone(),
            FOO.user.id,
            CourseUser {
                course_id: COURSE2.base.id.clone(),
                user_id: FOO.user.id,
                purchased: false,
            },
        )
        .with_update_course_user(
            COURSE2.base.id.clone(),
            FOO.user.id,
            CourseUserPatch {
                purchased: PatchValue::Update(true),
            },
        );

    let coin = MockCoinService::new().with_add_coins(
        FOO.user.id,
        -42,
        false,
        Some("Course \"Course 2\"".try_into().unwrap()),
        false,
        Ok(Balance::default()),
    );

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let template_email = MockTemplateEmailService::new()
        .with_send_course_purchase_confirmation_email(
            FOO.user
                .email
                .clone()
                .unwrap()
                .with_name(FOO.profile.display_name.clone().into_inner()),
            CoursePurchaseConfirmationTemplate {
                title: "Course 2".into(),
            },
            true,
        );

    let sut = CourseFeatureServiceImpl {
        auth,
        db,
        coin,
        template_email,
        user_repo,
        course_data_repo: course_data_repo(),
        course_repo,
    };

    // Act
    let result = sut.purchase(&"token".into(), COURSE2.base.id.clone()).await;

    // Assert
    result.unwrap();
}
