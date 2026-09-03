use std::sync::Arc;

use academy_core_withdrawal_contracts::{WithdrawalFeatureService, WithdrawalRecordConsentError};
use academy_models::withdrawal::WithdrawalReference;
use aide::{
    axum::{ApiRouter, routing},
    transform::TransformOperation,
};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    docs::TransformOperationExt,
    error_code,
    errors::{auth_error, auth_error_docs, internal_server_error, internal_server_error_docs},
    extractors::auth::ApiToken,
    models::withdrawal::{
        ApiWithdrawalConsent, ApiWithdrawalConsentDeclaration, ApiWithdrawalSubject,
    },
};

pub const TAG: &str = "Withdrawal";

pub fn router(service: Arc<impl WithdrawalFeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route(
            "/shop/consents",
            routing::post_with(record_consent, record_consent_docs),
        )
        .with_state(service)
        .with_path_items(|op| op.tag(TAG))
}

#[derive(Deserialize, JsonSchema)]
struct RecordConsentRequest {
    /// What is being purchased.
    subject: ApiWithdrawalSubject,
    /// Identifier of the purchased item, if it has one.
    #[serde(default)]
    reference: Option<WithdrawalReference>,
    #[serde(flatten)]
    declaration: ApiWithdrawalConsentDeclaration,
}

async fn record_consent(
    service: State<Arc<impl WithdrawalFeatureService>>,
    token: ApiToken,
    Json(RecordConsentRequest {
        subject,
        reference,
        declaration,
    }): Json<RecordConsentRequest>,
) -> Response {
    match service
        .record_consent(&token.0, subject.into(), reference, declaration.into())
        .await
    {
        Ok(consent) => Json(ApiWithdrawalConsent::from(consent)).into_response(),
        Err(WithdrawalRecordConsentError::ConsentMissing) => {
            WithdrawalConsentMissingError.into_response()
        }
        Err(WithdrawalRecordConsentError::Auth(err)) => auth_error(err),
        Err(WithdrawalRecordConsentError::Other(err)) => internal_server_error(err),
    }
}

fn record_consent_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Record the withdrawal declarations given before an order.")
        .description(
            "Purchases that are completed by this service record the declarations under \
             § 356 Abs. 5 Nr. 2 / Abs. 6 Nr. 2 BGB themselves. This endpoint records them for \
             purchases that are completed by another service, and has to be called before the \
             order is placed there.",
        )
        .add_response::<ApiWithdrawalConsent>(
            StatusCode::OK,
            "The declarations have been recorded.",
        )
        .add_error::<WithdrawalConsentMissingError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

error_code! {
    /// The consumer did not give the declarations that are required before an order can be placed.
    pub WithdrawalConsentMissingError(PRECONDITION_FAILED, "Withdrawal consent missing");
}
