use crate::PostgresTransaction;
use academy_models::{commercial_document::*, user::UserId};
use std::collections::{BTreeMap, BTreeSet};

// A printed credit number may contain a minimum-width year longer than four
// digits. Reconstruct the exact original reader prefix, never truncate it.
fn credit_period(number: &str) -> Option<(String, String, String)> {
    let (head, suffix) = number.strip_prefix('G')?.split_once('-')?;
    if head.len() < 6
        || suffix.is_empty()
        || !suffix.bytes().all(|b| b.is_ascii_digit())
        || !head.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let (year, month) = head.split_at(head.len() - 2);
    let year: i32 = year.parse().ok()?;
    let month: u32 = month.parse().ok()?;
    chrono::NaiveDate::from_ymd_opt(year, month, 1)?;
    let prefix = format!("G{year:04}{month:02}-");
    (number == format!("{prefix}{suffix}")).then(|| (prefix, year.to_string(), month.to_string()))
}

pub(crate) async fn document_inventory(
    tx: &mut PostgresTransaction,
    claimant: UserId,
) -> anyhow::Result<Vec<DocumentRecord>> {
    // An existing association is not an additive override of a higher numeric
    // denial. Generic legacy invoice associations cannot enumerate personal rows.
    // Own allocated references are independent metadata, not issued originals.
    let rows = tx.txn().query(
        "WITH candidates AS (
          SELECT d.number,d.kind,1 AS basis FROM financial_documents d
          WHERE (d.kind<>'invoice' AND coalesce(d.user_id=$1,EXISTS(SELECT 1 FROM moderation_retained_record_owners m WHERE m.subject=$1 AND m.kind='financial_document' AND m.record_id=d.number)))
             OR (d.kind='invoice' AND (commercial_invoice_original_owned($1,d.number)
                OR (commercial_invoice_numeric(d.number) IS NULL AND coalesce(d.user_id=$1,EXISTS(SELECT 1 FROM moderation_retained_record_owners m WHERE m.subject=$1 AND m.kind='financial_document' AND m.record_id=d.number)))))
          UNION ALL SELECT commercial_invoice_number(p.invoice_number),'invoice',0 FROM paypal_payments p WHERE p.user_id=$1 AND p.invoice_number IS NOT NULL
          UNION ALL SELECT commercial_invoice_number(p.invoice_number),'invoice',0 FROM paypal_coin_orders p WHERE p.user_id=$1 AND NOT EXISTS(SELECT 1 FROM paypal_payments h WHERE h.invoice_number=p.invoice_number)
          UNION ALL SELECT r.number,r.kind,2 FROM commercial_retention_owners r WHERE r.subject=$1 AND commercial_retention_owned($1,r.number,r.kind)
          UNION ALL SELECT r.number,'invoice',2 FROM commercial_invoice_owner_observations r WHERE r.subject=$1 AND r.qualified AND commercial_retention_owned($1,r.number,'invoice')
        ), known AS (SELECT number,kind,min(basis) AS basis FROM candidates WHERE number IS NOT NULL GROUP BY number,kind), admitted AS (
          SELECT k.*, CASE WHEN k.kind='invoice' THEN commercial_invoice_original_owned($1,k.number)
             ELSE EXISTS(SELECT 1 FROM financial_documents d WHERE d.number=k.number AND d.kind=k.kind AND coalesce(d.user_id=$1,EXISTS(SELECT 1 FROM moderation_retained_record_owners m WHERE m.subject=$1 AND m.kind='financial_document' AND m.record_id=d.number))) END AS original_owned,
             commercial_invoice_identity_pending(k.number) AS identity_pending,
             EXISTS(SELECT 1 FROM commercial_archive_work w WHERE w.number=k.number AND (w.source='record_disposal' OR w.disposal_started_at IS NOT NULL)) AS retired
          FROM known k
        ) SELECT a.number,a.kind,a.basis,a.original_owned,a.identity_pending,a.retired,
          CASE WHEN a.kind='invoice' AND a.original_owned AND NOT a.identity_pending AND NOT a.retired THEN EXISTS(SELECT 1 FROM invoice_originals i WHERE i.invoice_number=a.number) ELSE false END,
          CASE WHEN a.kind='invoice' AND a.original_owned AND NOT a.identity_pending AND NOT a.retired THEN (SELECT octet_length(i.pdf)::bigint FROM invoice_originals i WHERE i.invoice_number=a.number) END
        FROM admitted a ORDER BY a.kind,a.number",
        &[&*claimant],
    ).await?;
    let periods: BTreeSet<String> = rows
        .iter()
        .filter(|r| r.get::<_, &str>(1) == "credit_note")
        .filter_map(|r| credit_period(r.get(0)).map(|v| v.0))
        .collect();
    let periods: Vec<String> = periods.into_iter().collect();
    // Resolve only observed periods, including the actual live-number fallback.
    // No speculative months, full records, byte reads, archive locks or fanout.
    let credit_rows = tx.txn().query(
        "WITH periods AS (SELECT unnest($2::text[]) AS prefix), owned AS (
          SELECT p.prefix,d.number FROM periods p JOIN financial_documents d ON d.kind='credit_note' AND d.number ~ ('^'||p.prefix||'[0-9]+$')
          WHERE coalesce(d.user_id=$1,EXISTS(SELECT 1 FROM moderation_retained_record_owners m WHERE m.subject=$1 AND m.kind='financial_document' AND m.record_id=d.number))
          UNION SELECT p.prefix,p.prefix||n.number::text FROM periods p CROSS JOIN user_numbers n WHERE n.user_id=$1 AND NOT EXISTS(SELECT 1 FROM financial_documents d WHERE d.number=p.prefix||n.number::text)
        ) SELECT prefix,array_agg(number ORDER BY number) FROM owned GROUP BY prefix", &[&*claimant,&periods],
    ).await?;
    let credits: BTreeMap<String, Vec<String>> = credit_rows
        .into_iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect();
    rows.into_iter()
        .map(|r| {
            let number: String = r.get(0);
            let kind = match r.get::<_, &str>(1) {
                "invoice" => DocumentKind::Invoice,
                "credit_note" => DocumentKind::CreditNote,
                "final_statement" => DocumentKind::FinalStatement,
                _ => anyhow::bail!("Unsupported stored financial document kind"),
            };
            let mut admitted: bool = r.get(3);
            let mut ambiguous = false;
            let route = match kind {
                DocumentKind::Invoice => number
                    .strip_prefix('R')
                    .and_then(|s| s.parse::<i64>().ok())
                    .filter(|n| *n >= 0 && format!("R{n:07}") == number)
                    .map(|n| (n.to_string(), "original".into())),
                DocumentKind::FinalStatement => number
                    .strip_prefix('S')
                    .and_then(|s| s.parse::<u64>().ok())
                    .filter(|n| format!("S{n}") == number)
                    .map(|n| (n.to_string(), "original".into())),
                DocumentKind::CreditNote => credit_period(&number).map(|(prefix, year, month)| {
                    let resolved = credits.get(&prefix);
                    ambiguous = resolved.is_some_and(|v| v.len() > 1);
                    admitted = resolved.is_some_and(|v| v.as_slice() == [number.as_str()]);
                    (year, month)
                }),
                DocumentKind::Purchase => unreachable!(),
            };
            let reason = if r.get::<_, bool>(4) {
                Some(UnavailableReason::IdentityPendingReview)
            } else if r.get::<_, bool>(5) {
                Some(UnavailableReason::RetiredRecorded)
            } else if route.is_none() {
                Some(UnavailableReason::UnsupportedIdentifier)
            } else if ambiguous {
                Some(UnavailableReason::AmbiguousPeriod)
            } else if !admitted {
                Some(UnavailableReason::OriginalReaderNotAdmitted)
            } else if r.get::<_, bool>(6) && r.get::<_, Option<i64>>(7) == Some(0) {
                Some(UnavailableReason::EmptySelectedArtifact)
            } else {
                None
            };
            let can_observe =
                admitted && !r.get::<_, bool>(4) && !r.get::<_, bool>(5) && route.is_some();
            let (selection_source, observation) = if !can_observe {
                (SelectionSource::None, ArtifactObservation::Unchecked)
            } else if r.get::<_, bool>(6) {
                (
                    SelectionSource::DatabaseOriginal,
                    if r.get::<_, Option<i64>>(7) == Some(0) {
                        ArtifactObservation::Empty
                    } else {
                        ArtifactObservation::Nonempty
                    },
                )
            } else {
                (
                    SelectionSource::ArchiveUnchecked,
                    ArtifactObservation::Unchecked,
                )
            };
            let reader_state = if reason.is_some() {
                ReaderState::Unavailable
            } else if selection_source == SelectionSource::ArchiveUnchecked {
                ReaderState::ArchiveUnchecked
            } else {
                ReaderState::Candidate
            };
            let selector = if reason.is_none() {
                route.map(|(id, variant)| DocumentSelector { kind, id, variant })
            } else {
                None
            };
            Ok(DocumentRecord {
                family: DocumentFamily::Finance,
                kind,
                source_service: "backend".into(),
                source_subject: *claimant,
                owner_relation: OwnerRelation::Claimant,
                purchase_source: None,
                offer_id: None,
                printed_number: Some(number),
                record_basis: match r.get::<_, i32>(2) {
                    0 => RecordBasis::OwnInvoiceReference,
                    1 => RecordBasis::OwnedDocument,
                    _ => RecordBasis::QualifiedRetentionReference,
                },
                reader_state,
                reason,
                selector: selector.clone(),
                artifacts: vec![DocumentArtifact {
                    variant: "original".into(),
                    selection_source,
                    observation,
                    reader_state,
                    reason,
                    selector,
                }],
            })
        })
        .collect()
}
