use academy_demo::user::{ALL_USERS, FOO};
use academy_models::{
    publication::{NOTICE_HASH, ProfileVisibility, PublicationChoice, SCOPE_VERSION},
    user::UserId,
};
use academy_persistence_contracts::{
    Database, Transaction,
    publication::{PublicationRepository, PublicationWriteError},
    user::UserRepository,
};
use academy_persistence_postgres::{
    PostgresDatabase, publication::PostgresPublicationRepository, user::PostgresUserRepository,
};
use uuid::Uuid;

use crate::common::{setup, setup_before};

const MIGRATION: &str = "2026-10-03-140000_profile_publications";
const REPO: PostgresPublicationRepository = PostgresPublicationRepository;

fn choice(visibility: ProfileVisibility, revision: i64) -> PublicationChoice {
    PublicationChoice {
        profile_visibility: visibility,
        expected_revision: revision,
        request_id: Uuid::new_v4(),
        scope_version: Some(SCOPE_VERSION.into()),
        notice_hash: Some(NOTICE_HASH.into()),
        preview_token: None,
    }
}

async fn activate(db: &PostgresDatabase) {
    db.execute("UPDATE profile_publication_state SET policy_active=true")
        .await
        .unwrap();
}

async fn apply(
    db: &PostgresDatabase,
    user_id: UserId,
    choice: &PublicationChoice,
) -> Result<academy_models::publication::PublicationChoiceResult, PublicationWriteError> {
    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.choose(&mut txn, user_id, choice, true, true).await?;
    txn.commit().await.unwrap();
    Ok(result)
}

#[tokio::test]
async fn additive_migration_preserves_old_accounts_and_api_and_reapplies() {
    let db = setup_before(MIGRATION, true).await;
    db.execute(&format!(
        "UPDATE user_profiles SET leaderboard_opt_out=true WHERE user_id='{}'",
        *FOO.user.id
    ))
    .await
    .unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    let mut accounts = Vec::new();
    for user in ALL_USERS.iter() {
        accounts.push(
            PostgresUserRepository
                .get_composite(&mut txn, user.user.id)
                .await
                .unwrap()
                .unwrap(),
        );
    }
    txn.rollback().await.unwrap();
    assert_eq!(db.run_migrations(None).await.unwrap(), [MIGRATION]);
    let mut txn = db.begin_transaction().await.unwrap();
    for account in &accounts {
        assert_eq!(
            PostgresUserRepository
                .get_composite(&mut txn, account.user.id)
                .await
                .unwrap()
                .as_ref(),
            Some(account)
        );
        let setting = REPO
            .settings(&mut txn, account.user.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(setting.profile_visibility, ProfileVisibility::Private);
        assert_eq!(setting.visibility_revision, 0);
        assert!(setting.last_shared_receipt.is_none());
    }
    assert!(!REPO.epoch(&mut txn, true).await.unwrap().publishing_enabled);
    txn.rollback().await.unwrap();
    assert_eq!(db.revert_migrations(Some(1)).await.unwrap(), [MIGRATION]);
    assert_eq!(db.run_migrations(None).await.unwrap(), [MIGRATION]);
    let mut txn = db.begin_transaction().await.unwrap();
    // All alternate account creation paths use this repository or the same DB default.
    let mut imported = FOO.clone();
    imported.user.id = Uuid::new_v4().into();
    imported.user.name = "publication_import".try_into().unwrap();
    imported.user.email = Some("publication-import@example.com".parse().unwrap());
    PostgresUserRepository
        .create(
            &mut txn,
            &imported.user,
            &imported.profile,
            &imported.invoice_info,
        )
        .await
        .unwrap();
    assert_eq!(
        REPO.settings(&mut txn, imported.user.id)
            .await
            .unwrap()
            .unwrap()
            .profile_visibility,
        ProfileVisibility::Private
    );
}

#[tokio::test]
async fn receipts_cas_and_retries_never_undo_withdrawal() {
    let db = setup().await;
    activate(&db).await;
    let share = choice(ProfileVisibility::Shared, 1);
    let first = apply(&db, FOO.user.id, &share).await.unwrap();
    assert_eq!(first.current.visibility_revision, 2);
    let retry = apply(&db, FOO.user.id, &share).await.unwrap();
    assert!(retry.replayed);
    assert_eq!(retry.receipt, first.receipt);
    let mut changed_request = share.clone();
    changed_request.profile_visibility = ProfileVisibility::Private;
    assert!(matches!(
        apply(&db, FOO.user.id, &changed_request).await,
        Err(PublicationWriteError::Conflict)
    ));
    let private = choice(ProfileVisibility::Private, 2);
    let withdrawn = apply(&db, FOO.user.id, &private).await.unwrap();
    assert_eq!(withdrawn.current.visibility_revision, 3);
    let old_share = apply(&db, FOO.user.id, &share).await.unwrap();
    assert!(old_share.replayed);
    assert_eq!(old_share.receipt, first.receipt);
    assert_eq!(
        old_share.current.profile_visibility,
        ProfileVisibility::Private
    );
    assert_eq!(old_share.current.visibility_revision, 3);
    assert!(matches!(
        apply(&db, FOO.user.id, &choice(ProfileVisibility::Shared, 2)).await,
        Err(PublicationWriteError::Conflict)
    ));
    assert!(apply(&db, FOO.user.id, &private).await.unwrap().replayed);
    // A discarded earlier request still cannot replay against a later revision.
    apply(&db, FOO.user.id, &choice(ProfileVisibility::Shared, 3))
        .await
        .unwrap();
    assert!(matches!(
        apply(&db, FOO.user.id, &share).await,
        Err(PublicationWriteError::Conflict)
    ));
}

#[tokio::test]
async fn concurrent_tabs_allow_only_one_cas_winner_and_rollback_is_atomic() {
    let db = setup().await;
    activate(&db).await;
    let mut jobs = Vec::new();
    for _ in 0..8 {
        let db = db.clone();
        jobs.push(tokio::spawn(async move {
            apply(&db, FOO.user.id, &choice(ProfileVisibility::Shared, 1)).await
        }));
    }
    let mut winners = 0;
    for job in jobs {
        match job.await.unwrap() {
            Ok(_) => winners += 1,
            Err(PublicationWriteError::Conflict) => {}
            result => panic!("Unexpected CAS result: {result:?}"),
        }
    }
    assert_eq!(winners, 1);
    let mut txn = db.begin_transaction().await.unwrap();
    let epoch = REPO.epoch(&mut txn, true).await.unwrap();
    txn.rollback().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    REPO.choose(
        &mut txn,
        FOO.user.id,
        &choice(ProfileVisibility::Private, 2),
        true,
        true,
    )
    .await
    .unwrap();
    txn.rollback().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(REPO.epoch(&mut txn, true).await.unwrap(), epoch);
    assert_eq!(
        REPO.settings(&mut txn, FOO.user.id)
            .await
            .unwrap()
            .unwrap()
            .profile_visibility,
        ProfileVisibility::Shared
    );
}

#[tokio::test]
async fn snapshots_are_minimal_and_invalidate_identity_verification_and_deletion() {
    let db = setup().await;
    activate(&db).await;
    apply(&db, FOO.user.id, &choice(ProfileVisibility::Shared, 1))
        .await
        .unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    let snapshot = REPO.snapshot(&mut txn, true).await.unwrap();
    assert_eq!(snapshot.participants.len(), 1);
    let identity = serde_json::to_value(&snapshot.participants[0]).unwrap();
    assert_eq!(identity.as_object().unwrap().len(), 4);
    assert!(identity["avatar_url"].is_null());
    assert_eq!(snapshot.participants[0].user_id, FOO.user.id);
    txn.rollback().await.unwrap();
    db.execute(&format!(
        "UPDATE user_profiles SET display_name='Shared new name' WHERE user_id='{}'",
        *FOO.user.id
    ))
    .await
    .unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    let renamed = REPO.snapshot(&mut txn, true).await.unwrap();
    assert_ne!(
        renamed.epoch.publication_epoch,
        snapshot.epoch.publication_epoch
    );
    assert_eq!(&*renamed.participants[0].display_name, "Shared new name");
    txn.rollback().await.unwrap();
    db.execute(&format!(
        "UPDATE users SET email_verified=false WHERE id='{}'",
        *FOO.user.id
    ))
    .await
    .unwrap();
    assert!(matches!(
        apply(&db, FOO.user.id, &choice(ProfileVisibility::Shared, 2)).await,
        Err(PublicationWriteError::Unverified)
    ));
    let mut txn = db.begin_transaction().await.unwrap();
    let unverified = REPO.snapshot(&mut txn, true).await.unwrap();
    assert!(unverified.participants.is_empty());
    assert_ne!(
        unverified.epoch.publication_epoch,
        renamed.epoch.publication_epoch
    );
    txn.rollback().await.unwrap();
    // Losing verification never prevents withdrawal.
    apply(&db, FOO.user.id, &choice(ProfileVisibility::Private, 2))
        .await
        .unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    assert!(
        PostgresUserRepository
            .get_publication(&mut txn, FOO.user.id)
            .await
            .unwrap()
            .unwrap()
            .last_private_receipt
            .is_some()
    );
    PostgresUserRepository
        .delete(&mut txn, FOO.user.id)
        .await
        .unwrap();
    assert!(
        REPO.settings(&mut txn, FOO.user.id)
            .await
            .unwrap()
            .is_none()
    );
    txn.commit().await.unwrap();
}

#[tokio::test]
async fn invalid_scope_and_preview_do_not_write_or_grant_and_disabled_policy_stays_private() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();
    assert!(matches!(
        REPO.choose(
            &mut txn,
            FOO.user.id,
            &choice(ProfileVisibility::Shared, 0),
            true,
            true
        )
        .await,
        Err(PublicationWriteError::Disabled)
    ));
    txn.rollback().await.unwrap();
    activate(&db).await;
    let mut txn = db.begin_transaction().await.unwrap();
    let epoch = REPO.epoch(&mut txn, true).await.unwrap();
    assert!(matches!(
        REPO.choose(
            &mut txn,
            FOO.user.id,
            &choice(ProfileVisibility::Shared, 1),
            true,
            false
        )
        .await,
        Err(PublicationWriteError::InvalidPreview)
    ));
    let mut old_scope = choice(ProfileVisibility::Shared, 1);
    old_scope.scope_version = Some("obsolete".into());
    assert!(matches!(
        REPO.choose(&mut txn, FOO.user.id, &old_scope, true, true)
            .await,
        Err(PublicationWriteError::InvalidPreview)
    ));
    assert_eq!(REPO.epoch(&mut txn, true).await.unwrap(), epoch);
    txn.rollback().await.unwrap();
    apply(&db, FOO.user.id, &choice(ProfileVisibility::Shared, 1))
        .await
        .unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    let off = REPO.snapshot(&mut txn, false).await.unwrap();
    assert!(off.epoch.policy_active);
    assert!(!off.epoch.publishing_enabled);
    assert!(off.participants.is_empty());
    txn.rollback().await.unwrap();
    assert!(
        db.execute("UPDATE profile_publication_state SET policy_active=false")
            .await
            .is_err()
    );
    super::assert_down_refused(&db, MIGRATION, "Activated privacy policy").await;
}

#[tokio::test]
async fn active_legacy_clients_can_withdraw_but_cannot_share() {
    let db = setup().await;
    activate(&db).await;
    db.execute(&format!(
        "UPDATE user_profiles SET leaderboard_opt_out=false WHERE user_id='{}'",
        *FOO.user.id
    ))
    .await
    .unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    assert!(
        PostgresUserRepository
            .get_composite(&mut txn, FOO.user.id)
            .await
            .unwrap()
            .unwrap()
            .profile
            .leaderboard_opt_out
    );
    txn.rollback().await.unwrap();
    apply(&db, FOO.user.id, &choice(ProfileVisibility::Shared, 1))
        .await
        .unwrap();
    db.execute(&format!(
        "UPDATE user_profiles SET leaderboard_opt_out=true WHERE user_id='{}'",
        *FOO.user.id
    ))
    .await
    .unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    let withdrawn = REPO.settings(&mut txn, FOO.user.id).await.unwrap().unwrap();
    assert_eq!(withdrawn.profile_visibility, ProfileVisibility::Private);
    assert_eq!(withdrawn.visibility_revision, 3);
    assert_eq!(
        withdrawn.last_private_receipt.unwrap().source,
        "legacy_opt_out"
    );
    assert!(
        REPO.snapshot(&mut txn, true)
            .await
            .unwrap()
            .participants
            .is_empty()
    );
    txn.rollback().await.unwrap();
    // Direct defaults and incomplete receipts cannot manufacture sharing.
    assert!(db.execute(&format!("UPDATE user_profiles SET profile_visibility='shared',visibility_revision=4,last_shared_receipt='{{}}' WHERE user_id='{}'", *FOO.user.id)).await.is_err());
}
