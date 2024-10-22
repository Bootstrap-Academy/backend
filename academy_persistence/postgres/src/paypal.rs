use academy_di::Build;
use academy_models::paypal::{PaypalCoinOrder, PaypalOrderId};
use academy_persistence_contracts::paypal::PaypalRepository;
use bb8_postgres::tokio_postgres::Row;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{arg_indices, columns, ColumnCounter, PostgresTransaction};

#[derive(Debug, Clone, Build)]
pub struct PostgresPaypalRepository;

columns!(paypal_coin_order as "pco": "id", "user_id", "created_at", "captured_at", "coins", "invoice_number");

impl PaypalRepository<PostgresTransaction> for PostgresPaypalRepository {
    async fn create_coin_order(
        &self,
        txn: &mut PostgresTransaction,
        order: &PaypalCoinOrder,
    ) -> anyhow::Result<()> {
        txn.txn()
            .execute(
                &format!(
                    "insert into paypal_coin_orders ({}) values ({})",
                    PAYPAL_COIN_ORDER_COL_NAMES,
                    arg_indices(1..=PAYPAL_COIN_ORDER_CNT)
                ),
                &[
                    &*order.id,
                    &*order.user_id,
                    &order.created_at,
                    &order.captured_at,
                    &i64::try_from(order.coins)?,
                    &i64::try_from(order.invoice_number)?,
                ],
            )
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    async fn get_coin_order(
        &self,
        txn: &mut PostgresTransaction,
        order_id: &PaypalOrderId,
    ) -> anyhow::Result<Option<PaypalCoinOrder>> {
        txn.txn()
            .query_opt(
                &format!("select {PAYPAL_COIN_ORDER_COLS} from paypal_coin_orders pco where id=$1"),
                &[&**order_id],
            )
            .await
            .map_err(Into::into)
            .and_then(|row| {
                row.map(|row| decode_paypal_coin_order(&row, &mut Default::default()))
                    .transpose()
            })
    }

    async fn capture_coin_order(
        &self,
        txn: &mut PostgresTransaction,
        order_id: &PaypalOrderId,
        captured_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        txn.txn()
            .execute(
                "update paypal_coin_orders set captured_at=$2 where id=$1",
                &[&**order_id, &captured_at],
            )
            .await
            .map_err(Into::into)
            .map(|_| ())
    }

    async fn get_next_invoice_number(&self, txn: &mut PostgresTransaction) -> anyhow::Result<u64> {
        txn.txn()
            .query_one(
                "select coalesce(max(invoice_number), 0) + 1 from paypal_coin_orders",
                &[],
            )
            .await
            .map_err(Into::into)
            .and_then(|row| row.get::<_, i64>(0).try_into().map_err(Into::into))
    }
}

fn decode_paypal_coin_order(row: &Row, cnt: &mut ColumnCounter) -> anyhow::Result<PaypalCoinOrder> {
    Ok(PaypalCoinOrder {
        id: row.get::<_, String>(cnt.idx()).try_into()?,
        user_id: row.get::<_, Uuid>(cnt.idx()).into(),
        created_at: row.get(cnt.idx()),
        captured_at: row.get(cnt.idx()),
        coins: row.get::<_, i64>(cnt.idx()).try_into()?,
        invoice_number: row.get::<_, i64>(cnt.idx()).try_into()?,
    })
}
