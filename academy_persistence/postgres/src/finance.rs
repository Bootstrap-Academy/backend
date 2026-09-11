use academy_di::Build;
use academy_models::{
    finance::{FinancialDocument, FinancialDocumentKind, FinancialDocumentNumber},
    pagination::PaginationSlice,
    user::UserId,
};
use academy_persistence_contracts::finance::FinancialDocumentRepository;
use academy_utils::trace_instrument;
use chrono::{DateTime, Utc};
use clorinde::{
    client::Params,
    queries::{
        self,
        finance::{
            CountDocumentsParams, ListDocumentsParams, PseudonymizeDocumentsParams,
            RecordDocumentParams, SettleDocumentParams,
        },
    },
};
use futures::{StreamExt, TryStreamExt};

use crate::PostgresTransaction;

#[derive(Debug, Clone, Build)]
pub struct PostgresFinancialDocumentRepository;

impl FinancialDocumentRepository<PostgresTransaction> for PostgresFinancialDocumentRepository {
    async fn archive_inventory(
        &self,
        txn: &mut PostgresTransaction,
    ) -> anyhow::Result<Vec<(FinancialDocumentNumber, FinancialDocumentKind, bool, bool)>> {
        txn.txn().query("SELECT d.number,d.kind,true AS recorded,EXISTS(SELECT 1 FROM invoice_originals i WHERE i.invoice_number=d.number AND d.kind='invoice') AS database_original FROM financial_documents d UNION ALL SELECT i.invoice_number,'invoice',false,true FROM invoice_originals i WHERE NOT EXISTS(SELECT 1 FROM financial_documents d WHERE d.number=i.invoice_number) ORDER BY number", &[]).await?.into_iter().map(|r|Ok((FinancialDocumentNumber::try_new(r.get::<_,String>(0))?,r.get::<_,String>(1).parse()?,r.get(2),r.get(3)))).collect()
    }
    async fn lock_archive(
        &self,
        txn: &mut PostgresTransaction,
        number: &FinancialDocumentNumber,
    ) -> anyhow::Result<bool> {
        txn.txn()
            .execute(
                "SELECT pg_advisory_xact_lock(hashtextextended('commercial-archive:'||$1::text,0))",
                &[&number.as_str()],
            )
            .await?;
        // This error must precede every byte source. Returning false/None here
        // would let an original-reader fallback treat a suspect invoice as absent.
        anyhow::ensure!(
            !txn.txn()
                .query_one(
                    "SELECT commercial_invoice_identity_pending($1)",
                    &[&number.as_str()]
                )
                .await?
                .get::<_, bool>(0),
            "Original invoice identity is pending independent review; original bytes and history remain unchanged"
        );
        Ok(!txn.txn().query_one("SELECT EXISTS(SELECT 1 FROM commercial_archive_work WHERE number=$1 AND (source='record_disposal' OR disposal_started_at IS NOT NULL))", &[&number.as_str()]).await?.get::<_,bool>(0))
    }
    async fn begin_archive_disposal(
        &self,
        txn: &mut PostgresTransaction,
        number: &FinancialDocumentNumber,
        kind: FinancialDocumentKind,
    ) -> anyhow::Result<bool> {
        require_disposal_read_committed(txn).await?;
        txn.txn()
            .execute(
                "SELECT pg_advisory_xact_lock(hashtextextended('commercial-archive:'||$1::text,0))",
                &[&number.as_str()],
            )
            .await?;
        if txn
            .txn()
            .query_opt(
                "SELECT number FROM commercial_archive_work WHERE number=$1 AND kind=$2 FOR UPDATE",
                &[&number.as_str(), &kind.as_str()],
            )
            .await?
            .is_none()
        {
            return Ok(false);
        }
        // A fresh statement after the work-row wait observes current admission.
        // A prior intent or a stale work list is never a new file authority.
        let eligible = txn.txn().query_one("SELECT EXISTS(SELECT 1 FROM commercial_archive_work w WHERE number=$1 AND kind=$2 AND disposal_authorized AND file_removed_at IS NULL AND NOT EXISTS(SELECT 1 FROM financial_documents d WHERE d.number=w.number) AND NOT EXISTS(SELECT 1 FROM commercial_document_holds h WHERE h.number=w.number))", &[&number.as_str(), &kind.as_str()]).await?.get::<_, bool>(0);
        if !eligible {
            return Ok(false);
        }
        if kind == FinancialDocumentKind::Invoice {
            txn.txn()
                .execute(
                    "SELECT commercial_capture_retention_owner($1,'invoice')",
                    &[&number.as_str()],
                )
                .await?;
            if txn
                .txn()
                .query_one(
                    "SELECT commercial_invoice_identity_pending($1)",
                    &[&number.as_str()],
                )
                .await?
                .get::<_, bool>(0)
            {
                return Ok(false);
            }
        }
        txn.txn().execute("UPDATE commercial_archive_work SET disposal_started_at=coalesce(disposal_started_at,clock_timestamp()) WHERE number=$1 AND kind=$2", &[&number.as_str(), &kind.as_str()]).await?;
        if kind == FinancialDocumentKind::Invoice {
            txn.txn()
                .execute(
                    "DELETE FROM invoice_originals WHERE invoice_number=$1",
                    &[&number.as_str()],
                )
                .await?;
            txn.txn()
                .execute(
                    "DELETE FROM invoice_reconciliation WHERE invoice_number=$1",
                    &[&number.as_str()],
                )
                .await?;
        }
        Ok(true)
    }
    async fn pending_archive_disposals(
        &self,
        txn: &mut PostgresTransaction,
    ) -> anyhow::Result<Vec<(FinancialDocumentNumber, FinancialDocumentKind)>> {
        txn.txn().query("SELECT number,kind FROM commercial_archive_work WHERE disposal_authorized AND file_removed_at IS NULL AND (kind<>'invoice' OR NOT commercial_invoice_identity_pending(number)) ORDER BY recorded_at,number", &[]).await?
            .into_iter().map(|r| Ok((FinancialDocumentNumber::try_new(r.get::<_,String>(0))?,r.get::<_,String>(1).parse()?))).collect()
    }
    async fn acknowledge_archive_disposal(
        &self,
        txn: &mut PostgresTransaction,
        number: &FinancialDocumentNumber,
        kind: FinancialDocumentKind,
    ) -> anyhow::Result<()> {
        txn.txn().execute("UPDATE commercial_archive_work SET file_removed_at=clock_timestamp() WHERE number=$1 AND kind=$2 AND disposal_authorized AND disposal_started_at IS NOT NULL AND file_removed_at IS NULL", &[&number.as_str(),&kind.as_str()]).await?;
        Ok(())
    }
    async fn observe_unrecorded_archive(
        &self,
        txn: &mut PostgresTransaction,
        number: &FinancialDocumentNumber,
        kind: FinancialDocumentKind,
    ) -> anyhow::Result<()> {
        txn.txn().execute("INSERT INTO commercial_archive_work(number,kind,source) SELECT $1,$2,'unrecorded_archive' WHERE NOT EXISTS(SELECT 1 FROM financial_documents WHERE number=$1) ON CONFLICT DO NOTHING", &[&number.as_str(),&kind.as_str()]).await?;
        Ok(())
    }

    async fn owned_original_number(
        &self,
        txn: &mut PostgresTransaction,
        user: UserId,
        kind: FinancialDocumentKind,
        number: u64,
        month: u32,
    ) -> anyhow::Result<Option<FinancialDocumentNumber>> {
        let document = match kind {
            FinancialDocumentKind::Invoice => {
                let Ok(invoice) = i64::try_from(number) else {
                    return Ok(None);
                };
                let formatted = format!("R{number:07}");
                // One statement sees either live ownership or the atomically
                // published retained mapping. A conflicting live owner is not
                // overridden by an older fallback record.
                let allowed: bool = txn.txn().query_one(
                    "SELECT coalesce(
                      (SELECT user_id=$1 AND fulfilled_at IS NOT NULL FROM paypal_payments WHERE invoice_number=$2),
                      (SELECT user_id=$1 FROM paypal_coin_orders WHERE invoice_number=$2),
                      (SELECT coalesce(d.user_id=$1,EXISTS(SELECT 1 FROM moderation_retained_record_owners o WHERE o.subject=$1 AND o.kind='financial_document' AND o.record_id=d.number)) FROM financial_documents d WHERE d.number=$3 AND d.kind='invoice'),
                      false)",
                    &[&*user, &invoice, &formatted],
                ).await?.get(0);
                allowed.then_some(formatted)
            }
            FinancialDocumentKind::CreditNote => {
                let Ok(year) = i32::try_from(number) else {
                    return Ok(None);
                };
                if chrono::NaiveDate::from_ymd_opt(year, month, 1).is_none() {
                    return Ok(None);
                }
                let prefix = format!("G{year:04}{month:02}-");
                // An issued document's exact recorded owner takes precedence.
                // Older live archives may predate financial_documents, but an
                // erased account needs the exact retained document inventory.
                let rows = txn.txn().query(
                    "SELECT number FROM (
                      SELECT d.number FROM financial_documents d WHERE d.kind='credit_note'
                       AND d.number ~ ('^'||$2||'[0-9]+$')
                       AND coalesce(d.user_id=$1,EXISTS(SELECT 1 FROM moderation_retained_record_owners o WHERE o.subject=$1 AND o.kind='financial_document' AND o.record_id=d.number))
                      UNION
                      SELECT $2||n.number::text FROM user_numbers n WHERE n.user_id=$1
                       AND NOT EXISTS(SELECT 1 FROM financial_documents d WHERE d.number=$2||n.number::text)
                    ) owned ORDER BY number LIMIT 2",
                    &[&*user, &prefix],
                ).await?;
                // Conflicting retained period identities require human recovery,
                // rather than silently choosing one historical customer number.
                (rows.len() == 1).then(|| rows[0].get::<_, String>(0))
            }
            FinancialDocumentKind::FinalStatement => {
                let formatted = format!("S{number}");
                let allowed: bool = txn.txn().query_one(
                    "SELECT EXISTS(SELECT 1 FROM financial_documents d WHERE d.number=$2 AND d.kind='final_statement' AND coalesce(d.user_id=$1,EXISTS(SELECT 1 FROM moderation_retained_record_owners o WHERE o.subject=$1 AND o.kind='financial_document' AND o.record_id=d.number)))",
                    &[&*user, &formatted],
                ).await?.get(0);
                allowed.then_some(formatted)
            }
        };
        document
            .map(FinancialDocumentNumber::try_new)
            .transpose()
            .map_err(Into::into)
    }

    async fn original_invoice(
        &self,
        txn: &mut PostgresTransaction,
        number: &FinancialDocumentNumber,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        if !self.lock_archive(txn, number).await? {
            return Ok(None);
        }
        // Serialize first-original adoption/import even when no document or
        // live order exists yet. Every caller takes this lock before choosing
        // archive/rendered bytes; a concurrent cache cannot become a second
        // authoritative original. The lock changes no financial facts.
        txn.txn()
            .query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended('invoice-original:' || $1, 0))",
                &[&number.as_str()],
            )
            .await?;
        Ok(txn
            .txn()
            .query_opt(
                "SELECT pdf FROM invoice_originals WHERE invoice_number=$1",
                &[&number.as_str()],
            )
            .await?
            .map(|r| r.get(0)))
    }
    async fn record_original_invoice(
        &self,
        txn: &mut PostgresTransaction,
        number: &FinancialDocumentNumber,
        pdf: &[u8],
        provenance: &str,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.lock_archive(txn, number).await?,
            "Retired original identifier cannot be imported"
        );
        if let Some(old) = self.original_invoice(txn, number).await? {
            anyhow::ensure!(old == pdf, "An archived original cannot be replaced");
            return Ok(());
        }
        txn.txn()
            .execute(
                "INSERT INTO invoice_originals(invoice_number,pdf,provenance) VALUES($1,$2,$3)",
                &[&number.as_str(), &pdf, &provenance],
            )
            .await?;
        txn.txn().execute("INSERT INTO invoice_reconciliation(invoice_number,state,reason) VALUES($1,'evidenced','Original invoice bytes archived; capture, money and entitlement history unchanged') ON CONFLICT(invoice_number) DO UPDATE SET state=EXCLUDED.state,reason=EXCLUDED.reason", &[&number.as_str()]).await?;
        self.observe_unrecorded_archive(txn, number, FinancialDocumentKind::Invoice)
            .await?;
        txn.txn()
            .execute(
                "SELECT commercial_capture_retention_owner($1,'invoice')",
                &[&number.as_str()],
            )
            .await?;
        Ok(())
    }
    async fn flag_missing_invoice(
        &self,
        txn: &mut PostgresTransaction,
        number: &FinancialDocumentNumber,
    ) -> anyhow::Result<()> {
        if !self.lock_archive(txn, number).await? {
            return Ok(());
        }
        txn.txn().execute("INSERT INTO invoice_reconciliation(invoice_number,state,reason) VALUES($1,'evidence_missing','Original invoice missing; current prices, VAT, customer data and order creation time cannot establish historical invoice/capture facts') ON CONFLICT DO NOTHING", &[&number.as_str()]).await?;
        Ok(())
    }
    async fn invoice_reconciliation(
        &self,
        txn: &mut PostgresTransaction,
    ) -> anyhow::Result<String> {
        // Include orphan recorded invoices as unresolved without requiring a live account/order.
        txn.txn().execute("INSERT INTO invoice_reconciliation(invoice_number,state,reason) SELECT number,'evidence_missing','Recorded invoice has no live order; locate original archived bytes, never replay payment or entitlement' FROM financial_documents f WHERE kind='invoice' AND NOT EXISTS(SELECT 1 FROM paypal_coin_orders o WHERE commercial_invoice_number(o.invoice_number)=f.number) AND NOT EXISTS(SELECT 1 FROM invoice_originals i WHERE i.invoice_number=f.number) ON CONFLICT DO NOTHING", &[]).await?;
        Ok(txn.txn().query_one("SELECT coalesce(jsonb_agg(to_jsonb(r) ORDER BY recorded_at,invoice_number),'[]'::jsonb)::text FROM invoice_reconciliation r", &[]).await?.get(0))
    }

    #[trace_instrument(skip(self, txn, document), fields(document = %*document.number))]
    async fn record(
        &self,
        txn: &mut PostgresTransaction,
        document: &FinancialDocument,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.lock_archive(txn, &document.number).await?,
            "Retired original identifier cannot be adopted again"
        );
        let params = RecordDocumentParams {
            number: &*document.number,
            kind: document.kind.as_str(),
            user_id: document.user_id.map(|user_id| *user_id),
            issued_at: document.issued_at.into(),
            customer_details: document
                .customer_details
                .as_ref()
                .map(|details| details.iter().map(String::as_str).collect::<Vec<_>>()),
            coins: document.coins.map(i64::try_from).transpose()?,
            net_total_cents: document.net_total_cents,
            vat_total_cents: document.vat_total_cents,
            gross_total_cents: document.gross_total_cents,
            withdrawal_consent_at: document.withdrawal_consent_at.map(Into::into),
            withdrawal_text_version: document
                .withdrawal_text_version
                .as_ref()
                .map(|version| version.as_str()),
        };

        queries::finance::record_document()
            .params(txn.txn(), &params)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn get(
        &self,
        txn: &mut PostgresTransaction,
        number: &FinancialDocumentNumber,
    ) -> anyhow::Result<Option<FinancialDocument>> {
        queries::finance::get_document()
            .bind(txn.txn(), &&**number)
            .opt()
            .await
            .map_err(Into::into)
            .and_then(|row| row.map(decode_document).transpose())
    }

    #[trace_instrument(skip(self, txn))]
    async fn settle(
        &self,
        txn: &mut PostgresTransaction,
        number: &FinancialDocumentNumber,
        settled_at: DateTime<Utc>,
    ) -> anyhow::Result<bool> {
        let params = SettleDocumentParams {
            settled_at: settled_at.into(),
            number: &**number,
        };

        queries::finance::settle_document()
            .params(txn.txn(), &params)
            .await
            .map(|rows| rows > 0)
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn pseudonymize(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
        customer_details: &[String],
    ) -> anyhow::Result<u64> {
        let params = PseudonymizeDocumentsParams {
            customer_details: customer_details
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            user_id: *user_id,
        };

        queries::finance::pseudonymize_documents()
            .params(txn.txn(), &params)
            .await
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn list_by_user_id(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Vec<FinancialDocument>> {
        queries::finance::list_documents_by_user_id()
            .bind(txn.txn(), &user_id)
            .iter()
            .await?
            .map(|row| row.map_err(Into::into).and_then(decode_document))
            .try_collect()
            .await
    }

    #[trace_instrument(skip(self, txn, search))]
    async fn list(
        &self,
        txn: &mut PostgresTransaction,
        kind: Option<FinancialDocumentKind>,
        search: Option<String>,
        pagination: PaginationSlice,
    ) -> anyhow::Result<Vec<FinancialDocument>> {
        let search = search.as_deref().map(escape_like_pattern);
        let params = ListDocumentsParams {
            kind: kind.map(FinancialDocumentKind::as_str),
            search: search.as_deref(),
            limit: (*pagination.limit).try_into()?,
            offset: pagination.offset.try_into()?,
        };

        queries::finance::list_documents()
            .params(txn.txn(), &params)
            .iter()
            .await?
            .map(|row| row.map_err(Into::into).and_then(decode_document))
            .try_collect()
            .await
    }

    #[trace_instrument(skip(self, txn, search))]
    async fn count(
        &self,
        txn: &mut PostgresTransaction,
        kind: Option<FinancialDocumentKind>,
        search: Option<String>,
    ) -> anyhow::Result<u64> {
        let search = search.as_deref().map(escape_like_pattern);
        let params = CountDocumentsParams {
            kind: kind.map(FinancialDocumentKind::as_str),
            search: search.as_deref(),
        };

        queries::finance::count_documents()
            .params(txn.txn(), &params)
            .one()
            .await
            .map_err(Into::into)
            .and_then(|count| count.try_into().map_err(Into::into))
    }

    #[trace_instrument(skip(self, txn))]
    async fn list_numbers(
        &self,
        txn: &mut PostgresTransaction,
    ) -> anyhow::Result<Vec<FinancialDocumentNumber>> {
        queries::finance::list_document_numbers()
            .bind(txn.txn())
            .iter()
            .await?
            .map(|row| {
                row.map_err(Into::into)
                    .and_then(|number| FinancialDocumentNumber::try_new(number).map_err(Into::into))
            })
            .try_collect()
            .await
    }

    #[trace_instrument(skip(self, txn))]
    async fn list_issued_before(
        &self,
        txn: &mut PostgresTransaction,
        issued_before: DateTime<Utc>,
    ) -> anyhow::Result<Vec<FinancialDocument>> {
        queries::finance::list_documents_issued_before()
            .bind(txn.txn(), &issued_before.into())
            .iter()
            .await?
            .map(|row| row.map_err(Into::into).and_then(decode_document))
            .try_collect()
            .await
    }

    #[trace_instrument(skip(self, txn))]
    async fn delete_issued_before(
        &self,
        txn: &mut PostgresTransaction,
        issued_before: DateTime<Utc>,
    ) -> anyhow::Result<u64> {
        require_disposal_read_committed(txn).await?;
        // Candidate filtering is not authority after a wait. Acquire each exact
        // archive before its row, then reread eligibility in a separate statement.
        let mut documents = self.list_issued_before(txn, issued_before).await?;
        documents.sort_by(|a, b| a.number.as_str().cmp(b.number.as_str()));
        let mut deleted = 0;
        for document in documents {
            let number = document.number.as_str();
            txn.txn().execute("SELECT pg_advisory_xact_lock(hashtextextended('commercial-archive:'||$1::text,0))", &[&number]).await?;
            if txn
                .txn()
                .query_opt(
                    "SELECT number FROM financial_documents WHERE number=$1 FOR UPDATE",
                    &[&number],
                )
                .await?
                .is_none()
            {
                continue;
            }
            let eligible = txn.txn().query_one("SELECT EXISTS(SELECT 1 FROM financial_documents d WHERE number=$1 AND kind=$3 AND issued_at<$2 AND NOT EXISTS(SELECT 1 FROM commercial_document_holds h WHERE h.number=d.number) AND (kind<>'final_statement' OR EXISTS(SELECT 1 FROM commercial_statement_disposal_reviews r WHERE r.number=d.number AND r.authorized)) AND (kind<>'invoice' OR NOT commercial_invoice_identity_pending(d.number)) AND NOT EXISTS(SELECT 1 FROM commercial_archive_work w WHERE w.number=d.number AND (w.source='record_disposal' OR w.disposal_started_at IS NOT NULL)))", &[&number, &issued_before, &document.kind.as_str()]).await?.get::<_, bool>(0);
            if eligible {
                // The BEFORE guard may discover a conflict and skip this row.
                // Count the actual deletion, not the earlier candidate/admission.
                deleted += txn
                    .txn()
                    .execute(
                        "DELETE FROM financial_documents WHERE number=$1",
                        &[&number],
                    )
                    .await?;
            }
        }
        Ok(deleted)
    }
}

async fn require_disposal_read_committed(txn: &mut PostgresTransaction) -> anyhow::Result<()> {
    anyhow::ensure!(
        txn.txn()
            .query_one("SELECT current_setting('transaction_isolation')", &[])
            .await?
            .get::<_, &str>(0)
            == "read committed",
        "Document disposal requires READ COMMITTED; no admission or evidence change was made"
    );
    Ok(())
}

/// Escape the characters `like` gives a special meaning to, so that a search
/// term is matched literally.
///
/// The search term is put into the pattern of an `ilike`, where `%` matches
/// any run of characters and `_` any single one. Without this, an
/// administrator searching for `%` would get every document instead of the
/// documents that contain a percent sign. `\` is the escape character `like`
/// uses by default, so it has to be escaped first.
fn escape_like_pattern(search: &str) -> String {
    let mut pattern = String::with_capacity(search.len());
    for character in search.chars() {
        if matches!(character, '\\' | '%' | '_') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern
}

fn decode_document(value: queries::finance::Document) -> anyhow::Result<FinancialDocument> {
    Ok(FinancialDocument {
        number: value.number.try_into()?,
        kind: value.kind.parse::<FinancialDocumentKind>()?,
        user_id: value.user_id.map(Into::into),
        issued_at: value.issued_at.into(),
        customer_details: value.customer_details,
        coins: value.coins.map(u64::try_from).transpose()?,
        net_total_cents: value.net_total_cents,
        vat_total_cents: value.vat_total_cents,
        gross_total_cents: value.gross_total_cents,
        settled_at: value.settled_at.map(Into::into),
        withdrawal_consent_at: value.withdrawal_consent_at.map(Into::into),
        withdrawal_text_version: value
            .withdrawal_text_version
            .map(TryInto::try_into)
            .transpose()?,
    })
}

mod inventory;
pub(crate) use inventory::document_inventory;
