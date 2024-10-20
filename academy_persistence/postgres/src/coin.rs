use academy_di::Build;
use academy_models::{coin::Balance, user::UserId};
use academy_persistence_contracts::coin::{CoinRepoAddCoinsError, CoinRepository};
use academy_utils::trace_instrument;
use bb8_postgres::tokio_postgres::{self, Row};

use crate::{ColumnCounter, PostgresTransaction};

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
