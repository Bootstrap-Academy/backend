use std::str::FromStr;

use academy_di::Build;
use academy_models::{
    contract::{
        ContractCancellationType, ContractDeclaration, ContractDeclarationKind, ContractKind,
    },
    pagination::PaginationSlice,
};
use academy_persistence_contracts::contract::ContractRepository;
use academy_utils::trace_instrument;
use clorinde::{
    client::Params,
    queries::{
        self,
        contract::{CreateParams, ListParams},
    },
};
use futures::{StreamExt, TryStreamExt};

use crate::PostgresTransaction;

#[derive(Debug, Clone, Build)]
pub struct PostgresContractRepository;

impl ContractRepository<PostgresTransaction> for PostgresContractRepository {
    #[trace_instrument(skip(self, txn))]
    async fn create(
        &self,
        txn: &mut PostgresTransaction,
        declaration: ContractDeclaration,
    ) -> anyhow::Result<()> {
        let params = CreateParams {
            id: *declaration.id,
            kind: encode_kind(declaration.kind),
            received_at: declaration.received_at.into(),
            name: &*declaration.name,
            email: declaration.email.as_str(),
            user_id: declaration.user_id.map(|user_id| *user_id),
            contract: encode_contract(declaration.contract),
            cancellation_type: declaration.cancellation_type.map(encode_cancellation_type),
            details: &*declaration.details,
            requested_end: declaration.requested_end.map(Into::into),
            effective_end: declaration.effective_end.map(Into::into),
            processed_at: declaration.processed_at.map(Into::into),
        };

        queries::contract::create()
            .params(txn.txn(), &params)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn list(
        &self,
        txn: &mut PostgresTransaction,
        kind: Option<ContractDeclarationKind>,
        pagination: PaginationSlice,
    ) -> anyhow::Result<Vec<ContractDeclaration>> {
        let params = ListParams {
            kind: kind.map(encode_kind),
            limit: (*pagination.limit).try_into()?,
            offset: pagination.offset.try_into()?,
        };

        queries::contract::list()
            .params(txn.txn(), &params)
            .iter()
            .await?
            .map(|row| row.map_err(Into::into).and_then(decode_declaration))
            .try_collect()
            .await
    }

    #[trace_instrument(skip(self, txn))]
    async fn count(
        &self,
        txn: &mut PostgresTransaction,
        kind: Option<ContractDeclarationKind>,
    ) -> anyhow::Result<u64> {
        let kind = kind.map(encode_kind);

        queries::contract::count()
            .bind(txn.txn(), &kind)
            .one()
            .await
            .map_err(Into::into)
            .and_then(|row| row.try_into().map_err(Into::into))
    }
}

fn decode_declaration(
    value: queries::contract::ContractDeclaration,
) -> anyhow::Result<ContractDeclaration> {
    Ok(ContractDeclaration {
        id: value.id.into(),
        kind: decode_kind(value.kind),
        received_at: value.received_at.into(),
        name: value.name.try_into()?,
        email: FromStr::from_str(&value.email)?,
        user_id: value.user_id.map(Into::into),
        contract: decode_contract(value.contract),
        cancellation_type: value.cancellation_type.map(decode_cancellation_type),
        details: value.details.try_into()?,
        requested_end: value.requested_end.map(Into::into),
        effective_end: value.effective_end.map(Into::into),
        processed_at: value.processed_at.map(Into::into),
    })
}

fn encode_kind(kind: ContractDeclarationKind) -> clorinde::types::ContractDeclarationKind {
    match kind {
        ContractDeclarationKind::Cancellation => {
            clorinde::types::ContractDeclarationKind::cancellation
        }
        ContractDeclarationKind::Withdrawal => clorinde::types::ContractDeclarationKind::withdrawal,
    }
}

fn decode_kind(value: clorinde::types::ContractDeclarationKind) -> ContractDeclarationKind {
    match value {
        clorinde::types::ContractDeclarationKind::cancellation => {
            ContractDeclarationKind::Cancellation
        }
        clorinde::types::ContractDeclarationKind::withdrawal => ContractDeclarationKind::Withdrawal,
    }
}

fn encode_contract(contract: ContractKind) -> clorinde::types::ContractDeclarationContract {
    match contract {
        ContractKind::Premium => clorinde::types::ContractDeclarationContract::premium,
        ContractKind::Coins => clorinde::types::ContractDeclarationContract::coins,
        ContractKind::Other => clorinde::types::ContractDeclarationContract::other,
    }
}

fn decode_contract(value: clorinde::types::ContractDeclarationContract) -> ContractKind {
    match value {
        clorinde::types::ContractDeclarationContract::premium => ContractKind::Premium,
        clorinde::types::ContractDeclarationContract::coins => ContractKind::Coins,
        clorinde::types::ContractDeclarationContract::other => ContractKind::Other,
    }
}

fn encode_cancellation_type(
    cancellation_type: ContractCancellationType,
) -> clorinde::types::ContractCancellationType {
    match cancellation_type {
        ContractCancellationType::Ordinary => clorinde::types::ContractCancellationType::ordinary,
        ContractCancellationType::Extraordinary => {
            clorinde::types::ContractCancellationType::extraordinary
        }
    }
}

fn decode_cancellation_type(
    value: clorinde::types::ContractCancellationType,
) -> ContractCancellationType {
    match value {
        clorinde::types::ContractCancellationType::ordinary => ContractCancellationType::Ordinary,
        clorinde::types::ContractCancellationType::extraordinary => {
            ContractCancellationType::Extraordinary
        }
    }
}
