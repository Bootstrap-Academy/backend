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

/// A declaration made by a consumer regarding one of their contracts.
///
/// Declarations are evidence of a legal declaration and are therefore never
/// deleted, not even when the associated account is deleted.
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
    pub cancellation_type: Option<ContractCancellationType>,
    pub details: ContractDeclarationDetails,
    /// The end of the contract requested by the declarant, if any.
    pub requested_end: Option<DateTime<Utc>>,
    /// The end of the contract as determined by the backend, if known.
    pub effective_end: Option<DateTime<Utc>>,
    /// The point in time at which the declaration was processed manually.
    pub processed_at: Option<DateTime<Utc>>,
}
