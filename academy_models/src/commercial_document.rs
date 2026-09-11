//! Local original-document observations. These are neither download reservations
//! nor current learning/financial authority. All route identifiers remain strings.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentInventory {
    pub protocol: u8,
    pub claimant_subject: Uuid,
    pub observed_at: DateTime<Utc>,
    pub scope: InventoryScope,
    pub records: Vec<DocumentRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryScope {
    pub finance: String,
    pub purchases: String,
    pub archives_scanned: bool,
    pub remote_sources_queried: bool,
    pub catalog_complete: bool,
    pub historical_owner_inventory_complete: bool,
    pub known_local_enumeration_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentFamily {
    Purchase,
    Finance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DocumentKind {
    Purchase,
    Invoice,
    CreditNote,
    FinalStatement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerRelation {
    Claimant,
    SameCaseLearningSubject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordBasis {
    OriginalOffer,
    OwnedDocument,
    OwnInvoiceReference,
    QualifiedRetentionReference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReaderState {
    Candidate,
    Unavailable,
    ArchiveUnchecked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnavailableReason {
    MissingProgress,
    OriginalIdentityMismatch,
    UnsupportedIdentifier,
    OriginalReaderNotAdmitted,
    IdentityPendingReview,
    RetiredRecorded,
    AmbiguousPeriod,
    EmptySelectedArtifact,
    AbsentSelectedArtifact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionSource {
    Stored,
    Original,
    OriginalV2,
    Correction,
    DatabaseOriginal,
    ArchiveUnchecked,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactObservation {
    Nonempty,
    Empty,
    Absent,
    Unchecked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentSelector {
    pub kind: DocumentKind,
    pub id: String,
    pub variant: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentArtifact {
    pub variant: String,
    pub selection_source: SelectionSource,
    pub observation: ArtifactObservation,
    pub reader_state: ReaderState,
    pub reason: Option<UnavailableReason>,
    pub selector: Option<DocumentSelector>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentRecord {
    pub family: DocumentFamily,
    pub kind: DocumentKind,
    pub source_service: String,
    pub source_subject: Uuid,
    pub owner_relation: OwnerRelation,
    pub purchase_source: Option<String>,
    pub offer_id: Option<Uuid>,
    pub printed_number: Option<String>,
    pub record_basis: RecordBasis,
    pub reader_state: ReaderState,
    pub reason: Option<UnavailableReason>,
    /// Finance has one original selector. Purchases have seven distinct artifact
    /// selectors instead of silently selecting one document as the default.
    pub selector: Option<DocumentSelector>,
    pub artifacts: Vec<DocumentArtifact>,
}
