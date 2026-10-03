use academy_di::Build;
use academy_models::{
    publication::{
        NOTICE_HASH, ProfileVisibility, PublicationChoice, PublicationChoiceResult,
        PublicationEpoch, PublicationReceipt, PublicationSettings, PublicationSnapshot,
        PublicationWithdrawal, SCOPE_VERSION,
    },
    user::UserId,
};
use academy_persistence_contracts::publication::{PublicationRepository, PublicationWriteError};
use bb8_postgres::tokio_postgres::Row;
use chrono::{DateTime, Utc};

use crate::PostgresTransaction;

#[derive(Debug, Clone, Copy, Default, Build)]
pub struct PostgresPublicationRepository;

const SETTINGS_SQL: &str = "SELECT profile_visibility,visibility_revision,shared_scope_version,shared_notice_hash,shared_at,withdrawn_at,last_shared_receipt::text AS last_shared_receipt,last_private_receipt::text AS last_private_receipt FROM user_profiles WHERE user_id=$1";

fn decode_settings(row: &Row) -> anyhow::Result<PublicationSettings> {
    let visibility: &str = row.try_get("profile_visibility")?;
    Ok(PublicationSettings {
        profile_visibility: match visibility {
            "shared" => ProfileVisibility::Shared,
            "private" => ProfileVisibility::Private,
            _ => anyhow::bail!("Unknown publication visibility"),
        },
        visibility_revision: row.try_get("visibility_revision")?,
        shared_scope_version: row.try_get("shared_scope_version")?,
        shared_notice_hash: row.try_get("shared_notice_hash")?,
        shared_at: row
            .try_get::<_, Option<DateTime<Utc>>>("shared_at")?
            .map(|t| t.timestamp()),
        withdrawn_at: row
            .try_get::<_, Option<DateTime<Utc>>>("withdrawn_at")?
            .map(|t| t.timestamp()),
        last_shared_receipt: row
            .try_get::<_, Option<String>>("last_shared_receipt")?
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
        last_private_receipt: row
            .try_get::<_, Option<String>>("last_private_receipt")?
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
    })
}

fn decode_epoch(row: &Row, enabled: bool) -> anyhow::Result<PublicationEpoch> {
    let policy_active = row.try_get("policy_active")?;
    Ok(PublicationEpoch {
        scope_version: SCOPE_VERSION.into(),
        publication_epoch: row.try_get("publication_epoch")?,
        epoch_revision: row.try_get("epoch_revision")?,
        policy_active,
        publishing_enabled: enabled && policy_active,
    })
}

impl PublicationRepository<PostgresTransaction> for PostgresPublicationRepository {
    async fn epoch(
        &self,
        txn: &mut PostgresTransaction,
        enabled: bool,
    ) -> anyhow::Result<PublicationEpoch> {
        txn.txn()
            .execute(
                "SELECT profile_publication_recheck($1,$2)",
                &[&SCOPE_VERSION, &NOTICE_HASH],
            )
            .await?;
        let row = txn
            .txn()
            .query_one(
                "SELECT * FROM profile_publication_state WHERE singleton",
                &[],
            )
            .await?;
        decode_epoch(&row, enabled)
    }

    async fn snapshot(
        &self,
        txn: &mut PostgresTransaction,
        enabled: bool,
    ) -> anyhow::Result<PublicationSnapshot> {
        txn.txn()
            .execute(
                "SELECT profile_publication_recheck($1,$2)",
                &[&SCOPE_VERSION, &NOTICE_HASH],
            )
            .await?;
        // One statement binds participants and epoch to the same PostgreSQL snapshot.
        // user_composites excludes purpose-only retained service subjects.
        let row = txn.txn().query_one(
            "SELECT s.*,coalesce((SELECT jsonb_agg(jsonb_build_object(
             'user_id',u.id,'display_name',u.display_name,'avatar_url',NULL,
             'visibility_revision',p.visibility_revision) ORDER BY u.id)
             FROM user_composites u JOIN user_profiles p ON p.user_id=u.id
             WHERE s.policy_active AND $1 AND u.enabled AND u.email_verified AND u.email IS NOT NULL
              AND p.profile_visibility='shared' AND p.shared_scope_version=$2 AND p.shared_notice_hash=$3
              AND p.shared_at IS NOT NULL AND p.last_shared_receipt IS NOT NULL),'[]'::jsonb)::text AS participants
             FROM profile_publication_state s WHERE s.singleton", &[&enabled, &SCOPE_VERSION, &NOTICE_HASH]
        ).await?;
        Ok(PublicationSnapshot {
            epoch: decode_epoch(&row, enabled)?,
            participants: serde_json::from_str(row.try_get("participants")?)?,
        })
    }

    async fn settings(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Option<PublicationSettings>> {
        txn.txn()
            .query_opt(SETTINGS_SQL, &[&*user_id])
            .await?
            .as_ref()
            .map(decode_settings)
            .transpose()
    }

    async fn choose(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
        choice: &PublicationChoice,
        enabled: bool,
        preview_valid: bool,
    ) -> Result<PublicationChoiceResult, PublicationWriteError> {
        self.write_choice(txn, user_id, choice, enabled, preview_valid, false)
            .await
    }

    async fn withdraw(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
        withdrawal: &PublicationWithdrawal,
        enabled: bool,
    ) -> Result<PublicationChoiceResult, PublicationWriteError> {
        let choice = PublicationChoice {
            profile_visibility: ProfileVisibility::Private,
            expected_revision: withdrawal.expected_revision,
            request_id: withdrawal.request_id,
            scope_version: None,
            notice_hash: None,
            preview_token: None,
        };
        self.write_choice(txn, user_id, &choice, enabled, false, true)
            .await
    }
}

impl PostgresPublicationRepository {
    async fn write_choice(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
        choice: &PublicationChoice,
        enabled: bool,
        preview_valid: bool,
        support: bool,
    ) -> Result<PublicationChoiceResult, PublicationWriteError> {
        // Match the existing owner-first deletion/refresh lock order.
        let owner = txn
            .txn()
            .query_opt(
                "SELECT enabled,email_verified,email FROM users WHERE id=$1 FOR UPDATE",
                &[&*user_id],
            )
            .await
            .map_err(anyhow::Error::from)?
            .ok_or(PublicationWriteError::NotFound)?;
        let effective_enabled: bool = txn
            .txn()
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM user_composites WHERE id=$1 AND enabled)",
                &[&*user_id],
            )
            .await
            .map_err(anyhow::Error::from)?
            .get(0);
        if !support && (!owner.get::<_, bool>("enabled") || !effective_enabled) {
            return Err(PublicationWriteError::NotFound);
        }
        let row = txn
            .txn()
            .query_opt(&format!("{SETTINGS_SQL} FOR UPDATE"), &[&*user_id])
            .await
            .map_err(anyhow::Error::from)?
            .ok_or(PublicationWriteError::NotFound)?;
        let current = decode_settings(&row)?;
        let epoch = self.epoch(txn, enabled).await?;
        if !epoch.publishing_enabled {
            return Err(PublicationWriteError::Disabled);
        }
        let source = if support { "support" } else { "owner" };
        // At most the last sharing and last withdrawal receipts; never a click history.
        for receipt in [&current.last_shared_receipt, &current.last_private_receipt]
            .into_iter()
            .flatten()
        {
            if receipt.request_id == choice.request_id {
                if receipt.source != source
                    || receipt.expected_revision != choice.expected_revision
                    || receipt.profile_visibility != choice.profile_visibility
                    || (choice.profile_visibility == ProfileVisibility::Shared
                        && (receipt.scope_version != choice.scope_version
                            || receipt.notice_hash != choice.notice_hash))
                {
                    return Err(PublicationWriteError::Conflict);
                }
                return Ok(PublicationChoiceResult {
                    receipt: receipt.clone(),
                    current: current.clone(),
                    replayed: true,
                });
            }
        }
        if choice.expected_revision != current.visibility_revision {
            return Err(PublicationWriteError::Conflict);
        }
        if choice.profile_visibility == ProfileVisibility::Shared {
            if !owner.get::<_, bool>("email_verified")
                || owner.get::<_, Option<String>>("email").is_none()
            {
                return Err(PublicationWriteError::Unverified);
            }
            if !preview_valid
                || choice.scope_version.as_deref() != Some(SCOPE_VERSION)
                || choice.notice_hash.as_deref() != Some(NOTICE_HASH)
            {
                return Err(PublicationWriteError::InvalidPreview);
            }
        }
        let now: DateTime<Utc> = txn
            .txn()
            .query_one("SELECT clock_timestamp()", &[])
            .await
            .map_err(anyhow::Error::from)?
            .get(0);
        let shared = choice.profile_visibility == ProfileVisibility::Shared;
        let receipt = PublicationReceipt {
            request_id: choice.request_id,
            expected_revision: choice.expected_revision,
            visibility_revision: current
                .visibility_revision
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("Publication revision overflow"))?,
            profile_visibility: choice.profile_visibility,
            recorded_at: now.timestamp(),
            scope_version: shared.then(|| SCOPE_VERSION.into()),
            notice_hash: shared.then(|| NOTICE_HASH.into()),
            source: source.into(),
        };
        txn.txn().execute(
            "UPDATE user_profiles SET profile_visibility=$2,visibility_revision=$3,leaderboard_opt_out=NOT $4,
             shared_scope_version=CASE WHEN $4 THEN $5 ELSE shared_scope_version END,
             shared_notice_hash=CASE WHEN $4 THEN $6 ELSE shared_notice_hash END,
             shared_at=CASE WHEN $4 THEN $7 ELSE shared_at END,
             withdrawn_at=CASE WHEN $4 THEN withdrawn_at ELSE $7 END,
             last_shared_receipt=CASE WHEN $4 THEN $8::text::jsonb ELSE last_shared_receipt END,
             last_private_receipt=CASE WHEN $4 THEN last_private_receipt ELSE $8::text::jsonb END WHERE user_id=$1",
            &[&*user_id, &choice.profile_visibility.as_str(), &receipt.visibility_revision, &shared, &SCOPE_VERSION, &NOTICE_HASH, &now, &serde_json::to_string(&receipt).map_err(anyhow::Error::from)?]
        ).await.map_err(anyhow::Error::from)?;
        let current = self
            .settings(txn, user_id)
            .await?
            .ok_or(PublicationWriteError::NotFound)?;
        Ok(PublicationChoiceResult {
            current,
            receipt,
            replayed: false,
        })
    }
}
