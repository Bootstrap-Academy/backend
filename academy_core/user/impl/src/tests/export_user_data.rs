use std::time::Duration;

use academy_auth_contracts::MockAuthService;
use academy_cache_contracts::MockCacheService;
use academy_core_user_contracts::{
    UserExportError, UserFeatureService,
    export::{AccountDataExport, MockUserExportService, UserDataExport},
};
use academy_demo::{
    session::{ADMIN_1, BAR_1, FOO_1},
    user::{ADMIN, BAR, FOO},
};
use academy_extern_contracts::microservices::MockMicroservicesApiService;
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    user::UserIdOrSelf,
};
use academy_persistence_contracts::MockDatabase;
use academy_utils::assert_matches;
use serde_json::json;

use crate::{UserFeatureServiceImpl, tests::Sut};

fn account() -> AccountDataExport {
    AccountDataExport {
        user: FOO.clone(),
        sessions: Vec::new(),
        oauth2_links: Vec::new(),
        balance: Default::default(),
        transactions: Vec::new(),
        premium: None,
        premium_subscription: None,
        invoices: Vec::new(),
        contract_declarations: Vec::new(),
        withdrawal_consents: Vec::new(),
    }
}

fn services() -> std::collections::BTreeMap<String, serde_json::Value> {
    [("skills".to_owned(), json!({"xp": []}))].into()
}

fn rate_limit_key() -> String {
    format!("user_data_export_rate_limit_{}", *FOO.user.id)
}

/// The cache mock of a user who has not exported their data recently.
fn cache_not_limited() -> MockCacheService {
    MockCacheService::new()
        .with_get(rate_limit_key(), None::<bool>)
        .with_set(rate_limit_key(), true, Some(Duration::from_secs(600)))
}

#[tokio::test]
async fn ok_self() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_export = MockUserExportService::new().with_export(FOO.user.id, Some(account()));

    let microservices_api =
        MockMicroservicesApiService::new().with_export_user(FOO.user.id, services());

    let sut = UserFeatureServiceImpl {
        auth,
        cache: cache_not_limited(),
        db,
        user_export,
        microservices_api,
        ..Sut::default()
    };

    // Act
    let result = sut
        .export_user_data(&"token".into(), UserIdOrSelf::Slf)
        .await;

    // Assert
    assert_eq!(
        result.unwrap(),
        UserDataExport {
            account: account(),
            services: services()
        }
    );
}

#[tokio::test]
async fn ok_admin_is_not_rate_limited() {
    // Arrange
    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(false);

    let user_export = MockUserExportService::new().with_export(FOO.user.id, Some(account()));

    let microservices_api =
        MockMicroservicesApiService::new().with_export_user(FOO.user.id, services());

    // the cache mock has no expectations, so any access would fail the test
    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_export,
        microservices_api,
        ..Sut::default()
    };

    // Act
    let result = sut
        .export_user_data(&"token".into(), FOO.user.id.into())
        .await;

    // Assert
    result.unwrap();
}

#[tokio::test]
async fn rate_limited() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let cache = MockCacheService::new().with_get(rate_limit_key(), Some(true));

    let sut = UserFeatureServiceImpl {
        auth,
        cache,
        ..Sut::default()
    };

    // Act
    let result = sut
        .export_user_data(&"token".into(), UserIdOrSelf::Slf)
        .await;

    // Assert
    assert_matches!(result, Err(UserExportError::RateLimit));
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(None);

    let sut = UserFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut
        .export_user_data(&"token".into(), FOO.user.id.into())
        .await;

    // Assert
    assert_matches!(
        result,
        Err(UserExportError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

#[tokio::test]
async fn unauthorized() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((BAR.user.clone(), BAR_1.clone())));

    let sut = UserFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut
        .export_user_data(&"token".into(), FOO.user.id.into())
        .await;

    // Assert
    assert_matches!(
        result,
        Err(UserExportError::Auth(AuthError::Authorize(
            AuthorizeError::Admin
        )))
    );
}

#[tokio::test]
async fn not_found() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_export = MockUserExportService::new().with_export(FOO.user.id, None);

    let sut = UserFeatureServiceImpl {
        auth,
        cache: cache_not_limited(),
        db,
        user_export,
        ..Sut::default()
    };

    // Act
    let result = sut
        .export_user_data(&"token".into(), UserIdOrSelf::Slf)
        .await;

    // Assert
    assert_matches!(result, Err(UserExportError::NotFound));
}

/// An export that is missing the data of a microservice is never handed out.
#[tokio::test]
async fn microservice_error() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_export = MockUserExportService::new().with_export(FOO.user.id, Some(account()));

    let microservices_api = MockMicroservicesApiService::new().with_export_user_error(FOO.user.id);

    let sut = UserFeatureServiceImpl {
        auth,
        cache: cache_not_limited(),
        db,
        user_export,
        microservices_api,
        ..Sut::default()
    };

    // Act
    let result = sut
        .export_user_data(&"token".into(), UserIdOrSelf::Slf)
        .await;

    // Assert
    assert_matches!(result, Err(UserExportError::Other(_)));
}
