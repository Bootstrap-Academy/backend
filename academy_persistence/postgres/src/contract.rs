use std::str::FromStr;

use academy_di::Build;
use academy_models::{
    contract::{
        ContractCancellationType, ContractDeclaration, ContractDeclarationId,
        ContractDeclarationKind, ContractDeliveryAttempt, ContractDeliveryStatus, ContractKind,
        ContractProcessingNote,
    },
    pagination::PaginationSlice,
    user::UserId,
};
use academy_persistence_contracts::contract::ContractRepository;
use academy_utils::trace_instrument;
use chrono::{DateTime, Utc};
use clorinde::{
    client::Params,
    queries::{
        self,
        contract::{CreateParams, ListParams, SetProcessedParams},
    },
};
use futures::{StreamExt, TryStreamExt};

use crate::PostgresTransaction;

#[derive(Debug, Clone, Build)]
pub struct PostgresContractRepository;

impl ContractRepository<PostgresTransaction> for PostgresContractRepository {
    async fn recover_schedules(&self, txn: &mut PostgresTransaction) -> anyhow::Result<()> {
        let rows=txn.txn().query("SELECT DISTINCT a.user_id FROM contract_cancellation_schedule c JOIN premium_renewal_agreements a ON a.id=c.agreement_id WHERE c.completed_at IS NULL AND NOT EXISTS(SELECT 1 FROM contract_processing_actions x WHERE x.declaration_id=c.declaration_id AND x.action IN ('record_external_resolution','legacy_unknown')) AND (c.requested_end<=clock_timestamp() OR EXISTS(SELECT 1 FROM premium p WHERE p.user_id=a.user_id AND p.until>=c.requested_end) OR NOT EXISTS(SELECT 1 FROM premium_subscriptions s WHERE s.agreement_id=a.id)) ORDER BY a.user_id LIMIT 100", &[]).await?;
        for r in rows {
            let id = r.get::<_, uuid::Uuid>(0);
            txn.txn()
                .query_opt("SELECT id FROM users WHERE id=$1 FOR UPDATE", &[&id])
                .await?;
            reconcile_cancellations(txn, id.into()).await?;
        }
        let held=txn.txn().query_one("SELECT count(*) FROM contract_processing_actions a WHERE action='legacy_unknown' AND (EXISTS(SELECT 1 FROM contract_cancellation_schedule s WHERE s.declaration_id=a.declaration_id AND s.completed_at IS NULL) OR EXISTS(SELECT 1 FROM contract_delivery m WHERE m.declaration_id=a.declaration_id AND m.last_error='LegacyActionRequiresReview'))", &[]).await?.get::<_,i64>(0);
        if held > 0 {
            tracing::warn!(
                count = held,
                "Legacy declaration actions require explicit review and forward repair; automatic determinations and old retries remain held"
            );
        }
        let row=txn.txn().query_one("SELECT count(*), min(received_at) FROM contract_declarations WHERE processed_at IS NULL", &[]).await?;
        let count: i64 = row.get(0);
        if count > 0 {
            tracing::warn!(count,oldest=?row.get::<_,Option<DateTime<Utc>>>(1),"Declarations require immediate identity/date review; receipt-based rights remain effective");
        }
        let row = txn
            .txn()
            .query_one(
                "SELECT count(*),min(created_at) FROM contract_delivery WHERE accepted_at IS NULL AND superseded_at IS NULL",
                &[],
            )
            .await?;
        let count: i64 = row.get(0);
        if count > 0 {
            tracing::warn!(count,oldest=?row.get::<_,Option<DateTime<Utc>>>(1),"Pending declaration confirmations require delivery monitoring");
        }
        Ok(())
    }

    async fn lock_request(
        &self,
        txn: &mut PostgresTransaction,
        id: ContractDeclarationId,
    ) -> anyhow::Result<()> {
        txn.txn()
            .query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended($1, 12012))",
                &[&(*id).to_string()],
            )
            .await?;
        Ok(())
    }
    async fn lock_processing(
        &self,
        txn: &mut PostgresTransaction,
        id: ContractDeclarationId,
    ) -> anyhow::Result<()> {
        txn.txn().query_opt("SELECT u.id FROM users u JOIN contract_account_observation o ON o.user_id=u.id WHERE o.declaration_id=$1 FOR UPDATE OF u", &[&*id]).await?;
        txn.txn()
            .query_opt(
                "SELECT id FROM contract_declarations WHERE id=$1 FOR UPDATE",
                &[&*id],
            )
            .await?;
        Ok(())
    }
    async fn lock_resolution_delivery(
        &self,
        txn: &mut PostgresTransaction,
        message: &ContractDeliveryAttempt,
    ) -> anyhow::Result<bool> {
        Ok(txn.txn().query_opt("SELECT declaration_id FROM contract_delivery WHERE declaration_id=$1 AND kind='resolution' AND attempts=$2 AND accepted_at IS NULL AND superseded_at IS NULL AND next_attempt_at>clock_timestamp() FOR UPDATE", &[&*message.declaration_id,&message.generation]).await?.is_some())
    }
    async fn receipt_access(
        &self,
        txn: &mut PostgresTransaction,
        id: ContractDeclarationId,
    ) -> anyhow::Result<Option<String>> {
        Ok(txn
            .txn()
            .query_opt(
                "SELECT secret_hash FROM contract_receipt_access WHERE declaration_id=$1",
                &[&*id],
            )
            .await?
            .map(|r| r.get(0)))
    }
    async fn save_receipt_access(
        &self,
        txn: &mut PostgresTransaction,
        id: ContractDeclarationId,
        secret_hash: String,
    ) -> anyhow::Result<()> {
        txn.txn()
            .execute(
                "INSERT INTO contract_receipt_access VALUES ($1,$2)",
                &[&*id, &secret_hash],
            )
            .await?;
        Ok(())
    }
    async fn queue_delivery(
        &self,
        txn: &mut PostgresTransaction,
        message: ContractDeliveryAttempt,
    ) -> anyhow::Result<()> {
        txn.txn().execute("INSERT INTO contract_delivery(declaration_id,kind,recipient,subject,body,requested_agreement_id) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING", &[&*message.declaration_id,&message.kind,&message.recipient.as_str(),&message.subject,&message.body,&message.requested_agreement_id.map(|id|*id)]).await?;
        Ok(())
    }
    async fn claim_delivery(
        &self,
        txn: &mut PostgresTransaction,
        only: Option<ContractDeclarationId>,
        attempted: Vec<String>,
    ) -> anyhow::Result<Option<ContractDeliveryAttempt>> {
        txn.txn().query_opt("WITH due AS (SELECT declaration_id,kind FROM contract_delivery WHERE accepted_at IS NULL AND superseded_at IS NULL AND next_attempt_at <= clock_timestamp() AND ($1::uuid IS NULL OR (declaration_id=$1 AND kind='receipt')) AND NOT(declaration_id::text || ':' || kind = ANY($2::text[])) ORDER BY next_attempt_at,declaration_id,kind LIMIT 1 FOR UPDATE SKIP LOCKED) UPDATE contract_delivery d SET attempts=attempts+1,next_attempt_at=clock_timestamp()+interval '60 seconds',last_error='UnacknowledgedAttempt' FROM due WHERE d.declaration_id=due.declaration_id AND d.kind=due.kind RETURNING d.*", &[&only.map(|id|*id),&attempted]).await?.map(|r| Ok::<_,anyhow::Error>(ContractDeliveryAttempt {
            requested_agreement_id:r.get::<_,Option<uuid::Uuid>>("requested_agreement_id").map(Into::into),declaration_id:r.get::<_,uuid::Uuid>("declaration_id").into(), kind:r.get("kind"),recipient:r.get::<_,String>("recipient").parse()?,subject:r.get("subject"),body:r.get("body"),generation:r.get("attempts"),
        })).transpose()
    }
    async fn acknowledge_delivery(
        &self,
        txn: &mut PostgresTransaction,
        message: ContractDeliveryAttempt,
        accepted: bool,
    ) -> anyhow::Result<()> {
        txn.txn().execute("UPDATE contract_delivery SET accepted_at=CASE WHEN $4 THEN clock_timestamp() ELSE NULL END,next_attempt_at=clock_timestamp()+interval '60 seconds',last_error=CASE WHEN $4 THEN NULL ELSE 'DeliveryFailed' END WHERE declaration_id=$1 AND kind=$2 AND attempts=$3 AND accepted_at IS NULL AND superseded_at IS NULL", &[&*message.declaration_id,&message.kind,&message.generation,&accepted]).await?;
        Ok(())
    }
    async fn schedule_cancellation(
        &self,
        txn: &mut PostgresTransaction,
        declaration: ContractDeclaration,
        agreement_id: academy_models::premium::PremiumRenewalId,
        user_id: UserId,
    ) -> anyhow::Result<bool> {
        txn.txn()
            .query_opt("SELECT id FROM users WHERE id=$1 FOR UPDATE", &[&*user_id])
            .await?;
        let Some(row) = txn.txn().query_opt("SELECT u.email FROM premium_subscriptions s JOIN premium_renewal_agreements a ON a.id=s.agreement_id AND a.user_id=s.user_id JOIN users u ON u.id=s.user_id AND u.email_verified AND u.email IS NOT NULL JOIN contract_account_observation o ON o.declaration_id=$3 AND o.agreement_id=a.id AND o.user_id=s.user_id WHERE s.user_id=$1 AND a.id=$2 AND EXISTS (SELECT 1 FROM premium WHERE user_id=$1)", &[&*user_id,&*agreement_id,&*declaration.id]).await? else {return Ok(false)};
        txn.txn().execute("INSERT INTO contract_cancellation_schedule(declaration_id,agreement_id,requested_end,established_recipient) VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING", &[&*declaration.id,&*agreement_id,&declaration.requested_end.unwrap_or(declaration.received_at),&row.get::<_,String>(0)]).await?;
        reconcile_cancellations(txn, user_id).await?;
        Ok(true)
    }

    #[trace_instrument(skip(self, txn, declaration), fields(declaration_id = %*declaration.id))]
    async fn create(
        &self,
        txn: &mut PostgresTransaction,
        declaration: ContractDeclaration,
    ) -> anyhow::Result<()> {
        let observation = if let Some(user_id) = declaration.user_id {
            let account = txn.txn().query_one("SELECT clock_timestamp() AS observed_at,pg_current_snapshot()::text AS receipt_snapshot,(SELECT agreement_id FROM premium_subscriptions WHERE user_id=$1) AS agreement_id", &[&*user_id]).await?;
            let periods = txn.txn().query("SELECT id,since,until,clock_timestamp() AS observed_at FROM premium WHERE user_id=$1", &[&*user_id]).await?;
            txn.txn()
                .query_opt("SELECT id FROM users WHERE id=$1 FOR UPDATE", &[&*user_id])
                .await?;
            Some((user_id, account, periods))
        } else {
            None
        };
        let params = CreateParams {
            id: *declaration.id,
            kind: encode_kind(declaration.kind),
            received_at: declaration.received_at.into(),
            name: &*declaration.name,
            email: declaration.email.as_str(),
            user_id: declaration.user_id.map(|user_id| *user_id),
            contract: encode_contract(declaration.contract),
            contract_designation: declaration
                .contract_designation
                .as_deref()
                .map(ToOwned::to_owned),
            cancellation_type: declaration.cancellation_type.map(encode_cancellation_type),
            details: &*declaration.details,
            requested_end: declaration.requested_end.map(Into::into),
            effective_end: declaration.effective_end.map(Into::into),
            processed_at: declaration.processed_at.map(Into::into),
            processing_note: declaration
                .processing_note
                .as_deref()
                .map(ToOwned::to_owned),
        };

        queries::contract::create()
            .params(txn.txn(), &params)
            .await?;
        if let Some((user_id, account, periods)) = observation {
            txn.txn()
                .query_one(
                    "SELECT observe_contract_purchases($1,$2)",
                    &[&*declaration.id, &*user_id],
                )
                .await?;
            txn.txn().execute("INSERT INTO contract_account_observation(declaration_id,user_id,agreement_id,observed_at) VALUES($1,$2,$3,$4)", &[&*declaration.id,&*user_id,&account.get::<_,Option<uuid::Uuid>>("agreement_id"),&account.get::<_,DateTime<Utc>>("observed_at")]).await?;
            for period in periods {
                txn.txn().execute("INSERT INTO contract_period_observation(declaration_id,premium_id,since,until,source,observed_at) VALUES($1,$2,$3,$4,'receipt',$5)", &[&*declaration.id,&period.get::<_,uuid::Uuid>("id"),&period.get::<_,DateTime<Utc>>("since"),&period.get::<_,DateTime<Utc>>("until"),&period.get::<_,DateTime<Utc>>("observed_at")]).await?;
            }
            txn.txn().execute("INSERT INTO contract_period_observation(declaration_id,premium_id,since,until,source) SELECT $1,id,since,until,'after_account_lock' FROM premium WHERE user_id=$2", &[&*declaration.id,&*user_id]).await?;
            // Only a durable observation made after the operation committed can
            // prove it preceded receipt. A mutation time or later snapshot cannot.
            // Missing witnesses remain unknown, including process death after COMMIT.
            txn.txn().execute("INSERT INTO contract_premium_operations(declaration_id,operation_id,evidence) SELECT $1,p.id,to_jsonb(p) || jsonb_build_object('commit_observed_at',w.observed_at,'shared_receipt_clock',$5::boolean,'receipt_visibility',CASE WHEN pg_visible_in_snapshot(transaction_id,$4::text::pg_snapshot) THEN 'visible_at_receipt_read' ELSE 'in_flight_at_receipt_read' END,'receipt_ordering',CASE WHEN NOT $5 THEN 'commit_order_unknown_at_receipt' WHEN recorded_at >= $3 THEN 'write_at_or_after_receipt' WHEN NOT pg_visible_in_snapshot(transaction_id,$4::text::pg_snapshot) THEN 'in_flight_at_receipt_read' ELSE 'commit_order_unknown_at_receipt' END) FROM premium_period_changes p LEFT JOIN premium_period_commit_observation w ON w.operation_id=p.id WHERE user_id=$2 AND (new_period->>'until')::timestamptz >= $3 AND (recorded_at >= $3 OR NOT pg_visible_in_snapshot(transaction_id,$4::text::pg_snapshot) OR NOT $5 OR w.observed_at IS NULL OR w.observed_at >= $3) ON CONFLICT DO NOTHING", &[&*declaration.id,&*user_id,&declaration.received_at,&account.get::<_,String>("receipt_snapshot"),&txn.premium_shares_receipt_clock()]).await?;
            txn.txn().execute("INSERT INTO contract_period_observation(declaration_id,premium_id,since,until,source,observed_at) SELECT $1,(evidence->>'premium_id')::uuid,(period->>'since')::timestamptz,(period->>'until')::timestamptz,source,(evidence->>'recorded_at')::timestamptz FROM contract_premium_operations CROSS JOIN LATERAL (VALUES(evidence->'old_period','operation_before_write'),(evidence->'new_period','operation_after_write')) p(period,source) WHERE declaration_id=$1 AND period IS NOT NULL AND period<>'null'::jsonb ON CONFLICT DO NOTHING", &[&*declaration.id]).await?;
        }
        Ok(())
    }

    #[trace_instrument(skip(self, txn))]
    async fn get(
        &self,
        txn: &mut PostgresTransaction,
        id: ContractDeclarationId,
    ) -> anyhow::Result<Option<ContractDeclaration>> {
        let declaration = queries::contract::get()
            .bind(txn.txn(), &id)
            .opt()
            .await?
            .map(decode_declaration)
            .transpose()?;
        match declaration {
            Some(d) => Ok(Some(with_delivery(txn, d).await?)),
            None => Ok(None),
        }
    }

    #[trace_instrument(skip(self, txn))]
    async fn set_processed(
        &self,
        txn: &mut PostgresTransaction,
        id: ContractDeclarationId,
        processed_at: DateTime<Utc>,
        effective_end: Option<DateTime<Utc>>,
        processing_note: Option<ContractProcessingNote>,
        external_resolution: bool,
    ) -> anyhow::Result<Option<ContractDeclaration>> {
        self.lock_processing(txn, id).await?;
        // A sending resolution owns this row through its bounded SMTP call and
        // acknowledgement. A committed old claim waiting to send sees supersession.
        txn.txn().query_opt("SELECT declaration_id FROM contract_delivery WHERE declaration_id=$1 AND kind='resolution' FOR UPDATE", &[&*id]).await?;
        txn.txn().execute("INSERT INTO contract_processing_actions(declaration_id,action,recorded_at,effective_end,note,previous_effective_end,previous_resolution,previous_schedule) SELECT id,$2,$3,$4,$5,effective_end,(SELECT to_jsonb(m) FROM contract_delivery m WHERE m.declaration_id=$1 AND m.kind='resolution'),(SELECT to_jsonb(s) FROM contract_cancellation_schedule s WHERE s.declaration_id=$1) FROM contract_declarations WHERE id=$1", &[&*id,&if external_resolution { "record_external_resolution" } else { "schedule_premium_cancellation" },&processed_at,&effective_end,&processing_note.as_deref().map(ToOwned::to_owned)]).await?;
        if external_resolution {
            txn.txn().execute("UPDATE contract_cancellation_schedule SET completed_at=clock_timestamp(),effective_end=$2 WHERE declaration_id=$1", &[&*id,&effective_end]).await?;
            txn.txn().execute("UPDATE contract_delivery SET superseded_at=clock_timestamp(),last_error=CASE WHEN accepted_at IS NULL THEN 'SupersededByExternalResolution' ELSE last_error END WHERE declaration_id=$1 AND kind='resolution'", &[&*id]).await?;
            // An earlier automatic message may have reached SMTP even without
            // acknowledgement. Preserve it and issue a distinct explicit correction.
            let body = format!(
                "Bootstrap Academy GmbH\nBestätigung der individuell dokumentierten Erledigung\nReferenz: {}\nBestätigter Beendigungszeitpunkt: {}\nDiese Bestätigung ersetzt eine zuvor versandte automatische Bestimmung des Beendigungszeitpunkts. Die individuell geprüfte Erledigung und die gesondert dokumentierte Kommunikation sind maßgeblich. Bereits bezahlter Zugang bleibt erhalten.\n",
                *id,
                effective_end
                    .map(|end| end
                        .with_timezone(&chrono_tz::Europe::Berlin)
                        .format("%d.%m.%Y %H:%M:%S %Z")
                        .to_string())
                    .unwrap_or_else(|| "gesondert dokumentiert".into())
            );
            txn.txn().execute("INSERT INTO contract_delivery(declaration_id,kind,recipient,subject,body) SELECT declaration_id,'external_resolution',recipient,'Bestätigung der individuell dokumentierten Erledigung',$2 FROM contract_delivery WHERE declaration_id=$1 AND kind='resolution' AND attempts>0 ON CONFLICT DO NOTHING", &[&*id,&body]).await?;
        }
        let params = SetProcessedParams {
            processed_at: processed_at.into(),
            effective_end: effective_end.map(Into::into),
            processing_note: processing_note.as_deref().map(ToOwned::to_owned),
            id: *id,
        };

        let declaration = queries::contract::set_processed()
            .params(txn.txn(), &params)
            .opt()
            .await?
            .map(decode_declaration)
            .transpose()?;
        match declaration {
            Some(d) => Ok(Some(with_delivery(txn, d).await?)),
            None => Ok(None),
        }
    }

    #[trace_instrument(skip(self, txn))]
    async fn list(
        &self,
        txn: &mut PostgresTransaction,
        kind: Option<ContractDeclarationKind>,
        pagination: PaginationSlice,
    ) -> anyhow::Result<Vec<ContractDeclaration>> {
        let params = ListParams {
            kind: kind.map(encode_kind),
            limit: (*pagination.limit).try_into()?,
            offset: pagination.offset.try_into()?,
        };

        let declarations: Vec<ContractDeclaration> = queries::contract::list()
            .params(txn.txn(), &params)
            .iter()
            .await?
            .map(|row| row.map_err(Into::into).and_then(decode_declaration))
            .try_collect()
            .await?;
        let mut result = Vec::new();
        for d in declarations {
            result.push(with_delivery(txn, d).await?);
        }
        Ok(result)
    }

    #[trace_instrument(skip(self, txn))]
    async fn list_by_user_id(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Vec<ContractDeclaration>> {
        let declarations: Vec<ContractDeclaration> = queries::contract::list_by_user_id()
            .bind(txn.txn(), &user_id)
            .iter()
            .await?
            .map(|row| row.map_err(Into::into).and_then(decode_declaration))
            .try_collect()
            .await?;
        let mut result = Vec::new();
        for d in declarations {
            result.push(with_delivery(txn, d).await?);
        }
        Ok(result)
    }

    #[trace_instrument(skip(self, txn))]
    async fn count(
        &self,
        txn: &mut PostgresTransaction,
        kind: Option<ContractDeclarationKind>,
    ) -> anyhow::Result<u64> {
        let kind = kind.map(encode_kind);

        queries::contract::count()
            .bind(txn.txn(), &kind)
            .one()
            .await
            .map_err(Into::into)
            .and_then(|row| row.try_into().map_err(Into::into))
    }

    #[trace_instrument(skip(self, txn))]
    async fn delete_by_received_at(
        &self,
        txn: &mut PostgresTransaction,
        received_at: DateTime<Utc>,
    ) -> anyhow::Result<u64> {
        Ok(txn.txn().execute("DELETE FROM contract_declarations d WHERE NOT EXISTS(SELECT 1 FROM commercial_contract_holds h WHERE h.declaration_id=d.id) AND greatest(d.received_at,d.processed_at,d.requested_end,d.effective_end)<$1 AND NOT EXISTS(SELECT 1 FROM contract_processing_actions a WHERE a.declaration_id=d.id AND a.action='legacy_unknown') AND NOT EXISTS (SELECT 1 FROM contract_delivery o WHERE o.declaration_id=d.id AND o.accepted_at IS NULL AND o.superseded_at IS NULL) AND NOT EXISTS (SELECT 1 FROM contract_cancellation_schedule c WHERE c.declaration_id=d.id AND c.completed_at IS NULL) AND (d.processed_at IS NOT NULL OR NOT EXISTS(SELECT 1 FROM contract_delivery o WHERE o.declaration_id=d.id))", &[&received_at]).await?)
    }
}

fn decode_declaration(
    value: queries::contract::ContractDeclaration,
) -> anyhow::Result<ContractDeclaration> {
    Ok(ContractDeclaration {
        delivery: Vec::new(),
        operational_evidence: None,
        id: value.id.into(),
        kind: decode_kind(value.kind),
        received_at: value.received_at.into(),
        name: value.name.try_into()?,
        email: FromStr::from_str(&value.email)?,
        user_id: value.user_id.map(Into::into),
        contract: decode_contract(value.contract),
        contract_designation: value
            .contract_designation
            .map(TryInto::try_into)
            .transpose()?,
        cancellation_type: value.cancellation_type.map(decode_cancellation_type),
        details: value.details.try_into()?,
        requested_end: value.requested_end.map(Into::into),
        effective_end: value.effective_end.map(Into::into),
        processed_at: value.processed_at.map(Into::into),
        processing_note: value.processing_note.map(TryInto::try_into).transpose()?,
    })
}

fn encode_kind(kind: ContractDeclarationKind) -> clorinde::types::ContractDeclarationKind {
    match kind {
        ContractDeclarationKind::Cancellation => {
            clorinde::types::ContractDeclarationKind::cancellation
        }
        ContractDeclarationKind::Withdrawal => clorinde::types::ContractDeclarationKind::withdrawal,
    }
}

fn decode_kind(value: clorinde::types::ContractDeclarationKind) -> ContractDeclarationKind {
    match value {
        clorinde::types::ContractDeclarationKind::cancellation => {
            ContractDeclarationKind::Cancellation
        }
        clorinde::types::ContractDeclarationKind::withdrawal => ContractDeclarationKind::Withdrawal,
    }
}

fn encode_contract(contract: ContractKind) -> clorinde::types::ContractDeclarationContract {
    match contract {
        ContractKind::Premium => clorinde::types::ContractDeclarationContract::premium,
        ContractKind::Coins => clorinde::types::ContractDeclarationContract::coins,
        ContractKind::Other => clorinde::types::ContractDeclarationContract::other,
    }
}

fn decode_contract(value: clorinde::types::ContractDeclarationContract) -> ContractKind {
    match value {
        clorinde::types::ContractDeclarationContract::premium => ContractKind::Premium,
        clorinde::types::ContractDeclarationContract::coins => ContractKind::Coins,
        clorinde::types::ContractDeclarationContract::other => ContractKind::Other,
    }
}

fn encode_cancellation_type(
    cancellation_type: ContractCancellationType,
) -> clorinde::types::ContractCancellationType {
    match cancellation_type {
        ContractCancellationType::Ordinary => clorinde::types::ContractCancellationType::ordinary,
        ContractCancellationType::Extraordinary => {
            clorinde::types::ContractCancellationType::extraordinary
        }
    }
}

fn decode_cancellation_type(
    value: clorinde::types::ContractCancellationType,
) -> ContractCancellationType {
    match value {
        clorinde::types::ContractCancellationType::ordinary => ContractCancellationType::Ordinary,
        clorinde::types::ContractCancellationType::extraordinary => {
            ContractCancellationType::Extraordinary
        }
    }
}

/// Called with the per-user lock from every paid-period read/write. A schedule
/// targets the immutable agreement, never a replacement consent. The final
/// monthly end is known only when that period exists; no future debit time is invented.
pub async fn reconcile_cancellations(
    txn: &mut PostgresTransaction,
    user_id: UserId,
) -> anyhow::Result<()> {
    use academy_persistence_contracts::premium::PremiumRepository;
    let rows = txn.txn().query("SELECT c.declaration_id,c.agreement_id,c.requested_end,c.established_recipient, (SELECT received_at FROM contract_declarations WHERE id=c.declaration_id) AS received_at, (SELECT max(until) FROM premium WHERE user_id=$1) AS paid_until, (SELECT agreement_id FROM premium_subscriptions WHERE user_id=$1) AS current_id FROM contract_cancellation_schedule c JOIN premium_renewal_agreements a ON a.id=c.agreement_id WHERE a.user_id=$1 AND c.completed_at IS NULL AND NOT EXISTS(SELECT 1 FROM contract_processing_actions x WHERE x.declaration_id=c.declaration_id AND x.action IN ('record_external_resolution','legacy_unknown')) ORDER BY c.created_at FOR UPDATE OF c", &[&*user_id]).await?;
    for row in rows {
        let id: uuid::Uuid = row.get("declaration_id");
        let agreement: uuid::Uuid = row.get("agreement_id");
        let declared_target: DateTime<Utc> = row.get("requested_end");
        let target = declared_target.max(row.get::<_, DateTime<Utc>>("received_at"));
        let paid: Option<DateTime<Utc>> = row.get("paid_until");
        let current: Option<uuid::Uuid> = row.get("current_id");
        let now = Utc::now();
        if current == Some(agreement) && paid.is_some_and(|until| until < target) && now < target {
            continue;
        }
        let boundary:Option<DateTime<Utc>>=txn.txn().query_one("SELECT min(until) FROM contract_period_observation WHERE declaration_id=$1 AND until >= $2 AND (source='receipt' OR since < $2)", &[&id,&target]).await?.get(0);
        let effective = if current == Some(agreement) {
            crate::premium::PostgresPremiumRepository
                .set_subscription(txn, user_id, None)
                .await?;
            boundary.unwrap_or_else(|| {
                paid.map(|until| until.max(target.min(now)))
                    .unwrap_or(target.min(now))
            })
        } else {
            // A different cancellation, missed confirmation or insufficient balance
            // ended this agreement. Never use a replacement agreement's paid periods.
            txn.txn().query_opt("SELECT greatest(cancelled_at,paid_until) FROM premium_renewal_cancellations WHERE agreement_id=$1", &[&agreement]).await?.map(|r|r.get(0)).unwrap_or(now)
        };
        txn.txn().execute("UPDATE contract_cancellation_schedule SET completed_at=clock_timestamp(),effective_end=$2 WHERE declaration_id=$1", &[&id,&effective]).await?;
        txn.txn().execute("UPDATE contract_declarations SET effective_end=$2, processing_note=CASE WHEN $3 THEN concat_ws(E'\n',processing_note,'Prüfung erforderlich: spätere bezahlte Zeiträume/Abbuchungen mit ursprünglichem Kündigungseingang abgleichen; Zugang erhalten, keine automatische Erstattung verbucht.') ELSE processing_note END WHERE id=$1", &[&id,&effective,&paid.is_some_and(|until|until>effective)]).await?;
        let overlap: bool = txn.txn().query_one("SELECT EXISTS(SELECT 1 FROM contract_premium_operations WHERE declaration_id=$1 AND (evidence->>'receipt_ordering'='commit_order_unknown_at_receipt' OR ((evidence->>'transaction_started_at')::timestamptz <= $2 AND ((evidence->>'recorded_at')::timestamptz >= $2 OR evidence->>'receipt_visibility'='in_flight_at_receipt_read'))))", &[&id,&row.get::<_,DateTime<Utc>>("received_at")]).await?.get(0);
        if overlap {
            txn.txn().execute("UPDATE contract_declarations SET processing_note=concat_ws(E'\\n',processing_note,'Zeitliche Zuordnung unklar: Für einen Kauf/eine Periodenänderung ist ein Abschluss vor Erklärungseingang nicht belegt oder eine Überschneidung dokumentiert. Verfügbare ursprüngliche Grenzen, beide Periodenstände und tatsächliche Transaktionsbelege sind erhalten; zeitnahe rechtliche/finanzielle Prüfung erforderlich, keine automatische Erstattung oder Rückabwicklung.') WHERE id=$1", &[&id]).await?;
        }
        let body = format!(
            "Bootstrap Academy GmbH\nErgänzende Kündigungsbestätigung\nReferenz: {id}\nGewünschter Zeitpunkt: {}\nBeendigungszeitpunkt der zugeordneten Verlängerungsvereinbarung: {}\nDie automatische Verlängerung dieser Vereinbarung ist ausgeschaltet. Bereits bezahlter Zugang bleibt erhalten. Eine zuvor beendete Vereinbarung wird nicht reaktiviert. Bei Abweichungen oder weitergehenden Rechten prüfen wir Ihre Erklärung gesondert; der ursprüngliche Eingangszeitpunkt bleibt maßgeblich.\n",
            declared_target
                .with_timezone(&chrono_tz::Europe::Berlin)
                .format("%d.%m.%Y %H:%M:%S %Z"),
            effective
                .with_timezone(&chrono_tz::Europe::Berlin)
                .format("%d.%m.%Y %H:%M:%S %Z")
        );
        let body = if overlap {
            format!(
                "{body}Die zeitliche Zuordnung eines Kaufs/einer Periodenänderung zu Ihrem Erklärungseingang ist nicht abschließend belegt oder es ist eine Überschneidung dokumentiert. Die verfügbaren ursprünglichen Grenzen und Kaufbelege bleiben erhalten. Deren rechtliche/finanzielle Behandlung prüfen wir zeitnah gesondert; eine Erstattung oder Rückabwicklung ist damit nicht festgestellt.\n"
            )
        } else {
            body
        };
        txn.txn().execute("INSERT INTO contract_delivery(declaration_id,kind,recipient,subject,body) VALUES ($1,'resolution',$2,'Ergänzende Kündigungsbestätigung',$3) ON CONFLICT DO NOTHING", &[&id,&row.get::<_,String>("established_recipient"),&body]).await?;
    }
    Ok(())
}

async fn with_delivery(
    txn: &mut PostgresTransaction,
    mut declaration: ContractDeclaration,
) -> anyhow::Result<ContractDeclaration> {
    declaration.delivery=txn.txn().query("SELECT kind,attempts,next_attempt_at,accepted_at,last_error FROM contract_delivery WHERE declaration_id=$1 ORDER BY kind", &[&*declaration.id]).await?.into_iter().map(|r|ContractDeliveryStatus{kind:r.get(0),attempts:r.get(1),next_attempt_at:r.get(2),accepted_at:r.get(3),last_error:r.get(4)}).collect();
    declaration.operational_evidence=txn.txn().query_one("SELECT CASE WHEN EXISTS(SELECT 1 FROM contract_account_observation WHERE declaration_id=$1) OR EXISTS(SELECT 1 FROM contract_delivery WHERE declaration_id=$1) THEN jsonb_build_object('account_observation',(SELECT to_jsonb(o) FROM contract_account_observation o WHERE declaration_id=$1),'paid_period_observations',COALESCE((SELECT jsonb_agg(to_jsonb(p) ORDER BY observed_at,until) FROM contract_period_observation p WHERE declaration_id=$1),'[]'::jsonb),'premium_operations',COALESCE((SELECT jsonb_agg(evidence ORDER BY operation_id) FROM contract_premium_operations WHERE declaration_id=$1),'[]'::jsonb),'purchase_observations',COALESCE((SELECT jsonb_agg(to_jsonb(p) ORDER BY observed_at,order_id) FROM contract_purchase_observations p WHERE declaration_id=$1),'[]'::jsonb),'processing_action',(SELECT to_jsonb(a) FROM contract_processing_actions a WHERE a.declaration_id=$1),'schedule',(SELECT to_jsonb(s) FROM contract_cancellation_schedule s WHERE declaration_id=$1),'messages',COALESCE((SELECT jsonb_agg(to_jsonb(d) ORDER BY kind) FROM contract_delivery d WHERE declaration_id=$1),'[]'::jsonb))::text ELSE NULL END", &[&*declaration.id]).await?.get(0);
    Ok(declaration)
}
