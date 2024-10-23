use academy_auth_contracts::MockAuthService;
use academy_core_user_contracts::{
    update::MockUserUpdateService, UserFeatureService, UserUpdateError, UserUpdateRequest,
};
use academy_demo::{
    session::BAR_1,
    user::{BAR, FOO},
};
use academy_extern_contracts::vat::MockVatApiService;
use academy_models::user::{UserComposite, UserIdOrSelf, UserInvoiceInfo};
use academy_persistence_contracts::{
    coin::MockCoinRepository, user::MockUserRepository, MockDatabase,
};
use academy_utils::{assert_matches, patch::Patch, Apply};

use crate::{tests::Sut, UserFeatureServiceImpl};

#[tokio::test]
async fn ok() {
    // Arrange
    let expected = UserComposite {
        invoice_info: UserInvoiceInfo {
            business: Some(false),
            country: Some("Germany".try_into().unwrap()),
            ..Default::default()
        },
        ..BAR.clone().with(|u| u.user.email_verified = true)
    };

    let auth = MockAuthService::new().with_authenticate(Some((BAR.user.clone(), BAR_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_get_composite(
        BAR.user.id,
        Some(BAR.clone().with(|u| u.user.email_verified = true)),
    );

    let user_update = MockUserUpdateService::new().with_update_invoice_info(
        BAR.user.id,
        BAR.invoice_info.clone(),
        academy_models::user::UserInvoiceInfoPatch::new()
            .update_business(expected.invoice_info.business)
            .update_country(expected.invoice_info.country.clone()),
        expected.invoice_info.clone(),
    );

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        user_update,
        ..Sut::default()
    };

    // Act
    let result = sut
        .update_user(
            &"token".into(),
            UserIdOrSelf::Slf,
            UserUpdateRequest {
                invoice_info: expected.invoice_info.clone(),
                ..Default::default()
            },
        )
        .await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn ok_release_coins() {
    // Arrange
    let expected = UserComposite {
        invoice_info: FOO.invoice_info.clone(),
        ..BAR.clone().with(|u| u.user.email_verified = true)
    };

    let auth = MockAuthService::new().with_authenticate(Some((BAR.user.clone(), BAR_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_get_composite(
        BAR.user.id,
        Some(BAR.clone().with(|u| u.user.email_verified = true)),
    );

    let user_update = MockUserUpdateService::new().with_update_invoice_info(
        BAR.user.id,
        BAR.invoice_info.clone(),
        FOO.invoice_info.clone().into_patch(),
        FOO.invoice_info.clone(),
    );

    let vat_api = MockVatApiService::new()
        .with_is_vat_id_valid(FOO.invoice_info.vat_id.clone().unwrap().into_inner(), true);

    let coin_repo = MockCoinRepository::new().with_release_coins(BAR.user.id);

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        user_update,
        vat_api,
        coin_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .update_user(
            &"token".into(),
            UserIdOrSelf::Slf,
            UserUpdateRequest {
                invoice_info: expected.invoice_info.clone(),
                ..Default::default()
            },
        )
        .await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn invalid_vat_id() {
    // Arrange
    let expected = UserComposite {
        invoice_info: UserInvoiceInfo {
            business: Some(true),
            vat_id: Some("DE1234".try_into().unwrap()),
            ..Default::default()
        },
        ..BAR.clone()
    };

    let auth = MockAuthService::new().with_authenticate(Some((BAR.user.clone(), BAR_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(BAR.user.id, Some(BAR.clone()));

    let vat_api = MockVatApiService::new().with_is_vat_id_valid("DE1234".into(), false);

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        vat_api,
        ..Sut::default()
    };

    // Act
    let result = sut
        .update_user(
            &"token".into(),
            UserIdOrSelf::Slf,
            UserUpdateRequest {
                invoice_info: expected.invoice_info.clone(),
                ..Default::default()
            },
        )
        .await;

    // Assert
    assert_matches!(result, Err(UserUpdateError::InvalidVatId));
}
