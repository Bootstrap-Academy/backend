use academy_di::Build;
use academy_models::{
    finance::{FinancialDocument, FinancialDocumentKind, FinancialDocumentNumber},
    pagination::PaginationSlice,
    user::UserId,
};
use academy_persistence_contracts::finance::FinancialDocumentRepository;
use academy_utils::trace_instrument;
use chrono::{DateTime, Utc};
use clorinde::{
    client::Params,
    queries::{
        self,
        finance::{
            CountDocumentsParams, ListDocumentsParams, PseudonymizeDocumentsParams,
            RecordDocumentParams, SettleDocumentParams,
        },
    },
};
use futures::{StreamExt, TryStreamExt};

use crate::PostgresTransaction;

#[derive(Debug, Clone, Build)]
pub struct PostgresFinancialDocumentRepository;

impl FinancialDocumentRepository<PostgresTransaction> for PostgresFinancialDocumentRepository {
    #[trace_instrument(skip(self, txn, document), fields(document = %*document.number))]
    async fn record(
        &self,
        txn: &mut PostgresTransaction,
        document: &FinancialDocument,
    ) -> anyhow::Result<()> {
        let params = RecordDocumentParams {
            number: &*document.number,
            kind: document.kind.as_str(),
            user_id: document.user_id.map(|user_id| *user_id),
            issued_at: document.issued_at.into(),
            customer_details: document
                .customer_details
                .as_ref()
                .map(|details| details.iter().map(String::as_str).collect::<Vec<_>>()),
            coins: document.coins.map(i64::try_from).transpose()?,
            net_total_cents: document.net_total_cents,
            vat_total_cents: document.vat_total_cents,
            gross_total_cents: document.gross_total_cents,
        };

        queries::finance::record_document()
            .params(txn.txn(), &params)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn get(
        &self,
        txn: &mut PostgresTransaction,
        number: &FinancialDocumentNumber,
    ) -> anyhow::Result<Option<FinancialDocument>> {
        queries::finance::get_document()
            .bind(txn.txn(), &&**number)
            .opt()
            .await
            .map_err(Into::into)
            .and_then(|row| row.map(decode_document).transpose())
    }

    #[trace_instrument(skip(self, txn))]
    async fn settle(
        &self,
        txn: &mut PostgresTransaction,
        number: &FinancialDocumentNumber,
        settled_at: DateTime<Utc>,
    ) -> anyhow::Result<bool> {
        let params = SettleDocumentParams {
            settled_at: settled_at.into(),
            number: &**number,
        };

        queries::finance::settle_document()
            .params(txn.txn(), &params)
            .await
            .map(|rows| rows > 0)
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn pseudonymize(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
        customer_details: &[String],
    ) -> anyhow::Result<u64> {
        let params = PseudonymizeDocumentsParams {
            customer_details: customer_details
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            user_id: *user_id,
        };

        queries::finance::pseudonymize_documents()
            .params(txn.txn(), &params)
            .await
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn list_by_user_id(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Vec<FinancialDocument>> {
        queries::finance::list_documents_by_user_id()
            .bind(txn.txn(), &user_id)
            .iter()
            .await?
            .map(|row| row.map_err(Into::into).and_then(decode_document))
            .try_collect()
            .await
    }

    #[trace_instrument(skip(self, txn, search))]
    async fn list(
        &self,
        txn: &mut PostgresTransaction,
        kind: Option<FinancialDocumentKind>,
        search: Option<String>,
        pagination: PaginationSlice,
    ) -> anyhow::Result<Vec<FinancialDocument>> {
        let search = search.as_deref().map(escape_like_pattern);
        let params = ListDocumentsParams {
            kind: kind.map(FinancialDocumentKind::as_str),
            search: search.as_deref(),
            limit: (*pagination.limit).try_into()?,
            offset: pagination.offset.try_into()?,
        };

        queries::finance::list_documents()
            .params(txn.txn(), &params)
            .iter()
            .await?
            .map(|row| row.map_err(Into::into).and_then(decode_document))
            .try_collect()
            .await
    }

    #[trace_instrument(skip(self, txn, search))]
    async fn count(
        &self,
        txn: &mut PostgresTransaction,
        kind: Option<FinancialDocumentKind>,
        search: Option<String>,
    ) -> anyhow::Result<u64> {
        let search = search.as_deref().map(escape_like_pattern);
        let params = CountDocumentsParams {
            kind: kind.map(FinancialDocumentKind::as_str),
            search: search.as_deref(),
        };

        queries::finance::count_documents()
            .params(txn.txn(), &params)
            .one()
            .await
            .map_err(Into::into)
            .and_then(|count| count.try_into().map_err(Into::into))
    }

    #[trace_instrument(skip(self, txn))]
    async fn list_numbers(
        &self,
        txn: &mut PostgresTransaction,
    ) -> anyhow::Result<Vec<FinancialDocumentNumber>> {
        queries::finance::list_document_numbers()
            .bind(txn.txn())
            .iter()
            .await?
            .map(|row| {
                row.map_err(Into::into)
                    .and_then(|number| FinancialDocumentNumber::try_new(number).map_err(Into::into))
            })
            .try_collect()
            .await
    }

    #[trace_instrument(skip(self, txn))]
    async fn list_issued_before(
        &self,
        txn: &mut PostgresTransaction,
        issued_before: DateTime<Utc>,
    ) -> anyhow::Result<Vec<FinancialDocument>> {
        queries::finance::list_documents_issued_before()
            .bind(txn.txn(), &issued_before.into())
            .iter()
            .await?
            .map(|row| row.map_err(Into::into).and_then(decode_document))
            .try_collect()
            .await
    }

    #[trace_instrument(skip(self, txn))]
    async fn delete_issued_before(
        &self,
        txn: &mut PostgresTransaction,
        issued_before: DateTime<Utc>,
    ) -> anyhow::Result<u64> {
        queries::finance::delete_documents_issued_before()
            .bind(txn.txn(), &issued_before.into())
            .await
            .map_err(Into::into)
    }
}

/// Escape the characters `like` gives a special meaning to, so that a search
/// term is matched literally.
///
/// The search term is put into the pattern of an `ilike`, where `%` matches
/// any run of characters and `_` any single one. Without this, an
/// administrator searching for `%` would get every document instead of the
/// documents that contain a percent sign. `\` is the escape character `like`
/// uses by default, so it has to be escaped first.
fn escape_like_pattern(search: &str) -> String {
    let mut pattern = String::with_capacity(search.len());
    for character in search.chars() {
        if matches!(character, '\\' | '%' | '_') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern
}

fn decode_document(value: queries::finance::Document) -> anyhow::Result<FinancialDocument> {
    Ok(FinancialDocument {
        number: value.number.try_into()?,
        kind: value.kind.parse::<FinancialDocumentKind>()?,
        user_id: value.user_id.map(Into::into),
        issued_at: value.issued_at.into(),
        customer_details: value.customer_details,
        coins: value.coins.map(u64::try_from).transpose()?,
        net_total_cents: value.net_total_cents,
        vat_total_cents: value.vat_total_cents,
        gross_total_cents: value.gross_total_cents,
        settled_at: value.settled_at.map(Into::into),
    })
}
