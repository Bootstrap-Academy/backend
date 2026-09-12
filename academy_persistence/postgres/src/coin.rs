use std::ops::Range;

use academy_di::Build;
use academy_models::{
    coin::{Balance, CoinOperation, CoinOperationClaim, CoinOperationId, Transaction},
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
    async fn claim_operation(
        &self,
        txn: &mut PostgresTransaction,
        operation: &CoinOperation,
    ) -> anyhow::Result<CoinOperationClaim> {
        txn.txn()
            .execute(
                "SELECT pg_advisory_xact_lock(hashtextextended('coin-operation:'||$1::uuid,0))",
                &[&*operation.id],
            )
            .await?;
        // Exact completed legacy receipts are checked first and remain replayable
        // after erasure. A newly claimed disposition must not be paid by an old
        // wallet-only caller after the compatible worker activation boundary.
        if txn.txn().query_opt("SELECT 1 FROM commercial_operation_aliases WHERE operation_id=$1", &[&*operation.id]).await?.is_some()
            && txn.txn().query_opt("SELECT 1 FROM internal_coin_operations WHERE id=$1 AND completed_at IS NOT NULL", &[&*operation.id]).await?.is_none() {
            return Ok(CoinOperationClaim::Conflict);
        }
        // New acquisitions enter through the actual purchase service. Positive
        // generic requests can only complete a reviewed, exact reservation that
        // was imported from an already-earned outbox during the held cutover.
        // Existing completed receipts still replay without current user access.
        let inserted = if operation.coins <= 0 {
            txn.txn().execute(
            "INSERT INTO internal_coin_operations (id, user_id, coins, description, credit_note) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (id) DO NOTHING",
            &[&*operation.id, &*operation.user_id, &operation.coins, &operation.description.as_deref(), &operation.include_in_credit_note],
        ).await?
        } else {
            0
        };
        if inserted == 1 {
            return Ok(CoinOperationClaim::New);
        }
        let Some(row) = txn.txn().query_opt(
            "SELECT user_id = $2 AND coins = $3 AND description IS NOT DISTINCT FROM $4 AND credit_note = $5 AS matches, balance, withheld_balance, completed_at IS NOT NULL AS completed FROM internal_coin_operations WHERE id = $1",
            &[&*operation.id, &*operation.user_id, &operation.coins, &operation.description.as_deref(), &operation.include_in_credit_note],
        ).await? else { return Ok(CoinOperationClaim::CreditNotAuthorized); };
        if !row.get::<_, bool>("matches") {
            return Ok(CoinOperationClaim::Conflict);
        }
        if !row.get::<_, bool>("completed") {
            return Ok(CoinOperationClaim::New);
        }
        Ok(CoinOperationClaim::Completed(Balance {
            coins: row.try_get::<_, i64>("balance")?.try_into()?,
            withheld_coins: row.try_get::<_, i64>("withheld_balance")?.try_into()?,
        }))
    }

    async fn complete_operation(
        &self,
        txn: &mut PostgresTransaction,
        id: CoinOperationId,
        balance: Balance,
    ) -> anyhow::Result<()> {
        let coins = i64::try_from(balance.coins)?;
        let withheld = i64::try_from(balance.withheld_coins)?;
        anyhow::ensure!(txn.txn().execute(
            "UPDATE internal_coin_operations SET balance=$2, withheld_balance=$3, completed_at=now() WHERE id=$1 AND completed_at IS NULL",
            &[&*id, &coins, &withheld],
        ).await? == 1, "Coin operation was not reserved");
        Ok(())
    }

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
        // Existing-row MERGE does not acquire the FK parent first. Serialize all
        // ordinary wallet writers with erasure and commercial disposition before
        // taking the coin row, without changing T6's operation-first replay.
        txn.txn()
            .query_opt("SELECT id FROM users WHERE id=$1 FOR UPDATE", &[&*user_id])
            .await
            .map_err(anyhow::Error::from)?;
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
        txn.txn()
            .query_opt("SELECT id FROM users WHERE id=$1 FOR UPDATE", &[&*user_id])
            .await?;
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
    async fn get_all_transactions(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Vec<Transaction>> {
        queries::coin::list_all_transactions()
            .bind(txn.txn(), &user_id)
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
