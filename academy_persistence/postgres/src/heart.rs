use academy_di::Build;
use academy_models::{
    heart::{
        HeartOperation, HeartOperationClaim, HeartOperationOutcome, HeartOperationReceipt, Hearts,
    },
    user::UserId,
};
use academy_persistence_contracts::heart::HeartRepository;
use academy_utils::trace_instrument;
use clorinde::{
    client::Params,
    queries::{self, heart::SetParams},
};

use crate::PostgresTransaction;

#[derive(Debug, Clone, Build)]
pub struct PostgresHeartRepository;

impl HeartRepository<PostgresTransaction> for PostgresHeartRepository {
    async fn claim_operation(
        &self,
        txn: &mut PostgresTransaction,
        operation: &HeartOperation,
    ) -> anyhow::Result<HeartOperationClaim> {
        txn.txn()
            .execute(
                "SELECT pg_advisory_xact_lock(hashtextextended('heart-operation:'||$1::uuid,0))",
                &[&*operation.id],
            )
            .await?;
        let Some(row) = txn
            .txn()
            .query_opt(
                "SELECT user_id, half_hearts, reason, charged_half_hearts, hearts, outcome \
             FROM internal_heart_operations WHERE id=$1",
                &[&*operation.id],
            )
            .await?
        else {
            return Ok(HeartOperationClaim::New);
        };
        if row.get::<_, uuid::Uuid>("user_id") != *operation.user_id
            || u64::try_from(row.get::<_, i64>("half_hearts"))? != operation.half_hearts
            || row.get::<_, String>("reason") != operation.reason
        {
            return Ok(HeartOperationClaim::Conflict);
        }
        let outcome = match row.get::<_, &str>("outcome") {
            "charged" => HeartOperationOutcome::Charged,
            "premium" => HeartOperationOutcome::Premium,
            "insufficient" => HeartOperationOutcome::Insufficient,
            _ => anyhow::bail!("Invalid stored heart operation outcome"),
        };
        Ok(HeartOperationClaim::Completed(HeartOperationReceipt {
            operation_id: operation.id,
            user_id: operation.user_id,
            charged_half_hearts: row.get::<_, i64>("charged_half_hearts").try_into()?,
            hearts: row.get::<_, i64>("hearts").try_into()?,
            outcome,
        }))
    }

    async fn lock_user(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<bool> {
        Ok(txn
            .txn()
            .query_opt("SELECT id FROM users WHERE id=$1 FOR UPDATE", &[&*user_id])
            .await?
            .is_some())
    }

    async fn complete_operation(
        &self,
        txn: &mut PostgresTransaction,
        operation: &HeartOperation,
        receipt: HeartOperationReceipt,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            receipt.operation_id == operation.id && receipt.user_id == operation.user_id,
            "Heart operation receipt identity mismatch"
        );
        let outcome = match receipt.outcome {
            HeartOperationOutcome::Charged => "charged",
            HeartOperationOutcome::Premium => "premium",
            HeartOperationOutcome::Insufficient => "insufficient",
        };
        txn.txn()
            .execute(
                "INSERT INTO internal_heart_operations \
             (id,user_id,half_hearts,reason,charged_half_hearts,hearts,outcome) \
             VALUES ($1,$2,$3,$4,$5,$6,$7)",
                &[
                    &*operation.id,
                    &*operation.user_id,
                    &i64::try_from(operation.half_hearts)?,
                    &operation.reason,
                    &i64::try_from(receipt.charged_half_hearts)?,
                    &i64::try_from(receipt.hearts)?,
                    &outcome,
                ],
            )
            .await?;
        Ok(())
    }

    async fn export_operations(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<String> {
        Ok(txn
            .txn()
            .query_one(
                "SELECT coalesce(jsonb_agg(to_jsonb(o) ORDER BY created_at,id),'[]'::jsonb)::text \
             FROM internal_heart_operations o WHERE user_id=$1",
                &[&*user_id],
            )
            .await?
            .get(0))
    }

    #[trace_instrument(skip(self, txn))]
    async fn get(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Option<Hearts>> {
        // All paid refills and consumption share this lock, including an absent
        // hearts row. The caller applies one effective refill-time decision.
        txn.txn()
            .query_opt("SELECT id FROM users WHERE id=$1 FOR UPDATE", &[&*user_id])
            .await?;
        queries::heart::get()
            .bind(txn.txn(), &user_id)
            .opt()
            .await
            .map_err(Into::into)
            .and_then(|row| {
                row.map(|row| {
                    Ok(Hearts {
                        hearts: row.hearts.try_into()?,
                        last_refill: row.last_refill.into(),
                    })
                })
                .transpose()
            })
    }

    #[trace_instrument(skip(self, txn))]
    async fn set(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
        hearts: Hearts,
    ) -> anyhow::Result<()> {
        let params = SetParams {
            user_id: *user_id,
            hearts: hearts.hearts.try_into()?,
            last_refill: hearts.last_refill.into(),
        };

        queries::heart::set()
            .params(txn.txn(), &params)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}
