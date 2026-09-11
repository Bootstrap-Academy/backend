use academy_demo::{
    UUID1, UUID2,
    user::{ADMIN, BAR, FOO},
};
use academy_models::{
    premium::{Premium, PremiumPlan},
    user::UserId,
};
use academy_persistence_contracts::{Database, Transaction, premium::PremiumRepository};
use academy_persistence_postgres::{PostgresDatabase, premium::PostgresPremiumRepository};
use chrono::{TimeZone, Utc};

use crate::common::setup;

const REPO: PostgresPremiumRepository = PostgresPremiumRepository;

#[tokio::test]
async fn premium() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .get_latest_by_user_id(&mut txn, FOO.user.id)
        .await
        .unwrap();
    assert_eq!(result, None);

    let mut p1 = Premium {
        id: UUID1.into(),
        user_id: FOO.user.id,
        since: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
        until: Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap(),
    };
    let p2 = Premium {
        id: UUID2.into(),
        user_id: FOO.user.id,
        since: Utc.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap(),
        until: Utc.with_ymd_and_hms(2024, 4, 1, 0, 0, 0).unwrap(),
    };
    REPO.create(&mut txn, p1).await.unwrap();
    REPO.create(&mut txn, p2).await.unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .get_latest_by_user_id(&mut txn, FOO.user.id)
        .await
        .unwrap();
    assert_eq!(result, Some(p2));

    p1.until = Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap();
    REPO.extend(&mut txn, p1.id, p1.until).await.unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .get_latest_by_user_id(&mut txn, FOO.user.id)
        .await
        .unwrap();
    assert_eq!(result, Some(p1));
}

#[tokio::test]
async fn subscriptions() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.get_subscription(&mut txn, FOO.user.id).await.unwrap();
    assert_eq!(result, None);

    let result = REPO.list_subscription_users(&mut txn).await.unwrap();
    assert_eq!(result, []);

    REPO.set_subscription(&mut txn, FOO.user.id, Some(PremiumPlan::Monthly))
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.get_subscription(&mut txn, FOO.user.id).await.unwrap();
    assert_eq!(result, Some(PremiumPlan::Monthly));

    REPO.set_subscription(&mut txn, FOO.user.id, Some(PremiumPlan::Yearly))
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.get_subscription(&mut txn, FOO.user.id).await.unwrap();
    assert_eq!(result, Some(PremiumPlan::Yearly));

    let result = REPO.list_subscription_users(&mut txn).await.unwrap();
    assert_eq!(result, [FOO.user.id]);

    REPO.set_subscription(&mut txn, FOO.user.id, None)
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.get_subscription(&mut txn, FOO.user.id).await.unwrap();
    assert_eq!(result, None);

    let result = REPO.list_subscription_users(&mut txn).await.unwrap();
    assert_eq!(result, []);
}

// Seed both accounts directly so a broken subscribe operation does not prevent
// the cancellation and change regressions from reaching their own assertions.
async fn seed_subscriptions(db: &PostgresDatabase) {
    let txn = db.begin_transaction().await.unwrap();
    txn.txn()
        .execute(
            "insert into premium_subscriptions (user_id, plan) values ($1, 'monthly'), ($2, 'yearly')",
            &[&*FOO.user.id, &*BAR.user.id],
        )
        .await
        .unwrap();
    txn.commit().await.unwrap();
}

async fn assert_subscriptions(db: &PostgresDatabase, expected: &[(UserId, PremiumPlan)]) {
    let mut txn = db.begin_transaction().await.unwrap();
    let mut users = REPO.list_subscription_users(&mut txn).await.unwrap();
    let mut expected_users = expected.iter().map(|(id, _)| *id).collect::<Vec<_>>();
    users.sort();
    expected_users.sort();
    assert_eq!(users, expected_users);
    for &(user_id, plan) in expected {
        assert_eq!(
            REPO.get_subscription(&mut txn, user_id).await.unwrap(),
            Some(plan)
        );
    }
}

#[tokio::test]
async fn subscribing_and_changing_one_user_preserves_other_subscriptions() {
    let db = setup().await;
    seed_subscriptions(&db).await;

    for plan in [
        PremiumPlan::Monthly,
        PremiumPlan::Monthly,
        PremiumPlan::Yearly,
    ] {
        let mut txn = db.begin_transaction().await.unwrap();
        REPO.set_subscription(&mut txn, ADMIN.user.id, Some(plan))
            .await
            .unwrap();
        txn.commit().await.unwrap();
        assert_subscriptions(
            &db,
            &[
                (FOO.user.id, PremiumPlan::Monthly),
                (BAR.user.id, PremiumPlan::Yearly),
                (ADMIN.user.id, plan),
            ],
        )
        .await;
    }

    // Renewals may update a stored plan; that transition is scoped too.
    let mut txn = db.begin_transaction().await.unwrap();
    REPO.set_subscription(&mut txn, BAR.user.id, Some(PremiumPlan::Monthly))
        .await
        .unwrap();
    txn.commit().await.unwrap();
    assert_subscriptions(
        &db,
        &[
            (FOO.user.id, PremiumPlan::Monthly),
            (BAR.user.id, PremiumPlan::Monthly),
            (ADMIN.user.id, PremiumPlan::Yearly),
        ],
    )
    .await;
}

#[tokio::test]
async fn cancelling_one_user_preserves_other_subscriptions_and_paid_access() {
    let db = setup().await;
    seed_subscriptions(&db).await;
    let paid = Premium {
        id: UUID1.into(),
        user_id: FOO.user.id,
        since: Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap(),
        until: Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap(),
    };
    let mut txn = db.begin_transaction().await.unwrap();
    REPO.create(&mut txn, paid).await.unwrap();
    txn.commit().await.unwrap();

    // A repeated cancellation is harmless, including after the first commit.
    for _ in 0..2 {
        let mut txn = db.begin_transaction().await.unwrap();
        REPO.set_subscription(&mut txn, FOO.user.id, None)
            .await
            .unwrap();
        txn.commit().await.unwrap();
        assert_subscriptions(&db, &[(BAR.user.id, PremiumPlan::Yearly)]).await;
        let mut txn = db.begin_transaction().await.unwrap();
        assert_eq!(
            REPO.get_subscription(&mut txn, FOO.user.id).await.unwrap(),
            None
        );
        assert_eq!(
            REPO.get_latest_by_user_id(&mut txn, FOO.user.id)
                .await
                .unwrap(),
            Some(paid)
        );
    }
}

#[tokio::test]
async fn cancelling_a_non_subscriber_or_unknown_user_preserves_all_subscriptions() {
    let db = setup().await;
    seed_subscriptions(&db).await;

    // The public cancellation service calls this for a known email even when
    // that account has never subscribed. An absent user is a no-op as well.
    for user_id in [ADMIN.user.id, UUID1.into()] {
        let mut txn = db.begin_transaction().await.unwrap();
        REPO.set_subscription(&mut txn, user_id, None)
            .await
            .unwrap();
        txn.commit().await.unwrap();
        assert_subscriptions(
            &db,
            &[
                (FOO.user.id, PremiumPlan::Monthly),
                (BAR.user.id, PremiumPlan::Yearly),
            ],
        )
        .await;
    }
}

#[tokio::test]
async fn migration_archives_every_legacy_plan_and_response_then_defaults_off() {
    for plan in [PremiumPlan::Monthly, PremiumPlan::Yearly] {
        let db = setup().await;
        db.revert_migrations(Some(crate::repos::revert_through(
            "2026-09-07-140000_require_explicit_premium_renewal",
        )))
        .await
        .unwrap();
        let txn = db.begin_transaction().await.unwrap();
        for (idx, user) in [&FOO.user, &BAR.user, &ADMIN.user].into_iter().enumerate() {
            txn.txn().execute("insert into premium_subscriptions (user_id, plan) values ($1, $2::text::premium_plan)", &[&*user.id, &if plan == PremiumPlan::Monthly { "monthly" } else { "yearly" }]).await.unwrap();
            let id = uuid::Uuid::new_v4();
            txn.txn().execute("insert into premium (id,user_id,since,until) values ($1,$2,'2026-01-01','2099-01-01')", &[&id,&*user.id]).await.unwrap();
            txn.txn().execute("update users set terms_version=$2, terms_accepted_at=$3, terms_declined_at=$4 where id=$1", &[&*user.id, &(idx == 0).then_some("2026-09"), &(idx == 0).then_some(user.created_at), &(idx == 1).then_some(user.created_at)]).await.unwrap();
        }
        let before: Vec<(uuid::Uuid, chrono::DateTime<Utc>)> = txn
            .txn()
            .query("select id, until from premium order by id", &[])
            .await
            .unwrap()
            .into_iter()
            .map(|r| (r.get(0), r.get(1)))
            .collect();
        txn.commit().await.unwrap();
        db.run_migrations(None).await.unwrap();
        let mut txn = db.begin_transaction().await.unwrap();
        assert!(
            REPO.list_subscription_users(&mut txn)
                .await
                .unwrap()
                .is_empty()
        );
        let rows = txn.txn().query("select observed_terms_version, observed_terms_declined_at, jsonb_array_length(paid_periods), plan::text from premium_legacy_renewals order by user_id", &[]).await.unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows.iter()
                .filter(|r| r.get::<_, Option<String>>(0).is_some())
                .count(),
            1
        );
        assert_eq!(
            rows.iter()
                .filter(|r| r.get::<_, Option<chrono::DateTime<Utc>>>(1).is_some())
                .count(),
            1
        );
        for row in rows {
            assert_eq!(row.get::<_, i32>(2), 1);
            assert_eq!(
                row.get::<_, String>(3),
                if plan == PremiumPlan::Monthly {
                    "monthly"
                } else {
                    "yearly"
                }
            );
        }
        let after: Vec<(uuid::Uuid, chrono::DateTime<Utc>)> = txn
            .txn()
            .query("select id, until from premium order by id", &[])
            .await
            .unwrap()
            .into_iter()
            .map(|r| (r.get(0), r.get(1)))
            .collect();
        assert_eq!(before, after);
        txn.commit().await.unwrap();
        assert!(
            db.revert_migrations(Some(crate::repos::revert_through(
                "2026-09-07-160000_fix_premium_confirmation_deadline"
            )))
            .await
            .is_err(),
            "Evidence must block destructive downgrade"
        );
    }
}

#[tokio::test]
async fn renewal_evidence_price_delivery_and_cancellation_are_independent() {
    use academy_models::premium::PremiumRenewalAgreement;
    let db = setup().await;
    let agreement = PremiumRenewalAgreement {
        id: UUID1.into(),
        user_id: FOO.user.id,
        received_at: FOO.user.created_at,
        paid_period_id: Some(UUID2.into()),
        confirmation_deadline: Some(Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap()),
        offer_id: "exact-offer-v1".into(),
        monthly_price: 750,
        recipient: "customer@example.invalid".into(),
        document: "Immutable 750 coin monthly agreement".into(),
        terms_pdf: vec![1, 2, 3],
        withdrawal_pdf: vec![4, 5, 6],
    };
    let mut txn = db.begin_transaction().await.unwrap();
    REPO.create(
        &mut txn,
        Premium {
            id: UUID2.into(),
            user_id: FOO.user.id,
            since: FOO.user.created_at,
            until: Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap(),
        },
    )
    .await
    .unwrap();
    REPO.create_renewal(&mut txn, &agreement).await.unwrap();
    assert_eq!(
        REPO.get_renewal_agreement(&mut txn, agreement.id)
            .await
            .unwrap(),
        Some(agreement.clone())
    );
    let status = REPO
        .get_renewal(&mut txn, FOO.user.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status.monthly_price, 750);
    assert!(!status.confirmation_sent);
    assert_eq!(
        REPO.pending_renewal_confirmations(&mut txn).await.unwrap(),
        vec![agreement.clone()]
    );
    REPO.record_renewal_delivery(&mut txn, agreement.id, false)
        .await
        .unwrap();
    assert!(
        !REPO
            .get_renewal(&mut txn, FOO.user.id)
            .await
            .unwrap()
            .unwrap()
            .confirmation_sent
    );
    REPO.record_renewal_delivery(&mut txn, agreement.id, true)
        .await
        .unwrap();
    assert!(
        REPO.get_renewal(&mut txn, FOO.user.id)
            .await
            .unwrap()
            .unwrap()
            .confirmation_sent
    );
    assert!(
        REPO.pending_renewal_confirmations(&mut txn)
            .await
            .unwrap()
            .is_empty()
    );
    REPO.set_subscription(&mut txn, FOO.user.id, None)
        .await
        .unwrap();
    assert!(
        REPO.get_renewal(&mut txn, FOO.user.id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        REPO.get_renewal_agreement(&mut txn, agreement.id)
            .await
            .unwrap(),
        Some(agreement)
    );
    assert_eq!(
        txn.txn()
            .query_one("select attempts from premium_renewal_delivery", &[])
            .await
            .unwrap()
            .get::<_, i64>(0),
        2
    );
    assert_eq!(
        txn.txn()
            .query_one("select count(*) from premium_renewal_cancellations", &[])
            .await
            .unwrap()
            .get::<_, i64>(0),
        1
    );
    txn.commit().await.unwrap();
    let txn = db.begin_transaction().await.unwrap();
    assert!(
        txn.txn()
            .execute(
                "update premium_renewal_agreements set monthly_price=9999",
                &[]
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn legacy_setter_and_late_confirmation_never_authorize_a_debit() {
    use academy_models::premium::PremiumRenewalAgreement;
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();
    REPO.set_subscription(&mut txn, FOO.user.id, Some(PremiumPlan::Yearly))
        .await
        .unwrap();
    assert_eq!(REPO.get_renewal(&mut txn, FOO.user.id).await.unwrap(), None);
    REPO.create(
        &mut txn,
        Premium {
            id: UUID2.into(),
            user_id: FOO.user.id,
            since: FOO.user.created_at,
            until: Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
        },
    )
    .await
    .unwrap();
    REPO.create_renewal(
        &mut txn,
        &PremiumRenewalAgreement {
            id: UUID1.into(),
            user_id: FOO.user.id,
            received_at: FOO.user.created_at,
            paid_period_id: Some(UUID2.into()),
            confirmation_deadline: Some(Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap()),
            offer_id: "offer".into(),
            monthly_price: 1000,
            recipient: "customer@example.invalid".into(),
            document: "contract".into(),
            terms_pdf: vec![],
            withdrawal_pdf: vec![],
        },
    )
    .await
    .unwrap();
    REPO.record_renewal_delivery(&mut txn, UUID1.into(), true)
        .await
        .unwrap();
    assert!(
        !REPO
            .get_renewal(&mut txn, FOO.user.id)
            .await
            .unwrap()
            .unwrap()
            .confirmation_sent
    );
}

#[tokio::test]
async fn renewal_export_deletion_and_retention_cover_the_new_personal_data() {
    use academy_models::premium::PremiumRenewalAgreement;
    use academy_persistence_contracts::user::UserRepository;
    use academy_persistence_postgres::user::PostgresUserRepository;
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();
    let a = PremiumRenewalAgreement {
        id: UUID1.into(),
        user_id: FOO.user.id,
        received_at: FOO.user.created_at,
        paid_period_id: Some(UUID2.into()),
        confirmation_deadline: Some(Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap()),
        offer_id: "exact-offer".into(),
        monthly_price: 750,
        recipient: "customer@example.invalid".into(),
        document: "Retained monthly agreement".into(),
        terms_pdf: vec![1, 2, 3],
        withdrawal_pdf: vec![4, 5, 6],
    };
    REPO.create_renewal(&mut txn, &a).await.unwrap();
    let exported = REPO
        .export_renewal_evidence(&mut txn, FOO.user.id)
        .await
        .unwrap();
    assert!(
        exported.contains("customer@example.invalid")
            && exported.contains("Retained monthly agreement")
            && exported.contains("AQID")
    );
    assert!(
        !REPO
            .export_renewal_evidence(&mut txn, BAR.user.id)
            .await
            .unwrap()
            .contains("customer@example.invalid")
    );
    let future = Utc.with_ymd_and_hms(2100, 1, 1, 0, 0, 0).unwrap();
    assert_eq!(
        REPO.prune_renewal_evidence(&mut txn, future).await.unwrap(),
        0,
        "Active agreement retained"
    );
    PostgresUserRepository
        .delete(&mut txn, FOO.user.id)
        .await
        .unwrap();
    assert_eq!(
        REPO.get_subscription(&mut txn, FOO.user.id).await.unwrap(),
        None
    );
    assert!(
        REPO.pending_renewal_confirmations(&mut txn)
            .await
            .unwrap()
            .is_empty(),
        "No mail after deletion"
    );
    assert_eq!(
        REPO.get_renewal_agreement(&mut txn, a.id).await.unwrap(),
        Some(a)
    );
    assert_eq!(
        REPO.prune_renewal_evidence(&mut txn, Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap())
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        REPO.prune_renewal_evidence(&mut txn, future).await.unwrap(),
        1
    );
    for table in [
        "premium_renewal_agreements",
        "premium_renewal_delivery",
        "premium_renewal_cancellations",
    ] {
        assert_eq!(
            txn.txn()
                .query_one(&format!("select count(*) from {table}"), &[])
                .await
                .unwrap()
                .get::<_, i64>(0),
            0
        );
    }
}

#[tokio::test]
async fn successful_delivery_uses_completion_time_and_keeps_first_success() {
    use academy_models::premium::PremiumRenewalAgreement;
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();
    let deadline = txn
        .txn()
        .query_one("select clock_timestamp() + interval '50 milliseconds'", &[])
        .await
        .unwrap()
        .get(0);
    let agreement = PremiumRenewalAgreement {
        id: UUID1.into(),
        user_id: FOO.user.id,
        received_at: FOO.user.created_at,
        paid_period_id: Some(UUID2.into()),
        confirmation_deadline: Some(deadline),
        offer_id: "timing".into(),
        monthly_price: 750,
        recipient: "local@example.invalid".into(),
        document: "saved bytes".into(),
        terms_pdf: vec![1],
        withdrawal_pdf: vec![2],
    };
    REPO.create_renewal(&mut txn, &agreement).await.unwrap();
    // The real transaction predates the deadline, but the send completes after it.
    txn.txn()
        .execute("select pg_sleep(0.1)", &[])
        .await
        .unwrap();
    REPO.record_renewal_delivery(&mut txn, agreement.id, true)
        .await
        .unwrap();
    let first: chrono::DateTime<Utc> = txn
        .txn()
        .query_one("select sent_at from premium_renewal_delivery", &[])
        .await
        .unwrap()
        .get(0);
    assert!(first >= deadline);
    assert!(
        !REPO
            .get_renewal(&mut txn, FOO.user.id)
            .await
            .unwrap()
            .unwrap()
            .confirmation_sent
    );
    for sent in [false, true] {
        REPO.record_renewal_delivery(&mut txn, agreement.id, sent)
            .await
            .unwrap();
        assert_eq!(
            txn.txn()
                .query_one("select sent_at from premium_renewal_delivery", &[])
                .await
                .unwrap()
                .get::<_, chrono::DateTime<Utc>>(0),
            first
        );
    }
    // Changing even the latest period cannot repair this agreement's deadline.
    REPO.create(
        &mut txn,
        Premium {
            id: UUID2.into(),
            user_id: FOO.user.id,
            since: FOO.user.created_at,
            until: Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap(),
        },
    )
    .await
    .unwrap();
    assert!(
        !REPO
            .get_renewal(&mut txn, FOO.user.id)
            .await
            .unwrap()
            .unwrap()
            .confirmation_sent
    );
    REPO.get_latest_by_user_id(&mut txn, FOO.user.id)
        .await
        .unwrap();
    assert_eq!(
        REPO.get_subscription(&mut txn, FOO.user.id).await.unwrap(),
        None
    );
    assert_eq!(
        REPO.get_renewal_agreement(&mut txn, agreement.id)
            .await
            .unwrap(),
        Some(agreement)
    );
}

#[tokio::test]
async fn deadline_boundary_is_strict_and_snapshot_cannot_be_rewritten() {
    use academy_models::premium::PremiumRenewalAgreement;
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();
    let deadline = Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap();
    let agreement = PremiumRenewalAgreement {
        id: UUID1.into(),
        user_id: FOO.user.id,
        received_at: FOO.user.created_at,
        paid_period_id: Some(UUID2.into()),
        confirmation_deadline: Some(deadline),
        offer_id: "boundary".into(),
        monthly_price: 750,
        recipient: "local@example.invalid".into(),
        document: "saved bytes".into(),
        terms_pdf: vec![1],
        withdrawal_pdf: vec![2],
    };
    REPO.create_renewal(&mut txn, &agreement).await.unwrap();
    for (offset, timely) in [(-1i64, true), (0, false), (1, false)] {
        let sent = deadline + chrono::Duration::microseconds(offset);
        txn.txn()
            .execute("update premium_renewal_delivery set sent_at=$1", &[&sent])
            .await
            .unwrap();
        assert_eq!(
            REPO.get_renewal(&mut txn, FOO.user.id)
                .await
                .unwrap()
                .unwrap()
                .confirmation_sent,
            timely
        );
    }
    txn.commit().await.unwrap();
    for column in [
        "confirmation_deadline=clock_timestamp()",
        "paid_period_id=null",
    ] {
        let txn = db.begin_transaction().await.unwrap();
        assert!(
            txn.txn()
                .execute(
                    &format!("update premium_renewal_agreements set {column}"),
                    &[]
                )
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn cancellation_during_locked_delivery_stays_terminal_and_workers_skip_locked_rows() {
    use academy_models::premium::PremiumRenewalAgreement;
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();
    let agreement = PremiumRenewalAgreement {
        id: UUID1.into(),
        user_id: FOO.user.id,
        received_at: FOO.user.created_at,
        paid_period_id: Some(UUID2.into()),
        confirmation_deadline: Some(Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap()),
        offer_id: "concurrent".into(),
        monthly_price: 750,
        recipient: "local@example.invalid".into(),
        document: "saved bytes".into(),
        terms_pdf: vec![1],
        withdrawal_pdf: vec![2],
    };
    REPO.create_renewal(&mut txn, &agreement).await.unwrap();
    txn.commit().await.unwrap();
    let mut delivery = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.pending_renewal_confirmations(&mut delivery)
            .await
            .unwrap(),
        vec![agreement.clone()]
    );
    let mut other = db.begin_transaction().await.unwrap();
    assert!(
        REPO.pending_renewal_confirmations(&mut other)
            .await
            .unwrap()
            .is_empty()
    );
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        REPO.set_subscription(&mut other, FOO.user.id, None),
    )
    .await
    .unwrap()
    .unwrap();
    other.commit().await.unwrap();
    REPO.record_renewal_delivery(&mut delivery, agreement.id, true)
        .await
        .unwrap();
    delivery.commit().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(REPO.get_renewal(&mut txn, FOO.user.id).await.unwrap(), None);
    assert_eq!(
        REPO.get_renewal_agreement(&mut txn, agreement.id)
            .await
            .unwrap(),
        Some(agreement)
    );
}

#[tokio::test]
async fn deadline_migration_retains_unproven_agreements_but_disables_their_debits() {
    let db = setup().await;
    db.revert_migrations(Some(crate::repos::revert_through(
        "2026-09-07-160000_fix_premium_confirmation_deadline",
    )))
    .await
    .unwrap();
    let txn = db.begin_transaction().await.unwrap();
    txn.txn()
        .execute(
            "insert into premium (id,user_id,since,until) values ($1,$2,'2026-01-01T00:00:00Z','2099-01-01T00:00:00Z')",
            &[&UUID2, &*FOO.user.id],
        )
        .await
        .unwrap();
    txn.txn().execute("insert into premium_renewal_agreements (id,user_id,received_at,offer_id,monthly_price,recipient,document,terms_pdf,withdrawal_pdf) values ($1,$2,current_timestamp,'old',750,'local@example.invalid','original',decode('01','hex'),decode('02','hex'))", &[&UUID1, &*FOO.user.id]).await.unwrap();
    txn.txn().execute("insert into premium_renewal_delivery (agreement_id,sent_at) values ($1,current_timestamp)", &[&UUID1]).await.unwrap();
    txn.txn().execute("insert into premium_subscriptions (user_id,plan,agreement_id) values ($1,'monthly',$2)", &[&*FOO.user.id,&UUID1]).await.unwrap();
    txn.commit().await.unwrap();
    db.run_migrations(None).await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.get_subscription(&mut txn, FOO.user.id).await.unwrap(),
        None
    );
    let retained = REPO
        .get_renewal_agreement(&mut txn, UUID1.into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retained.document, "original");
    assert_eq!(retained.terms_pdf, vec![1]);
    assert_eq!(retained.confirmation_deadline, None);
    assert_eq!(retained.paid_period_id, None);
    assert_eq!(
        REPO.get_latest_by_user_id(&mut txn, FOO.user.id)
            .await
            .unwrap()
            .unwrap()
            .until,
        Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap()
    );
    assert_eq!(
        txn.txn()
            .query_one(
                "select count(*) from premium_renewal_cancellations where agreement_id=$1",
                &[&UUID1]
            )
            .await
            .unwrap()
            .get::<_, i64>(0),
        1
    );
    txn.commit().await.unwrap();
    assert!(
        db.revert_migrations(Some(crate::repos::revert_through(
            "2026-09-07-160000_fix_premium_confirmation_deadline"
        )))
        .await
        .is_err()
    );
}
