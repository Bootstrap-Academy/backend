use academy_di::Build;
use academy_models::{
    coin::Balance,
    paypal::{PaypalCapture, PaypalCoinOrder, PaypalOrderId, PaypalPayment, PaypalPaymentSnapshot},
    user::UserId,
};
use academy_persistence_contracts::paypal::PaypalRepository;
use chrono::{DateTime, Utc};
use clorinde::{
    client::Params,
    queries::{
        self,
        paypal::{CaptureCoinOrderParams, CreateCoinOrderParams},
    },
};
use futures::{Stream, StreamExt, TryFutureExt, TryStreamExt};

use crate::PostgresTransaction;

#[derive(Debug, Clone, Build)]
pub struct PostgresPaypalRepository;

impl PaypalRepository<PostgresTransaction> for PostgresPaypalRepository {
    async fn lock_contract_order(
        &self,
        txn: &mut PostgresTransaction,
        id: uuid::Uuid,
    ) -> anyhow::Result<Option<PaypalOrderId>> {
        txn.txn()
            .query_one(
                "SELECT order_id FROM purchase_progress WHERE order_id=$1 FOR UPDATE",
                &[&id],
            )
            .await?;
        txn.txn()
            .query_opt(
                "SELECT paypal_order_id FROM paypal_contract_orders WHERE contract_order_id=$1",
                &[&id],
            )
            .await?
            .map(|r| PaypalOrderId::try_new(r.get::<_, String>(0)).map_err(Into::into))
            .transpose()
    }

    async fn create_payment(
        &self,
        txn: &mut PostgresTransaction,
        payment: &PaypalPayment,
    ) -> anyhow::Result<()> {
        let snapshot = &payment.snapshot;
        let invoice = i64::try_from(snapshot.order.invoice_number)?;
        txn.txn().execute(
            "INSERT INTO paypal_payments (order_id,invoice_number,user_id,request_id,snapshot) VALUES ($1,$2,$3,$4,$5)",
            &[&*snapshot.order.id, &invoice, &*snapshot.order.user_id, &snapshot.request_id, &serde_json::to_string(snapshot)?],
        ).await?;
        if let Some(id) = snapshot.contract_order_id {
            txn.txn()
                .execute(
                    "INSERT INTO paypal_contract_orders VALUES($1,$2)",
                    &[&id, &snapshot.order.id.as_str()],
                )
                .await?;
        }
        Ok(())
    }

    async fn get_payment(
        &self,
        txn: &mut PostgresTransaction,
        id: &PaypalOrderId,
    ) -> anyhow::Result<Option<PaypalPayment>> {
        txn.txn()
            .query_opt(
                "SELECT * FROM paypal_payments WHERE order_id=$1 FOR UPDATE",
                &[&**id],
            )
            .await?
            .map(decode_payment)
            .transpose()
    }

    async fn get_payment_by_invoice(
        &self,
        txn: &mut PostgresTransaction,
        invoice: u64,
    ) -> anyhow::Result<Option<PaypalPayment>> {
        txn.txn()
            .query_opt(
                "SELECT * FROM paypal_payments WHERE invoice_number=$1 FOR UPDATE",
                &[&i64::try_from(invoice)?],
            )
            .await?
            .map(decode_payment)
            .transpose()
    }

    async fn update_payment(
        &self,
        txn: &mut PostgresTransaction,
        payment: &PaypalPayment,
    ) -> anyhow::Result<()> {
        let capture = payment
            .capture
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let balance = payment
            .balance
            .map(|b| i64::try_from(b.coins))
            .transpose()?;
        let withheld = payment
            .balance
            .map(|b| i64::try_from(b.withheld_coins))
            .transpose()?;
        anyhow::ensure!(txn.txn().execute(
            "UPDATE paypal_payments SET started_at=$2,attempts=$3,capture_id=$4,capture=$5,balance=$6,withheld_balance=$7,fulfilled_at=$8,receipt_sent_at=$9,receipt_attempts=$10,last_error=$11 WHERE order_id=$1",
            &[&*payment.snapshot.order.id, &payment.started_at, &payment.attempts, &payment.capture.as_ref().map(|c| c.id.as_str()), &capture, &balance, &withheld, &payment.fulfilled_at, &payment.receipt_sent_at, &payment.receipt_attempts, &payment.last_error],
        ).await? == 1, "Payment disappeared");
        Ok(())
    }

    async fn pending_payments(
        &self,
        txn: &mut PostgresTransaction,
    ) -> anyhow::Result<Vec<PaypalOrderId>> {
        txn.txn().query("SELECT order_id FROM paypal_payments WHERE started_at IS NOT NULL AND receipt_sent_at IS NULL ORDER BY started_at,order_id", &[]).await?
            .into_iter().map(|row| row.get::<_, String>(0).try_into().map_err(Into::into)).collect()
    }

    async fn create_coin_order(
        &self,
        txn: &mut PostgresTransaction,
        order: &PaypalCoinOrder,
    ) -> anyhow::Result<()> {
        let params = CreateCoinOrderParams {
            id: &*order.id,
            user_id: *order.user_id,
            created_at: order.created_at.into(),
            captured_at: order.captured_at.map(Into::into),
            coins: order.coins.try_into()?,
            invoice_number: order.invoice_number.try_into()?,
            withdrawal_consent_at: order.withdrawal_consent_at.map(Into::into),
            withdrawal_text_version: order.withdrawal_text_version.as_deref(),
        };

        queries::paypal::create_coin_order()
            .params(txn.txn(), &params)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    async fn count_coin_orders(&self, txn: &mut PostgresTransaction) -> anyhow::Result<u64> {
        queries::paypal::count_coin_orders()
            .bind(txn.txn())
            .one()
            .await
            .map_err(Into::into)
            .and_then(|cnt| cnt.try_into().map_err(Into::into))
    }

    fn stream_coin_orders(
        &self,
        txn: &mut PostgresTransaction,
    ) -> impl Stream<Item = anyhow::Result<PaypalCoinOrder>> {
        async {
            queries::paypal::list_coin_orders()
                .bind(txn.txn())
                .iter()
                .await
                .map_err(Into::into)
                .map(|s| s.map(|row| row.map_err(Into::into).and_then(decode_paypal_coin_order)))
        }
        .try_flatten_stream()
    }

    async fn list_coin_orders_by_user_id(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Vec<PaypalCoinOrder>> {
        queries::paypal::list_coin_orders_by_user_id()
            .bind(txn.txn(), &user_id)
            .iter()
            .await?
            .map(|row| row.map_err(Into::into).and_then(decode_paypal_coin_order))
            .try_collect()
            .await
    }

    async fn get_coin_order(
        &self,
        txn: &mut PostgresTransaction,
        order_id: &PaypalOrderId,
    ) -> anyhow::Result<Option<PaypalCoinOrder>> {
        queries::paypal::get_coin_order()
            .bind(txn.txn(), &**order_id)
            .opt()
            .await
            .map_err(Into::into)
            .and_then(|row| row.map(decode_paypal_coin_order).transpose())
    }

    async fn get_coin_order_by_invoice_number(
        &self,
        txn: &mut PostgresTransaction,
        invoice_number: u64,
    ) -> anyhow::Result<Option<PaypalCoinOrder>> {
        queries::paypal::get_coin_order_by_invoice_number()
            .bind(txn.txn(), &invoice_number.try_into()?)
            .opt()
            .await
            .map_err(Into::into)
            .and_then(|row| row.map(decode_paypal_coin_order).transpose())
    }

    async fn capture_coin_order(
        &self,
        txn: &mut PostgresTransaction,
        order_id: &PaypalOrderId,
        captured_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let params = CaptureCoinOrderParams {
            id: &**order_id,
            captured_at: captured_at.into(),
        };

        queries::paypal::capture_coin_order()
            .params(txn.txn(), &params)
            .await
            .map_err(Into::into)
            .map(|_| ())
    }

    async fn get_next_invoice_number(&self, txn: &mut PostgresTransaction) -> anyhow::Result<u64> {
        queries::paypal::get_next_invoice_number()
            .bind(txn.txn())
            .one()
            .await
            .map_err(Into::into)
            .and_then(|row| row.try_into().map_err(Into::into))
    }
}

fn decode_paypal_coin_order(value: queries::paypal::CoinOrder) -> anyhow::Result<PaypalCoinOrder> {
    Ok(PaypalCoinOrder {
        id: value.id.try_into()?,
        user_id: value.user_id.into(),
        created_at: value.created_at.into(),
        captured_at: value.captured_at.map(Into::into),
        coins: value.coins.try_into()?,
        invoice_number: value.invoice_number.try_into()?,
        withdrawal_consent_at: value.withdrawal_consent_at.map(Into::into),
        withdrawal_text_version: value
            .withdrawal_text_version
            .map(TryInto::try_into)
            .transpose()?,
    })
}

fn decode_payment(row: bb8_postgres::tokio_postgres::Row) -> anyhow::Result<PaypalPayment> {
    Ok(PaypalPayment {
        snapshot: serde_json::from_str::<PaypalPaymentSnapshot>(&row.get::<_, String>("snapshot"))?,
        started_at: row.get("started_at"),
        attempts: row.get("attempts"),
        capture: row
            .get::<_, Option<String>>("capture")
            .map(|s| serde_json::from_str::<PaypalCapture>(&s))
            .transpose()?,
        balance: row
            .get::<_, Option<i64>>("balance")
            .map(|coins| -> anyhow::Result<Balance> {
                Ok(Balance {
                    coins: coins.try_into()?,
                    withheld_coins: row.get::<_, i64>("withheld_balance").try_into()?,
                })
            })
            .transpose()?,
        fulfilled_at: row.get("fulfilled_at"),
        receipt_sent_at: row.get("receipt_sent_at"),
        receipt_attempts: row.get("receipt_attempts"),
        last_error: row.get("last_error"),
    })
}
