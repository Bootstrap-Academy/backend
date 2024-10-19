use academy_di::Build;
use academy_models::{coin::Balance, user::UserId};
use academy_persistence_contracts::coin::CoinRepository;
use academy_utils::trace_instrument;
use bb8_postgres::tokio_postgres::Row;

use crate::{arg_indices, columns, ColumnCounter, PostgresTransaction};

#[derive(Debug, Clone, Build)]
pub struct PostgresCoinRepository;

columns!(coin as "c": "user_id", "coins", "withheld_coins");

impl CoinRepository<PostgresTransaction> for PostgresCoinRepository {
    #[trace_instrument(skip(self, txn))]
    async fn get_balance(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Option<Balance>> {
        txn.txn()
            .query_opt(
                &format!("select {COIN_COLS} from coins c where user_id=$1"),
                &[&*user_id],
            )
            .await
            .map_err(Into::into)
            .and_then(|row| {
                row.map(|row| decode_balance(&row, &mut Default::default()))
                    .transpose()
            })
    }

    #[trace_instrument(skip(self, txn))]
    async fn save_balance(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
        balance: Balance,
    ) -> anyhow::Result<()> {
        let coins = i64::try_from(balance.coins)?;
        let withheld_coins = i64::try_from(balance.withheld_coins)?;

        txn.txn()
            .execute(
                &format!(
                    "insert into coins ({COIN_COL_NAMES}) values ({}) on conflict (user_id) do \
                     update set coins=$2, withheld_coins=$3",
                    arg_indices(1..=COIN_CNT)
                ),
                &[&*user_id, &coins, &withheld_coins],
            )
            .await
            .map_err(Into::into)
            .map(|_| ())
    }
}

fn decode_balance(row: &Row, cnt: &mut ColumnCounter) -> anyhow::Result<Balance> {
    cnt.idx(); // user_id
    Ok(Balance {
        coins: row.get::<_, i64>(cnt.idx()).try_into()?,
        withheld_coins: row.get::<_, i64>(cnt.idx()).try_into()?,
    })
}
