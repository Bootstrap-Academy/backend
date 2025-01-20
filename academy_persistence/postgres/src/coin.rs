use std::ops::Range;

use academy_di::Build;
use academy_models::{
    coin::{Balance, Transaction},
    user::UserId,
};
use academy_persistence_contracts::coin::{CoinRepoAddCoinsError, CoinRepository};
use academy_utils::trace_instrument;
use bb8_postgres::tokio_postgres;
use chrono::{DateTime, Utc};
use clorinde::{
    client::Params,
    queries::{
        self,
        coin::{AddCoinsParams, CreateTransactionParams, ListTransactionsParams},
    },
};
use futures::{StreamExt, TryStreamExt};

use crate::PostgresTransaction;

#[derive(Debug, Clone, Build)]
pub struct PostgresCoinRepository;

impl CoinRepository<PostgresTransaction> for PostgresCoinRepository {
    #[trace_instrument(skip(self, txn))]
    async fn get_balance(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Balance> {
        queries::coin::get_balance()
            .bind(txn.txn(), &user_id)
            .opt()
            .await
            .map_err(Into::into)
            .and_then(|row| row.map(decode_balance).unwrap_or(Ok(Balance::default())))
    }

    #[trace_instrument(skip(self, txn))]
    async fn add_coins(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
        coins: i64,
        withhold: bool,
    ) -> Result<Balance, CoinRepoAddCoinsError> {
        let (coins, withheld_coins) = if withhold { (0, coins) } else { (coins, 0) };

        let params = AddCoinsParams {
            user_id: *user_id,
            coins,
            withheld_coins,
        };

        txn.savepoint(|txn| async {
            queries::coin::add_coins()
                .params(txn, &params)
                .one()
                .await
                .map_err(map_add_coins_error)
                .and_then(|row| decode_balance(row).map_err(Into::into))
        })
        .await
    }

    #[trace_instrument(skip(self, txn))]
    async fn release_coins(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<()> {
        queries::coin::release_coins()
            .bind(txn.txn(), &user_id)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn get_transactions(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
        datetime_range: Range<DateTime<Utc>>,
    ) -> anyhow::Result<Vec<Transaction>> {
        let params = ListTransactionsParams {
            user_id: *user_id,
            start: datetime_range.start.into(),
            end: datetime_range.end.into(),
        };

        queries::coin::list_transactions()
            .params(txn.txn(), &params)
            .iter()
            .await?
            .map(|row| row.map_err(Into::into).and_then(decode_transaction))
            .try_collect()
            .await
    }

    #[trace_instrument(skip(self, txn))]
    async fn create_transaction(
        &self,
        txn: &mut PostgresTransaction,
        transaction: &Transaction,
    ) -> anyhow::Result<()> {
        let params = CreateTransactionParams {
            id: *transaction.id,
            user_id: *transaction.user_id,
            created_at: transaction.created_at.into(),
            coins: transaction.coins,
            description: transaction.description.as_deref(),
            include_in_credit_note: transaction.include_in_credit_note,
        };

        queries::coin::create_transaction()
            .params(txn.txn(), &params)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}

fn decode_balance(value: queries::coin::Balance) -> anyhow::Result<Balance> {
    Ok(Balance {
        coins: value.coins.try_into()?,
        withheld_coins: value.withheld_coins.try_into()?,
    })
}

fn decode_transaction(value: queries::coin::Transaction) -> anyhow::Result<Transaction> {
    Ok(Transaction {
        id: value.id.into(),
        user_id: value.user_id.into(),
        created_at: value.created_at.into(),
        coins: value.coins,
        description: value.description.map(TryInto::try_into).transpose()?,
        include_in_credit_note: value.include_in_credit_note,
    })
}

fn map_add_coins_error(err: tokio_postgres::Error) -> CoinRepoAddCoinsError {
    match err.as_db_error() {
        Some(err) if err.constraint() == Some("coins_coins_check") => {
            CoinRepoAddCoinsError::NotEnoughCoins
        }
        Some(err) if err.constraint() == Some("coins_withheld_coins_check") => {
            CoinRepoAddCoinsError::NotEnoughCoins
        }
        _ => CoinRepoAddCoinsError::Other(err.into()),
    }
}
