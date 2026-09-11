//! Serial functional tests, restricted to an explicitly owned disposable fixture.
//! Synthetic proof hashes exercise the repository/SQL boundary, not HTTP authentication.
mod common;

use academy_demo::user::FOO;
use academy_persistence_contracts::{
    Database, Transaction,
    moderation::{ModerationConflict, ModerationRepository},
};
use academy_persistence_postgres::{
    PostgresDatabase, moderation::PostgresModerationRepository as Repo,
};
use serde_json::{Value, json};
use uuid::Uuid;

const CLAIM: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const RIGHTS: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

async fn setup() -> (PostgresDatabase, Uuid, Uuid) {
    let db = common::setup().await;
    let txn = db.begin_transaction().await.unwrap();
    let case: Uuid = txn
        .txn()
        .query_one("SELECT commercial_lock_subject($1)", &[&*FOO.user.id])
        .await
        .unwrap()
        .get(0);
    txn.txn().execute("UPDATE commercial_cases SET contact='synthetic@example.invalid',contact_verified=true WHERE id=$1", &[&case]).await.unwrap();
    txn.txn().execute("INSERT INTO commercial_access_keys(hash,case_id,epoch,provenance) VALUES($1,$2,1,'{\"test\":true}')", &[&CLAIM,&case]).await.unwrap();
    txn.txn().execute("INSERT INTO moderation_capabilities(hash,subject,scope,expires_at) VALUES($1,$2,'rights',clock_timestamp()+interval '1 hour')", &[&RIGHTS,&*FOO.user.id]).await.unwrap();
    txn.commit().await.unwrap();
    let first = call(&db, "learning_start", json!({"command_id":Uuid::new_v4(), "hash":hash(), "_claim_hash":CLAIM,"use_retained_value":true})).await.unwrap();
    (
        db,
        case,
        Uuid::parse_str(first["subject"].as_str().unwrap()).unwrap(),
    )
}
fn hash() -> String {
    Uuid::new_v4().simple().to_string().repeat(2)
}
fn body(subject: Uuid) -> Value {
    json!({"command_id":Uuid::new_v4(),"expected_subject":subject,"hash":hash(),"_claim_hash":CLAIM})
}
async fn call(db: &PostgresDatabase, operation: &str, body: Value) -> anyhow::Result<Value> {
    let mut txn = db.begin_transaction().await?;
    let result = Repo
        .commercial_operation(&mut txn, operation, Some(FOO.user.id), &body)
        .await?;
    txn.commit().await?;
    Ok(result)
}
async fn snapshot(db: &PostgresDatabase) -> Value {
    let txn = db.begin_transaction().await.unwrap();
    let row = txn.txn().query_one("SELECT jsonb_build_object('subjects',(SELECT jsonb_agg(s ORDER BY subject) FROM commercial_learning_subjects s),'keys',(SELECT jsonb_agg(k ORDER BY hash) FROM commercial_learning_keys k),'journal',(SELECT jsonb_agg(j ORDER BY command_id) FROM commercial_journal j),'cases',(SELECT jsonb_agg(c ORDER BY id) FROM commercial_cases c))::text", &[]).await.unwrap();
    serde_json::from_str(row.get(0)).unwrap()
}
async fn refused(db: &PostgresDatabase, value: Value) {
    let before = snapshot(db).await;
    let error = call(db, "learning_access", value)
        .await
        .expect_err("a new command must not select an unrecorded subject");
    assert!(
        error.is::<ModerationConflict>(),
        "expected existing protocol conflict: {error:?}"
    );
    assert_eq!(
        snapshot(db).await,
        before,
        "refusal must preserve subject, key, epoch and journal records"
    );
}

#[tokio::test]
async fn fresh_refresh_requires_exact_well_formed_selected_subject() {
    let (db, _, subject) = setup().await;
    for invalid in [
        Value::Null,
        json!(123),
        json!("invalid"),
        json!(Uuid::new_v4()),
    ] {
        let mut request = body(subject);
        request["expected_subject"] = invalid;
        refused(&db, request).await;
    }
    let mut missing = body(subject);
    missing.as_object_mut().unwrap().remove("expected_subject");
    refused(&db, missing).await;
}

#[tokio::test]
async fn replacement_election_cannot_expand_an_earlier_selected_subject() {
    let (db, _, subject) = setup().await;
    let request = body(subject);
    let txn = db.begin_transaction().await.unwrap();
    txn.txn()
        .execute(
            "UPDATE commercial_learning_subjects SET erased_at=clock_timestamp() WHERE subject=$1",
            &[&subject],
        )
        .await
        .unwrap();
    txn.commit().await.unwrap();
    let replacement=call(&db,"learning_start",json!({"command_id":Uuid::new_v4(),"hash":hash(),"_claim_hash":CLAIM,"use_retained_value":true})).await.unwrap();
    assert_ne!(replacement["subject"], json!(subject));
    refused(&db, request).await;
}

#[tokio::test]
async fn same_subject_receipt_replays_before_current_selection_but_not_without_personal_proof() {
    let (db, _, subject) = setup().await;
    let request = body(subject);
    let receipt = call(&db, "learning_access", request.clone()).await.unwrap();
    assert_eq!(receipt["subject"], json!(subject));
    assert_eq!(receipt["purpose"], "retained_learning");
    for flag in [
        "ordinary_authority",
        "financial_authority",
        "claims_satisfied",
    ] {
        assert_eq!(receipt[flag], false);
    }
    let txn = db.begin_transaction().await.unwrap();
    txn.txn().execute("UPDATE commercial_learning_subjects SET erased_at=clock_timestamp(),authority_epoch=authority_epoch+1 WHERE subject=$1", &[&subject]).await.unwrap();
    txn.commit().await.unwrap();
    let before = snapshot(&db).await;
    assert_eq!(
        call(&db, "learning_access", request.clone()).await.unwrap(),
        receipt
    );
    assert_eq!(snapshot(&db).await, before);
    let mut changed = request.clone();
    changed["hash"] = json!(hash());
    refused(&db, changed).await;
    let mut changed = request.clone();
    changed["expected_subject"] = json!(Uuid::new_v4());
    refused(&db, changed).await;
    let mut absent = request.clone();
    absent.as_object_mut().unwrap().remove("_claim_hash");
    refused(&db, absent).await;
    let txn = db.begin_transaction().await.unwrap();
    txn.txn()
        .execute(
            "UPDATE commercial_access_keys SET revoked_at=clock_timestamp() WHERE hash=$1",
            &[&CLAIM],
        )
        .await
        .unwrap();
    txn.commit().await.unwrap();
    refused(&db, request.clone()).await;
    let mut rights = request;
    rights.as_object_mut().unwrap().remove("_claim_hash");
    rights["_moderation_hash"] = json!(RIGHTS);
    assert_eq!(call(&db, "learning_access", rights).await.unwrap(), receipt);
}

#[tokio::test]
async fn expired_revoked_erased_and_restricted_learning_keys_do_not_become_authority() {
    let (db, case, subject) = setup().await;
    let request = body(subject);
    call(&db, "learning_access", request.clone()).await.unwrap();
    let authority = || json!({"hash":request["hash"]});
    assert_eq!(
        call(&db, "learning_authority", authority()).await.unwrap()["subject"],
        json!(subject)
    );
    for sql in [
        "UPDATE commercial_learning_keys SET expires_at=clock_timestamp()-interval '1 second'",
        "UPDATE commercial_learning_keys SET revoked_at=clock_timestamp()",
        "UPDATE commercial_learning_subjects SET erased_at=clock_timestamp()",
        "UPDATE commercial_cases SET contact_verified=false",
        "UPDATE commercial_cases SET access_epoch=access_epoch+1",
        "UPDATE commercial_cases SET contact_epoch=contact_epoch+1",
    ] {
        let mut txn = db.begin_transaction().await.unwrap();
        txn.txn().batch_execute(sql).await.unwrap();
        assert!(
            Repo.commercial_operation(&mut txn, "learning_authority", None, &authority())
                .await
                .unwrap()
                .is_null(),
            "{sql}"
        );
        drop(txn); // Each independent restriction rolls back; the original key is preserved.
    }
    let txn = db.begin_transaction().await.unwrap();
    txn.txn().execute("INSERT INTO commercial_learning_restrictions(id,case_id,source_reference,evidence) VALUES($1,$2,'synthetic','{}')", &[&Uuid::new_v4(),&case]).await.unwrap();
    txn.commit().await.unwrap();
    assert!(
        call(&db, "learning_authority", authority())
            .await
            .unwrap()
            .is_null()
    );
    refused(&db, body(subject)).await;
}

#[tokio::test]
async fn legacy_no_target_receipt_survives_function_upgrade_with_original_expiry_and_key() {
    let (db, _, subject) = setup().await;
    let predecessor =
        include_str!("../migrations/2026-09-08-210000_learning_purchase_authority/up.sql");
    let (_, function) = predecessor
        .split_once("CREATE OR REPLACE FUNCTION commercial_learning_operation")
        .unwrap();
    db.execute(&format!(
        "CREATE OR REPLACE FUNCTION commercial_learning_operation{function}"
    ))
    .await
    .unwrap();
    let mut request = body(subject);
    request.as_object_mut().unwrap().remove("expected_subject");
    let original = call(&db, "learning_access", request.clone()).await.unwrap();
    db.execute(include_str!(
        "../migrations/2026-09-09-050000_learning_refresh_target/up.sql"
    ))
    .await
    .unwrap();
    let txn = db.begin_transaction().await.unwrap();
    txn.txn().execute("UPDATE commercial_learning_keys SET expires_at=clock_timestamp()-interval '1 second' WHERE command_id=$1", &[&Uuid::parse_str(request["command_id"].as_str().unwrap()).unwrap()]).await.unwrap();
    txn.txn()
        .execute(
            "UPDATE commercial_learning_subjects SET erased_at=clock_timestamp() WHERE subject=$1",
            &[&subject],
        )
        .await
        .unwrap();
    txn.commit().await.unwrap();
    let before = snapshot(&db).await;
    assert_eq!(
        call(&db, "learning_access", request.clone()).await.unwrap(),
        original
    );
    assert_eq!(snapshot(&db).await, before);
    assert!(
        call(&db, "learning_authority", json!({"hash":request["hash"]}))
            .await
            .unwrap()
            .is_null()
    );
    let mut changed = request.clone();
    changed["hash"] = json!(hash());
    refused(&db, changed).await;
    request["command_id"] = json!(Uuid::new_v4());
    refused(&db, request).await;
}

#[tokio::test]
async fn summary_start_and_revoke_keep_their_existing_scoped_semantics() {
    let (db, _, subject) = setup().await;
    let before = snapshot(&db).await;
    let summary = call(&db, "learning_summary", json!({"_claim_hash":CLAIM}))
        .await
        .unwrap();
    assert_eq!(summary["subjects"].as_array().unwrap().len(), 1);
    assert_eq!(summary["subjects"][0]["subject"], json!(subject));
    assert_eq!(snapshot(&db).await, before);
    assert!(
        call(&db, "learning_summary", json!({}))
            .await
            .unwrap_err()
            .is::<ModerationConflict>()
    );
    let existing = call(
        &db,
        "learning_start",
        json!({"command_id":Uuid::new_v4(),"hash":hash(),"_claim_hash":CLAIM}),
    )
    .await
    .unwrap();
    assert_eq!(existing["subject"], json!(subject));
    let revoke = json!({"command_id":Uuid::new_v4(),"_claim_hash":CLAIM});
    let receipt = call(&db, "learning_revoke", revoke.clone()).await.unwrap();
    let revoked = snapshot(&db).await;
    assert_eq!(revoked["subjects"][0]["authority_epoch"], 2);
    assert!(
        revoked["keys"]
            .as_array()
            .unwrap()
            .iter()
            .all(|k| !k["revoked_at"].is_null())
    );
    assert_eq!(call(&db, "learning_revoke", revoke).await.unwrap(), receipt);
    assert_eq!(snapshot(&db).await, revoked);
    let txn = db.begin_transaction().await.unwrap();
    txn.txn()
        .execute(
            "UPDATE commercial_learning_subjects SET erased_at=clock_timestamp() WHERE subject=$1",
            &[&subject],
        )
        .await
        .unwrap();
    txn.commit().await.unwrap();
    let request = json!({"command_id":Uuid::new_v4(),"hash":hash(),"_claim_hash":CLAIM});
    assert!(
        call(&db, "learning_start", request.clone())
            .await
            .unwrap_err()
            .is::<ModerationConflict>()
    );
    let txn = db.begin_transaction().await.unwrap();
    txn.txn()
        .batch_execute("UPDATE commercial_cases SET contact_verified=false")
        .await
        .unwrap();
    txn.commit().await.unwrap();
    let before = snapshot(&db).await;
    let mut explicit = request;
    explicit["use_retained_value"] = json!(true);
    assert!(
        call(&db, "learning_start", explicit)
            .await
            .unwrap_err()
            .is::<ModerationConflict>()
    );
    assert_eq!(snapshot(&db).await, before);
}

#[tokio::test]
async fn fresh_issuance_requires_current_personal_proof_and_accepts_semantic_uuid() {
    let (db, _, subject) = setup().await;
    let mut request = body(subject);
    request["expected_subject"] = json!(subject.to_string().to_uppercase());
    request.as_object_mut().unwrap().remove("_claim_hash");
    request["_moderation_hash"] = json!(RIGHTS);
    assert_eq!(
        call(&db, "learning_access", request).await.unwrap()["subject"],
        json!(subject)
    );
    for field in ["absent", "learning"] {
        let mut request = body(subject);
        request.as_object_mut().unwrap().remove("_claim_hash");
        if field == "learning" {
            request["_claim_hash"] = snapshot(&db).await["keys"][0]["hash"].clone();
        }
        refused(&db, request).await;
    }
    for sql in [
        "UPDATE moderation_capabilities SET scope='case'",
        "UPDATE moderation_capabilities SET expires_at=clock_timestamp()-interval '1 second'",
        "UPDATE moderation_capabilities SET revoked_at=clock_timestamp()",
    ] {
        let mut txn = db.begin_transaction().await.unwrap();
        txn.txn().batch_execute(sql).await.unwrap();
        let mut request = body(subject);
        request.as_object_mut().unwrap().remove("_claim_hash");
        request["_moderation_hash"] = json!(RIGHTS);
        let error = Repo
            .commercial_operation(&mut txn, "learning_access", Some(FOO.user.id), &request)
            .await
            .unwrap_err();
        assert!(error.is::<ModerationConflict>());
        drop(txn);
    }
}
