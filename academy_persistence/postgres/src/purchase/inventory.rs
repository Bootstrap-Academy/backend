use crate::PostgresTransaction;
use academy_models::{commercial_document::*, user::UserId};
use uuid::Uuid;

pub(crate) async fn document_inventory(
    tx: &mut PostgresTransaction,
    claimant: UserId,
) -> anyhow::Result<Vec<DocumentRecord>> {
    // Only identity scalars and selected lengths leave PostgreSQL. In particular
    // this must not reuse the offer/status decoder or the original-byte reader.
    let rows = tx.txn().query(
        "SELECT o.id,o.user_id,o.source,o.offer->>'id',o.offer->>'user_id',p.order_id IS NOT NULL,
         octet_length(o.terms_pdf)::bigint,octet_length(o.withdrawal_pdf)::bigint,
         octet_length(a.confirmation_body)::bigint,
         octet_length(w.statement)::bigint,w.statement_version,octet_length(tc.statement)::bigint,
         octet_length(f.statement)::bigint,f.statement_version,octet_length(fc.statement)::bigint
         FROM purchase_offers o
         LEFT JOIN purchase_progress p ON p.order_id=o.id
         LEFT JOIN purchase_acceptances a ON a.order_id=o.id
         LEFT JOIN purchase_provision_observations w ON w.order_id=o.id
         LEFT JOIN purchase_document_corrections tc ON tc.order_id=w.order_id AND tc.document_kind='timing'
         LEFT JOIN purchase_fulfillments f ON f.order_id=o.id
         LEFT JOIN purchase_document_corrections fc ON fc.order_id=f.order_id AND fc.document_kind='fulfillment'
         WHERE commercial_owned_service_subject($1,o.user_id) ORDER BY o.created_at,o.id",
        &[&*claimant],
    ).await?;
    rows.into_iter()
        .map(|r| {
            let id: Uuid = r.get(0);
            let owner: Uuid = r.get(1);
            let source: String = r.get(2);
            anyhow::ensure!(
                ["backend", "skills", "events", "paypal"].contains(&source.as_str()),
                "Unsupported stored purchase source"
            );
            let id_matches = r
                .get::<_, Option<String>>(3)
                .and_then(|s| Uuid::parse_str(&s).ok())
                == Some(id);
            let owner_matches = r
                .get::<_, Option<String>>(4)
                .and_then(|s| Uuid::parse_str(&s).ok())
                == Some(owner);
            let reason = if !id_matches || !owner_matches {
                Some(UnavailableReason::OriginalIdentityMismatch)
            } else if !r.get::<_, bool>(5) {
                Some(UnavailableReason::MissingProgress)
            } else {
                None
            };
            let artifact = |variant: &str,
                            length: Option<i64>,
                            selection_source: SelectionSource| {
                let observation = match length {
                    Some(0) => ArtifactObservation::Empty,
                    Some(_) => ArtifactObservation::Nonempty,
                    None => ArtifactObservation::Absent,
                };
                let reason = reason.or(match observation {
                    ArtifactObservation::Empty => Some(UnavailableReason::EmptySelectedArtifact),
                    ArtifactObservation::Absent => Some(UnavailableReason::AbsentSelectedArtifact),
                    _ => None,
                });
                DocumentArtifact {
                    variant: variant.into(),
                    selection_source,
                    observation,
                    reader_state: if reason.is_some() {
                        ReaderState::Unavailable
                    } else {
                        ReaderState::Candidate
                    },
                    reason,
                    selector: reason.is_none().then(|| DocumentSelector {
                        kind: DocumentKind::Purchase,
                        id: id.to_string(),
                        variant: variant.into(),
                    }),
                }
            };
            let original = |length: Option<i64>| {
                if length.is_some() {
                    SelectionSource::Original
                } else {
                    SelectionSource::None
                }
            };
            let current = |length: Option<i64>, version: Option<i32>, correction: Option<i64>| {
                if correction.is_some() {
                    (correction, SelectionSource::Correction)
                } else if version == Some(2) {
                    (length, SelectionSource::OriginalV2)
                } else {
                    (None, SelectionSource::None)
                }
            };
            let timing = current(r.get(9), r.get(10), r.get(11));
            let fulfillment = current(r.get(12), r.get(13), r.get(14));
            Ok(DocumentRecord {
                family: DocumentFamily::Purchase,
                kind: DocumentKind::Purchase,
                source_service: "backend".into(),
                source_subject: owner,
                owner_relation: if owner == *claimant {
                    OwnerRelation::Claimant
                } else {
                    OwnerRelation::SameCaseLearningSubject
                },
                purchase_source: Some(source),
                offer_id: Some(id),
                printed_number: None,
                record_basis: RecordBasis::OriginalOffer,
                reader_state: if reason.is_some() {
                    ReaderState::Unavailable
                } else {
                    ReaderState::Candidate
                },
                reason,
                selector: None,
                artifacts: vec![
                    artifact("terms", r.get(6), SelectionSource::Stored),
                    artifact("withdrawal", r.get(7), SelectionSource::Stored),
                    artifact(
                        "confirmation",
                        r.get(8),
                        if r.get::<_, Option<i64>>(8).is_some() {
                            SelectionSource::Stored
                        } else {
                            SelectionSource::None
                        },
                    ),
                    artifact("timing", timing.0, timing.1),
                    artifact("timing-original", r.get(9), original(r.get(9))),
                    artifact("fulfillment", fulfillment.0, fulfillment.1),
                    artifact("fulfillment-original", r.get(12), original(r.get(12))),
                ],
            })
        })
        .collect()
}
