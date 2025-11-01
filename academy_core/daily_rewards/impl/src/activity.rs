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
use serde_json::{Map, Value};
use tokio_postgres::{NoTls, error::SqlState};
use tracing::warn;
use uuid::Uuid;

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

#[derive(Debug, Default)]
struct LectureMetadata {
    course_title: Option<String>,
    section_id: Option<String>,
    section_title: Option<String>,
    lecture_title: Option<String>,
}

#[derive(Debug)]
struct SubtaskSampleInput {
    task_id: String,
    subtask_id: String,
    subtask_type: String,
    challenge_title: Option<String>,
    category_id: Option<Uuid>,
    skill_ids: Vec<String>,
    course_id: Option<String>,
    section_id: Option<String>,
    lecture_id: Option<String>,
}

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
                        false,
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

                    match detect_subtask(
                        &conn,
                        user_id,
                        day_start,
                        day_end,
                        &["coding_challenge"],
                        true,
                    )
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

    let course_id: String = first_row.get("course_id");
    let lecture_id: String = first_row.get("lecture_id");
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
    let metadata = fetch_lecture_metadata(conn, &course_id, &lecture_id).await?;
    let sample = build_lecture_sample(course_id, lecture_id, metadata);

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
    is_lab: bool,
) -> Result<Option<DailyRewardActivity>> {
    let first_row = conn
        .query_opt(
            "select \
                cs.task_id, \
                cs.ty, \
                cus.subtask_id, \
                cus.solved_timestamp, \
                cc.title as challenge_title, \
                cc.category_id, \
                cc.skill_ids, \
                cct.course_id::text as course_id, \
                cct.section_id::text as section_id, \
                cct.lecture_id::text as lecture_id \
             from challenges_user_subtasks cus \
             join challenges_subtasks cs on cs.id = cus.subtask_id \
             left join challenges_challenges cc on cc.task_id = cs.task_id \
             left join challenges_course_tasks cct on cct.task_id = cs.task_id \
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

    let task_id: Uuid = first_row.get("task_id");
    let subtask_id: Uuid = first_row.get("subtask_id");
    let subtask_type: String = first_row.get("ty");
    let first_solved: DateTime<Utc> = first_row.get("solved_timestamp");
    let challenge_title: Option<String> = first_row
        .try_get::<_, Option<String>>("challenge_title")
        .unwrap_or(None);
    let category_id: Option<Uuid> = first_row
        .try_get::<_, Option<Uuid>>("category_id")
        .unwrap_or(None);
    let skill_ids: Vec<String> = match first_row.try_get::<_, Vec<String>>("skill_ids") {
        Ok(ids) => ids,
        Err(_) => first_row
            .try_get::<_, Vec<Uuid>>("skill_ids")
            .map(|ids| ids.into_iter().map(|id| id.to_string()).collect())
            .unwrap_or_default(),
    };
    let course_id: Option<String> = first_row
        .try_get::<_, Option<String>>("course_id")
        .unwrap_or(None)
        .and_then(normalize_db_string);
    let section_id: Option<String> = first_row
        .try_get::<_, Option<String>>("section_id")
        .unwrap_or(None)
        .and_then(normalize_db_string);
    let lecture_id: Option<String> = first_row
        .try_get::<_, Option<String>>("lecture_id")
        .unwrap_or(None)
        .and_then(normalize_db_string);

    let sample = build_subtask_sample(
        SubtaskSampleInput {
            task_id: task_id.to_string(),
            subtask_id: subtask_id.to_string(),
            subtask_type: subtask_type.clone(),
            challenge_title,
            category_id,
            skill_ids,
            course_id,
            section_id,
            lecture_id,
        },
        is_lab,
    );

    Ok(Some(DailyRewardActivity {
        first_detected_at: first_solved,
        last_detected_at: last_solved,
        activity_sample: Some(sample),
    }))
}

fn build_lecture_sample(
    course_id: String,
    lecture_id: String,
    metadata: Option<LectureMetadata>,
) -> Value {
    let mut sample = Map::new();
    sample.insert("course_id".into(), Value::String(course_id));
    sample.insert("lecture_id".into(), Value::String(lecture_id));

    if let Some(metadata) = metadata {
        insert_optional_string(&mut sample, "course_title", metadata.course_title);
        insert_optional_string(&mut sample, "section_id", metadata.section_id.clone());
        insert_optional_string(&mut sample, "section_title", metadata.section_title);
        insert_optional_string(&mut sample, "lecture_title", metadata.lecture_title);
    }

    Value::Object(sample)
}

fn build_subtask_sample(input: SubtaskSampleInput, is_lab: bool) -> Value {
    let SubtaskSampleInput {
        task_id,
        subtask_id,
        subtask_type,
        challenge_title,
        category_id,
        skill_ids,
        course_id,
        section_id,
        lecture_id,
    } = input;

    let mut sample = Map::new();
    sample.insert("task_id".into(), Value::String(task_id.clone()));
    sample.insert("subtask_id".into(), Value::String(subtask_id.clone()));
    sample.insert("subtask_type".into(), Value::String(subtask_type.clone()));

    insert_optional_string(&mut sample, "course_id", course_id.clone());
    insert_optional_string(&mut sample, "section_id", section_id.clone());
    insert_optional_string(&mut sample, "lecture_id", lecture_id.clone());

    if let Some(category_id) = category_id {
        sample.insert("category_id".into(), Value::String(category_id.to_string()));
    }

    if !skill_ids.is_empty() {
        sample.insert(
            "skill_ids".into(),
            Value::Array(skill_ids.iter().cloned().map(Value::String).collect()),
        );
    }

    if is_lab {
        insert_optional_string(&mut sample, "lab_title", challenge_title);
        sample.insert("challenge_id".into(), Value::String(task_id));
        sample.insert("coding_challenge_id".into(), Value::String(subtask_id));
    } else {
        insert_optional_string(&mut sample, "task_title", challenge_title);
        sample.insert("solve_id".into(), Value::String(task_id.clone()));
        sample.insert(
            "quizzes_from".into(),
            Value::String(derive_quizzes_from(
                course_id.as_deref(),
                &skill_ids,
                &subtask_type,
            )),
        );
        if subtask_type.contains("matching") {
            sample.insert("matching_id".into(), Value::String(subtask_id.clone()));
        }
        sample.insert("query_subtask_id".into(), Value::String(subtask_id));
    }

    Value::Object(sample)
}

async fn fetch_lecture_metadata(
    conn: &PgConnection<'_>,
    course_id: &str,
    lecture_id: &str,
) -> Result<Option<LectureMetadata>> {
    let course_uuid = match Uuid::parse_str(course_id) {
        Ok(uuid) => uuid,
        Err(err) => {
            warn!(error = %err, %course_id, "Skipping lecture metadata; invalid course_id");
            return Ok(None);
        }
    };
    let lecture_uuid = match Uuid::parse_str(lecture_id) {
        Ok(uuid) => uuid,
        Err(err) => {
            warn!(error = %err, %lecture_id, "Skipping lecture metadata; invalid lecture_id");
            return Ok(None);
        }
    };

    let row = match conn
        .query_opt(
            "select \
                course.title as course_title, \
                section.id as section_id, \
                section.title as section_title, \
                lecture.title as lecture_title \
             from skills_course_lectures lecture \
             join skills_course_sections section \
               on section.course_id = lecture.course_id \
              and section.id = lecture.section_id \
             join skills_courses course \
               on course.id = lecture.course_id \
             where lecture.course_id = $1 \
               and lecture.id = $2 \
             limit 1",
            &[&course_uuid, &lecture_uuid],
        )
        .await
    {
        Ok(row) => row,
        Err(err) => {
            if is_metadata_lookup_error(&err) {
                warn!(error = %err, "Skipping lecture metadata enrichment");
                return Ok(None);
            }
            return Err(err.into());
        }
    };

    let metadata = row.map(|row| LectureMetadata {
        course_title: row
            .try_get::<_, Option<String>>("course_title")
            .unwrap_or(None),
        section_id: row
            .try_get::<_, Option<Uuid>>("section_id")
            .unwrap_or(None)
            .map(|id| id.to_string()),
        section_title: row
            .try_get::<_, Option<String>>("section_title")
            .unwrap_or(None),
        lecture_title: row
            .try_get::<_, Option<String>>("lecture_title")
            .unwrap_or(None),
    });

    Ok(metadata)
}

fn is_metadata_lookup_error(err: &tokio_postgres::Error) -> bool {
    matches!(
        err.code(),
        Some(
            &SqlState::UNDEFINED_TABLE
                | &SqlState::UNDEFINED_FUNCTION
                | &SqlState::UNDEFINED_COLUMN
                | &SqlState::INVALID_SCHEMA_NAME
        )
    )
}

fn normalize_db_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn insert_optional_string(map: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        if !value.trim().is_empty() {
            map.insert(key.to_owned(), Value::String(value));
        }
    }
}

fn derive_quizzes_from(
    course_id: Option<&str>,
    skill_ids: &[String],
    subtask_type: &str,
) -> String {
    if course_id.is_some() {
        return "course".to_owned();
    }
    if !skill_ids.is_empty() {
        return "skill".to_owned();
    }
    if subtask_type.contains("matching") {
        return "course".to_owned();
    }
    "quiz".to_owned()
}
