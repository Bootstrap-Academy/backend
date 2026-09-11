use chrono::{DateTime, Utc};

use crate::{
    email_address::EmailAddress,
    macros::{id, nutype_string},
    user::UserId,
};

id!(ContractDeclarationId);

/// The kind of declaration made by the declarant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContractDeclarationKind {
    /// Cancellation of a contract (§ 312k BGB)
    Cancellation,
    /// Withdrawal from a contract (§ 356a BGB)
    Withdrawal,
}

/// The contract the declaration refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContractKind {
    Premium,
    Coins,
    Other,
}

/// The kind of cancellation declared by the declarant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContractCancellationType {
    Ordinary,
    Extraordinary,
}

nutype_string!(ContractDeclarantName(
    sanitize(trim),
    validate(len_char_min = 1, len_char_max = 256)
));

nutype_string!(ContractDeclarationDetails(
    validate(len_char_max = 4096),
    derive(Default),
    default = ""
));

// The designation of the contract as the declarant wrote it. § 312k Abs. 2
// S. 2 Nr. 2 BGB requires the cancellation form to let the consumer name the
// contract in their own words, so this is stored next to the `ContractKind`
// they picked and never derived from it.
nutype_string!(ContractDesignation(
    sanitize(trim),
    validate(len_char_min = 1, len_char_max = 1024)
));

// A note an administrator leaves when a declaration has been processed.
nutype_string!(ContractProcessingNote(
    sanitize(trim),
    validate(len_char_min = 1, len_char_max = 4096)
));

/// A declaration made by a consumer regarding one of their contracts.
///
/// A declaration is evidence and therefore outlives the account it belongs to:
/// deleting the account only drops the reference to it. It is removed by
/// `academy task prune-database` once a claim out of the declared contract is
/// time-barred, `contract.retention_years` years after the end of the calendar
/// year of the latest receipt, processing or requested/effective end. Pending
/// processing, delivery and schedules are protected; this is not an automatic
/// determination of statutory limitation in every individual case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractDeclaration {
    pub id: ContractDeclarationId,
    pub kind: ContractDeclarationKind,
    /// The point in time at which the declaration was received.
    pub received_at: DateTime<Utc>,
    pub name: ContractDeclarantName,
    pub email: EmailAddress,
    /// The account matching the declarant's email address, if any.
    pub user_id: Option<UserId>,
    pub contract: ContractKind,
    /// The contract as the declarant named it, if they named it.
    pub contract_designation: Option<ContractDesignation>,
    pub cancellation_type: Option<ContractCancellationType>,
    pub details: ContractDeclarationDetails,
    /// The end of the contract requested by the declarant, if any.
    pub requested_end: Option<DateTime<Utc>>,
    /// The end of the contract as determined by the backend or, once the
    /// declaration has been processed, by an administrator.
    pub effective_end: Option<DateTime<Utc>>,
    /// The point in time at which the declaration was processed manually.
    pub processed_at: Option<DateTime<Utc>>,
    /// What was done when the declaration was processed.
    pub processing_note: Option<ContractProcessingNote>,
    /// Operational evidence; never included in an anonymous receipt.
    pub delivery: Vec<ContractDeliveryStatus>,
    /// JSON archive of receipt-time observations, period changes, schedule and immutable message bytes.
    pub operational_evidence: Option<String>,
}

/// The secret is a separate random capability, never included in a receipt or URL.
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct ContractRequestKey {
    pub id: ContractDeclarationId,
    pub secret: ContractDeclarationId,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
pub struct ContractDeliveryStatus {
    pub kind: String,
    pub attempts: i64,
    #[schemars(with = "String")]
    pub next_attempt_at: DateTime<Utc>,
    /// SMTP acceptance only, not proof of delivery to the inbox.
    #[schemars(with = "Option<String>")]
    pub accepted_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractDeliveryAttempt {
    pub requested_agreement_id: Option<crate::premium::PremiumRenewalId>,
    pub declaration_id: ContractDeclarationId,
    pub kind: String,
    pub recipient: EmailAddress,
    pub subject: String,
    pub body: String,
    pub generation: i64,
}
