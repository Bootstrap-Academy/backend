use crate::common::setup;
use academy_demo::{UUID1, UUID2, user::FOO};
use academy_models::{contract::*, premium::PremiumRenewalId};
use academy_persistence_contracts::{
    Database, Transaction, contract::ContractRepository, premium::PremiumRepository,
};
use academy_persistence_postgres::{
    PostgresTransaction, contract::PostgresContractRepository as Repo,
    premium::PostgresPremiumRepository as PremiumRepo,
};
use chrono::{TimeDelta, Utc};
fn declaration(id: uuid::Uuid, requested: Option<chrono::DateTime<Utc>>) -> ContractDeclaration {
    ContractDeclaration {
        id: id.into(),
        kind: ContractDeclarationKind::Cancellation,
        received_at: Utc::now(),
        name: "Synthetic declarant".try_into().unwrap(),
        email: FOO.user.email.clone().unwrap(),
        user_id: Some(FOO.user.id),
        contract: ContractKind::Premium,
        contract_designation: Some("Synthetic agreement".try_into().unwrap()),
        cancellation_type: Some(ContractCancellationType::Ordinary),
        details: "Original request".try_into().unwrap(),
        requested_end: requested,
        effective_end: None,
        processed_at: None,
        processing_note: None,
        delivery: vec![],
        operational_evidence: None,
    }
}
async fn fixture(txn: &mut PostgresTransaction) -> (PremiumRenewalId, chrono::DateTime<Utc>) {
    let until = chrono::DateTime::from_timestamp_micros(Utc::now().timestamp_micros()).unwrap()
        + TimeDelta::days(10);
    txn.txn()
        .execute(
            "INSERT INTO premium(id,user_id,since,until) VALUES ($1,$2,$3,$4)",
            &[
                &UUID1,
                &*FOO.user.id,
                &(until - TimeDelta::days(30)),
                &until,
            ],
        )
        .await
        .unwrap();
    txn.txn().execute("INSERT INTO premium_renewal_agreements(id,user_id,received_at,offer_id,monthly_price,recipient,document,terms_pdf,withdrawal_pdf,paid_period_id,confirmation_deadline) VALUES ($1,$2,clock_timestamp(),'test',777,$3,'immutable',decode('00','hex'),decode('00','hex'),$4,$5)", &[&UUID2,&*FOO.user.id,&FOO.user.email.as_ref().unwrap().as_str(),&UUID1,&until]).await.unwrap();
    txn.txn().execute("INSERT INTO premium_renewal_delivery(agreement_id,sent_at) VALUES ($1,clock_timestamp())", &[&UUID2]).await.unwrap();
    txn.txn()
        .execute(
            "INSERT INTO premium_subscriptions(user_id,plan,agreement_id) VALUES ($1,'monthly',$2)",
            &[&*FOO.user.id, &UUID2],
        )
        .await
        .unwrap();
    (UUID2.into(), until)
}
fn message(id: ContractDeclarationId) -> ContractDeliveryAttempt {
    ContractDeliveryAttempt {
        requested_agreement_id: None,
        declaration_id: id,
        kind: "receipt".into(),
        recipient: FOO.user.email.clone().unwrap(),
        subject: "Original".into(),
        body: "Immutable bytes ü\nline 2".into(),
        generation: 0,
    }
}

#[tokio::test]
async fn committed_period_upgrade_keeps_old_order_unknown_and_retains_new_witness() {
    let db = setup().await;
    db.revert_migrations(Some(1)).await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    fixture(&mut txn).await;
    let old_operation: i64 = txn
        .txn()
        .query_one("SELECT id FROM premium_period_changes", &[])
        .await
        .unwrap()
        .get(0);
    txn.commit().await.unwrap();
    db.run_migrations(None).await.unwrap();
    let txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        txn.txn()
            .query_one(
                "SELECT count(*) FROM premium_period_commit_observation",
                &[]
            )
            .await
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    let observed: chrono::DateTime<Utc> = txn.txn().query_one("INSERT INTO premium_period_commit_observation(operation_id,observed_at) VALUES($1,'2000-01-01Z') RETURNING observed_at", &[&old_operation]).await.unwrap().get(0);
    assert!(observed > Utc::now() - TimeDelta::minutes(1));
    txn.commit().await.unwrap();
    assert!(db.revert_migrations(Some(1)).await.is_err());
}

#[tokio::test]
async fn future_date_waits_for_the_first_paid_boundary_without_reactivating_replacement() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();
    let (agreement, until) = fixture(&mut txn).await;
    let d = declaration(uuid::Uuid::new_v4(), Some(until + TimeDelta::days(15)));
    Repo.create(&mut txn, d.clone()).await.unwrap();
    assert!(
        Repo.schedule_cancellation(&mut txn, d.clone(), agreement, FOO.user.id)
            .await
            .unwrap()
    );
    assert!(
        PremiumRepo
            .get_subscription(&mut txn, FOO.user.id)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        Repo.get(&mut txn, d.id)
            .await
            .unwrap()
            .unwrap()
            .effective_end
            .is_none()
    );
    let actual_end = until + TimeDelta::days(31);
    PremiumRepo
        .extend(&mut txn, UUID1.into(), actual_end)
        .await
        .unwrap();
    assert!(
        PremiumRepo
            .get_subscription(&mut txn, FOO.user.id)
            .await
            .unwrap()
            .is_none()
    );
    let resolved = Repo.get(&mut txn, d.id).await.unwrap().unwrap();
    assert_eq!(resolved.requested_end, d.requested_end);
    assert_eq!(resolved.effective_end, Some(actual_end));
    assert_eq!(
        PremiumRepo
            .get_latest_by_user_id(&mut txn, FOO.user.id)
            .await
            .unwrap()
            .unwrap()
            .until,
        actual_end
    );
    assert!(
        resolved
            .operational_evidence
            .unwrap()
            .contains("paid_period_observations")
    );
    // A new unsupported legacy setter stays nonbillable; the old scheduled action cannot target it.
    PremiumRepo
        .set_subscription(
            &mut txn,
            FOO.user.id,
            Some(academy_models::premium::PremiumPlan::Monthly),
        )
        .await
        .unwrap();
    assert!(
        !Repo
            .schedule_cancellation(&mut txn, d, agreement, FOO.user.id)
            .await
            .unwrap()
    );
    assert!(
        PremiumRepo
            .get_subscription(&mut txn, FOO.user.id)
            .await
            .unwrap()
            .is_some()
    );
    txn.commit().await.unwrap();
}
#[tokio::test]
async fn late_verification_preserves_receipt_boundary_and_flags_later_paid_period() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();
    let (agreement, until) = fixture(&mut txn).await;
    let d = declaration(uuid::Uuid::new_v4(), None);
    Repo.create(&mut txn, d.clone()).await.unwrap();
    let later = until + TimeDelta::days(30);
    PremiumRepo
        .extend(&mut txn, UUID1.into(), later)
        .await
        .unwrap();
    assert!(
        Repo.schedule_cancellation(&mut txn, d.clone(), agreement, FOO.user.id)
            .await
            .unwrap()
    );
    let resolved = Repo.get(&mut txn, d.id).await.unwrap().unwrap();
    assert_eq!(resolved.effective_end, Some(until));
    assert!(resolved.processing_note.unwrap().contains("Abbuchungen"));
    assert_eq!(
        PremiumRepo
            .get_latest_by_user_id(&mut txn, FOO.user.id)
            .await
            .unwrap()
            .unwrap()
            .until,
        later
    );
    txn.commit().await.unwrap();
}
#[tokio::test]
async fn later_request_cannot_postpone_an_earlier_schedule() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();
    let (agreement, until) = fixture(&mut txn).await;
    for days in [15, 75] {
        let d = declaration(uuid::Uuid::new_v4(), Some(until + TimeDelta::days(days)));
        Repo.create(&mut txn, d.clone()).await.unwrap();
        assert!(
            Repo.schedule_cancellation(&mut txn, d, agreement, FOO.user.id)
                .await
                .unwrap()
        );
    }
    PremiumRepo
        .extend(&mut txn, UUID1.into(), until + TimeDelta::days(31))
        .await
        .unwrap();
    Repo.recover_schedules(&mut txn).await.unwrap();
    assert!(
        PremiumRepo
            .get_subscription(&mut txn, FOO.user.id)
            .await
            .unwrap()
            .is_none()
    );
    let rows = txn
        .txn()
        .query(
            "SELECT effective_end FROM contract_cancellation_schedule ORDER BY requested_end",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(
        rows[0].get::<_, chrono::DateTime<Utc>>(0),
        rows[1].get::<_, chrono::DateTime<Utc>>(0)
    );
}
#[tokio::test]
async fn outbox_claim_commit_and_generation_survive_rejected_and_lost_ack() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();
    let d = declaration(uuid::Uuid::new_v4(), None);
    Repo.create(&mut txn, d.clone()).await.unwrap();
    Repo.queue_delivery(&mut txn, message(d.id)).await.unwrap();
    Repo.save_receipt_access(&mut txn, d.id, "privatehash".into())
        .await
        .unwrap();
    txn.commit().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    let first = Repo
        .claim_delivery(&mut txn, None, vec![])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.generation, 1);
    txn.rollback().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    let first = Repo
        .claim_delivery(&mut txn, None, vec![])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.generation, 1);
    txn.commit().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    assert!(
        Repo.claim_delivery(&mut txn, None, vec![])
            .await
            .unwrap()
            .is_none()
    );
    // Synthetic lease expiry isolates ownership fencing; full process probes cover wall time.
    txn.txn()
        .execute(
            "UPDATE contract_delivery SET next_attempt_at=clock_timestamp()-interval '1 second'",
            &[],
        )
        .await
        .unwrap();
    txn.commit().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    let replacement = Repo
        .claim_delivery(&mut txn, None, vec![])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(replacement.generation, 2);
    assert_eq!(replacement.body, first.body);
    txn.commit().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    Repo.acknowledge_delivery(&mut txn, first, true)
        .await
        .unwrap();
    assert!(
        Repo.get(&mut txn, d.id).await.unwrap().unwrap().delivery[0]
            .accepted_at
            .is_none()
    );
    Repo.acknowledge_delivery(&mut txn, replacement, true)
        .await
        .unwrap();
    txn.commit().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    assert!(
        Repo.get(&mut txn, d.id).await.unwrap().unwrap().delivery[0]
            .accepted_at
            .is_some()
    );
    assert_eq!(
        Repo.receipt_access(&mut txn, d.id).await.unwrap(),
        Some("privatehash".into())
    );
    // Accepted receipt cannot be queued twice or have its bytes overwritten.
    let mut changed = message(d.id);
    changed.body = "different".into();
    Repo.queue_delivery(&mut txn, changed).await.unwrap();
    assert_eq!(
        txn.txn()
            .query_one(
                "SELECT body FROM contract_delivery WHERE declaration_id=$1",
                &[&*d.id]
            )
            .await
            .unwrap()
            .get::<_, String>(0),
        message(d.id).body
    );
}
#[tokio::test]
async fn pending_and_future_evidence_is_retained_and_receipts_outlive_accounts() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();
    let d = declaration(
        uuid::Uuid::new_v4(),
        Some(Utc::now() + TimeDelta::days(400)),
    );
    Repo.create(&mut txn, d.clone()).await.unwrap();
    Repo.queue_delivery(&mut txn, message(d.id)).await.unwrap();
    Repo.save_receipt_access(&mut txn, d.id, "secret".into())
        .await
        .unwrap();
    txn.txn()
        .execute("DELETE FROM users WHERE id=$1", &[&*FOO.user.id])
        .await
        .unwrap();
    assert!(
        Repo.get(&mut txn, d.id)
            .await
            .unwrap()
            .unwrap()
            .user_id
            .is_none()
    );
    assert_eq!(
        Repo.receipt_access(&mut txn, d.id).await.unwrap(),
        Some("secret".into())
    );
    assert_eq!(
        Repo.delete_by_received_at(&mut txn, Utc::now() + TimeDelta::days(5000))
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn berlin_midnight_dst_and_month_end_use_instants_not_database_date_casts() {
    for (paid, requested, should_stop) in [
        (
            "2027-03-28T00:00:00+01:00",
            "2027-03-28T00:00:00+01:00",
            true,
        ),
        (
            "2027-03-28T23:30:00+02:00",
            "2027-03-28T00:00:00+01:00",
            true,
        ),
        (
            "2027-03-27T23:59:59+01:00",
            "2027-03-28T00:00:00+01:00",
            false,
        ),
        (
            "2027-10-31T23:30:00+01:00",
            "2027-10-31T00:00:00+02:00",
            true,
        ),
        (
            "2028-02-29T00:00:00+01:00",
            "2028-02-29T00:00:00+01:00",
            true,
        ),
    ] {
        let db = setup().await;
        let mut txn = db.begin_transaction().await.unwrap();
        let (agreement, _) = fixture(&mut txn).await;
        let paid = chrono::DateTime::parse_from_rfc3339(paid)
            .unwrap()
            .with_timezone(&Utc);
        let requested = chrono::DateTime::parse_from_rfc3339(requested)
            .unwrap()
            .with_timezone(&Utc);
        PremiumRepo
            .extend(&mut txn, UUID1.into(), paid)
            .await
            .unwrap();
        let d = declaration(uuid::Uuid::new_v4(), Some(requested));
        Repo.create(&mut txn, d.clone()).await.unwrap();
        assert!(
            Repo.schedule_cancellation(&mut txn, d.clone(), agreement, FOO.user.id)
                .await
                .unwrap()
        );
        assert_eq!(
            PremiumRepo
                .get_subscription(&mut txn, FOO.user.id)
                .await
                .unwrap()
                .is_none(),
            should_stop
        );
        assert_eq!(
            Repo.get(&mut txn, d.id)
                .await
                .unwrap()
                .unwrap()
                .effective_end,
            should_stop.then_some(paid)
        );
    }
}

#[tokio::test]
async fn receipt_observation_serializes_with_a_concurrent_manual_extension() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();
    let (agreement, until) = fixture(&mut txn).await;
    txn.commit().await.unwrap();
    let mut receipt_txn = db.begin_transaction().await.unwrap();
    let d = declaration(uuid::Uuid::new_v4(), None);
    Repo.create(&mut receipt_txn, d.clone()).await.unwrap();
    let other = db.clone();
    let mut writer = tokio::spawn(async move {
        let mut txn = other.begin_transaction().await.unwrap();
        PremiumRepo
            .get_latest_by_user_id(&mut txn, FOO.user.id)
            .await
            .unwrap();
        PremiumRepo
            .extend(&mut txn, UUID1.into(), until + TimeDelta::days(31))
            .await
            .unwrap();
        txn.commit().await.unwrap();
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut writer)
            .await
            .is_err()
    );
    receipt_txn.commit().await.unwrap();
    writer.await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    assert!(
        Repo.schedule_cancellation(&mut txn, d.clone(), agreement, FOO.user.id)
            .await
            .unwrap()
    );
    assert_eq!(
        Repo.get(&mut txn, d.id)
            .await
            .unwrap()
            .unwrap()
            .effective_end,
        Some(until)
    );
}

#[tokio::test]
async fn past_requested_date_keeps_original_intent_but_cannot_end_before_receipt() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();
    let (agreement, until) = fixture(&mut txn).await;
    let historical_end = chrono::DateTime::from_timestamp_micros(Utc::now().timestamp_micros())
        .unwrap()
        - TimeDelta::days(20);
    txn.txn()
        .execute(
            "INSERT INTO premium(id,user_id,since,until) VALUES ($1,$2,$3,$4)",
            &[
                &uuid::Uuid::new_v4(),
                &*FOO.user.id,
                &(historical_end - TimeDelta::days(30)),
                &historical_end,
            ],
        )
        .await
        .unwrap();
    let d = declaration(
        uuid::Uuid::new_v4(),
        Some(historical_end - TimeDelta::days(1)),
    );
    Repo.create(&mut txn, d.clone()).await.unwrap();
    assert!(
        Repo.schedule_cancellation(&mut txn, d.clone(), agreement, FOO.user.id)
            .await
            .unwrap()
    );
    let resolved = Repo.get(&mut txn, d.id).await.unwrap().unwrap();
    assert_eq!(resolved.requested_end, d.requested_end);
    assert_eq!(resolved.effective_end, Some(until));
}

#[tokio::test]
async fn terminal_action_fences_a_committed_claim_and_retains_correction_evidence() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();
    let (agreement, until) = fixture(&mut txn).await;
    let d = declaration(uuid::Uuid::new_v4(), None);
    Repo.create(&mut txn, d.clone()).await.unwrap();
    Repo.schedule_cancellation(&mut txn, d.clone(), agreement, FOO.user.id)
        .await
        .unwrap();
    txn.commit().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    let claim = Repo
        .claim_delivery(&mut txn, None, vec![])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claim.kind, "resolution");
    txn.commit().await.unwrap();

    let end = until - TimeDelta::days(2);
    let mut txn = db.begin_transaction().await.unwrap();
    Repo.set_processed(
        &mut txn,
        d.id,
        Utc::now(),
        Some(end),
        Some(
            "Identity, individual resolution and external communication verified"
                .try_into()
                .unwrap(),
        ),
        true,
    )
    .await
    .unwrap();
    txn.commit().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    assert!(
        !Repo
            .lock_resolution_delivery(&mut txn, &claim)
            .await
            .unwrap()
    );
    // A late failed acknowledgement also cannot clear supersession or change it.
    Repo.acknowledge_delivery(&mut txn, claim.clone(), false)
        .await
        .unwrap();
    Repo.recover_schedules(&mut txn).await.unwrap();
    let result = Repo.get(&mut txn, d.id).await.unwrap().unwrap();
    assert_eq!(result.effective_end, Some(end));
    let evidence: serde_json::Value =
        serde_json::from_str(&result.operational_evidence.unwrap()).unwrap();
    assert_eq!(
        evidence["processing_action"]["action"],
        "record_external_resolution"
    );
    assert_eq!(
        evidence["processing_action"]["previous_resolution"]["body"],
        claim.body
    );
    assert!(evidence["processing_action"]["previous_schedule"]["completed_at"].is_string());
    let correction = Repo
        .claim_delivery(&mut txn, None, vec![])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(correction.kind, "external_resolution");
    assert!(correction.body.contains("ersetzt"));
    assert!(
        Repo.claim_delivery(&mut txn, None, vec![])
            .await
            .unwrap()
            .is_none()
    );
    txn.commit().await.unwrap();
}

#[tokio::test]
async fn documented_scheduling_remains_automatic_and_operation_copies_survive_account_erasure() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();
    let (agreement, until) = fixture(&mut txn).await;
    let d = declaration(uuid::Uuid::new_v4(), Some(until + TimeDelta::days(15)));
    Repo.create(&mut txn, d.clone()).await.unwrap();
    Repo.schedule_cancellation(&mut txn, d.clone(), agreement, FOO.user.id)
        .await
        .unwrap();
    Repo.set_processed(
        &mut txn,
        d.id,
        Utc::now(),
        None,
        Some(
            "Verified original agreement and receipt date; future scheduling documented"
                .try_into()
                .unwrap(),
        ),
        false,
    )
    .await
    .unwrap();
    let later = until + TimeDelta::days(31);
    PremiumRepo
        .extend(&mut txn, UUID1.into(), later)
        .await
        .unwrap();
    assert_eq!(
        Repo.get(&mut txn, d.id)
            .await
            .unwrap()
            .unwrap()
            .effective_end,
        Some(later)
    );
    let copies: i64 = txn
        .txn()
        .query_one(
            "SELECT count(*) FROM contract_premium_operations WHERE declaration_id=$1",
            &[&*d.id],
        )
        .await
        .unwrap()
        .get(0);
    assert!(copies > 0);
    txn.txn()
        .execute("DELETE FROM users WHERE id=$1", &[&*FOO.user.id])
        .await
        .unwrap();
    let after: i64 = txn
        .txn()
        .query_one(
            "SELECT count(*) FROM contract_premium_operations WHERE declaration_id=$1",
            &[&*d.id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(copies, after);
    let journal: i64 = txn
        .txn()
        .query_one(
            "SELECT count(*) FROM premium_period_changes WHERE user_id=$1",
            &[&*FOO.user.id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(journal, 0);
    txn.commit().await.unwrap();
    assert!(
        db.revert_migrations(Some(1)).await.is_err(),
        "Retained operation/action evidence must require forward repair"
    );
}
