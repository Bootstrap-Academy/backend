use std::sync::Arc;

use academy_core_finance_contracts::{
    FinanceDownloadError, FinanceFeatureService, FinanceGetDownloadTokenError,
};
use aide::{
    axum::{routing, ApiRouter},
    transform::TransformOperation,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::{headers::ContentType, TypedHeader};
use mime::APPLICATION_PDF;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    docs::TransformOperationExt,
    error_code,
    errors::{
        auth_error, auth_error_docs, internal_server_error, internal_server_error_docs,
        InvalidTokenError,
    },
    extractors::auth::ApiToken,
};

pub const TAG: &str = "Finance";

pub fn router(service: Arc<impl FinanceFeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route(
            "/finance/token",
            routing::get_with(get_download_token, get_download_token_docs),
        )
        .api_route(
            "/finance/invoices/{token}/{invoice_number}/invoice.pdf",
            routing::get_with(download_invoice, download_invoice_docs),
        )
        .api_route(
            "/finance/credit_notes/{token}/{year}/{month}/credit_note.pdf",
            routing::get_with(download_credit_note, download_credit_note_docs),
        )
        .with_state(service)
        .with_path_items(|op| op.tag(TAG))
}

async fn get_download_token(
    service: State<Arc<impl FinanceFeatureService>>,
    token: ApiToken,
) -> Response {
    match service.get_download_token(&token.0).await {
        Ok(token) => Json(token).into_response(),
        Err(FinanceGetDownloadTokenError::Auth(err)) => auth_error(err),
        Err(FinanceGetDownloadTokenError::Other(err)) => internal_server_error(err),
    }
}

fn get_download_token_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Return a token to download finance documents.")
        .add_response::<String>(StatusCode::OK, None)
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct DownloadInvoicePath {
    token: String,
    invoice_number: u64,
}

async fn download_invoice(
    service: State<Arc<impl FinanceFeatureService>>,
    Path(DownloadInvoicePath {
        token,
        invoice_number,
    }): Path<DownloadInvoicePath>,
) -> Response {
    match service.download_invoice(&token, invoice_number).await {
        Ok(pdf) => (TypedHeader(ContentType::from(APPLICATION_PDF)), pdf).into_response(),
        Err(FinanceDownloadError::InvalidToken) => InvalidTokenError.into_response(),
        Err(FinanceDownloadError::NotFound) => InvoiceNotFoundError.into_response(),
        Err(FinanceDownloadError::Other(err)) => internal_server_error(err),
    }
}

fn download_invoice_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Download an invoice")
        .add_error::<InvalidTokenError>()
        .add_error::<InvoiceNotFoundError>()
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct DownloadCreditNotePath {
    token: String,
    year: i32,
    month: u32,
}

async fn download_credit_note(
    service: State<Arc<impl FinanceFeatureService>>,
    Path(DownloadCreditNotePath { token, year, month }): Path<DownloadCreditNotePath>,
) -> Response {
    match service.download_credit_note(&token, year, month).await {
        Ok(pdf) => (TypedHeader(ContentType::from(APPLICATION_PDF)), pdf).into_response(),
        Err(FinanceDownloadError::InvalidToken) => InvalidTokenError.into_response(),
        Err(FinanceDownloadError::NotFound) => CreditNoteNotYetAvailableError.into_response(),
        Err(FinanceDownloadError::Other(err)) => internal_server_error(err),
    }
}

fn download_credit_note_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Download a credit note")
        .add_error::<InvalidTokenError>()
        .add_error::<CreditNoteNotYetAvailableError>()
        .with(internal_server_error_docs)
}

error_code! {
    /// The invoice does not exist.
    InvoiceNotFoundError(NOT_FOUND, "Invoice not found");
    /// The credit note is not available yet.
    CreditNoteNotYetAvailableError(NOT_FOUND, "Credit note not yet available");
}
