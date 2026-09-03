use std::borrow::Cow;

use academy_models::{
    contract::{
        ContractCancellationType, ContractDeclarantName, ContractDeclaration,
        ContractDeclarationDetails, ContractDeclarationId, ContractDeclarationKind, ContractKind,
    },
    email_address::EmailAddress,
    user::UserId,
};
use chrono::{DateTime, Utc};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

/// An RFC 3339 timestamp (e.g. `2026-12-31T23:59:59Z`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ApiTimestamp(pub DateTime<Utc>);

impl JsonSchema for ApiTimestamp {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("Timestamp")
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json_schema!({"type": "string", "format": "date-time"})
    }
}

impl From<DateTime<Utc>> for ApiTimestamp {
    fn from(value: DateTime<Utc>) -> Self {
        Self(value)
    }
}

impl From<ApiTimestamp> for DateTime<Utc> {
    fn from(value: ApiTimestamp) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiContractDeclarationKind {
    Cancellation,
    Withdrawal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiContractKind {
    Premium,
    Coins,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiContractCancellationType {
    Ordinary,
    Extraordinary,
}

/// A contract declaration as returned by the public endpoints.
///
/// Deliberately does not expose the id of the matched account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ApiContractDeclaration {
    pub id: ContractDeclarationId,
    pub kind: ApiContractDeclarationKind,
    pub received_at: ApiTimestamp,
    pub name: ContractDeclarantName,
    pub email: EmailAddress,
    pub contract: ApiContractKind,
    pub cancellation_type: Option<ApiContractCancellationType>,
    pub details: Option<ContractDeclarationDetails>,
    pub requested_end: Option<ApiTimestamp>,
    pub effective_end: Option<ApiTimestamp>,
    pub processed_at: Option<ApiTimestamp>,
}

/// A contract declaration as returned by the admin endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ApiAdminContractDeclaration {
    #[serde(flatten)]
    pub declaration: ApiContractDeclaration,
    /// The account matching the declarant's email address, if any
    pub user_id: Option<UserId>,
}

impl From<ContractDeclarationKind> for ApiContractDeclarationKind {
    fn from(value: ContractDeclarationKind) -> Self {
        match value {
            ContractDeclarationKind::Cancellation => Self::Cancellation,
            ContractDeclarationKind::Withdrawal => Self::Withdrawal,
        }
    }
}

impl From<ApiContractDeclarationKind> for ContractDeclarationKind {
    fn from(value: ApiContractDeclarationKind) -> Self {
        match value {
            ApiContractDeclarationKind::Cancellation => Self::Cancellation,
            ApiContractDeclarationKind::Withdrawal => Self::Withdrawal,
        }
    }
}

impl From<ContractKind> for ApiContractKind {
    fn from(value: ContractKind) -> Self {
        match value {
            ContractKind::Premium => Self::Premium,
            ContractKind::Coins => Self::Coins,
            ContractKind::Other => Self::Other,
        }
    }
}

impl From<ApiContractKind> for ContractKind {
    fn from(value: ApiContractKind) -> Self {
        match value {
            ApiContractKind::Premium => Self::Premium,
            ApiContractKind::Coins => Self::Coins,
            ApiContractKind::Other => Self::Other,
        }
    }
}

impl From<ContractCancellationType> for ApiContractCancellationType {
    fn from(value: ContractCancellationType) -> Self {
        match value {
            ContractCancellationType::Ordinary => Self::Ordinary,
            ContractCancellationType::Extraordinary => Self::Extraordinary,
        }
    }
}

impl From<ApiContractCancellationType> for ContractCancellationType {
    fn from(value: ApiContractCancellationType) -> Self {
        match value {
            ApiContractCancellationType::Ordinary => Self::Ordinary,
            ApiContractCancellationType::Extraordinary => Self::Extraordinary,
        }
    }
}

impl From<ContractDeclaration> for ApiContractDeclaration {
    fn from(value: ContractDeclaration) -> Self {
        Self {
            id: value.id,
            kind: value.kind.into(),
            received_at: value.received_at.into(),
            name: value.name,
            email: value.email,
            contract: value.contract.into(),
            cancellation_type: value.cancellation_type.map(Into::into),
            details: Some(value.details).filter(|details| !details.trim().is_empty()),
            requested_end: value.requested_end.map(Into::into),
            effective_end: value.effective_end.map(Into::into),
            processed_at: value.processed_at.map(Into::into),
        }
    }
}

impl From<ContractDeclaration> for ApiAdminContractDeclaration {
    fn from(value: ContractDeclaration) -> Self {
        Self {
            user_id: value.user_id,
            declaration: value.into(),
        }
    }
}
