use academy_persistence_contracts::{Database, Transaction};
use academy_persistence_postgres::MIGRATIONS;
use common::{setup, setup_clean};

mod common;

async fn fingerprint(db: &common::Db) -> std::collections::BTreeMap<String, String> {
    let tx = db.begin_transaction().await.unwrap();
    let mut state = std::collections::BTreeMap::new();
    for row in tx
        .txn()
        .query(
            "SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename",
            &[],
        )
        .await
        .unwrap()
    {
        let table: String = row.get(0);
        let quoted = table.replace('"', "\"\"");
        let data: String = tx.txn().query_one(&format!("SELECT coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]')::text FROM \"{quoted}\" t"), &[]).await.unwrap().get(0);
        state.insert(table, data);
    }
    tx.commit().await.unwrap();
    state
}

async fn current_refusal(db: &common::Db) {
    let data = fingerprint(db).await;
    let before = common::fixture::evidence_path("current-migrations", "sql");
    common::fixture::preserve(db, "current-before-refusal").await;
    let tx = db.begin_transaction().await.unwrap();
    let original: String = tx
        .txn()
        .query_one(
            "SELECT jsonb_agg(to_jsonb(t) ORDER BY name)::text FROM _migrations t",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    tx.commit().await.unwrap();
    let error = db.revert_migrations(Some(1)).await.unwrap_err();
    assert!(
        format!("{error:#}").contains(
            "Historical moderation email protection requires a reviewed forward migration"
        )
    );
    let tx = db.begin_transaction().await.unwrap();
    let after: String = tx
        .txn()
        .query_one(
            "SELECT jsonb_agg(to_jsonb(t) ORDER BY name)::text FROM _migrations t",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(after, original);
    tx.commit().await.unwrap();
    std::fs::write(before, after).unwrap();
    common::fixture::preserve(db, "current-after-refusal").await;
    assert!(db.run_migrations(None).await.unwrap().is_empty());
    assert_eq!(fingerprint(db).await, data);
}

async fn historical_matrix(with_data: bool) {
    // A separate empty database precedes the first unconditional preservation guard.
    let last = "2026-09-08-100000_purchase_contract_evidence";
    let db = common::fixture::fresh_history().await;
    let end = MIGRATIONS.iter().position(|m| m.name == last).unwrap() + 1;
    let names = MIGRATIONS[..end].iter().map(|m| m.name).collect::<Vec<_>>();
    assert_eq!(common::apply_through(&db, Some(last)).await, names);
    if with_data {
        common::seed(&db).await;
    }
    for i in 1..=end {
        let mut reverted = db.revert_migrations(Some(i)).await.unwrap();
        reverted.reverse();
        assert_eq!(reverted, names[end - i..]);
        let applied = common::apply_through(&db, Some(last)).await;
        assert_eq!(applied, names[end - i..]);
    }
}

#[tokio::test]
async fn migrations_clean() {
    let db = setup_clean().await;
    assert_eq!(
        db.run_migrations(None).await.unwrap(),
        MIGRATIONS.iter().map(|m| m.name).collect::<Vec<_>>()
    );
    current_refusal(&db).await;
    historical_matrix(false).await;
}

#[tokio::test]
async fn migrations_with_data() {
    let db = setup().await;
    current_refusal(&db).await;
    historical_matrix(true).await;
}
