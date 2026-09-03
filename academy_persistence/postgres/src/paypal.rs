use academy_di::Build;
use academy_models::paypal::{PaypalCoinOrder, PaypalOrderId};
use academy_persistence_contracts::paypal::PaypalRepository;
use chrono::{DateTime, Utc};
use clorinde::{
    client::Params,
    queries::{
        self,
        paypal::{CaptureCoinOrderParams, CreateCoinOrderParams},
    },
};
use futures::{Stream, StreamExt, TryFutureExt};

use crate::PostgresTransaction;

#[derive(Debug, Clone, Build)]
pub struct PostgresPaypalRepository;

impl PaypalRepository<PostgresTransaction> for PostgresPaypalRepository {
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
