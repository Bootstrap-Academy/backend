//! Owned ordinary paging fixture; no old setup is executed.
mod common;
use academy_persistence_contracts::{Database, Transaction};
use academy_persistence_postgres::{PostgresDatabase, PostgresTransaction};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;
const FORWARD: &str = "2026-09-11-040000_retention_family_paging";
const FAMILIES: [&str; 5] = [
    "statements",
    "archives",
    "retained_owner_associations",
    "invoice_identity_reviews",
    "unqualified_invoice_owner_observations",
];
async fn setup() -> PostgresDatabase {
    common::setup().await
}

async fn staff(db: &PostgresDatabase) -> (Uuid, Value) {
    let actor = *academy_demo::user::FOO.user.id;
    let session = Uuid::new_v4();
    let tx = db.begin_transaction().await.unwrap();
    tx.txn()
        .execute(
            "UPDATE users SET admin=true,enabled=true WHERE id=$1",
            &[&actor],
        )
        .await
        .unwrap();
    tx.txn().execute("INSERT INTO sessions(id,user_id,created_at,updated_at,mfa_verified) VALUES($1,$2,clock_timestamp(),clock_timestamp(),true)", &[&session,&actor]).await.unwrap();
    tx.txn()
        .execute(
            "INSERT INTO session_refresh_tokens(session_id,refresh_token_hash) VALUES($1,$2)",
            &[&session, &vec![0x5au8; 32]],
        )
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (
        actor,
        json!({"_staff_session":session,"_staff_refresh_hash":"5a".repeat(32)}),
    )
}

fn body(proof: &Value, family: &str, limit: u64, cursor: Value) -> Value {
    let mut b = proof.clone();
    b["family"] = json!(family);
    b["limit"] = json!(limit);
    b["cursor"] = cursor;
    b
}

async fn operation(
    tx: &PostgresTransaction,
    op: &str,
    actor: Uuid,
    b: &Value,
) -> Result<Value, bb8_postgres::tokio_postgres::Error> {
    let r = tx
        .txn()
        .query_one(
            "SELECT commercial_operation($1,$2,$3::text::jsonb)::text",
            &[&op, &actor, &b.to_string()],
        )
        .await?;
    Ok(serde_json::from_str(r.get::<_, &str>(0)).unwrap())
}

async fn page(
    db: &PostgresDatabase,
    actor: Uuid,
    proof: &Value,
    family: &str,
    limit: u64,
    cursor: Value,
) -> Value {
    let tx = db.begin_transaction().await.unwrap();
    tx.txn()
        .batch_execute("SET TRANSACTION READ ONLY")
        .await
        .unwrap();
    let v = operation(
        &tx,
        "admin_retention_page",
        actor,
        &body(proof, family, limit, cursor),
    )
    .await
    .unwrap();
    assert_eq!(v["kind"], "page", "{v}");
    tx.commit().await.unwrap();
    v["value"].clone()
}

async fn fingerprint(db: &PostgresDatabase) -> BTreeMap<String, String> {
    let tx = db.begin_transaction().await.unwrap();
    let mut out = BTreeMap::new();
    for r in tx
        .txn()
        .query(
            "SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename",
            &[],
        )
        .await
        .unwrap()
    {
        let name: String = r.get(0);
        let sql = format!(
            "SELECT md5(coalesce(jsonb_agg(x ORDER BY x::text)::text,'[]')) FROM (SELECT to_jsonb(t) x FROM public.\"{}\" t) q",
            name.replace('"', "\"\"")
        );
        let value: String = tx.txn().query_one(&sql, &[]).await.unwrap().get(0);
        out.insert(name, value);
    }
    out
}

async fn seed(db: &PostgresDatabase) {
    let tx = db.begin_transaction().await.unwrap();
    // Literal text and equal dates exercise every PK tie, including UUID before
    // remaining text components in the owner-observation family.
    tx.txn().batch_execute("INSERT INTO financial_documents(number,kind,issued_at,customer_details) SELECT 'S-page-'||lpad(n::text,3,'0'),'final_statement','2000-01-01',ARRAY['synthetic'] FROM generate_series(1,121)n;
     UPDATE commercial_statement_disposal_reviews SET review_due_at='2030-01-01',assessment=CASE WHEN right(number,1)='1' THEN 'null'::jsonb ELSE NULL END WHERE number LIKE 'S-page-%';
     INSERT INTO commercial_archive_work(number,kind,source,recorded_at,review_due_at,disposal_authorized,disposal_started_at,assessment)
      SELECT 'literal '||lpad((n/3)::text,3,'0'),(ARRAY['invoice','credit_note','final_statement'])[n%3+1],CASE WHEN n%2=0 THEN 'record_disposal' ELSE 'unrecorded_archive' END,'2000-01-01','2030-01-01',n%2=0,CASE WHEN n%2=0 THEN '2020-01-01'::timestamptz END,'900719925474099312345678901234567890.123456789'::jsonb FROM generate_series(0,125)n;
     INSERT INTO commercial_retention_owners(number,kind,subject,observed_at,review_due_at,source)
      SELECT 'literal '||lpad((n/8)::text,3,'0'),CASE WHEN n%4<2 THEN 'ä' ELSE 'z' END,('00000000-0000-0000-0000-'||lpad((n%2+1)::text,12,'0'))::uuid,'2000-01-01','2030-01-01','' FROM generate_series(0,127)n ON CONFLICT DO NOTHING;
     INSERT INTO commercial_invoice_identity_reviews(number,reason,source_key,observed_at,evidence)
      SELECT 'literal '||lpad((n/8)::text,3,'0'),CASE WHEN n%4<2 THEN 'ä' ELSE 'z' END,CASE WHEN n%2=0 THEN '' ELSE '🙂' END,'2030-01-01','900719925474099312345678901234567890'::jsonb FROM generate_series(0,255)n ON CONFLICT DO NOTHING;
     INSERT INTO commercial_invoice_owner_observations(number,subject,basis,evidence_hash,evidence,qualified,observed_at)
      SELECT 'literal '||lpad((n/8)::text,3,'0'),('00000000-0000-0000-0000-'||lpad((n%8/4+1)::text,12,'0'))::uuid,CASE WHEN n%4<2 THEN 'ä' ELSE 'z' END,CASE WHEN n%2=0 THEN '' ELSE '🙂' END,CASE WHEN n%2=0 THEN 'null'::jsonb ELSE '[1,true,90071992547409931234567890]'::jsonb END,false,'2030-01-01' FROM generate_series(0,127)n;").await.unwrap();
    // Associations intentionally use enough distinct numbers to exceed100 after
    // collisions from repeated literal-key combinations have been deduplicated.
    tx.txn().batch_execute("INSERT INTO commercial_retention_owners(number,kind,subject,observed_at,review_due_at,source) SELECT 'tail-'||n,'',('00000000-0000-0000-0000-'||lpad(n::text,12,'0'))::uuid,'2000-01-01','2030-01-01','unconstrained' FROM generate_series(1,65)n;
     INSERT INTO commercial_archive_work(number,kind,source,review_due_at,file_removed_at) VALUES('excluded','invoice','record_disposal','2030-01-01','2020-01-01');
     INSERT INTO commercial_invoice_owner_observations(number,subject,basis,evidence_hash,evidence,qualified,observed_at) VALUES('excluded','00000000-0000-0000-0000-000000000001','','','null',true,'2030-01-01');
     INSERT INTO commercial_invoice_identity_reviews(number,reason,source_key,observed_at,evidence) VALUES('literal 000','pending invoice','exact','2030-01-01','null');").await.unwrap();
    tx.commit().await.unwrap();
}

fn tuple(row: &Value, family: &str) -> Value {
    let date = if matches!(
        family,
        "invoice_identity_reviews" | "unqualified_invoice_owner_observations"
    ) {
        "observed_at"
    } else {
        "review_due_at"
    };
    let keys: &[&str] = match family {
        "statements" => &["number"],
        "archives" => &["number", "kind"],
        "retained_owner_associations" => &["number", "kind", "subject"],
        "invoice_identity_reviews" => &["number", "reason", "source_key"],
        _ => &["number", "subject", "basis", "evidence_hash"],
    };
    let mut v = json!({"at":row[date]});
    for k in keys {
        v[k] = row[k].clone();
    }
    v
}

async fn expected(db: &PostgresDatabase, family: &str) -> Vec<Value> {
    let (table, date, cols, order, pred) = match family {
        "statements" => (
            "commercial_statement_disposal_reviews r JOIN financial_documents d USING(number)",
            "r.review_due_at",
            "'number',r.number",
            "r.review_due_at,r.number COLLATE \"C\"",
            "true",
        ),
        "archives" => (
            "commercial_archive_work r",
            "review_due_at",
            "'number',number,'kind',kind",
            "review_due_at,number COLLATE \"C\",kind COLLATE \"C\"",
            "file_removed_at IS NULL",
        ),
        "retained_owner_associations" => (
            "commercial_retention_owners r",
            "review_due_at",
            "'number',number,'kind',kind,'subject',subject",
            "review_due_at,number COLLATE \"C\",kind COLLATE \"C\",subject",
            "true",
        ),
        "invoice_identity_reviews" => (
            "commercial_invoice_identity_reviews r",
            "observed_at",
            "'number',number,'reason',reason,'source_key',source_key",
            "observed_at,number COLLATE \"C\",reason COLLATE \"C\",source_key COLLATE \"C\"",
            "true",
        ),
        _ => (
            "commercial_invoice_owner_observations r",
            "observed_at",
            "'number',number,'subject',subject,'basis',basis,'evidence_hash',evidence_hash",
            "observed_at,number COLLATE \"C\",subject,basis COLLATE \"C\",evidence_hash COLLATE \"C\"",
            "NOT qualified",
        ),
    };
    let tx = db.begin_transaction().await.unwrap();
    tx.txn()
        .batch_execute("SET LOCAL TimeZone='UTC';SET LOCAL DateStyle='ISO,YMD'")
        .await
        .unwrap();
    tx.txn().query(&format!("SELECT jsonb_build_object('at',{date}::text,{cols})::text FROM {table} WHERE {pred} ORDER BY {order}"),&[]).await.unwrap().iter().map(|r|serde_json::from_str(r.get::<_,&str>(0)).unwrap()).collect()
}

#[tokio::test]
async fn paging_all_five_over100_full_ties_and_changed_limits_are_lossless_when_unchanged() {
    let db = setup().await;
    let (actor, proof) = staff(&db).await;
    seed(&db).await;
    let before = fingerprint(&db).await;
    for family in FAMILIES {
        let target = expected(&db, family).await;
        assert!(target.len() > 100, "{family}:{}", target.len());
        let mut cursor = Value::Null;
        let mut got = Vec::new();
        let mut page_no = 0;
        loop {
            let limit = if page_no % 2 == 0 { 17 } else { 31 };
            let v = page(&db, actor, &proof, family, limit, cursor).await;
            let rows = v["rows"].as_array().unwrap();
            assert!(rows.len() <= usize::try_from(limit).unwrap());
            got.extend(rows.iter().map(|r| tuple(r, family)));
            if v["exhausted"] == true {
                assert!(v["next_cursor"].is_null());
                break;
            }
            assert_eq!(
                v["next_cursor"]["after"],
                tuple(rows.last().unwrap(), family)
            );
            cursor = v["next_cursor"].clone();
            page_no += 1;
            assert!(page_no < 30);
        }
        assert_eq!(got, target, "{family}");
        assert_eq!(
            got.iter()
                .map(Value::to_string)
                .collect::<BTreeSet<_>>()
                .len(),
            got.len()
        );
        println!(
            "FAMILY {family}: {} exact ordered tuples; unchanged traversal",
            got.len()
        );
    }
    assert_eq!(before, fingerprint(&db).await);
}

#[tokio::test]
async fn paging_uses_c_order_in_icu_database_and_keeps_opaque_null_and_numeric_evidence() {
    let db = setup().await;
    let (actor, proof) = staff(&db).await;
    seed(&db).await;
    let tx = db.begin_transaction().await.unwrap();
    let c: bool = tx
        .txn()
        .query_one(
            "SELECT ('ä'::text COLLATE \"default\" < 'z') AND ('ä'::text COLLATE \"C\" > 'z')",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(c, "nondefault ICU order differs from C");
    drop(tx);
    let v = page(&db, actor, &proof, "archives", 100, Value::Null).await;
    assert!(
        v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["file_removed_at"].is_null())
    );
    assert!(
        v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["disposal_authorized"] == false)
    );
    assert!(
        v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| !r["disposal_started_at"].is_null() && r["source"] == "record_disposal")
    );
    assert_eq!(
        v["rows"][0]["assessment_json"],
        "900719925474099312345678901234567890.123456789"
    );
    let v = page(&db, actor, &proof, "statements", 100, Value::Null).await;
    assert!(
        v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["assessment_json"].is_null())
    );
    assert!(
        v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["assessment_json"] == "null")
    );
    let v = page(
        &db,
        actor,
        &proof,
        "invoice_identity_reviews",
        100,
        Value::Null,
    )
    .await;
    assert!(
        v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["evidence_json"] == "900719925474099312345678901234567890")
    );
    let v = page(
        &db,
        actor,
        &proof,
        "unqualified_invoice_owner_observations",
        100,
        Value::Null,
    )
    .await;
    assert!(
        v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["evidence_json"] == "null")
    );
    assert!(
        v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["qualified"] == false)
    );
}

#[tokio::test]
async fn paging_native_dates_roundtrip_extremes_and_do_not_change_caller_settings() {
    let db = setup().await;
    let (actor, proof) = staff(&db).await;
    let tx = db.begin_transaction().await.unwrap();
    for (n, date) in [
        "-infinity",
        "4714-11-24 00:00:00+00 BC",
        "0001-01-01 00:00:00+00 BC",
        "0001-01-01 00:00:00+00",
        "2000-02-29 23:59:59.123456+00",
        "12000-01-01 00:00:00+00",
        "294276-12-31 23:59:59.999999+00",
        "infinity",
    ]
    .iter()
    .enumerate()
    {
        tx.txn().execute("INSERT INTO commercial_archive_work(number,kind,source,recorded_at,review_due_at,disposal_started_at) VALUES($1,'invoice','record_disposal',$2::text::timestamptz,$2::text::timestamptz,$2::text::timestamptz)",&[&format!("native-{n}"),date]).await.unwrap();
    }
    tx.commit().await.unwrap();
    let tx = db.begin_transaction().await.unwrap();
    tx.txn().batch_execute("SET TRANSACTION READ ONLY;SET LOCAL DateStyle='German,DMY';SET LOCAL TimeZone='Pacific/Auckland'").await.unwrap();
    let settings: String = tx
        .txn()
        .query_one(
            "SELECT current_setting('DateStyle')||'/'||current_setting('TimeZone')",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    let mut cursor = Value::Null;
    let mut dates = Vec::new();
    loop {
        let v = operation(
            &tx,
            "admin_retention_page",
            actor,
            &body(&proof, "archives", 1, cursor),
        )
        .await
        .unwrap();
        assert_eq!(v["kind"], "page");
        let p = &v["value"];
        let row = &p["rows"][0];
        dates.push(row["review_due_at"].clone());
        assert_eq!(row["recorded_at"], row["review_due_at"]);
        assert_eq!(row["disposal_started_at"], row["review_due_at"]);
        if p["exhausted"] == true {
            break;
        }
        cursor = p["next_cursor"].clone();
    }
    assert_eq!(dates.len(), 8);
    assert_eq!(dates[0], "-infinity");
    assert_eq!(dates[7], "infinity");
    assert_eq!(dates[1], "4714-11-24 00:00:00+00 BC");
    assert_eq!(dates[6], "294276-12-31 23:59:59.999999+00");
    assert_eq!(
        settings,
        tx.txn()
            .query_one(
                "SELECT current_setting('DateStyle')||'/'||current_setting('TimeZone')",
                &[]
            )
            .await
            .unwrap()
            .get::<_, String>(0)
    );
    println!("NATIVE PAGE DATES {dates:?}");
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn paging_malformed_cursors_are400_envelopes_while_staff_and_data_errors_remain_errors() {
    let db = setup().await;
    let (actor, proof) = staff(&db).await;
    let valid = body(&proof, "archives", 1, Value::Null);
    let mut bad = Vec::new();
    // RP1: exact cursor keys do not make a null or nonstring family valid.
    // The null case reached the projection with the original SQL validator.
    for family in [
        Value::Null,
        json!(false),
        json!(1),
        json!([]),
        json!({}),
        json!("statements"),
    ] {
        let mut b = valid.clone();
        b["cursor"] = json!({"protocol":1,"family":family,"after":{"at":"infinity","number":"","kind":"invoice"}});
        bad.push(b);
    }
    for replacement in [json!(0), json!(101), json!(1.0), json!("1"), json!(true)] {
        let mut b = valid.clone();
        b["limit"] = replacement;
        bad.push(b);
    }
    let mut b = valid.clone();
    b.as_object_mut().unwrap().remove("cursor");
    bad.push(b);
    let mut b = valid.clone();
    b["subject"] = json!(actor);
    bad.push(b);
    for date in [
        "",
        "today",
        "2026-09-11T00:00:00Z",
        "2026-09-11 00:00:00.000000+00",
        "999999999999999999999-01-01",
        "294277-01-01 00:00:00+00",
        "2026-99-99",
        "2026-01-01 00:00:00+16",
    ] {
        let mut b = valid.clone();
        b["cursor"] = json!({"protocol":1,"family":"archives","after":{"at":date,"number":"","kind":"invoice"}});
        bad.push(b);
    }
    let mut b = valid.clone();
    b["cursor"] = json!({"protocol":1,"family":"statements","after":{"at":"infinity","number":""}});
    bad.push(b);
    let mut b = body(
        &proof,
        "retained_owner_associations",
        1,
        json!({"protocol":1,"family":"retained_owner_associations","after":{"at":"infinity","number":"","kind":"","subject":"AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA"}}),
    );
    bad.push(b.clone());
    b["cursor"]["after"]["subject"] = json!(false);
    bad.push(b);
    let before = fingerprint(&db).await;
    for b in bad {
        let tx = db.begin_transaction().await.unwrap();
        assert_eq!(
            operation(&tx, "admin_retention_page", actor, &b)
                .await
                .unwrap(),
            json!({"kind":"malformed"}),
            "{b}"
        );
    }
    assert_eq!(before, fingerprint(&db).await);
    let tx = db.begin_transaction().await.unwrap();
    let error = operation(&tx, "admin_retention_page", Uuid::new_v4(), &valid)
        .await
        .unwrap_err();
    assert_eq!(error.code().unwrap().code(), "P0001");
    drop(tx);
    let tx = db.begin_transaction().await.unwrap();
    tx.txn()
        .batch_execute("ALTER TABLE commercial_archive_work RENAME TO hidden_archive_work")
        .await
        .unwrap();
    let error = operation(&tx, "admin_retention_page", actor, &valid)
        .await
        .unwrap_err();
    assert_eq!(error.code().unwrap().code(), "42P01");
    drop(tx);
}

#[tokio::test]
async fn paging_empty_exact_limit_live_move_removed_cursor_and_restart() {
    let db = setup().await;
    let (actor, proof) = staff(&db).await;
    let empty = page(&db, actor, &proof, "archives", 3, Value::Null).await;
    assert_eq!(empty["rows"], json!([]));
    assert_eq!(empty["exhausted"], true);
    let tx = db.begin_transaction().await.unwrap();
    tx.txn().batch_execute("INSERT INTO commercial_archive_work(number,kind,source,review_due_at) VALUES('a','invoice','unrecorded_archive','2030-01-01'),('b','invoice','unrecorded_archive','2030-01-01'),('c','invoice','unrecorded_archive','2030-01-01')").await.unwrap();
    tx.commit().await.unwrap();
    let exact = page(&db, actor, &proof, "archives", 3, Value::Null).await;
    assert_eq!(exact["rows"].as_array().unwrap().len(), 3);
    assert_eq!(exact["exhausted"], true);
    assert!(exact["next_cursor"].is_null());
    let first = page(&db, actor, &proof, "archives", 1, Value::Null).await;
    assert_eq!(first["rows"][0]["number"], "a");
    let cursor = first["next_cursor"].clone();
    let tx = db.begin_transaction().await.unwrap();
    tx.txn().batch_execute("DELETE FROM commercial_archive_work WHERE number='a';UPDATE commercial_archive_work SET review_due_at='2020-01-01' WHERE number='b';UPDATE commercial_archive_work SET file_removed_at=clock_timestamp() WHERE number='c';INSERT INTO commercial_archive_work(number,kind,source,review_due_at) VALUES('d','invoice','record_disposal','2040-01-01')").await.unwrap();
    tx.commit().await.unwrap();
    let suffix = page(&db, actor, &proof, "archives", 100, cursor).await;
    assert_eq!(suffix["rows"].as_array().unwrap().len(), 1);
    assert_eq!(suffix["rows"][0]["number"], "d");
    let head = page(&db, actor, &proof, "archives", 100, Value::Null).await;
    assert_eq!(head["rows"].as_array().unwrap().len(), 2);
    assert_eq!(head["rows"][0]["number"], "b");
    let tx = db.begin_transaction().await.unwrap();
    tx.txn()
        .batch_execute(
            "UPDATE commercial_archive_work SET review_due_at='2050-01-01' WHERE number='b'",
        )
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let again = page(
        &db,
        actor,
        &proof,
        "archives",
        100,
        json!({"protocol":1,"family":"archives","after":tuple(&head["rows"][0],"archives")}),
    )
    .await;
    assert!(
        again["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["number"] == "b")
    );
}

#[tokio::test]
async fn paging_current_staff_each_page_and_read_only_has_no_journal_or_capture() {
    let db = setup().await;
    let (actor, proof) = staff(&db).await;
    seed(&db).await;
    for mode in ["admin", "mfa", "refresh"] {
        let tx = db.begin_transaction().await.unwrap();
        let session: Uuid = serde_json::from_value(proof["_staff_session"].clone()).unwrap();
        match mode {
            "admin" => {
                tx.txn()
                    .execute("UPDATE users SET admin=false WHERE id=$1", &[&actor])
                    .await
                    .unwrap();
            }
            "mfa" => {
                tx.txn()
                    .execute(
                        "UPDATE sessions SET mfa_verified=false WHERE id=$1",
                        &[&session],
                    )
                    .await
                    .unwrap();
            }
            _ => {
                tx.txn()
                    .execute(
                        "DELETE FROM session_refresh_tokens WHERE session_id=$1",
                        &[&session],
                    )
                    .await
                    .unwrap();
            }
        }
        let e = operation(
            &tx,
            "admin_retention_page",
            actor,
            &body(&proof, "archives", 1, Value::Null),
        )
        .await
        .unwrap_err();
        assert_eq!(e.code().unwrap().code(), "P0001");
        drop(tx);
    }
    let before = fingerprint(&db).await;
    for _ in 0..2 {
        for family in FAMILIES {
            page(&db, actor, &proof, family, 100, Value::Null).await;
        }
    }
    assert_eq!(before, fingerprint(&db).await);
    println!("READ ONLY {} public tables unchanged", before.len());
}

#[tokio::test]
async fn paging_down_up_restores_immediate_dispatcher_and_keeps_invoice_guard() {
    let db = common::setup_through(Some("2026-09-11-040000_retention_family_paging")).await;
    let (actor, proof) = staff(&db).await;
    seed(&db).await;
    let tx = db.begin_transaction().await.unwrap();
    let old:String=tx.txn().query_one("SELECT prosrc FROM pg_proc WHERE oid='commercial_operation_before_retention_page(text,uuid,jsonb)'::regprocedure",&[]).await.unwrap().get(0);
    let guard: String = tx
        .txn()
        .query_one(
            "SELECT pg_get_functiondef('commercial_invoice_before_delete()'::regprocedure)",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    let queue = operation(&tx, "retention_queue", actor, &proof)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let before = fingerprint(&db).await;
    assert_eq!(db.revert_migrations(Some(1)).await.unwrap(), vec![FORWARD]);
    let tx = db.begin_transaction().await.unwrap();
    assert_eq!(old,tx.txn().query_one("SELECT prosrc FROM pg_proc WHERE oid='commercial_operation(text,uuid,jsonb)'::regprocedure",&[]).await.unwrap().get::<_,String>(0));
    assert_eq!(
        guard,
        tx.txn()
            .query_one(
                "SELECT pg_get_functiondef('commercial_invoice_before_delete()'::regprocedure)",
                &[]
            )
            .await
            .unwrap()
            .get::<_, String>(0)
    );
    assert_eq!(
        queue,
        operation(&tx, "retention_queue", actor, &proof)
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
    let boundary = fingerprint(&db).await;
    assert!(
        db.revert_migrations(Some(1))
            .await
            .unwrap_err()
            .to_string()
            .contains("Failed to revert migration")
    );
    assert_eq!(boundary, fingerprint(&db).await);
    assert_eq!(
        common::apply_through(&db, Some("2026-09-11-040000_retention_family_paging")).await,
        vec![FORWARD]
    );
    assert_eq!(before, fingerprint(&db).await);
    let tx = db.begin_transaction().await.unwrap();
    assert_eq!(
        queue,
        operation(&tx, "retention_queue", actor, &proof)
            .await
            .unwrap()
    );
    drop(tx);
    page(&db, actor, &proof, "archives", 1, Value::Null).await;
}
