use academy_auth_contracts::MockAuthService;
use academy_core_withdrawal_contracts::{
    WithdrawalFeatureService, WithdrawalRecordConsentError, consent::MockWithdrawalConsentService,
};
use academy_demo::{UUID1, session::FOO_1, user::FOO};
use academy_models::{
    auth::{AuthError, AuthenticateError},
    withdrawal::{WithdrawalConsent, WithdrawalConsentDeclaration, WithdrawalSubject},
};
use academy_persistence_contracts::MockDatabase;
use academy_utils::assert_matches;

use crate::{WithdrawalFeatureServiceImpl, tests::Sut};

fn declaration() -> WithdrawalConsentDeclaration {
    WithdrawalConsentDeclaration {
        given: true,
        text_version: Some("2026-09".try_into().unwrap()),
    }
}

fn expected_consent() -> WithdrawalConsent {
    WithdrawalConsent {
        id: UUID1.into(),
        user_id: FOO.user.id,
        subject: WithdrawalSubject::Course,
        reference: Some("html".try_into().unwrap()),
        text_version: "2026-09".try_into().unwrap(),
        consented_at: FOO.user.created_at,
    }
}

#[tokio::test]
async fn ok() {
    // Arrange
    let expected = expected_consent();

    let db = MockDatabase::build(true);

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let withdrawal_consent = MockWithdrawalConsentService::new().with_record(expected.clone());

    let sut = WithdrawalFeatureServiceImpl {
        db,
        auth,
        withdrawal_consent,
    };

    // Act
    let result = sut
        .record_consent(
            &"token".into(),
            expected.subject,
            expected.reference.clone(),
            declaration(),
        )
        .await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn consent_not_given() {
    // Arrange
    let sut = Sut::default();

    // Act
    let result = sut
        .record_consent(
            &"token".into(),
            WithdrawalSubject::Course,
            None,
            WithdrawalConsentDeclaration {
                given: false,
                ..declaration()
            },
        )
        .await;

    // Assert
    assert_matches!(result, Err(WithdrawalRecordConsentError::ConsentMissing));
}

#[tokio::test]
async fn text_version_missing() {
    // Arrange
    let sut = Sut::default();

    // Act
    let result = sut
        .record_consent(
            &"token".into(),
            WithdrawalSubject::Course,
            None,
            WithdrawalConsentDeclaration {
                given: true,
                text_version: None,
            },
        )
        .await;

    // Assert
    assert_matches!(result, Err(WithdrawalRecordConsentError::ConsentMissing));
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(None);

    let sut = WithdrawalFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut
        .record_consent(
            &"token".into(),
            WithdrawalSubject::Course,
            None,
            declaration(),
        )
        .await;

    // Assert
    assert_matches!(
        result,
        Err(WithdrawalRecordConsentError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}
