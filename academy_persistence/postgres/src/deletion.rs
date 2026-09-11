//! Durable per-service work, created by the account DELETE transaction.
use academy_models::user::UserId;
use chrono::{DateTime, Utc};

use crate::PostgresTransaction;

#[derive(Debug)]
pub struct DeletionWork {
    pub user_id: UserId,
    pub service: String,
    pub requested_at: DateTime<Utc>,
    pub attempts: i64,
}

/// Reserve one attempt with SKIP LOCKED. Commit this transaction BEFORE delivery:
/// its 60-second lease/scheduling must survive acknowledgement rollback or a crash.
/// `attempts` is also the generation fencing late acknowledgements after takeover.
pub async fn claim(
    txn: &mut PostgresTransaction,
    attempted: &[String],
) -> anyhow::Result<Option<DeletionWork>> {
    Ok(txn
        .txn()
        .query_opt(
            "WITH due AS (SELECT user_id, service FROM user_deletion_work
         WHERE next_attempt_at <= now() AND NOT (user_id::text || ':' || service = ANY($1::text[])) ORDER BY next_attempt_at, user_id, service
         LIMIT 1 FOR UPDATE SKIP LOCKED)
         UPDATE user_deletion_work AS work SET attempts=work.attempts+1,
             next_attempt_at=now()+interval '60 seconds', last_error='UnacknowledgedAttempt'
         FROM due WHERE work.user_id=due.user_id AND work.service=due.service
         RETURNING work.user_id, work.service, work.requested_at, work.attempts",
            &[&attempted],
        )
        .await?
        .map(|row| DeletionWork {
            user_id: row.get::<_, uuid::Uuid>(0).into(),
            service: row.get(1),
            requested_at: row.get(2),
            attempts: row.get(3),
        }))
}

pub async fn acknowledge(
    txn: &mut PostgresTransaction,
    work: &DeletionWork,
    success: bool,
) -> anyhow::Result<bool> {
    let affected = if success {
        // Completion needs no second personal-data archive. Financial/contract evidence
        // has its own lifecycle and is not touched by this queue.
        txn.txn()
            .execute(
                "DELETE FROM user_deletion_work WHERE user_id=$1 AND service=$2 AND attempts=$3",
                &[&*work.user_id, &work.service, &work.attempts],
            )
            .await?
    } else {
        txn.txn()
            .execute(
                "UPDATE user_deletion_work SET
            next_attempt_at=now()+interval '60 seconds', last_error='DeliveryFailed'
            WHERE user_id=$1 AND service=$2 AND attempts=$3",
                &[&*work.user_id, &work.service, &work.attempts],
            )
            .await?
    };
    Ok(affected == 1)
}

pub async fn backlog(
    txn: &mut PostgresTransaction,
) -> anyhow::Result<Vec<(String, i64, DateTime<Utc>)>> {
    Ok(txn.txn().query("SELECT service, count(*), min(requested_at) FROM user_deletion_work GROUP BY service ORDER BY service", &[])
        .await?.into_iter().map(|row| (row.get(0), row.get(1), row.get(2))).collect())
}
