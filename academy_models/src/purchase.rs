//! Accepted offers and consumer artifacts are independent of mutable catalog/account state.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Customer-visible hearts use two of the integer units used by task debits.
/// Keep integer arithmetic so odd quantities and large configured values stay exact.
pub fn display_hearts(units: u64) -> String {
    if units.is_multiple_of(2) {
        (units / 2).to_string()
    } else {
        format!("{},5", units / 2)
    }
}

/// A whole number with a separator between groups of three digits, e.g. `10.000` for
/// German and `10,000` for English.
pub fn group_digits(number: u64, separator: char) -> String {
    let digits = number.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(separator);
        }
        grouped.push(digit);
    }
    grouped
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseProduct {
    pub kind: String,
    pub reference: String,
    pub title: String,
    pub description: String,
    pub coins: u64,
    /// Authoritative product data (period, instructor, scope, access conditions).
    pub facts: Value,
    pub revision: String,
    pub service_starts_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseOffer {
    pub id: Uuid,
    pub user_id: Uuid,
    pub source: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub recipient: String,
    pub product: PurchaseProduct,
    pub document_hash: String,
    pub hash: String,
    pub text: String,
    pub declaration: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provision_window_seconds: Option<u64>,
    /// The order summary line by line, for offers whose `text` is built from it. Older
    /// offers and the offers of other services have none and keep their own `text`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<OfferSummary>,
    /// How the offer is accepted. Absent for every offer before stage 2 of the order dialog:
    /// those keep their acceptance box and declaration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceptance: Option<OfferAcceptance>,
    /// The version of the terms the offer binds, for offers of stage 2 on (`2026-10`), so a
    /// client can name and link exactly that version. Absent for offers before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terms_version: Option<String>,
}

/// How an offer is accepted (stage 2 of the order dialog, terms 2026-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfferAcceptance {
    /// The order button alone accepts the offer with its terms and withdrawal policy (AGB
    /// 2026-10, 11.1). A checkbox remains only for a `declaration` the offer carries: the
    /// early start of a Premium period that begins within the withdrawal period. An offer
    /// with an empty declaration needs none, and none is recorded for it.
    Button,
}

impl PurchaseOffer {
    /// Whether accepting this offer needs the declaration on the early start.
    pub fn requires_early_performance(&self) -> bool {
        match self.acceptance {
            Some(OfferAcceptance::Button) => !self.declaration.is_empty(),
            None => self.product.coins > 0,
        }
    }
}

/// Stage 2 of the order dialog: from the start of terms 2026-10 on, the backend issues its own
/// offers under their documents. Offers issued before keep their own documents and rules.
///
/// Everything comes from the manifest that `scripts/render-purchase-documents.py --release
/// 2026-10` writes next to the two PDFs: the start, the terms version, the file names and their
/// SHA-256. The date printed in the PDFs and the moment of the switch therefore come from one
/// value.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurchaseStage2Config {
    /// Absolute path of `purchase-document-manifest-2026-10.json`; the PDFs lie next to it.
    pub manifest_path: std::path::PathBuf,
    /// SHA-256 of the manifest, as the render script printed it.
    pub manifest_sha256: String,
}

/// The manifest of rendered purchase documents (`scripts/render-purchase-documents.py`). Only
/// the values the backend binds are read; the rest documents the rendering.
#[derive(Debug, Clone, Deserialize)]
pub struct PurchaseDocumentManifest {
    pub release: String,
    /// The start of the terms, an ISO timestamp with an offset.
    pub effective_from: DateTime<chrono::FixedOffset>,
    /// The frontend commit the texts were rendered from; `-dirty` marks a checkout with changes.
    pub frontend_commit: String,
    pub bundle: PurchaseDocumentManifestBundle,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PurchaseDocumentManifestBundle {
    pub terms_version: String,
    /// File name of the terms PDF next to the manifest.
    pub terms_pdf: String,
    pub terms_sha256: String,
    /// File name of the withdrawal instructions PDF next to the manifest.
    pub withdrawal_pdf: String,
    pub withdrawal_sha256: String,
}

impl PurchaseDocumentManifest {
    pub fn parse(bytes: &[u8]) -> anyhow::Result<Self> {
        Ok(serde_json::from_slice(bytes)?)
    }
}

/// An offer's order summary in German and in English.
///
/// The German wording is the agreed one: `PurchaseOffer::text` consists of exactly its
/// lines, one per line, and its declaration is `PurchaseOffer::declaration`. The English
/// wording translates it for the English interface; the German one is binding (AGB 1.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfferSummary {
    pub de: OfferWording,
    pub en: OfferWording,
}

/// The lines of an order summary in one language, in the order the order dialog shows
/// them, and the request for an early start in the same language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfferWording {
    pub lines: Vec<OfferLine>,
    pub declaration: String,
}

impl OfferWording {
    /// The lines as one text, one per line.
    pub fn text(&self) -> String {
        self.lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfferLine {
    pub role: OfferLineRole,
    pub text: String,
}

/// What a line of an order summary states. The dialog highlights the product, the term
/// and the price, which § 312j Abs. 2 BGB wants right above the order button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfferLineRole {
    /// What is bought, with its quantity or period.
    Product,
    /// A further characteristic, or when it is provided.
    Detail,
    /// The term of a Premium period.
    Term,
    /// The total price including VAT, and that nothing else is charged.
    Price,
    /// How the order is paid.
    Payment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PurchaseAcceptance {
    pub order_id: Uuid,
    pub offer_hash: String,
    pub accepted: bool,
    pub early_performance_requested: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseStatus {
    pub offer: PurchaseOffer,
    pub state: String,
    pub accepted_at: Option<DateTime<Utc>>,
    pub confirmation_smtp_accepted_at: Option<DateTime<Utc>>,
    pub fulfillment: Option<Value>,
    pub financial_evidence: Option<Value>,
    pub review_reason: Option<String>,
    #[serde(default)]
    pub provision_deadline: Option<DateTime<Utc>>,
    #[serde(default)]
    pub provision_timing: Option<Value>,
    #[serde(default)]
    pub document_corrections: Vec<String>,
    /// Why the order failed without a charge, in words a client can act on. Absent for any
    /// other state or reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refusal: Option<PurchaseRefusal>,
}

/// `review_reason` of an order whose offer expired or no longer matches the product or the
/// account when it was accepted.
pub const OFFER_CHANGED_REASON: &str = "Offer expired or changed; no charge";

/// Why an order failed without a charge, for clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PurchaseRefusal {
    /// The offer expired or no longer matches the product or the account: nothing was
    /// ordered, and a new offer can be asked for.
    OfferChanged,
}

impl PurchaseRefusal {
    /// The refusal that a stored state and review reason stand for.
    pub fn of(state: &str, review_reason: Option<&str>) -> Option<Self> {
        (state == "failed" && review_reason == Some(OFFER_CHANGED_REASON))
            .then_some(Self::OfferChanged)
    }
}

impl PurchaseStatus {
    pub fn provision_deadline(&self) -> Option<DateTime<Utc>> {
        let elapsed = self.offer.provision_window_seconds.and_then(|seconds| {
            self.accepted_at?
                .checked_add_signed(chrono::TimeDelta::seconds(seconds.try_into().ok()?))
        });
        match (elapsed, self.offer.product.service_starts_at) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PurchaseRecord {
    pub status: PurchaseStatus,
    pub terms_pdf: Vec<u8>,
    pub withdrawal_pdf: Vec<u8>,
    pub confirmation_body: Option<String>,
    pub delivery_generation: i64,
    pub submission: Option<PurchaseAcceptance>,
    pub message_metadata: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_are_grouped_by_three() {
        assert_eq!(group_digits(0, '.'), "0");
        assert_eq!(group_digits(500, '.'), "500");
        assert_eq!(group_digits(1000, '.'), "1.000");
        assert_eq!(group_digits(10000, ','), "10,000");
        assert_eq!(group_digits(1_000_000, '.'), "1.000.000");
    }

    #[test]
    fn a_refusal_is_named_only_for_a_changed_offer_and_old_statuses_still_read() {
        assert_eq!(
            PurchaseRefusal::of("failed", Some(OFFER_CHANGED_REASON)),
            Some(PurchaseRefusal::OfferChanged)
        );
        for (state, reason) in [
            ("failed", Some("No refill necessary; no charge")),
            ("failed", None),
            ("review", Some(OFFER_CHANGED_REASON)),
            ("fulfilled", None),
        ] {
            assert_eq!(
                PurchaseRefusal::of(state, reason),
                None,
                "{state} {reason:?}"
            );
        }
        let status = serde_json::json!({
            "offer":{"id":Uuid::nil(),"user_id":Uuid::nil(),"source":"backend",
                "created_at":"2026-10-17T00:00:00Z","expires_at":"2026-10-17T00:20:00Z",
                "recipient":"synthetic@example.invalid","product":{"kind":"hearts","reference":"hearts","title":"t","description":"d","coins":50,"facts":{},"revision":"r","service_starts_at":null},
                "document_hash":"d","hash":"h","text":"t","declaration":"d"},
            "state":"failed","accepted_at":null,"confirmation_smtp_accepted_at":null,
            "fulfillment":null,"financial_evidence":null,"review_reason":OFFER_CHANGED_REASON
        });
        // A status written before the field reads without it, and writes no field while
        // there is no refusal.
        let mut read: PurchaseStatus = serde_json::from_value(status).unwrap();
        assert_eq!(read.refusal, None);
        assert!(
            serde_json::to_value(&read)
                .unwrap()
                .get("refusal")
                .is_none()
        );
        read.refusal = Some(PurchaseRefusal::OfferChanged);
        assert_eq!(
            serde_json::to_value(&read).unwrap()["refusal"],
            serde_json::json!("offer_changed")
        );
    }

    #[test]
    fn offers_without_summary_serialize_as_before() {
        let wording = OfferWording {
            lines: vec![
                OfferLine {
                    role: OfferLineRole::Product,
                    text: "Herzen auffüllen".into(),
                },
                OfferLine {
                    role: OfferLineRole::Price,
                    text: "Gesamtpreis".into(),
                },
            ],
            declaration: "Erklärung".into(),
        };
        assert_eq!(wording.text(), "Herzen auffüllen\nGesamtpreis");
        let line = serde_json::to_value(&wording.lines[0]).unwrap();
        assert_eq!(
            line,
            serde_json::json!({"role":"product","text":"Herzen auffüllen"})
        );
        // A stored offer from before the summary reads back without one and writes no field.
        let stored = serde_json::json!({"id":Uuid::nil(),"user_id":Uuid::nil(),"source":"backend",
            "created_at":"2026-10-01T00:00:00Z","expires_at":"2026-10-01T00:20:00Z","recipient":"r",
            "product":{"kind":"hearts","reference":"hearts","title":"t","description":"d","coins":50,
                "facts":{},"revision":"r","service_starts_at":null},
            "document_hash":"d","hash":"h","text":"t","declaration":"e"});
        let offer: PurchaseOffer = serde_json::from_value(stored.clone()).unwrap();
        assert!(offer.summary.is_none());
        assert_eq!(serde_json::to_value(&offer).unwrap(), stored);
    }
}
