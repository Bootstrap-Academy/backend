//! Purchases of a retained learning subject after a deletion, for offers of stage 2 of the
//! order dialog (terms 2026-10). Serial functional tests in an owned disposable fixture;
//! synthetic proof hashes exercise the SQL boundary, not HTTP authentication.
mod common;

use academy_demo::user::FOO;
use academy_persistence_contracts::{Database, Transaction, moderation::ModerationRepository};
use academy_persistence_postgres::{
    PostgresDatabase, moderation::PostgresModerationRepository as Repo,
};
use serde_json::{Value, json};
use uuid::Uuid;

const CLAIM: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

/// A commercial case for FOO with a claim key, and the learning subject it retained.
async fn setup() -> (PostgresDatabase, Uuid) {
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
    txn.commit().await.unwrap();
    let hash = Uuid::new_v4().simple().to_string().repeat(2);
    let first = call(&db, "learning_start", json!({"command_id":Uuid::new_v4(),"hash":hash,"_claim_hash":CLAIM,"use_retained_value":true})).await.unwrap();
    let subject = Uuid::parse_str(first["subject"].as_str().unwrap()).unwrap();
    (db, subject)
}

async fn call(db: &PostgresDatabase, operation: &str, body: Value) -> anyhow::Result<Value> {
    let mut txn = db.begin_transaction().await?;
    let result = Repo
        .commercial_operation(&mut txn, operation, Some(FOO.user.id), &body)
        .await?;
    txn.commit().await?;
    Ok(result)
}

/// An open offer of 50 coins for the subject; `acceptance` and `declaration` as an offer of
/// the backend carries them.
async fn open_offer(
    db: &PostgresDatabase,
    subject: Uuid,
    acceptance: Option<&str>,
    declaration: &str,
) -> Value {
    let id = Uuid::new_v4();
    let mut offer = json!({"id":id,"user_id":subject,"source":"backend","hash":format!("hash-{id}"),
        "declaration":declaration,"product":{"kind":"hearts","coins":50}});
    if let Some(acceptance) = acceptance {
        offer["acceptance"] = json!(acceptance);
    }
    let txn = db.begin_transaction().await.unwrap();
    txn.txn().execute("INSERT INTO purchase_offers(id,user_id,source,offer,terms_pdf,withdrawal_pdf,created_at,expires_at) VALUES($1,$2,'backend',$3::text::jsonb,'\\x25504446'::bytea,'\\x25504446'::bytea,clock_timestamp(),clock_timestamp()+interval '10 minutes')",
        &[&id,&subject,&offer.to_string()]).await.unwrap();
    txn.txn()
        .execute("INSERT INTO purchase_progress(order_id) VALUES($1)", &[&id])
        .await
        .unwrap();
    txn.commit().await.unwrap();
    offer
}

fn acceptance(offer: &Value, early: Value) -> Value {
    json!({"order_id":offer["id"],"offer_hash":offer["hash"],"accepted":true,"early_performance_requested":early})
}

async fn authorize(db: &PostgresDatabase, offer: &Value, early: Value) -> anyhow::Result<Value> {
    call(db, "purchase_authorize", json!({"command_id":Uuid::new_v4(),"_claim_hash":CLAIM,"acceptance":acceptance(offer, early)})).await
}

async fn stored(db: &PostgresDatabase, offer: &Value) -> Value {
    let txn = db.begin_transaction().await.unwrap();
    let id = Uuid::parse_str(offer["id"].as_str().unwrap()).unwrap();
    let text: String = txn
        .txn()
        .query_one("SELECT exact_acceptance::text FROM commercial_purchase_authorizations WHERE order_id=$1", &[&id])
        .await
        .unwrap()
        .get(0);
    txn.commit().await.unwrap();
    serde_json::from_str(&text).unwrap()
}

/// The submission the backend records for the acceptance (`accept_for`).
async fn submit(db: &PostgresDatabase, offer: &Value, early: bool) -> anyhow::Result<()> {
    let txn = db.begin_transaction().await?;
    let id = Uuid::parse_str(offer["id"].as_str().unwrap()).unwrap();
    txn.txn()
        .execute(
            "INSERT INTO purchase_submissions(order_id,request) VALUES($1,$2::text::jsonb)",
            &[&id, &acceptance(offer, json!(early)).to_string()],
        )
        .await?;
    txn.commit().await?;
    Ok(())
}

#[tokio::test]
async fn a_stage_two_offer_without_a_declaration_is_authorized_and_records_none() {
    let (db, subject) = setup().await;
    for sent in [false, true] {
        let offer = open_offer(&db, subject, Some("button"), "").await;
        authorize(&db, &offer, json!(sent)).await.unwrap();
        // Stored as the backend records it: no declaration, whatever the request said.
        assert_eq!(stored(&db, &offer).await, acceptance(&offer, json!(false)));
        if sent {
            assert!(
                submit(&db, &offer, true).await.is_err(),
                "a declaration that was never shown"
            );
        }
        submit(&db, &offer, false).await.unwrap();
    }
}

#[tokio::test]
async fn a_declaration_is_still_required_where_the_offer_carries_one() {
    let (db, subject) = setup().await;
    // Stage 2 with the declaration on the early start of Premium, and every offer of stage 1.
    for (stage, declaration) in [
        (Some("button"), "Ich verlange ausdrücklich …"),
        (None, "Ich verlange ausdrücklich …"),
    ] {
        let offer = open_offer(&db, subject, stage, declaration).await;
        assert!(
            authorize(&db, &offer, json!(false)).await.is_err(),
            "{stage:?}"
        );
        authorize(&db, &offer, json!(true)).await.unwrap();
        assert_eq!(stored(&db, &offer).await, acceptance(&offer, json!(true)));
        submit(&db, &offer, true).await.unwrap();
    }
    // A stage-1 offer without a declaration text still needs the request: its rule is unchanged.
    let offer = open_offer(&db, subject, None, "").await;
    assert!(authorize(&db, &offer, json!(false)).await.is_err());
    // The request is a boolean.
    let offer = open_offer(&db, subject, Some("button"), "").await;
    assert!(authorize(&db, &offer, json!("false")).await.is_err());
    authorize(&db, &offer, json!(false)).await.unwrap();
}
