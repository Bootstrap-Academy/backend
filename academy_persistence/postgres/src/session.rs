use std::convert::TryFrom;

use academy_di::Build;
use academy_models::{
    session::{ActiveUsersBucket, Session, SessionId, SessionPatchRef, SessionRefreshTokenHash},
    user::UserId,
};
use academy_persistence_contracts::session::SessionRepository;
use academy_utils::trace_instrument;
use anyhow::anyhow;
use chrono::{DateTime, Duration, Utc};
use clorinde::{
    client::Params,
    queries::{
        self,
        session::{CreateParams, UpdateParams},
    },
};
use futures::{StreamExt, TryStreamExt};

use crate::{PostgresTransaction, decode_sha256hash};

#[derive(Debug, Clone, Build)]
pub struct PostgresSessionRepository;

impl SessionRepository<PostgresTransaction> for PostgresSessionRepository {
    #[trace_instrument(skip(self, txn))]
    async fn get(
        &self,
        txn: &mut PostgresTransaction,
        session_id: SessionId,
    ) -> anyhow::Result<Option<Session>> {
        queries::session::get()
            .bind(txn.txn(), &session_id)
            .opt()
            .await
            .map_err(Into::into)
            .and_then(|row| row.map(decode_session).transpose())
    }

    #[trace_instrument(skip(self, txn))]
    async fn get_by_refresh_token_hash(
        &self,
        txn: &mut PostgresTransaction,
        refresh_token_hash: SessionRefreshTokenHash,
    ) -> anyhow::Result<Option<Session>> {
        queries::session::get_by_refresh_token_hash()
            .bind(txn.txn(), &refresh_token_hash.as_slice())
            .opt()
            .await
            .map_err(Into::into)
            .and_then(|row| row.map(decode_session).transpose())
    }

    #[trace_instrument(skip(self, txn))]
    async fn list_by_user(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Vec<Session>> {
        queries::session::list_by_user()
            .bind(txn.txn(), &user_id)
            .iter()
            .await?
            .map(|row| row.map_err(Into::into).and_then(decode_session))
            .try_collect()
            .await
    }

    #[trace_instrument(skip(self, txn))]
    async fn create(&self, txn: &mut PostgresTransaction, session: &Session) -> anyhow::Result<()> {
        let params = CreateParams {
            id: *session.id,
            user_id: *session.user_id,
            device_name: session.device_name.as_deref(),
            created_at: session.created_at.into(),
            updated_at: session.updated_at.into(),
        };

        queries::session::create()
            .params(txn.txn(), &params)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn update(
        &self,
        txn: &mut PostgresTransaction,
        session_id: SessionId,
        SessionPatchRef {
            device_name,
            updated_at,
        }: SessionPatchRef<'_>,
    ) -> anyhow::Result<bool> {
        let params = UpdateParams {
            id: *session_id,
            clear_device_name: device_name.is_update_and(|x| x.is_none()),
            device_name: device_name.update().and_then(Option::as_deref),
            updated_at: updated_at.update().copied().map(Into::into),
        };

        queries::session::update()
            .params(txn.txn(), &params)
            .await
            .map(|n| n != 0)
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn delete(
        &self,
        txn: &mut PostgresTransaction,
        session_id: SessionId,
    ) -> anyhow::Result<bool> {
        queries::session::delete()
            .bind(txn.txn(), &session_id)
            .await
            .map(|n| n != 0)
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn delete_by_user(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<()> {
        queries::session::delete_by_user()
            .bind(txn.txn(), &user_id)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn active_users(
        &self,
        txn: &mut PostgresTransaction,
        start: DateTime<Utc>,
        bucket: Duration,
        bucket_count: i64,
    ) -> anyhow::Result<Vec<ActiveUsersBucket>> {
        let bucket_seconds = bucket.num_seconds();
        if bucket_seconds <= 0 {
            return Err(anyhow!("Bucket duration must be positive"));
        }
        let rows = txn
            .txn()
            .query(
                r#"
WITH series AS (
    SELECT ($1::timestamptz + (($3::bigint || ' seconds')::interval * idx)) AS bucket_start
    FROM generate_series(0::bigint, ($2::bigint) - 1) AS gs(idx)
)
SELECT
    bucket_start,
    COUNT(DISTINCT s.user_id) AS user_count
FROM series
LEFT JOIN sessions s
    ON s.updated_at >= bucket_start
   AND s.updated_at < bucket_start + ($3::bigint || ' seconds')::interval
GROUP BY bucket_start
ORDER BY bucket_start
"#,
                &[&start, &bucket_count, &bucket_seconds],
            )
            .await
            .map_err(anyhow::Error::from)?;

        rows.into_iter()
            .map(|row| {
                let bucket_start = row.get::<_, DateTime<Utc>>(0);
                let count = row.get::<_, i64>(1);
                let active_users = u64::try_from(count)
                    .map_err(|_| anyhow!("Active user count cannot be negative"))?;
                Ok(ActiveUsersBucket {
                    bucket_start,
                    active_users,
                })
            })
            .collect()
    }

    #[trace_instrument(skip(self, txn))]
    async fn delete_by_updated_at(
        &self,
        txn: &mut PostgresTransaction,
        updated_at: DateTime<Utc>,
    ) -> anyhow::Result<u64> {
        queries::session::delete_by_updated_at()
            .bind(txn.txn(), &updated_at.into())
            .await
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn list_refresh_token_hashes_by_user(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Vec<SessionRefreshTokenHash>> {
        queries::session::list_refresh_token_hashes_by_user()
            .bind(txn.txn(), &user_id)
            .iter()
            .await?
            .map(|row| {
                row.map_err(Into::into)
                    .and_then(decode_sha256hash)
                    .map(SessionRefreshTokenHash::from)
            })
            .try_collect()
            .await
    }

    #[trace_instrument(skip(self, txn))]
    async fn get_refresh_token_hash(
        &self,
        txn: &mut PostgresTransaction,
        session_id: SessionId,
    ) -> anyhow::Result<Option<SessionRefreshTokenHash>> {
        queries::session::get_refresh_token_hash()
            .bind(txn.txn(), &session_id)
            .opt()
            .await
            .map_err(Into::into)
            .and_then(|row| {
                row.map(|row| decode_sha256hash(row).map(Into::into))
                    .transpose()
            })
    }

    #[trace_instrument(skip(self, txn))]
    async fn save_refresh_token_hash(
        &self,
        txn: &mut PostgresTransaction,
        session_id: SessionId,
        refresh_token_hash: SessionRefreshTokenHash,
    ) -> anyhow::Result<()> {
        queries::session::set_refresh_token_hash()
            .bind(txn.txn(), &session_id, &refresh_token_hash.as_slice())
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}

fn decode_session(value: queries::session::Session) -> anyhow::Result<Session> {
    Ok(Session {
        id: value.id.into(),
        user_id: value.user_id.into(),
        device_name: value.device_name.map(TryInto::try_into).transpose()?,
        created_at: value.created_at.into(),
        updated_at: value.updated_at.into(),
    })
}
