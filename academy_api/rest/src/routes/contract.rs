use std::sync::Arc;

use academy_core_contract_contracts::{
    ContractCancellationRequest, ContractDeclarationListQuery, ContractDeclarationListResult,
    ContractDeclarationProcessingUpdate, ContractDeclarationResult, ContractDeclareError,
    ContractFeatureService, ContractListError, ContractProcessingAction, ContractSetProcessedError,
    ContractWithdrawalRequest,
};
use academy_models::{
    contract::{
        ContractDeclarantName, ContractDeclarationDetails, ContractDeclarationId,
        ContractDesignation, ContractProcessingNote, ContractRequestKey,
    },
    email_address::EmailAddress,
};
use aide::{
    axum::{ApiRouter, routing},
    transform::TransformOperation,
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    docs::TransformOperationExt,
    error_code,
    errors::{auth_error, auth_error_docs, internal_server_error, internal_server_error_docs},
    extractors::auth::ApiToken,
    middlewares::client_ip::ClientIp,
    models::{
        ApiPaginationSlice, StringOption,
        contract::{
            ApiAdminContractDeclaration, ApiContractCancellationType, ApiContractDeclarationKind,
            ApiContractKind, ApiPublicContractReceipt, ApiTimestamp,
        },
    },
};

pub const TAG: &str = "Contract";

/// Route of the administrative declaration listing.
///
/// Named because the administrative audit log records reads of this route; see
/// [`crate::middlewares::admin_audit`].
pub const DECLARATIONS_ROUTE: &str = "/contracts/declarations";

/// Route of the administrative endpoint that records the processing of a
/// declaration. Every request to it is recorded in the administrative audit
/// log, because it changes state; see [`crate::middlewares::admin_audit`].
pub const DECLARATION_ROUTE: &str = "/contracts/declarations/{declaration_id}";

pub fn router(service: Arc<impl ContractFeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route(
            "/contracts/receipts",
            routing::post_with(lookup_receipt, |op| {
                op.summary("Recover a receipt with its private request capability")
                    .add_response::<DeclarationResponse>(StatusCode::OK, None)
            }),
        )
        .api_route(
            "/contracts/cancellations",
            routing::post_with(declare_cancellation, declare_cancellation_docs),
        )
        .api_route(
            "/contracts/withdrawals",
            routing::post_with(declare_withdrawal, declare_withdrawal_docs),
        )
        .api_route(
            DECLARATIONS_ROUTE,
            routing::get_with(list_declarations, list_declarations_docs),
        )
        .api_route(
            DECLARATION_ROUTE,
            routing::patch_with(set_declaration_processed, set_declaration_processed_docs),
        )
        .with_state(service)
        .with_path_items(|op| op.tag(TAG))
}

#[derive(Serialize, JsonSchema)]
struct DeclarationResponse {
    /// The stored declaration
    declaration: ApiPublicContractReceipt,
    /// Whether the confirmation email has been sent to the declarant
    confirmation_email_sent: bool,
}

impl From<ContractDeclarationResult> for DeclarationResponse {
    fn from(value: ContractDeclarationResult) -> Self {
        Self {
            declaration: value.declaration.into(),
            confirmation_email_sent: value.confirmation_email_sent,
        }
    }
}

#[derive(Deserialize, JsonSchema)]
struct DeclareCancellationRequest {
    request_key: Option<ContractRequestKey>,
    renewal_agreement_id: Option<academy_models::premium::PremiumRenewalId>,
    /// Full name of the declarant
    name: ContractDeclarantName,
    /// Email address of the declarant
    email: EmailAddress,
    /// The contract the declaration refers to
    contract: ApiContractKind,
    /// The contract as the declarant names it, in their own words. Accepted
    /// for every kind of contract (§ 312k Abs. 2 S. 2 Nr. 2 BGB).
    #[serde(default)]
    contract_designation: StringOption<ContractDesignation>,
    /// Whether the contract is cancelled ordinarily or extraordinarily
    cancellation_type: ApiContractCancellationType,
    /// Optional reason for the cancellation
    #[serde(default)]
    details: StringOption<ContractDeclarationDetails>,
    /// The end of the contract requested by the declarant
    #[serde(default)]
    requested_end: Option<ApiTimestamp>,
}

async fn declare_cancellation(
    service: State<Arc<impl ContractFeatureService>>,
    Extension(ClientIp(client_ip)): Extension<ClientIp>,
    Json(DeclareCancellationRequest {
        request_key,
        renewal_agreement_id,
        name,
        email,
        contract,
        contract_designation,
        cancellation_type,
        details,
        requested_end,
    }): Json<DeclareCancellationRequest>,
) -> Response {
    match service
        .declare_cancellation(
            client_ip,
            ContractCancellationRequest {
                request_key,
                renewal_agreement_id,
                name,
                email,
                contract: contract.into(),
                contract_designation: contract_designation.into(),
                cancellation_type: cancellation_type.into(),
                details: Option::from(details).unwrap_or_default(),
                requested_end: requested_end.map(Into::into),
            },
        )
        .await
    {
        Ok(result) => Json(DeclarationResponse::from(result)).into_response(),
        Err(ContractDeclareError::NotFound) => DeclarationNotFoundError.into_response(),
        Err(ContractDeclareError::RequestConflict) => DeclarationConflictError.into_response(),
        Err(ContractDeclareError::RateLimit) => TooManyRequestsError.into_response(),
        Err(ContractDeclareError::Other(err)) => internal_server_error(err),
    }
}

fn declare_cancellation_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Declare the cancellation of a contract.")
        .description(
            "No authentication required. The receipt contains only submitted data. Optional exact \
             renewal agreement identification permits ordinary Premium period-end scheduling. \
             Email/name alone cause no account effects. Requested date and legal effect are distinct; \
             unverified or extraordinary requests require prompt review without changing receipt-based rights.",
        )
        .add_response::<DeclarationResponse>(StatusCode::OK, "The declaration has been recorded.")
        .add_error::<TooManyRequestsError>()
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct DeclareWithdrawalRequest {
    request_key: Option<ContractRequestKey>,
    /// Full name of the declarant
    name: ContractDeclarantName,
    /// Email address of the declarant
    email: EmailAddress,
    /// The contract or order the declaration refers to
    contract: ApiContractKind,
    /// The contract or order as the declarant names it, in their own words
    #[serde(default)]
    contract_designation: StringOption<ContractDesignation>,
    /// Optional additional information
    #[serde(default)]
    details: StringOption<ContractDeclarationDetails>,
}

async fn declare_withdrawal(
    service: State<Arc<impl ContractFeatureService>>,
    Extension(ClientIp(client_ip)): Extension<ClientIp>,
    Json(DeclareWithdrawalRequest {
        request_key,
        name,
        email,
        contract,
        contract_designation,
        details,
    }): Json<DeclareWithdrawalRequest>,
) -> Response {
    match service
        .declare_withdrawal(
            client_ip,
            ContractWithdrawalRequest {
                request_key,
                name,
                email,
                contract: contract.into(),
                contract_designation: contract_designation.into(),
                details: Option::from(details).unwrap_or_default(),
            },
        )
        .await
    {
        Ok(result) => Json(DeclarationResponse::from(result)).into_response(),
        Err(ContractDeclareError::NotFound) => DeclarationNotFoundError.into_response(),
        Err(ContractDeclareError::RequestConflict) => DeclarationConflictError.into_response(),
        Err(ContractDeclareError::RateLimit) => TooManyRequestsError.into_response(),
        Err(ContractDeclareError::Other(err)) => internal_server_error(err),
    }
}

fn declare_withdrawal_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Declare the withdrawal from a contract.")
        .description(
            "Does not require authentication. The declaration is stored with its receipt \
             timestamp and confirmed to the declarant by email (§ 356a BGB).",
        )
        .add_response::<DeclarationResponse>(StatusCode::OK, "The declaration has been recorded.")
        .add_error::<TooManyRequestsError>()
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct ListDeclarationsFilter {
    /// Filter by `kind`
    kind: Option<ApiContractDeclarationKind>,
}

#[derive(Serialize, JsonSchema)]
struct ListResult {
    /// The total number of declarations matching the given query
    total: u64,
    /// The paginated list of declarations matching the given query
    declarations: Vec<ApiAdminContractDeclaration>,
}

async fn list_declarations(
    service: State<Arc<impl ContractFeatureService>>,
    token: ApiToken,
    Query(pagination): Query<ApiPaginationSlice>,
    Query(ListDeclarationsFilter { kind }): Query<ListDeclarationsFilter>,
) -> Response {
    match service
        .list_declarations(
            &token.0,
            ContractDeclarationListQuery {
                kind: kind.map(Into::into),
                pagination: pagination.into(),
            },
        )
        .await
    {
        Ok(ContractDeclarationListResult {
            total,
            declarations,
        }) => Json(ListResult {
            total,
            declarations: declarations.into_iter().map(Into::into).collect(),
        })
        .into_response(),
        Err(ContractListError::Auth(err)) => auth_error(err),
        Err(ContractListError::Other(err)) => internal_server_error(err),
    }
}

fn list_declarations_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Return all contract declarations matching the given query.")
        .description(
            "Requires admin privileges. `processed_at` and `processing_note` say whether and how \
             a declaration has already been dealt with.",
        )
        .add_response::<ListResult>(StatusCode::OK, None)
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct DeclarationPath {
    declaration_id: ContractDeclarationId,
}

#[derive(Deserialize, JsonSchema)]
struct SetProcessedRequest {
    #[serde(default)]
    identity_verified: bool,
    #[serde(default)]
    action: ContractProcessingAction,
    verified_user_id: Option<academy_models::user::UserId>,
    renewal_agreement_id: Option<academy_models::premium::PremiumRenewalId>,
    /// The end of the contract as it was confirmed to the declarant. Left
    /// unchanged if not given.
    #[serde(default)]
    effective_end: Option<ApiTimestamp>,
    /// What was done. Left unchanged if not given.
    #[serde(default)]
    note: StringOption<ContractProcessingNote>,
}

async fn set_declaration_processed(
    service: State<Arc<impl ContractFeatureService>>,
    token: ApiToken,
    Path(DeclarationPath { declaration_id }): Path<DeclarationPath>,
    Json(SetProcessedRequest {
        identity_verified,
        action,
        verified_user_id,
        renewal_agreement_id,
        effective_end,
        note,
    }): Json<SetProcessedRequest>,
) -> Response {
    match service
        .set_declaration_processed(
            &token.0,
            declaration_id,
            ContractDeclarationProcessingUpdate {
                identity_verified,
                action,
                verified_user_id,
                renewal_agreement_id,
                effective_end: effective_end.map(Into::into),
                note: note.into(),
            },
        )
        .await
    {
        Ok(declaration) => Json(ApiAdminContractDeclaration::from(declaration)).into_response(),
        Err(ContractSetProcessedError::Invalid) => DeclarationInvalidError.into_response(),
        Err(ContractSetProcessedError::Conflict) => DeclarationConflictError.into_response(),
        Err(ContractSetProcessedError::NotFound) => DeclarationNotFoundError.into_response(),
        Err(ContractSetProcessedError::Auth(err)) => auth_error(err),
        Err(ContractSetProcessedError::Other(err)) => internal_server_error(err),
    }
}

fn set_declaration_processed_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Record that a contract declaration has been processed.")
        .description(
            "Requires MFA admin privileges, independently verified identity and a resolution note. \
             Either record an already implemented external resolution and communication (a cancellation \
             requires its established end), or schedule the original identified current Premium agreement \
             using the declaration's original receipt and requested date. Processing time never changes \
             receipt-based rights. Conflicting or already processed actions are rejected.",
        )
        .add_response::<ApiAdminContractDeclaration>(
            StatusCode::OK,
            "The declaration has been updated.",
        )
        .add_error::<DeclarationNotFoundError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

error_code! {
    DeclarationConflictError(CONFLICT, "Declaration request conflicts with stored state");
    DeclarationInvalidError(BAD_REQUEST, "Verified action, identity, date and resolution note required");
    /// Too many requests.
    pub TooManyRequestsError(TOO_MANY_REQUESTS, "Too many requests");
    /// The contract declaration does not exist.
    DeclarationNotFoundError(NOT_FOUND, "Declaration not found");
}

async fn lookup_receipt(
    service: State<Arc<impl ContractFeatureService>>,
    Json(key): Json<ContractRequestKey>,
) -> Response {
    match service.lookup_receipt(key).await {
        Ok(result) => Json(DeclarationResponse::from(result)).into_response(),
        Err(ContractDeclareError::NotFound) => DeclarationNotFoundError.into_response(),
        Err(ContractDeclareError::RequestConflict) => DeclarationConflictError.into_response(),
        Err(ContractDeclareError::RateLimit) => TooManyRequestsError.into_response(),
        Err(ContractDeclareError::Other(err)) => internal_server_error(err),
    }
}
