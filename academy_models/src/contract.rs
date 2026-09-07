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
    validate(len_char_min = 1, len_char_max = 256)
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
/// year in which it was received (§ 195, § 199 Abs. 1 BGB).
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
}
