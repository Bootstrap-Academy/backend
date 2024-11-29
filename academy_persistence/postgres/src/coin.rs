use std::ops::Range;

use academy_di::Build;
use academy_models::{
    coin::{Balance, Transaction},
    user::UserId,
};
use academy_persistence_contracts::coin::{CoinRepoAddCoinsError, CoinRepository};
use academy_utils::trace_instrument;
use bb8_postgres::tokio_postgres::{self, Row};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{arg_indices, columns, ColumnCounter, PostgresTransaction};

columns!(transaction as "t": "id", "user_id", "created_at", "coins", "description", "include_in_credit_note");

#[derive(Debug, Clone, Build)]
pub struct PostgresCoinRepository;

impl CoinRepository<PostgresTransaction> for PostgresCoinRepository {
    #[trace_instrument(skip(self, txn))]
    async fn get_balance(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Balance> {
        txn.txn()
            .query_opt(
                "select coins, withheld_coins from coins c where user_id=$1",
                &[&*user_id],
            )
            .await
            .map_err(Into::into)
            .and_then(|row| {
                row.map(|row| decode_balance(&row, &mut Default::default()))
                    .unwrap_or(Ok(Balance::default()))
            })
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

        if let Some(row) = txn
            .txn()
            .query_opt(
                "update coins set coins=coins+$2, withheld_coins=withheld_coins+$3 where \
                 user_id=$1 returning coins, withheld_coins",
                &[&*user_id, &coins, &withheld_coins],
            )
            .await
            .map_err(map_add_coins_error)?
        {
            return decode_balance(&row, &mut Default::default()).map_err(Into::into);
        }

        if coins < 0 || withheld_coins < 0 {
            return Err(CoinRepoAddCoinsError::NotEnoughCoins);
        }

        txn.txn()
            .execute(
                "insert into coins as c (user_id, coins, withheld_coins) values ($1, $2, $3)",
                &[&*user_id, &coins, &withheld_coins],
            )
            .await
            .map_err(map_add_coins_error)?;

        Ok(Balance {
            coins: coins as _,
            withheld_coins: withheld_coins as _,
        })
    }

    #[trace_instrument(skip(self, txn))]
    async fn release_coins(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<()> {
        txn.txn()
            .execute(
                "update coins set coins=coins+withheld_coins, withheld_coins=0 where user_id=$1",
                &[&*user_id],
            )
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    async fn get_transactions(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
        datetime_range: Range<DateTime<Utc>>,
    ) -> anyhow::Result<Vec<Transaction>> {
        txn.txn()
            .query(
                &format!(
                    "select {TRANSACTION_COLS} from transactions t where user_id=$1 and $2 <= \
                     created_at and created_at < $3 order by created_at asc"
                ),
                &[&*user_id, &datetime_range.start, &datetime_range.end],
            )
            .await
            .map_err(Into::into)
            .and_then(|rows| {
                rows.into_iter()
                    .map(|row| decode_transaction(&row, &mut Default::default()))
                    .collect()
            })
    }

    async fn create_transaction(
        &self,
        txn: &mut PostgresTransaction,
        transaction: &Transaction,
    ) -> anyhow::Result<()> {
        txn.txn()
            .execute(
                &format!(
                    "insert into transactions ({TRANSACTION_COL_NAMES}) values ({})",
                    arg_indices(1..=TRANSACTION_CNT)
                ),
                &[
                    &*transaction.id,
                    &*transaction.user_id,
                    &transaction.created_at,
                    &transaction.coins,
                    &transaction.description.as_deref(),
                    &transaction.include_in_credit_note,
                ],
            )
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}

fn decode_balance(row: &Row, cnt: &mut ColumnCounter) -> anyhow::Result<Balance> {
    Ok(Balance {
        coins: row.get::<_, i64>(cnt.idx()).try_into()?,
        withheld_coins: row.get::<_, i64>(cnt.idx()).try_into()?,
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

fn decode_transaction(row: &Row, cnt: &mut ColumnCounter) -> anyhow::Result<Transaction> {
    Ok(Transaction {
        id: row.get::<_, Uuid>(cnt.idx()).into(),
        user_id: row.get::<_, Uuid>(cnt.idx()).into(),
        created_at: row.get(cnt.idx()),
        coins: row.get(cnt.idx()),
        description: row
            .get::<_, Option<String>>(cnt.idx())
            .map(TryInto::try_into)
            .transpose()?,
        include_in_credit_note: row.get(cnt.idx()),
    })
}
