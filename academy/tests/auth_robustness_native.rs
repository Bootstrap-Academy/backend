//! Local invariants: production services, disposable PostgreSQL and owned Valkey.
use std::{sync::Arc, time::Duration};

use academy::environment::{ConfigProvider, Provider, types};
use academy_cache_contracts::CacheService;
use academy_core_session_contracts::{SessionFeatureService, session::SessionService};
use academy_core_user_contracts::UserFeatureService;
use academy_demo::user::{ADMIN, BAR, FOO};
use academy_di::Provide;
use academy_models::{VerificationCode, auth::Login, user::UserComposite};
use academy_persistence_contracts::{Database, Transaction};

#[path = "../../academy_persistence/postgres/tests/common/mod.rs"]
mod common;

async fn fixture() -> (Provider, types::Database, types::Cache) {
    let db = common::setup().await;
    let config = academy_config::load().unwrap();
    let expected_port = std::env::var("AUTH_REVIEW_VALKEY_PORT").unwrap();
    assert_eq!(
        config.cache.url,
        format!("redis://127.0.0.1:{expected_port}/0")
    );
    let cache = academy::cache::connect(&config.cache).await.unwrap();
    cache.clear().await.unwrap();
    let provider = Provider::new(
        ConfigProvider::new(&config).unwrap(),
        db.clone(),
        cache.clone(),
        academy_email_impl::EmailServiceImpl::dummy().await,
    );
    (provider, db, cache)
}

async fn login(
    provider: &mut Provider,
    db: &types::Database,
    user: &UserComposite,
    mfa: bool,
) -> Login {
    let service: types::Session = provider.provide();
    let mut txn = db.begin_transaction().await.unwrap();
    let result = service
        .create(&mut txn, user.clone(), None, false, mfa)
        .await
        .unwrap();
    txn.commit().await.unwrap();
    result
}

fn code() -> VerificationCode {
    "ABCD-EFGH-IJKL-MNOP".try_into().unwrap()
}

async fn reset_code(cache: &types::Cache) {
    cache
        .set(
            &format!("reset_password_code:v2:{}", FOO.user.id.hyphenated()),
            &(FOO.user.email.clone().unwrap(), code()),
            Some(Duration::from_secs(60)),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn reset_must_revoke_existing_access_and_refresh_tokens() {
    let (mut provider, db, cache) = fixture().await;
    let old = login(&mut provider, &db, &FOO, false).await;
    reset_code(&cache).await;
    let users: types::UserFeature = provider.provide();
    users
        .reset_password(
            FOO.user.email.clone().unwrap(),
            code(),
            "new-owned-review-password".try_into().unwrap(),
        )
        .await
        .unwrap();
    let sessions: types::SessionFeature = provider.provide();
    let access_still_valid = sessions
        .get_current_session(&old.access_token)
        .await
        .is_ok();
    let refresh_still_valid = sessions.refresh_session(&old.refresh_token).await.is_ok();
    assert!(
        !access_still_valid && !refresh_still_valid,
        "reset retained authority: access={access_still_valid}, refresh={refresh_still_valid}"
    );
}

#[tokio::test]
async fn reset_code_must_be_single_use_under_concurrency() {
    let (mut provider, db, cache) = fixture().await;
    reset_code(&cache).await;
    let users: Arc<types::UserFeature> = Arc::new(provider.provide());
    let blocker = db.begin_transaction().await.unwrap();
    blocker
        .txn()
        .query_one(
            "SELECT id FROM users WHERE id=$1 FOR UPDATE",
            &[&*FOO.user.id],
        )
        .await
        .unwrap();
    let first_service = Arc::clone(&users);
    let first = tokio::spawn(async move {
        first_service
            .reset_password(
                FOO.user.email.clone().unwrap(),
                code(),
                "first-owned-review-password".try_into().unwrap(),
            )
            .await
    });
    let second = tokio::spawn(async move {
        users
            .reset_password(
                FOO.user.email.clone().unwrap(),
                code(),
                "second-owned-review-password".try_into().unwrap(),
            )
            .await
    });
    // Both requests must wait at the owner lock before releasing the held row.
    // This rules out a timing-only reproduction of GET followed by DEL.
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let observer = db.begin_transaction().await.unwrap();
            let waiting: i64 = observer.txn().query_one("SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE '%FOR UPDATE%'", &[]).await.unwrap().get(0);
            if waiting >= 2 { break; }
            tokio::task::yield_now().await;
        }
    }).await.expect("two reset requests must be waiting at the owner lock");
    blocker.commit().await.unwrap();
    let winners =
        usize::from(first.await.unwrap().is_ok()) + usize::from(second.await.unwrap().is_ok());
    assert_eq!(
        winners, 1,
        "one reset code authorized multiple committed password changes"
    );
}

#[tokio::test]
async fn verification_code_must_stay_bound_to_original_account() {
    let (mut provider, db, cache) = fixture().await;
    cache
        .set(
            &format!("verification:v2:{}", *code()),
            &(FOO.user.id, FOO.user.email.clone().unwrap()),
            Some(Duration::from_secs(60)),
        )
        .await
        .unwrap();
    let txn = db.begin_transaction().await.unwrap();
    txn.txn()
        .execute(
            "UPDATE users SET email='review-new@example.com', email_verified=false WHERE id=$1",
            &[&*FOO.user.id],
        )
        .await
        .unwrap();
    txn.txn()
        .execute(
            "UPDATE users SET email=$2, email_verified=false WHERE id=$1",
            &[&*BAR.user.id, &FOO.user.email.as_ref().unwrap().as_str()],
        )
        .await
        .unwrap();
    txn.commit().await.unwrap();
    let users: types::UserFeature = provider.provide();
    let result = users.verify_email(code()).await;
    let txn = db.begin_transaction().await.unwrap();
    let other_verified: bool = txn
        .txn()
        .query_one(
            "SELECT email_verified FROM users WHERE id=$1",
            &[&*BAR.user.id],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        !other_verified,
        "a code issued for one account verified another account: accepted={}",
        result.is_ok()
    );
}

#[tokio::test]
async fn reset_code_must_be_invalid_after_email_change() {
    let (mut provider, db, cache) = fixture().await;
    reset_code(&cache).await;
    let txn = db.begin_transaction().await.unwrap();
    txn.txn()
        .execute(
            "UPDATE users SET email='review-changed@example.com', email_verified=false WHERE id=$1",
            &[&*FOO.user.id],
        )
        .await
        .unwrap();
    txn.commit().await.unwrap();
    let users: types::UserFeature = provider.provide();
    let result = users
        .reset_password(
            "review-changed@example.com".parse().unwrap(),
            code(),
            "owned-review-password".try_into().unwrap(),
        )
        .await;
    assert!(
        result.is_err(),
        "reset code issued to an old email still changed the account password"
    );
}

#[tokio::test]
async fn refresh_has_one_winner_and_logout_rejects_old_authority() {
    let (mut provider, db, _) = fixture().await;
    let old = login(&mut provider, &db, &FOO, false).await;
    let sessions: types::SessionFeature = provider.provide();
    let (a, b) = tokio::join!(
        sessions.refresh_session(&old.refresh_token),
        sessions.refresh_session(&old.refresh_token)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let winner = a.or(b).unwrap();
    assert!(
        sessions
            .get_current_session(&old.access_token)
            .await
            .is_err()
    );
    assert!(sessions.refresh_session(&old.refresh_token).await.is_err());
    assert!(
        sessions
            .get_current_session(&winner.access_token)
            .await
            .is_ok()
    );
    sessions
        .delete_current_session(&winner.access_token)
        .await
        .unwrap();
    assert!(
        sessions
            .get_current_session(&winner.access_token)
            .await
            .is_err()
    );
    assert!(
        sessions
            .refresh_session(&winner.refresh_token)
            .await
            .is_err()
    );
    // The cache never supplies authority after a flush.
    let cache: types::Cache = provider.provide();
    cache.clear().await.unwrap();
    assert!(
        sessions
            .get_current_session(&winner.access_token)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn admin_authority_requires_stored_mfa_and_revocation_is_immediate() {
    let (mut provider, db, _) = fixture().await;
    let weak = login(&mut provider, &db, &ADMIN, false).await;
    let strong = login(&mut provider, &db, &ADMIN, true).await;
    let users: types::UserFeature = provider.provide();
    assert!(
        users
            .get_user(&weak.access_token, FOO.user.id.into())
            .await
            .is_err()
    );
    assert!(
        users
            .get_user(&strong.access_token, FOO.user.id.into())
            .await
            .is_ok()
    );
    let txn = db.begin_transaction().await.unwrap();
    txn.txn()
        .execute(
            "UPDATE sessions SET mfa_verified=false WHERE id=$1",
            &[&*strong.session.id],
        )
        .await
        .unwrap();
    txn.commit().await.unwrap();
    assert!(
        users
            .get_user(&strong.access_token, FOO.user.id.into())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn concurrent_failures_must_not_lose_login_throttle_counts() {
    use academy_core_session_contracts::login_throttle::SessionLoginThrottleService;
    use academy_core_session_impl::login_throttle::{
        SessionLoginThrottleConfig, SessionLoginThrottleServiceImpl,
    };
    use academy_shared_impl::{hash::HashServiceImpl, time::TimeServiceImpl};
    let (_, _, cache) = fixture().await;
    let service = SessionLoginThrottleServiceImpl::new(
        TimeServiceImpl,
        HashServiceImpl,
        cache,
        SessionLoginThrottleConfig {
            fails_before_lock: 8,
            fail_window: Duration::from_secs(60),
            lock_initial: Duration::from_secs(60),
            lock_max: Duration::from_secs(60),
            fails_per_ip: 8,
            ip_window: Duration::from_secs(60),
        },
    );
    let name = academy_models::user::UserNameOrEmailAddress::Name(
        "owned-review-login".try_into().unwrap(),
    );
    let failures = (0..8).map(|_| service.record_account_failure(&name));
    for result in futures::future::join_all(failures).await {
        result.unwrap();
    }
    assert!(
        service
            .check(&name, "127.0.0.1".parse().unwrap())
            .await
            .is_err(),
        "eight completed concurrent failures did not trigger the eight-failure account lock"
    );
}

#[tokio::test]
async fn cache_must_not_log_single_use_credentials_in_key_fields() {
    use std::io;
    use std::sync::Mutex;
    use tracing::instrument::WithSubscriber;
    #[derive(Clone)]
    struct Capture(Arc<Mutex<Vec<u8>>>);
    impl io::Write for Capture {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let (_, _, cache) = fixture().await;
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let writer = Capture(Arc::clone(&bytes));
    // The application's formatter also emits events inside the instrumented spans.
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    let key = format!("verification:v2:{}", *code());
    cache
        .get::<String>(&key)
        .with_subscriber(subscriber)
        .await
        .unwrap();
    let logged = String::from_utf8(bytes.lock().unwrap().clone())
        .unwrap()
        .contains(&*code());
    assert!(
        !logged,
        "verification credential present in cache key log field with application-style TRACE formatting"
    );
}

#[tokio::test]
async fn expired_or_wrong_reset_code_never_changes_password() {
    let (mut provider, db, cache) = fixture().await;
    let users: types::UserFeature = provider.provide();
    let txn = db.begin_transaction().await.unwrap();
    let before: String = txn
        .txn()
        .query_one(
            "SELECT password_hash FROM user_passwords WHERE user_id=$1",
            &[&*FOO.user.id],
        )
        .await
        .unwrap()
        .get(0);
    txn.commit().await.unwrap();
    reset_code(&cache).await;
    assert!(
        users
            .reset_password(
                FOO.user.email.clone().unwrap(),
                "ZZZZ-ZZZZ-ZZZZ-ZZZZ".try_into().unwrap(),
                "owned-invalid-attempt".try_into().unwrap()
            )
            .await
            .is_err()
    );
    cache
        .remove(&format!(
            "reset_password_code:v2:{}",
            FOO.user.id.hyphenated()
        ))
        .await
        .unwrap();
    assert!(
        users
            .reset_password(
                FOO.user.email.clone().unwrap(),
                code(),
                "owned-expired-attempt".try_into().unwrap()
            )
            .await
            .is_err()
    );
    let txn = db.begin_transaction().await.unwrap();
    let after: String = txn
        .txn()
        .query_one(
            "SELECT password_hash FROM user_passwords WHERE user_id=$1",
            &[&*FOO.user.id],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        before == after,
        "rejected reset changed the stored password"
    );
}

#[tokio::test]
async fn account_password_change_revokes_every_session() {
    use academy_core_user_contracts::{PasswordUpdate, UserUpdateRequest, UserUpdateUserRequest};
    use academy_utils::patch::PatchValue;
    let (mut provider, db, _) = fixture().await;
    let first = login(&mut provider, &db, &FOO, false).await;
    let second = login(&mut provider, &db, &FOO, false).await;
    let users: types::UserFeature = provider.provide();
    users
        .update_user(
            &first.access_token,
            FOO.user.id.into(),
            UserUpdateRequest {
                user: UserUpdateUserRequest {
                    password: PatchValue::Update(PasswordUpdate::Change(
                        "owned-new-password".try_into().unwrap(),
                    )),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let sessions: types::SessionFeature = provider.provide();
    for old in [first, second] {
        assert!(
            sessions
                .get_current_session(&old.access_token)
                .await
                .is_err(),
            "password change retained access authority"
        );
        assert!(
            sessions.refresh_session(&old.refresh_token).await.is_err(),
            "password change retained refresh authority"
        );
    }
}

#[tokio::test]
async fn reset_failure_preserves_password_and_sessions() {
    let (mut provider, db, cache) = fixture().await;
    let old = login(&mut provider, &db, &FOO, false).await;
    reset_code(&cache).await;
    let txn = db.begin_transaction().await.unwrap();
    let before: String = txn
        .txn()
        .query_one(
            "SELECT password_hash FROM user_passwords WHERE user_id=$1",
            &[&*FOO.user.id],
        )
        .await
        .unwrap()
        .get(0);
    txn.txn().batch_execute("CREATE FUNCTION auth_review_fail_delete() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'owned rollback probe'; END $$; CREATE TRIGGER auth_review_fail_delete BEFORE DELETE ON sessions FOR EACH ROW EXECUTE FUNCTION auth_review_fail_delete();").await.unwrap();
    txn.commit().await.unwrap();
    let users: types::UserFeature = provider.provide();
    let result = users
        .reset_password(
            FOO.user.email.clone().unwrap(),
            code(),
            "owned-rollback-password".try_into().unwrap(),
        )
        .await;
    assert!(result.is_err(), "reset ignored failed session revocation");
    let txn = db.begin_transaction().await.unwrap();
    let after: String = txn
        .txn()
        .query_one(
            "SELECT password_hash FROM user_passwords WHERE user_id=$1",
            &[&*FOO.user.id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(before, after, "failed reset committed its new password");
    txn.commit().await.unwrap();
    let sessions: types::SessionFeature = provider.provide();
    assert!(
        sessions
            .get_current_session(&old.access_token)
            .await
            .is_ok(),
        "rollback invalidated access authority"
    );
    assert!(
        sessions.refresh_session(&old.refresh_token).await.is_ok(),
        "rollback invalidated refresh authority"
    );
}

#[tokio::test]
async fn revoked_session_waiting_for_account_lock_cannot_change_password() {
    use academy_core_user_contracts::{PasswordUpdate, UserUpdateRequest, UserUpdateUserRequest};
    use academy_utils::patch::PatchValue;
    let (mut provider, db, _) = fixture().await;
    let old = login(&mut provider, &db, &FOO, false).await;
    let users: types::UserFeature = provider.provide();
    let blocker = db.begin_transaction().await.unwrap();
    blocker
        .txn()
        .query_one(
            "SELECT id FROM users WHERE id=$1 FOR UPDATE",
            &[&*FOO.user.id],
        )
        .await
        .unwrap();
    let mutation = tokio::spawn(async move {
        users
            .update_user(
                &old.access_token,
                FOO.user.id.into(),
                UserUpdateRequest {
                    user: UserUpdateUserRequest {
                        password: PatchValue::Update(PasswordUpdate::Change(
                            "owned-stale-password".try_into().unwrap(),
                        )),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let observer = db.begin_transaction().await.unwrap();
            let waiting: i64 = observer.txn().query_one("SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE '%FOR UPDATE%'", &[]).await.unwrap().get(0);
            if waiting >= 1 { break; }
            tokio::task::yield_now().await;
        }
    }).await.unwrap();
    blocker
        .txn()
        .execute("DELETE FROM sessions WHERE user_id=$1", &[&*FOO.user.id])
        .await
        .unwrap();
    blocker.commit().await.unwrap();
    assert!(
        mutation.await.unwrap().is_err(),
        "revoked session changed password after waiting for account lock"
    );
}

#[tokio::test]
async fn password_login_waiting_for_reset_must_recheck_password() {
    use academy_core_session_contracts::SessionCreateCommand;
    use academy_demo::user::FOO_PASSWORD;
    use academy_models::{mfa::MfaAuthentication, user::UserNameOrEmailAddress};
    let (mut provider, db, _) = fixture().await;
    let sessions: types::SessionFeature = provider.provide();
    let blocker = db.begin_transaction().await.unwrap();
    blocker
        .txn()
        .query_one(
            "SELECT id FROM users WHERE id=$1 FOR UPDATE",
            &[&*FOO.user.id],
        )
        .await
        .unwrap();
    let pending = tokio::spawn(async move {
        sessions
            .create_session(
                "127.0.0.1".parse().unwrap(),
                SessionCreateCommand {
                    name_or_email: UserNameOrEmailAddress::Name(FOO.user.name.clone()),
                    password: FOO_PASSWORD.clone(),
                    mfa: MfaAuthentication::default(),
                    device_name: None,
                },
                None,
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let observer = db.begin_transaction().await.unwrap();
            let waiting: i64 = observer.txn().query_one("SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE '%FOR UPDATE%'", &[]).await.unwrap().get(0);
            if waiting >= 1 { break; }
            tokio::task::yield_now().await;
        }
    }).await.unwrap();
    // A different, valid Argon2 hash stands in for the reset's committed password.
    blocker.txn().execute("UPDATE user_passwords SET password_hash=(SELECT password_hash FROM user_passwords WHERE user_id=$2) WHERE user_id=$1", &[&*FOO.user.id, &*BAR.user.id]).await.unwrap();
    blocker.commit().await.unwrap();
    assert!(
        pending.await.unwrap().is_err(),
        "login verified the old password before waiting for reset"
    );
}
