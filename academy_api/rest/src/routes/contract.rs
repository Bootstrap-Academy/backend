use std::sync::Arc;

use academy_core_contract_contracts::{
    ContractCancellationRequest, ContractDeclarationListQuery, ContractDeclarationListResult,
    ContractDeclarationResult, ContractDeclareError, ContractFeatureService, ContractListError,
    ContractWithdrawalRequest,
};
use academy_models::{
    contract::{ContractDeclarantName, ContractDeclarationDetails},
    email_address::EmailAddress,
};
use aide::{
    axum::{ApiRouter, routing},
    transform::TransformOperation,
};
use axum::{
    Extension, Json,
    extract::{Query, State},
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
            ApiAdminContractDeclaration, ApiContractCancellationType, ApiContractDeclaration,
            ApiContractDeclarationKind, ApiContractKind, ApiTimestamp,
        },
    },
};

pub const TAG: &str = "Contract";

pub fn router(service: Arc<impl ContractFeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route(
            "/contracts/cancellations",
            routing::post_with(declare_cancellation, declare_cancellation_docs),
        )
        .api_route(
            "/contracts/withdrawals",
            routing::post_with(declare_withdrawal, declare_withdrawal_docs),
        )
        .api_route(
            "/contracts/declarations",
            routing::get_with(list_declarations, list_declarations_docs),
        )
        .with_state(service)
        .with_path_items(|op| op.tag(TAG))
}

#[derive(Serialize, JsonSchema)]
struct DeclarationResponse {
    /// The stored declaration
    declaration: ApiContractDeclaration,
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
    /// Full name of the declarant
    name: ContractDeclarantName,
    /// Email address of the declarant
    email: EmailAddress,
    /// The contract the declaration refers to
    contract: ApiContractKind,
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
        name,
        email,
        contract,
        cancellation_type,
        details,
        requested_end,
    }): Json<DeclareCancellationRequest>,
) -> Response {
    match service
        .declare_cancellation(
            client_ip,
            ContractCancellationRequest {
                name,
                email,
                contract: contract.into(),
                cancellation_type: cancellation_type.into(),
                details: Option::from(details).unwrap_or_default(),
                requested_end: requested_end.map(Into::into),
            },
        )
        .await
    {
        Ok(result) => Json(DeclarationResponse::from(result)).into_response(),
        Err(ContractDeclareError::RateLimit) => TooManyRequestsError.into_response(),
        Err(ContractDeclareError::Other(err)) => internal_server_error(err),
    }
}

fn declare_cancellation_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Declare the cancellation of a contract.")
        .description(
            "Does not require authentication. The declaration is stored with its receipt \
             timestamp and confirmed to the declarant by email (§ 312k Abs. 4 BGB). If the email \
             address matches an account with a premium membership, the automatic renewal is \
             switched off and the end of the contract is returned in `effective_end`.",
        )
        .add_response::<DeclarationResponse>(StatusCode::OK, "The declaration has been recorded.")
        .add_error::<TooManyRequestsError>()
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct DeclareWithdrawalRequest {
    /// Full name of the declarant
    name: ContractDeclarantName,
    /// Email address of the declarant
    email: EmailAddress,
    /// The contract or order the declaration refers to
    contract: ApiContractKind,
    /// Optional additional information
    #[serde(default)]
    details: StringOption<ContractDeclarationDetails>,
}

async fn declare_withdrawal(
    service: State<Arc<impl ContractFeatureService>>,
    Extension(ClientIp(client_ip)): Extension<ClientIp>,
    Json(DeclareWithdrawalRequest {
        name,
        email,
        contract,
        details,
    }): Json<DeclareWithdrawalRequest>,
) -> Response {
    match service
        .declare_withdrawal(
            client_ip,
            ContractWithdrawalRequest {
                name,
                email,
                contract: contract.into(),
                details: Option::from(details).unwrap_or_default(),
            },
        )
        .await
    {
        Ok(result) => Json(DeclarationResponse::from(result)).into_response(),
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
        .description("Requires admin privileges.")
        .add_response::<ListResult>(StatusCode::OK, None)
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

error_code! {
    /// Too many requests.
    TooManyRequestsError(TOO_MANY_REQUESTS, "Too many requests");
}
