use std::time::Duration;

use academy_core_daily_rewards_contracts::{
    DailyRewardActivity, DailyRewardActivityService, DailyRewardActivitySnapshot,
    DailyRewardUnavailableReason,
};
use academy_models::user::UserId;
use anyhow::{Context, Result};
use bb8::{Pool, PooledConnection};
use bb8_postgres::PostgresConnectionManager;
use chrono::{DateTime, Utc};
use serde_json::json;
use tokio_postgres::NoTls;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct PostgresActivityConfig {
    pub dsn: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout: Duration,
    pub idle_timeout: Option<Duration>,
    pub max_lifetime: Option<Duration>,
}

pub type SkillsActivityConfig = PostgresActivityConfig;
pub type ChallengesActivityConfig = PostgresActivityConfig;

#[derive(Debug, Clone)]
pub struct DailyRewardActivityServiceImpl {
    skills: Option<ActivityPool>,
    challenges: Option<ActivityPool>,
}

#[derive(Debug, Clone)]
struct ActivityPool {
    pool: Pool<PostgresConnectionManager<NoTls>>,
}

type PgConnection<'a> = PooledConnection<'a, PostgresConnectionManager<NoTls>>;

impl ActivityPool {
    async fn new(config: &PostgresActivityConfig) -> Result<Self> {
        let manager = PostgresConnectionManager::new(config.dsn.parse()?, NoTls);
        let min_idle = (config.min_connections > 0).then_some(config.min_connections);
        let pool = Pool::builder()
            .max_size(config.max_connections)
            .min_idle(min_idle)
            .connection_timeout(config.acquire_timeout)
            .idle_timeout(config.idle_timeout)
            .max_lifetime(config.max_lifetime)
            .build(manager)
            .await?;

        Ok(Self { pool })
    }

    async fn connection(&self) -> Result<PgConnection<'_>> {
        self.pool.get().await.map_err(Into::into)
    }
}

impl DailyRewardActivityServiceImpl {
    pub async fn new(
        skills: Option<SkillsActivityConfig>,
        challenges: Option<ChallengesActivityConfig>,
    ) -> Result<Self> {
        let skills_pool = match skills {
            Some(ref cfg) => Some(
                ActivityPool::new(cfg)
                    .await
                    .context("Failed to initialise skills activity pool")?,
            ),
            None => None,
        };

        let challenges_pool = match challenges {
            Some(ref cfg) => Some(
                ActivityPool::new(cfg)
                    .await
                    .context("Failed to initialise challenges activity pool")?,
            ),
            None => None,
        };

        Ok(Self {
            skills: skills_pool,
            challenges: challenges_pool,
        })
    }
}

impl DailyRewardActivityService for DailyRewardActivityServiceImpl {
    async fn detect(
        &self,
        user_id: UserId,
        day_start: DateTime<Utc>,
        day_end: DateTime<Utc>,
    ) -> Result<DailyRewardActivitySnapshot> {
        let mut snapshot = DailyRewardActivitySnapshot::default();

        if let Some(skills) = &self.skills {
            match skills.connection().await {
                Ok(conn) => match detect_lecture(&conn, user_id, day_start, day_end).await {
                    Ok(Some(activity)) => snapshot.lecture.detected = Some(activity),
                    Ok(None) => {}
                    Err(err) => {
                        warn!(error = %err, "Failed to detect lecture completion");
                        snapshot.lecture.unavailable_reason =
                            Some(DailyRewardUnavailableReason::Unknown);
                    }
                },
                Err(err) => {
                    warn!(error = %err, "Failed to acquire skills connection");
                    snapshot.lecture.unavailable_reason =
                        Some(DailyRewardUnavailableReason::Unknown);
                }
            }
        }

        if let Some(challenges) = &self.challenges {
            match challenges.connection().await {
                Ok(conn) => {
                    match detect_subtask(
                        &conn,
                        user_id,
                        day_start,
                        day_end,
                        &["matching", "multiple_choice_question", "question"],
                    )
                    .await
                    {
                        Ok(Some(activity)) => snapshot.practice.detected = Some(activity),
                        Ok(None) => {}
                        Err(err) => {
                            warn!(error = %err, "Failed to detect practice completion");
                            snapshot.practice.unavailable_reason =
                                Some(DailyRewardUnavailableReason::Unknown);
                        }
                    }

                    match detect_subtask(&conn, user_id, day_start, day_end, &["coding_challenge"])
                        .await
                    {
                        Ok(Some(activity)) => snapshot.lab.detected = Some(activity),
                        Ok(None) => {}
                        Err(err) => {
                            warn!(error = %err, "Failed to detect lab completion");
                            snapshot.lab.unavailable_reason =
                                Some(DailyRewardUnavailableReason::Unknown);
                        }
                    }
                }
                Err(err) => {
                    warn!(error = %err, "Failed to acquire challenges connection");
                    snapshot.practice.unavailable_reason =
                        Some(DailyRewardUnavailableReason::Unknown);
                    snapshot.lab.unavailable_reason = Some(DailyRewardUnavailableReason::Unknown);
                }
            }
        }

        Ok(snapshot)
    }
}

async fn detect_lecture(
    conn: &PgConnection<'_>,
    user_id: UserId,
    day_start: DateTime<Utc>,
    day_end: DateTime<Utc>,
) -> Result<Option<DailyRewardActivity>> {
    let first_row = conn
        .query_opt(
            "select course_id, lecture_id, completed \
             from skills_lecture_progress \
             where user_id = $1::uuid \
               and completed >= $2 \
               and completed < $3 \
             order by completed asc \
             limit 1",
            &[&*user_id, &day_start, &day_end],
        )
        .await?;

    let Some(first_row) = first_row else {
        return Ok(None);
    };

    let last_completed = conn
        .query_opt(
            "select max(completed) as completed \
             from skills_lecture_progress \
             where user_id = $1::uuid \
               and completed >= $2 \
               and completed < $3",
            &[&*user_id, &day_start, &day_end],
        )
        .await?
        .and_then(|row| row.get::<_, Option<DateTime<Utc>>>("completed"))
        .unwrap_or_else(|| first_row.get::<_, DateTime<Utc>>("completed"));

    let first_completed: DateTime<Utc> = first_row.get("completed");
    let sample = json!({
        "course_id": first_row.get::<_, String>("course_id"),
        "lecture_id": first_row.get::<_, String>("lecture_id"),
    });

    Ok(Some(DailyRewardActivity {
        first_detected_at: first_completed,
        last_detected_at: last_completed,
        activity_sample: Some(sample),
    }))
}

async fn detect_subtask(
    conn: &PgConnection<'_>,
    user_id: UserId,
    day_start: DateTime<Utc>,
    day_end: DateTime<Utc>,
    allowed_types: &[&str],
) -> Result<Option<DailyRewardActivity>> {
    let first_row = conn
        .query_opt(
            "select cs.task_id, cs.ty, cus.subtask_id, cus.solved_timestamp \
             from challenges_user_subtasks cus \
             join challenges_subtasks cs on cs.id = cus.subtask_id \
             where cus.user_id = $1::uuid \
               and cus.solved_timestamp >= $2 \
               and cus.solved_timestamp < $3 \
               and cs.enabled is true \
               and cs.retired is false \
               and cs.ty = any($4) \
             order by cus.solved_timestamp asc \
             limit 1",
            &[&*user_id, &day_start, &day_end, &allowed_types],
        )
        .await?;

    let Some(first_row) = first_row else {
        return Ok(None);
    };

    let last_solved = conn
        .query_opt(
            "select max(cus.solved_timestamp) as solved_timestamp \
             from challenges_user_subtasks cus \
             join challenges_subtasks cs on cs.id = cus.subtask_id \
             where cus.user_id = $1::uuid \
               and cus.solved_timestamp >= $2 \
               and cus.solved_timestamp < $3 \
               and cs.enabled is true \
               and cs.retired is false \
               and cs.ty = any($4)",
            &[&*user_id, &day_start, &day_end, &allowed_types],
        )
        .await?
        .and_then(|row| row.get::<_, Option<DateTime<Utc>>>("solved_timestamp"))
        .unwrap_or_else(|| first_row.get::<_, DateTime<Utc>>("solved_timestamp"));

    let first_solved: DateTime<Utc> = first_row.get("solved_timestamp");
    let sample = json!({
        "task_id": first_row.get::<_, String>("task_id"),
        "subtask_id": first_row.get::<_, String>("subtask_id"),
        "subtask_type": first_row.get::<_, String>("ty"),
    });

    Ok(Some(DailyRewardActivity {
        first_detected_at: first_solved,
        last_detected_at: last_solved,
        activity_sample: Some(sample),
    }))
}
