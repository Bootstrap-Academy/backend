use academy_di::Build;
use academy_models::{
    premium::{
        Premium, PremiumId, PremiumPlan, PremiumRenewalAgreement, PremiumRenewalId,
        PremiumRenewalStatus,
    },
    user::UserId,
};
use academy_persistence_contracts::premium::PremiumRepository;
use academy_utils::trace_instrument;
use chrono::{DateTime, Utc};
use clorinde::{
    client::Params,
    queries::{
        self,
        premium::{CreateParams, ExtendParams},
    },
};
use futures::{StreamExt, TryStreamExt};

use crate::PostgresTransaction;

#[derive(Debug, Clone, Build)]
pub struct PostgresPremiumRepository;

impl PremiumRepository<PostgresTransaction> for PostgresPremiumRepository {
    async fn export_renewal_evidence(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<String> {
        Ok(txn.txn().query_one(
            "select jsonb_build_object(\
             'legacy_observations', (select to_jsonb(l) from premium_legacy_renewals l where l.user_id=$1), \
             'agreements', coalesce((select jsonb_agg(to_jsonb(a)-'terms_pdf'-'withdrawal_pdf' || jsonb_build_object(\
                 'terms_pdf_base64',encode(a.terms_pdf,'base64'),'withdrawal_pdf_base64',encode(a.withdrawal_pdf,'base64'),\
                 'delivery',to_jsonb(d),'cancellation',to_jsonb(c)) order by a.received_at) \
                 from premium_renewal_agreements a left join premium_renewal_delivery d on d.agreement_id=a.id \
                 left join premium_renewal_cancellations c on c.agreement_id=a.id where a.user_id=$1), '[]'))::text", &[&*user_id],
        ).await?.get(0))
    }

    async fn prune_renewal_evidence(
        &self,
        txn: &mut PostgresTransaction,
        cutoff: DateTime<Utc>,
    ) -> anyhow::Result<u64> {
        // Declaration-specific copies outlive account deletion and this journal.
        txn.txn().execute("DELETE FROM premium_period_changes WHERE greatest(recorded_at,(new_period->>'until')::timestamptz,(old_period->>'until')::timestamptz)<$1", &[&cutoff]).await?;
        let agreements = txn.txn().execute(
            "delete from premium_renewal_agreements a where not exists(select 1 from commercial_renewal_holds h where h.agreement_id=a.id) and not exists (select 1 from contract_cancellation_schedule c where c.agreement_id=a.id) and not exists (select 1 from premium_subscriptions s where s.agreement_id=a.id) \
             and greatest(a.received_at, (select greatest(c.cancelled_at,c.paid_until) from premium_renewal_cancellations c where c.agreement_id=a.id)) < $1", &[&cutoff],
        ).await?;
        let legacy = txn.txn().execute(
            "delete from premium_legacy_renewals l where not exists(select 1 from commercial_legacy_renewal_holds h where h.user_id=l.user_id) and greatest(l.archived_at, \
             (select max((p->>'until')::timestamptz) from jsonb_array_elements(l.paid_periods) p)) < $1", &[&cutoff],
        ).await?;
        Ok(agreements + legacy)
    }
    async fn renewal_allowed(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<bool> {
        lock_user(txn, user_id).await?;
        Ok(txn
            .txn()
            .query_opt(
                "select enabled from user_composites where id=$1",
                &[&*user_id],
            )
            .await?
            .is_some_and(|row| row.get::<_, bool>(0)))
    }

    async fn get_renewal(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Option<PremiumRenewalStatus>> {
        Ok(txn.txn().query_opt(
            "select a.id, a.monthly_price, coalesce(d.sent_at < a.confirmation_deadline, false) as sent from premium_subscriptions s \
             join premium_renewal_agreements a on a.id=s.agreement_id and a.user_id=s.user_id \
             join premium_renewal_delivery d on d.agreement_id=a.id \
             where s.user_id=$1 and s.plan='monthly'",
            &[&*user_id],
        ).await?.map(|r| PremiumRenewalStatus { id: r.get::<_, uuid::Uuid>(0).into(), monthly_price: r.get::<_, i64>(1) as u64, confirmation_sent: r.get(2) }))
    }

    async fn get_renewal_agreement(
        &self,
        txn: &mut PostgresTransaction,
        id: PremiumRenewalId,
    ) -> anyhow::Result<Option<PremiumRenewalAgreement>> {
        Ok(txn
            .txn()
            .query_opt(
                "select * from premium_renewal_agreements where id=$1",
                &[&*id],
            )
            .await?
            .map(decode_agreement))
    }

    async fn create_renewal(
        &self,
        txn: &mut PostgresTransaction,
        agreement: &PremiumRenewalAgreement,
    ) -> anyhow::Result<()> {
        lock_user(txn, agreement.user_id).await?;
        anyhow::ensure!(
            agreement.paid_period_id.is_some() && agreement.confirmation_deadline.is_some(),
            "New renewal requires an immutable paid-period deadline"
        );
        let price = i64::try_from(agreement.monthly_price)?;
        txn.txn().execute(
            "insert into premium_renewal_agreements (id,user_id,received_at,offer_id,monthly_price,recipient,document,terms_pdf,withdrawal_pdf,paid_period_id,confirmation_deadline) \
             values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
            &[&*agreement.id,&*agreement.user_id,&agreement.received_at,&agreement.offer_id,&price,&agreement.recipient,&agreement.document,&agreement.terms_pdf,&agreement.withdrawal_pdf,&agreement.paid_period_id.map(|id| *id),&agreement.confirmation_deadline],
        ).await?;
        txn.txn()
            .execute(
                "insert into premium_renewal_delivery (agreement_id) values ($1)",
                &[&*agreement.id],
            )
            .await?;
        // Retain the superseded declaration as well as an explicit end marker.
        self.set_subscription(txn, agreement.user_id, None).await?;
        txn.txn().execute(
            "insert into premium_subscriptions (user_id,plan,agreement_id) values ($1,'monthly',$2)",
            &[&*agreement.user_id,&*agreement.id],
        ).await?;
        Ok(())
    }

    async fn pending_renewal_confirmations(
        &self,
        txn: &mut PostgresTransaction,
    ) -> anyhow::Result<Vec<PremiumRenewalAgreement>> {
        // Row locks prevent simultaneous workers sending the same outbox item.
        // A crash after SMTP acceptance can repeat a confirmation, never a charge.
        Ok(txn.txn().query(
            "select a.* from premium_renewal_agreements a join premium_renewal_delivery d on d.agreement_id=a.id join users u on u.id=a.user_id \
             where d.sent_at is null order by a.received_at limit 100 for update of d skip locked", &[],
        ).await?.into_iter().map(decode_agreement).collect())
    }

    async fn record_renewal_delivery(
        &self,
        txn: &mut PostgresTransaction,
        id: PremiumRenewalId,
        sent: bool,
    ) -> anyhow::Result<()> {
        txn.txn().execute(
            // Evaluated after SMTP returns, even for later items in a long batch.
            // Never overwrite a previously recorded successful acceptance.
            "update premium_renewal_delivery set attempts=attempts+1, last_attempt_at=clock_timestamp(), \
             sent_at=case when $2 then coalesce(sent_at,clock_timestamp()) else sent_at end where agreement_id=$1", &[&*id,&sent],
        ).await?;
        Ok(())
    }

    #[trace_instrument(skip(self, txn))]
    async fn get_latest_by_user_id(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Option<Premium>> {
        // Serialize renewal/purchase with cancellation before reading expiry.
        lock_user(txn, user_id).await?;
        // Reconcile the immutable activation deadline BEFORE a manual purchase
        // can extend/create paid access. No task or earlier status read is needed.
        // Delivery workers lock only outbox rows, so cancellation does not wait
        // on SMTP or acquire those locks in the opposite order.
        let missed = txn.txn().query_opt(
            "select a.id from premium_subscriptions s \
             join premium_renewal_agreements a on a.id=s.agreement_id and a.user_id=s.user_id \
             join premium_renewal_delivery d on d.agreement_id=a.id \
             where s.user_id=$1 and (a.confirmation_deadline is null or a.confirmation_deadline <= clock_timestamp()) \
             and not coalesce(d.sent_at < a.confirmation_deadline, false)", &[&*user_id],
        ).await?.is_some();
        if missed {
            self.set_subscription(txn, user_id, None).await?;
        }
        crate::contract::reconcile_cancellations(txn, user_id).await?;
        queries::premium::get_latest_by_user_id()
            .bind(txn.txn(), &user_id)
            .opt()
            .await
            .map_err(Into::into)
            .map(|row| row.map(decode_premium))
    }

    #[trace_instrument(skip(self, txn))]
    async fn create(&self, txn: &mut PostgresTransaction, premium: Premium) -> anyhow::Result<()> {
        let params = CreateParams {
            id: *premium.id,
            user_id: *premium.user_id,
            since: premium.since.into(),
            until: premium.until.into(),
        };

        queries::premium::create()
            .params(txn.txn(), &params)
            .await?;
        txn.observe_premium_after_commit(*premium.user_id);
        crate::contract::reconcile_cancellations(txn, premium.user_id).await
    }

    #[trace_instrument(skip(self, txn))]
    async fn extend(
        &self,
        txn: &mut PostgresTransaction,
        id: PremiumId,
        until: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let params = ExtendParams {
            id: *id,
            until: until.into(),
        };

        queries::premium::extend()
            .params(txn.txn(), &params)
            .await?;
        let row = txn
            .txn()
            .query_one("SELECT user_id FROM premium WHERE id=$1", &[&*id])
            .await?;
        txn.observe_premium_after_commit(row.get(0));
        crate::contract::reconcile_cancellations(txn, row.get::<_, uuid::Uuid>(0).into()).await
    }

    #[trace_instrument(skip(self, txn))]
    async fn list_subscription_users(
        &self,
        txn: &mut PostgresTransaction,
    ) -> anyhow::Result<Vec<UserId>> {
        queries::premium::list_subscription_users()
            .bind(txn.txn())
            .iter()
            .await?
            .map(|row| row.map_err(Into::into).map(UserId::from))
            .try_collect()
            .await
    }

    #[trace_instrument(skip(self, txn))]
    async fn get_subscription(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Option<PremiumPlan>> {
        queries::premium::get_subscription()
            .bind(txn.txn(), &user_id)
            .opt()
            .await
            .map_err(Into::into)
            .map(|row| row.map(decode_plan))
    }

    #[trace_instrument(skip(self, txn))]
    async fn set_subscription(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
        plan: Option<PremiumPlan>,
    ) -> anyhow::Result<()> {
        lock_user(txn, user_id).await?;
        txn.txn().execute(
            "insert into premium_renewal_cancellations (agreement_id, paid_until) \
             select agreement_id, (select max(until) from premium where user_id=$1) from premium_subscriptions where user_id=$1 and agreement_id is not null \
             on conflict do nothing", &[&*user_id],
        ).await?;
        match plan {
            Some(plan) => {
                queries::premium::set_subscription()
                    .bind(txn.txn(), &user_id, &encode_plan(plan))
                    .await?;
                // This low-level legacy setter is not a consent record. Only
                // create_renewal may attach a billable agreement.
                txn.txn()
                    .execute(
                        "update premium_subscriptions set agreement_id=null where user_id=$1",
                        &[&*user_id],
                    )
                    .await?;
            }
            None => {
                queries::premium::delete_subscription()
                    .bind(txn.txn(), &user_id)
                    .await?;
            }
        }
        Ok(())
    }
}

async fn lock_user(txn: &PostgresTransaction, user_id: UserId) -> anyhow::Result<()> {
    txn.txn()
        .query_opt("select id from users where id=$1 for update", &[&*user_id])
        .await?;
    Ok(())
}

fn decode_agreement(r: bb8_postgres::tokio_postgres::Row) -> PremiumRenewalAgreement {
    PremiumRenewalAgreement {
        id: r.get::<_, uuid::Uuid>("id").into(),
        user_id: r.get::<_, uuid::Uuid>("user_id").into(),
        received_at: r.get("received_at"),
        paid_period_id: r
            .get::<_, Option<uuid::Uuid>>("paid_period_id")
            .map(Into::into),
        confirmation_deadline: r.get("confirmation_deadline"),
        offer_id: r.get("offer_id"),
        monthly_price: r.get::<_, i64>("monthly_price") as u64,
        recipient: r.get("recipient"),
        document: r.get("document"),
        terms_pdf: r.get("terms_pdf"),
        withdrawal_pdf: r.get("withdrawal_pdf"),
    }
}

fn decode_premium(value: queries::premium::Premium) -> Premium {
    Premium {
        id: value.id.into(),
        user_id: value.user_id.into(),
        since: value.since.into(),
        until: value.until.into(),
    }
}

fn encode_plan(plan: PremiumPlan) -> clorinde::types::PremiumPlan {
    match plan {
        PremiumPlan::Monthly => clorinde::types::PremiumPlan::monthly,
        PremiumPlan::Yearly => clorinde::types::PremiumPlan::yearly,
    }
}

fn decode_plan(value: clorinde::types::PremiumPlan) -> PremiumPlan {
    match value {
        clorinde::types::PremiumPlan::monthly => PremiumPlan::Monthly,
        clorinde::types::PremiumPlan::yearly => PremiumPlan::Yearly,
    }
}
