use crate::PostgresTransaction;
use academy_di::Build;
use academy_models::{
    purchase::{PurchaseRecord, PurchaseStatus, display_hearts},
    user::UserId,
};
use academy_persistence_contracts::{
    moderation::CommercialPurchaseReadError, purchase::PurchaseRepository,
};
use bb8_postgres::tokio_postgres::Row;
use uuid::Uuid;

#[derive(Debug, Clone, Build)]
pub struct PostgresPurchaseRepository;

/// Personal original-order observations. Deliberately separate from the locking,
/// document-bearing purchase reader used by existing purchase operations.
pub(crate) async fn commercial_status(
    txn: &mut PostgresTransaction,
    id: Uuid,
    owner: UserId,
) -> Result<PurchaseStatus, CommercialPurchaseReadError> {
    let row = txn.txn().query_opt(
        "SELECT o.id,o.user_id,o.offer::text,p.state,p.review_reason,p.smtp_accepted_at,a.accepted_at,f.result::text,CASE WHEN cash.order_id IS NOT NULL THEN cash.evidence::text WHEN d.order_id IS NOT NULL THEN jsonb_build_object('charged_coins',-(d.ledger->>'coins')::bigint,'ledger_id',d.ledger->>'id','no_charge',false)::text WHEN a.order_id IS NOT NULL AND (o.offer->'product'->>'coins')::bigint=0 AND p.state IN ('paid','fulfilled','review') THEN jsonb_build_object('charged_coins',0,'ledger_id',NULL,'no_charge',true)::text END,(SELECT evidence::text FROM purchase_provision_observations w WHERE w.order_id=o.id),(SELECT coalesce(jsonb_agg(c.document_kind ORDER BY c.document_kind),'[]'::jsonb)::text FROM purchase_document_corrections c WHERE c.order_id=o.id) FROM purchase_offers o LEFT JOIN purchase_progress p ON p.order_id=o.id LEFT JOIN purchase_acceptances a ON a.order_id=o.id LEFT JOIN purchase_fulfillments f ON f.order_id=o.id LEFT JOIN purchase_debits d ON d.order_id=o.id LEFT JOIN purchase_cash_captures cash ON cash.order_id=o.id WHERE o.id=$1 AND o.user_id=$2",
        &[&id, &*owner],
    ).await.map_err(anyhow::Error::from)?
        .ok_or(CommercialPurchaseReadError::Unavailable)?;
    let decode = || -> anyhow::Result<PurchaseStatus> {
        let state = row
            .get::<_, Option<String>>(3)
            .ok_or_else(|| anyhow::anyhow!("Known offer has no progress observation"))?;
        let mut status = PurchaseStatus {
            offer: serde_json::from_str(row.get(2))?,
            state,
            review_reason: row.get(4),
            confirmation_smtp_accepted_at: row.get(5),
            accepted_at: row.get(6),
            fulfillment: row
                .get::<_, Option<String>>(7)
                .map(|v| serde_json::from_str(&v))
                .transpose()?,
            financial_evidence: row
                .get::<_, Option<String>>(8)
                .map(|v| serde_json::from_str(&v))
                .transpose()?,
            provision_timing: row
                .get::<_, Option<String>>(9)
                .map(|v| serde_json::from_str(&v))
                .transpose()?,
            document_corrections: serde_json::from_str(row.get(10))?,
            provision_deadline: None,
        };
        anyhow::ensure!(
            row.get::<_, Uuid>(0) == id
                && row.get::<_, Uuid>(1) == *owner
                && status.offer.id == id
                && status.offer.user_id == *owner,
            "Stored original offer identity differs from its admitted owner"
        );
        status.provision_deadline = status.provision_deadline();
        Ok(status)
    };
    decode().map_err(|_| CommercialPurchaseReadError::Unavailable)
}

const SELECT: &str = "SELECT o.offer::text,o.terms_pdf,o.withdrawal_pdf,p.state,p.review_reason,p.smtp_accepted_at,p.generation,a.accepted_at,a.confirmation_body,f.result::text,s.request::text,a.message_metadata::text,CASE WHEN cash.order_id IS NOT NULL THEN cash.evidence::text WHEN d.order_id IS NOT NULL THEN jsonb_build_object('charged_coins',-(d.ledger->>'coins')::bigint,'ledger_id',d.ledger->>'id','no_charge',false)::text WHEN a.order_id IS NOT NULL AND (o.offer->'product'->>'coins')::bigint=0 AND p.state IN ('paid','fulfilled','review') THEN jsonb_build_object('charged_coins',0,'ledger_id',NULL,'no_charge',true)::text END,(SELECT evidence::text FROM purchase_provision_observations w WHERE w.order_id=o.id),(SELECT coalesce(jsonb_agg(c.document_kind ORDER BY c.document_kind),'[]'::jsonb)::text FROM purchase_document_corrections c WHERE c.order_id=o.id) FROM purchase_offers o JOIN purchase_progress p ON p.order_id=o.id LEFT JOIN purchase_submissions s ON s.order_id=o.id LEFT JOIN purchase_acceptances a ON a.order_id=o.id LEFT JOIN purchase_fulfillments f ON f.order_id=o.id LEFT JOIN purchase_debits d ON d.order_id=o.id LEFT JOIN purchase_cash_captures cash ON cash.order_id=o.id";

fn decode(r: Row) -> anyhow::Result<PurchaseRecord> {
    let mut record = PurchaseRecord {
        status: PurchaseStatus {
            offer: serde_json::from_str(r.get(0))?,
            state: r.get(3),
            review_reason: r.get(4),
            confirmation_smtp_accepted_at: r.get(5),
            accepted_at: r.get(7),
            provision_deadline: None,
            document_corrections: serde_json::from_str(r.get(14))?,
            provision_timing: r
                .get::<_, Option<String>>(13)
                .map(|s| serde_json::from_str(&s))
                .transpose()?,
            financial_evidence: r
                .get::<_, Option<String>>(12)
                .map(|v| serde_json::from_str(&v))
                .transpose()?,
            fulfillment: r
                .get::<_, Option<String>>(9)
                .map(|s| serde_json::from_str(&s))
                .transpose()?,
        },
        terms_pdf: r.get(1),
        withdrawal_pdf: r.get(2),
        confirmation_body: r.get(8),
        delivery_generation: r.get(6),
        submission: r
            .get::<_, Option<String>>(10)
            .map(|s| serde_json::from_str(&s))
            .transpose()?,
        message_metadata: r
            .get::<_, Option<String>>(11)
            .map(|s| serde_json::from_str(&s))
            .transpose()?,
    };
    record.status.provision_deadline = record.status.provision_deadline();
    Ok(record)
}

impl PurchaseRepository<PostgresTransaction> for PostgresPurchaseRepository {
    async fn observe_provision(
        &self,
        txn: &mut PostgresTransaction,
        id: Uuid,
    ) -> anyhow::Result<()> {
        // This method is only called in a NEW transaction after the producer
        // committed. The read must finish before the database clock is sampled.
        let Some(row) = txn.txn().query_opt("SELECT f.result::text,purchase_provision_deadline(f.order_id),o.source='events' AND o.offer->'product'->'facts'->>'availability_protocol'='committed_candidate_v1' FROM purchase_fulfillments f JOIN purchase_offers o ON o.id=f.order_id WHERE f.order_id=$1", &[&id]).await? else { return Ok(()); };
        let now = self.timestamp(txn).await?;
        let deadline: Option<chrono::DateTime<chrono::Utc>> = row.get(1);
        let fulfillment: serde_json::Value = serde_json::from_str(row.get(0))?;
        // Events' validated source observation may arrive later: it proves
        // earlier usable source state, independently of this backend mapping.
        let source_observed = (row.get::<_, Option<bool>>(2) == Some(true)
            && fulfillment["timing_basis"] == "committed_candidate_v1")
            .then(|| {
                fulfillment["availability_observed_at"]
                    .as_str()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            })
            .flatten();
        let timely = deadline.is_some_and(|d| now < d || source_observed.is_some_and(|s| s < d));
        let evidence = serde_json::json!({"basis":"committed_provision_observed_v1","observed_at":now,"source_availability_observed_at":source_observed,"deadline":deadline,"committed_before_deadline_proven":timely,"fulfillment":fulfillment});
        let source_detail = source_observed.map(|s| format!("Die nutzbaren Zugangsdaten wurden bereits am {s} in einer neuen Transaktion der Buchungsplattform beobachtet; dieser erhaltene Nachweis wird bei der Fristprüfung berücksichtigt.\n")).unwrap_or_default();
        let statement = format!(
            "Zeitnachweis zur Bereitstellung – Bootstrap Academy\nBestellung: {id}\nDer gebuchte Vorgang war bei der Prüfung am {now} (UTC) bereits gespeichert. Dieser Beobachtungszeitpunkt ist kein genauer Beginn der ersten Nutzbarkeit und belegt weder die tatsächliche Nutzung noch den Abschluss einer Leistung.\n{source_detail}Gespeicherter Vorgang oder gesondert nachgewiesener Buchungszugang vor der vereinbarten Frist beobachtet: {timely}. Bei später oder fehlender Beobachtung bleibt die zeitliche Einordnung zur Klärung offen; eine tatsächlich bereitgestellte Leistung und Ihre Ansprüche werden nicht rückwirkend aufgehoben.\n"
        );
        txn.txn().execute("INSERT INTO purchase_provision_observations(order_id,observed_at,evidence,statement,statement_version) VALUES($1,$2,$3::text::jsonb,$4,2) ON CONFLICT DO NOTHING", &[&id,&now,&evidence.to_string(),&statement]).await?;
        txn.txn().execute("UPDATE purchase_progress p SET state='review',review_reason='Actual provision retained; timely committed availability is not proven, claims require review' FROM purchase_provision_observations w WHERE w.order_id=p.order_id AND p.order_id=$1 AND p.state='fulfilled' AND purchase_provision_deadline(p.order_id) IS NOT NULL AND NOT (w.evidence->>'committed_before_deadline_proven')::boolean", &[&id]).await?;
        Ok(())
    }
    async fn timing_statement(
        &self,
        txn: &mut PostgresTransaction,
        id: Uuid,
        original: bool,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(txn
            .txn()
            .query_opt(
                "SELECT CASE WHEN $2 THEN w.statement ELSE coalesce(c.statement,CASE WHEN w.statement_version=2 THEN w.statement END) END FROM purchase_provision_observations w LEFT JOIN purchase_document_corrections c ON c.order_id=w.order_id AND c.document_kind='timing' WHERE w.order_id=$1",
                &[&id, &original],
            )
            .await?
            .and_then(|r| r.get::<_, Option<String>>(0).map(String::into_bytes)))
    }
    async fn timestamp(
        &self,
        txn: &mut PostgresTransaction,
    ) -> anyhow::Result<chrono::DateTime<chrono::Utc>> {
        Ok(txn
            .txn()
            .query_one("SELECT clock_timestamp()", &[])
            .await?
            .get(0))
    }
    async fn cash_capture(
        &self,
        txn: &mut PostgresTransaction,
        id: Uuid,
        evidence: &str,
    ) -> anyhow::Result<()> {
        if let Some(row) = txn
            .txn()
            .query_opt(
                "SELECT evidence::text FROM purchase_cash_captures WHERE order_id=$1",
                &[&id],
            )
            .await?
        {
            anyhow::ensure!(
                serde_json::from_str::<serde_json::Value>(row.get(0))?
                    == serde_json::from_str::<serde_json::Value>(evidence)?,
                "Capture evidence conflict"
            );
        } else {
            txn.txn()
                .execute(
                    "INSERT INTO purchase_cash_captures VALUES($1,$2::text::jsonb)",
                    &[&id, &evidence],
                )
                .await?;
        }
        Ok(())
    }
    async fn invoice_artifact(
        &self,
        txn: &mut PostgresTransaction,
        order: &str,
        candidate: &str,
    ) -> anyhow::Result<String> {
        txn.txn().execute("INSERT INTO paypal_receipt_artifacts(order_id,artifact) VALUES($1,$2::text::jsonb) ON CONFLICT DO NOTHING", &[&order,&candidate]).await?;
        Ok(txn
            .txn()
            .query_one(
                "SELECT artifact::text FROM paypal_receipt_artifacts WHERE order_id=$1",
                &[&order],
            )
            .await?
            .get(0))
    }
    async fn invoice_attempt(
        &self,
        txn: &mut PostgresTransaction,
        order: &str,
        attempt: Uuid,
        observation: &str,
    ) -> anyhow::Result<()> {
        txn.txn().execute("INSERT INTO paypal_receipt_observations(order_id,attempt,observation) VALUES($1,$2,$3) ON CONFLICT DO NOTHING", &[&order,&attempt,&observation]).await?;
        Ok(())
    }

    async fn export(&self, txn: &mut PostgresTransaction, user: Uuid) -> anyhow::Result<String> {
        Ok(txn.txn().query_one("SELECT coalesce(jsonb_agg(jsonb_build_object('offer',o.offer,'terms_pdf_base64',encode(o.terms_pdf,'base64'),'withdrawal_pdf_base64',encode(o.withdrawal_pdf,'base64'),'submission',s.request,'acceptance',to_jsonb(a),'progress',to_jsonb(p),'debit',to_jsonb(d),'cash_capture',(SELECT evidence FROM purchase_cash_captures c WHERE c.order_id=o.id),'paypal_receipt',(SELECT to_jsonb(r) FROM paypal_contract_orders x JOIN paypal_receipt_artifacts r ON r.order_id=x.paypal_order_id WHERE x.contract_order_id=o.id),'fulfillment',to_jsonb(f),'document_corrections',(SELECT coalesce(jsonb_agg(to_jsonb(c) ORDER BY document_kind),'[]'::jsonb) FROM purchase_document_corrections c WHERE c.order_id=o.id),'provision_timing',(SELECT evidence FROM purchase_provision_observations w WHERE w.order_id=o.id),'provision_timing_document',(SELECT to_jsonb(w) FROM purchase_provision_observations w WHERE w.order_id=o.id),'delivery_attempts',(SELECT coalesce(jsonb_agg(to_jsonb(t) ORDER BY generation,observed_at),'[]'::jsonb) FROM purchase_delivery_attempts t WHERE t.order_id=o.id)) ORDER BY o.created_at),'[]'::jsonb)::text FROM purchase_offers o JOIN purchase_progress p ON p.order_id=o.id LEFT JOIN purchase_submissions s ON s.order_id=o.id LEFT JOIN purchase_acceptances a ON a.order_id=o.id LEFT JOIN purchase_debits d ON d.order_id=o.id LEFT JOIN purchase_fulfillments f ON f.order_id=o.id WHERE o.user_id=$1", &[&user]).await?.get(0))
    }

    async fn record_debit(&self, txn: &mut PostgresTransaction, id: Uuid) -> anyhow::Result<()> {
        txn.txn().execute("INSERT INTO purchase_debits(order_id,ledger,tender_observations) SELECT $1,to_jsonb(t),jsonb_build_object('available_before',c.coins-t.coins,'available_after',c.coins,'withheld_before',c.withheld_coins,'withheld_after',c.withheld_coins,'captured_purchase_units',coalesce((SELECT sum(p.coins) FROM paypal_coin_orders p WHERE p.user_id=t.user_id AND p.captured_at IS NOT NULL),0),'captured_purchase_basis',coalesce((SELECT jsonb_agg(jsonb_build_object('order_id',p.id,'coins',p.coins,'captured_at',p.captured_at,'invoice_number',p.invoice_number)) FROM paypal_coin_orders p WHERE p.user_id=t.user_id AND p.captured_at IS NOT NULL),'[]'::jsonb),'prior_cash_refunds','unknown','purchased_reward_allocation','unknown') FROM transactions t JOIN coins c ON c.user_id=t.user_id WHERE t.id=$1", &[&id]).await?;
        txn.txn().query("SELECT observe_contract_purchases(d.id,o.user_id) FROM purchase_offers o JOIN contract_declarations d ON d.user_id=o.user_id WHERE o.id=$1", &[&id]).await?;
        Ok(())
    }
    async fn submit(
        &self,
        txn: &mut PostgresTransaction,
        id: Uuid,
        payload: &str,
    ) -> anyhow::Result<()> {
        txn.txn()
            .execute(
                "INSERT INTO purchase_submissions(order_id,request) VALUES($1,$2::text::jsonb)",
                &[&id, &payload],
            )
            .await?;
        Ok(())
    }
    async fn bind_period(&self, txn: &mut PostgresTransaction, id: Uuid) -> anyhow::Result<bool> {
        // Account lock is held: calendar increments follow authoritative acceptance order.
        let earlier: bool = txn.txn().query_one("SELECT EXISTS(SELECT 1 FROM purchase_offers o JOIN purchase_acceptances a ON a.order_id=o.id JOIN purchase_progress p ON p.order_id=o.id WHERE o.user_id=(SELECT user_id FROM purchase_offers WHERE id=$1) AND o.offer->'product'->>'kind' LIKE 'premium_%' AND p.state IN ('accepted','paid','review') AND a.acceptance_sequence<(SELECT acceptance_sequence FROM purchase_acceptances WHERE order_id=$1))", &[&id]).await?.get(0);
        if earlier {
            txn.txn().execute("UPDATE purchase_progress SET next_attempt_at=clock_timestamp()+interval '60 seconds' WHERE order_id=$1", &[&id]).await?;
            return Ok(false);
        }

        txn.txn()
            .query_one(
                "SELECT set_config('academy.purchase_order_id',$1,true)",
                &[&id.to_string()],
            )
            .await?;
        Ok(true)
    }
    async fn create(
        &self,
        txn: &mut PostgresTransaction,
        record: &PurchaseRecord,
    ) -> anyhow::Result<()> {
        let o = &record.status.offer;
        txn.txn().execute("INSERT INTO purchase_offers(id,user_id,source,offer,terms_pdf,withdrawal_pdf,created_at,expires_at) VALUES($1,$2,$3,$4::text::jsonb,$5,$6,$7,$8)", &[&o.id,&o.user_id,&o.source,&serde_json::to_string(o)?,&record.terms_pdf,&record.withdrawal_pdf,&o.created_at,&o.expires_at]).await?;
        txn.txn()
            .execute(
                "INSERT INTO purchase_progress(order_id) VALUES($1)",
                &[&o.id],
            )
            .await?;
        Ok(())
    }
    async fn get(
        &self,
        txn: &mut PostgresTransaction,
        id: Uuid,
    ) -> anyhow::Result<Option<PurchaseRecord>> {
        txn.txn()
            .query_opt(&format!("{SELECT} WHERE o.id=$1 FOR UPDATE OF p"), &[&id])
            .await?
            .map(decode)
            .transpose()
    }
    async fn lock_user(&self, txn: &mut PostgresTransaction, user: Uuid) -> anyhow::Result<bool> {
        Ok(txn
            .txn()
            .query_one("SELECT commercial_purchase_lock($1)", &[&user])
            .await?
            .get(0))
    }
    async fn accept(
        &self,
        txn: &mut PostgresTransaction,
        id: Uuid,
        body: &str,
        metadata: &str,
        accepted_at: chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<()> {
        txn.txn()
            .execute(
                "INSERT INTO purchase_acceptances(order_id,confirmation_body,message_metadata,accepted_at) VALUES($1,$2,$3::text::jsonb,$4)",
                &[&id, &body, &metadata, &accepted_at],
            )
            .await?;
        // For a new coin-funded order, accept_for changed the wallet before
        // this acceptance. Only its ensuing matching ledger consumes this
        // transaction-local provenance; free/cash orders create no such debit.
        // A later ledger-only repair must not infer original wallet facts.
        txn.txn()
            .query_one(
                "SELECT set_config('academy.new_purchase_wallet_ledger',$1,true)",
                &[&id.to_string()],
            )
            .await?;
        self.state(txn, id, "accepted", None).await
    }
    async fn state(
        &self,
        txn: &mut PostgresTransaction,
        id: Uuid,
        state: &str,
        reason: Option<&str>,
    ) -> anyhow::Result<()> {
        txn.txn()
            .execute(
                "UPDATE purchase_progress SET state=$2,review_reason=$3 WHERE order_id=$1",
                &[&id, &state, &reason],
            )
            .await?;
        Ok(())
    }
    async fn fulfill(
        &self,
        txn: &mut PostgresTransaction,
        id: Uuid,
        result: &str,
    ) -> anyhow::Result<()> {
        let data: serde_json::Value = serde_json::from_str(result)?;
        let detail = if data.get("purchased_until").is_some() {
            format!(
                "Dieser Bestellung zugeordneter Premium-Zeitraum: {} bis {}. Die automatische Verlängerung wurde durch diesen Kauf nicht geändert. Die Zuordnung eines Zeitraums ist kein Nachweis des genauen Beginns der technischen Nutzbarkeit.",
                data["purchased_since"].as_str().unwrap_or(""),
                data["purchased_until"].as_str().unwrap_or("")
            )
        } else if data.get("added").is_some() {
            format!(
                "{} Herzen gebucht; gespeicherter Bestand nach dieser Buchung: {} Herzen. Dies ist nicht der aktuelle Bestand und kein Nachweis eines genauen Bereitstellungszeitpunkts. Ein Lösungsversuch kostet je nach Aufgabentyp ein halbes oder ein ganzes Herz, auch bei richtiger Lösung.",
                display_hearts(
                    data["added"]
                        .as_u64()
                        .ok_or_else(|| anyhow::anyhow!("Missing half-heart fulfillment units"))?
                ),
                display_hearts(
                    data["hearts_after"]
                        .as_u64()
                        .ok_or_else(|| anyhow::anyhow!("Missing half-heart balance units"))?
                )
            )
        } else if data["kind"] == "course_access_provided" {
            format!(
                "Für diese Bestellung wurde die Kursfreischaltung gespeichert. Im Vorgang erfasster Bearbeitungszeitpunkt: {}. Dieser Zeitpunkt wurde vor Abschluss der Freischaltung erfasst und belegt nicht, ab wann der Zugang erstmals nutzbar war. Weder ein genauer Beginn der Nutzbarkeit noch das Ansehen oder der Abschluss des Kurses werden damit bestätigt.",
                data["provided_at"].as_str().unwrap_or("")
            )
        } else if data["kind"] == "booking_access_provided"
            && data["timing_basis"] == "committed_candidate_v1"
            && txn.txn().query_one("SELECT source='events' AND offer->'product'->'facts'->>'availability_protocol'='committed_candidate_v1' FROM purchase_offers WHERE id=$1", &[&id]).await?.get::<_, Option<bool>>(0) == Some(true)
        {
            format!(
                "Buchungszugang spätestens am {} als verfügbar beobachtet. Vereinbarter Termin: {} bis {}. Die tatsächliche Durchführung des Termins wird hiermit nicht bestätigt.",
                data["availability_observed_at"].as_str().unwrap_or(""),
                data["scheduled_start"].as_str().unwrap_or(""),
                data["scheduled_end"].as_str().unwrap_or("")
            )
        } else if data["kind"] == "coin_balance_provided" {
            format!(
                "MorphCoins-Gutschrift gespeichert. Im Vorgang erfasster Bearbeitungszeitpunkt: {}. Dieser Zeitpunkt wurde vor Abschluss der Gutschrift erfasst und belegt nicht, ab wann das Guthaben erstmals nutzbar war. Gespeicherter Bestand nach dieser Buchung: {} MorphCoins; dies ist nicht der aktuelle Bestand.",
                data["provided_at"].as_str().unwrap_or(""),
                data["balance"]["coins"]
            )
        } else {
            "Der gemeldete Vorgang wurde gespeichert. Seine Zeitangaben weisen ohne einen gesonderten passenden Nachweis weder die erste Nutzbarkeit eines Zugangs noch die tatsächliche Durchführung einer Leistung nach. Die ursprünglichen Vorgangsdaten bleiben erhalten.".to_owned()
        };
        let statement = format!(
            "Nachweis zum gebuchten Vorgang – Bootstrap Academy\nDokumentfassung: 2\nBestellung: {id}\n{detail}\nDie bestätigten Vertragsbedingungen und gesetzlichen Rechte bleiben maßgeblich.\n"
        );
        txn.txn().execute("INSERT INTO purchase_fulfillments(order_id,result,statement,statement_version) VALUES($1,$2::text::jsonb,$3,2)", &[&id,&result,&statement]).await?;
        self.state(txn, id, "fulfilled", None).await
    }
    async fn fulfillment_statement(
        &self,
        txn: &mut PostgresTransaction,
        id: Uuid,
        original: bool,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(txn
            .txn()
            .query_opt(
                "SELECT CASE WHEN $2 THEN f.statement ELSE coalesce(c.statement,CASE WHEN f.statement_version=2 THEN f.statement END) END FROM purchase_fulfillments f LEFT JOIN purchase_document_corrections c ON c.order_id=f.order_id AND c.document_kind='fulfillment' WHERE f.order_id=$1",
                &[&id, &original],
            )
            .await?
            .and_then(|r| r.get::<_, Option<String>>(0).map(String::into_bytes)))
    }
    async fn pending(&self, txn: &mut PostgresTransaction) -> anyhow::Result<Vec<Uuid>> {
        Ok(txn.txn().query("SELECT p.order_id FROM purchase_progress p JOIN purchase_offers o ON o.id=p.order_id WHERE ((p.state IN ('paid','awaiting_payment') AND purchase_provision_deadline(p.order_id)<=clock_timestamp()) OR (p.state='fulfilled' AND o.offer ? 'provision_window_seconds' AND NOT EXISTS(SELECT 1 FROM purchase_provision_observations w WHERE w.order_id=p.order_id)) OR (p.state IN ('paid','fulfilled','review') AND ((o.source='backend' AND p.state='paid' AND p.smtp_accepted_at IS NOT NULL AND p.next_attempt_at<=clock_timestamp()) OR (p.smtp_accepted_at IS NULL AND p.next_attempt_at<=clock_timestamp() AND coalesce(p.lease_until,'-infinity')<clock_timestamp())))) ORDER BY p.next_attempt_at,p.order_id LIMIT 100",&[]).await?.iter().map(|r|r.get(0)).collect())
    }
    async fn claim(
        &self,
        txn: &mut PostgresTransaction,
        id: Uuid,
    ) -> anyhow::Result<Option<PurchaseRecord>> {
        // Deadline failure is durable before a late handoff; confirmation is still owed.
        txn.txn().execute("UPDATE purchase_progress p SET state='review',review_reason='Provision deadline passed; actual payment and unperformed or uncertain claim retained' WHERE p.order_id=$1 AND p.state IN ('paid','awaiting_payment') AND purchase_provision_deadline(p.order_id)<=clock_timestamp() AND NOT EXISTS(SELECT 1 FROM purchase_fulfillments f WHERE f.order_id=p.order_id)", &[&id]).await?;
        let claimed=txn.txn().execute("UPDATE purchase_progress SET attempts=attempts+1,generation=generation+1,lease_until=clock_timestamp()+interval '60 seconds',next_attempt_at=clock_timestamp()+interval '60 seconds' WHERE order_id=$1 AND state IN ('paid','fulfilled','review') AND smtp_accepted_at IS NULL AND next_attempt_at<=clock_timestamp() AND coalesce(lease_until,'-infinity')<clock_timestamp()",&[&id]).await?;
        if claimed == 0 {
            return Ok(None);
        }
        txn.txn().execute("INSERT INTO purchase_delivery_attempts(order_id,generation,observation) SELECT order_id,generation,'started' FROM purchase_progress WHERE order_id=$1", &[&id]).await?;
        self.get(txn, id).await
    }
    async fn acknowledge(
        &self,
        txn: &mut PostgresTransaction,
        id: Uuid,
        generation: i64,
        outcome: &str,
    ) -> anyhow::Result<()> {
        txn.txn().execute("INSERT INTO purchase_delivery_attempts(order_id,generation,observation) VALUES($1,$2,$3) ON CONFLICT DO NOTHING", &[&id,&generation,&outcome]).await?;
        let sent = outcome == "smtp_accepted";
        txn.txn().execute("UPDATE purchase_progress SET smtp_accepted_at=CASE WHEN $3 THEN coalesce(smtp_accepted_at,clock_timestamp()) ELSE smtp_accepted_at END,lease_until=NULL,review_reason=CASE WHEN state='review' THEN review_reason WHEN $3 THEN NULL ELSE 'confirmation_delivery_pending' END WHERE order_id=$1 AND generation=$2",&[&id,&generation,&sent]).await?;
        // The service-start gate commits atomically with a possibly late handoff.
        txn.txn().execute("UPDATE purchase_progress p SET state='review',review_reason='Confirmation exceeded provision deadline; unperformed claim retained' WHERE p.order_id=$1 AND p.state='paid' AND p.smtp_accepted_at>=purchase_provision_deadline(p.order_id)", &[&id]).await?;
        Ok(())
    }
    async fn list(
        &self,
        txn: &mut PostgresTransaction,
        user: Uuid,
    ) -> anyhow::Result<Vec<PurchaseStatus>> {
        txn.txn()
            .query(
                &format!("{SELECT} WHERE o.user_id=$1 ORDER BY o.created_at DESC"),
                &[&user],
            )
            .await?
            .into_iter()
            .map(|r| decode(r).map(|r| r.status))
            .collect()
    }
}

mod inventory;
pub(crate) use inventory::document_inventory;
