use academy_auth_contracts::MockAuthService;
use academy_core_finance_contracts::invoice::{MockFinanceInvoiceService, PendingFinalStatement};
use academy_core_user_contracts::{UserDeleteError, UserFeatureService};
use academy_demo::{
    session::{ADMIN_1, BAR_1, FOO_1},
    user::{ADMIN, BAR, FOO},
};
use academy_extern_contracts::microservices::MockMicroservicesApiService;
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    finance::RETENTION_MARKER,
    session::SessionRefreshTokenHash,
    user::UserIdOrSelf,
};
use academy_persistence_contracts::{
    MockDatabase, finance::MockFinancialDocumentRepository, user::MockUserRepository,
};
use academy_utils::assert_matches;

use crate::{UserFeatureServiceImpl, tests::Sut};

// Intake commits separately even when the later erasure cannot commit.
fn deletion_database(erase_commits: bool) -> MockDatabase {
    let mut transactions = std::collections::VecDeque::new();
    for commit in [true, erase_commits] {
        let mut txn = academy_persistence_contracts::MockTransaction::new();
        if commit {
            txn.expect_commit()
                .once()
                .return_once(|| Box::pin(async { Ok(()) }));
        }
        transactions.push_back(txn);
    }
    let mut db = MockDatabase::new();
    db.expect_begin_transaction().times(2).returning(move || {
        let txn = transactions
            .pop_front()
            .expect("exactly intake then erasure transaction");
        Box::pin(async { Ok(txn) })
    });
    db
}

fn refresh_token_hashes() -> Vec<SessionRefreshTokenHash> {
    vec![
        academy_models::Sha256Hash([1; 32]).into(),
        academy_models::Sha256Hash([2; 32]).into(),
    ]
}

fn pending_final_statement() -> PendingFinalStatement {
    PendingFinalStatement {
        number: "S7".try_into().unwrap(),
        html: "<html>the statement</html>".into(),
    }
}

#[tokio::test]
async fn ok_self() {
    // Arrange
    let auth = MockAuthService::new()
        .with_authenticate(Some((FOO.user.clone(), FOO_1.clone())))
        // The access tokens are invalidated only after the deletion has been
        // committed, so the hashes are read while the sessions still exist.
        .with_list_refresh_token_hashes(FOO.user.id, refresh_token_hashes())
        .with_invalidate_access_tokens_of(refresh_token_hashes());

    let db = deletion_database(true);

    let user_repo = MockUserRepository::new()
        .with_record_deletion_request(FOO.user.id)
        .with_lock_for_deletion(FOO.user.id, true)
        .with_delete(FOO.user.id, true);

    // The unused share of the purchased Morphcoins is recorded before the
    // account is gone.
    let finance_invoice = MockFinanceInvoiceService::new()
        .with_create_final_statement(FOO.user.id, Some(pending_final_statement()))
        // The pdf is produced after the commit.
        .with_archive_final_statement(pending_final_statement());

    // Invoices and credit notes are kept, but no longer name the account.
    let document_repo = MockFinancialDocumentRepository::new().with_pseudonymize(
        FOO.user.id,
        vec![RETENTION_MARKER.into()],
        1,
    );

    let microservices_api = MockMicroservicesApiService::new().with_delete_user(FOO.user.id);

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        finance_invoice,
        document_repo,
        microservices_api,
        ..Sut::default()
    };

    // Act
    let result = sut.delete_user(&"token".into(), UserIdOrSelf::Slf).await;

    // Assert
    result.unwrap();
}

#[tokio::test]
async fn admin_cannot_erase_another_account_without_case_workflow() {
    let sut = UserFeatureServiceImpl {
        auth: MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone()))),
        ..Sut::default()
    };
    // No database, finance or distributed deletion call may run.
    assert_matches!(
        sut.delete_user(&"token".into(), FOO.user.id.into()).await,
        Err(UserDeleteError::ModerationRequired)
    );
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
    let result = sut.delete_user(&"token".into(), FOO.user.id.into()).await;

    // Assert
    assert_matches!(
        result,
        Err(UserDeleteError::Auth(AuthError::Authenticate(
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
    let result = sut.delete_user(&"token".into(), FOO.user.id.into()).await;

    // Assert
    assert_matches!(
        result,
        Err(UserDeleteError::Auth(AuthError::Authorize(
            AuthorizeError::Admin
        )))
    );
}

#[tokio::test]
async fn not_found() {
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));
    let db = deletion_database(false);
    let user_repo = MockUserRepository::new()
        .with_record_deletion_request(FOO.user.id)
        .with_lock_for_deletion(FOO.user.id, false);
    let finance_invoice = MockFinanceInvoiceService::new();
    let document_repo = MockFinancialDocumentRepository::new();

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        finance_invoice,
        document_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.delete_user(&"token".into(), FOO.user.id.into()).await;

    // Assert
    assert_matches!(result, Err(UserDeleteError::NotFound));
}

#[tokio::test]
async fn cache_failure_after_commit_does_not_skip_fanout_or_final_statement() {
    // Arrange
    let mut auth = MockAuthService::new()
        .with_authenticate(Some((FOO.user.clone(), FOO_1.clone())))
        // The access tokens are invalidated only after the deletion has been
        // committed, so the hashes are read while the sessions still exist.
        .with_list_refresh_token_hashes(FOO.user.id, refresh_token_hashes());
    auth.expect_invalidate_access_tokens_of()
        .once()
        .return_once(|_| {
            Box::pin(std::future::ready(Err(anyhow::anyhow!(
                "synthetic cache outage"
            ))))
        });

    let db = deletion_database(true);

    let user_repo = MockUserRepository::new()
        .with_record_deletion_request(FOO.user.id)
        .with_lock_for_deletion(FOO.user.id, true)
        .with_delete(FOO.user.id, true);

    // The unused share of the purchased Morphcoins is recorded before the
    // account is gone.
    let finance_invoice = MockFinanceInvoiceService::new()
        .with_create_final_statement(FOO.user.id, Some(pending_final_statement()))
        // The pdf is produced after the commit.
        .with_archive_final_statement(pending_final_statement());

    // Invoices and credit notes are kept, but no longer name the account.
    let document_repo = MockFinancialDocumentRepository::new().with_pseudonymize(
        FOO.user.id,
        vec![RETENTION_MARKER.into()],
        1,
    );

    let microservices_api = MockMicroservicesApiService::new().with_delete_user(FOO.user.id);

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        finance_invoice,
        document_repo,
        microservices_api,
        ..Sut::default()
    };

    // Act
    let result = sut.delete_user(&"token".into(), UserIdOrSelf::Slf).await;

    // Assert
    result.unwrap();
}
