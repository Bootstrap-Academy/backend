use std::time::Duration;

use academy_core_daily_rewards_contracts::{
    DailyRewardActivity, DailyRewardActivityService, DailyRewardActivitySnapshot,
    DailyRewardActivityState, DailyRewardUnavailableReason,
};
use academy_models::{auth::AccessToken, user::UserId};
use anyhow::{Context, Result, anyhow};
use bb8::{Pool, PooledConnection};
use bb8_postgres::PostgresConnectionManager;
use chrono::{DateTime, NaiveDateTime, Utc};
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Map, Value};
use tokio_postgres::{NoTls, Row, error::SqlState};
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
pub struct SkillsRecommendationConfig {
    pub base_url: String,
    pub timeout: Option<Duration>,
}

#[derive(Debug, Clone)]
struct SkillsRecommendationClient {
    client: Client,
    base_url: Url,
}

impl SkillsRecommendationClient {
    fn new(config: &SkillsRecommendationConfig) -> Result<Self> {
        let base_url = Url::parse(&config.base_url)
            .context("Failed to parse skills recommendations base URL")?;

        let mut builder = Client::builder();
        if let Some(timeout) = config.timeout {
            builder = builder.timeout(timeout);
        }

        let client = builder
            .build()
            .context("Failed to initialise skills recommendations HTTP client")?;

        Ok(Self { client, base_url })
    }

    async fn ext_lecture(&self, token: &AccessToken) -> Result<Option<NextLectureRecommendation>> {
        self.fetch_payload(token, "courses/next/lecture").await
    }

    async fn ext_task(&self, token: &AccessToken) -> Result<Option<NextTaskRecommendation>> {
        self.fetch_payload(token, "courses/next/task").await
    }

    async fn ext_lab(&self, token: &AccessToken) -> Result<Option<NextLabRecommendation>> {
        self.fetch_payload(token, "courses/next/lab").await
    }

    async fn fetch_payload<T>(&self, token: &AccessToken, path: &str) -> Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        let url = self
            .base_url
            .join(path)
            .context("Failed to build skills recommendation URL")?;

        let response = self
            .client
            .get(url)
            .bearer_auth(token.as_str())
            .send()
            .await
            .context("Failed to fetch skills recommendation")?;

        match response.status() {
            StatusCode::NO_CONTENT | StatusCode::NOT_FOUND => Ok(None),
            _ => {
                let response = response
                    .error_for_status()
                    .context("Skills recommendation request failed")?;
                let payload = response
                    .json::<T>()
                    .await
                    .context("Failed to deserialize skills recommendation response")?;
                Ok(Some(payload))
            }
        }
    }
}

trait TimestampColumnAccessor {
    fn try_get_datetime_utc(&self, column: &str) -> Result<Option<DateTime<Utc>>>;
    fn try_get_naive_datetime(&self, column: &str) -> Result<Option<NaiveDateTime>>;
}

struct PgTimestampRow<'a> {
    row: &'a Row,
}

impl<'a> TimestampColumnAccessor for PgTimestampRow<'a> {
    fn try_get_datetime_utc(&self, column: &str) -> Result<Option<DateTime<Utc>>> {
        self.row
            .try_get::<_, Option<DateTime<Utc>>>(column)
            .map_err(Into::into)
    }

    fn try_get_naive_datetime(&self, column: &str) -> Result<Option<NaiveDateTime>> {
        self.row
            .try_get::<_, Option<NaiveDateTime>>(column)
            .map_err(Into::into)
    }
}

fn read_optional_timestamp<R>(row: &R, column: &str) -> Result<Option<DateTime<Utc>>>
where
    R: TimestampColumnAccessor + ?Sized,
{
    match row.try_get_datetime_utc(column) {
        Ok(value) => Ok(value),
        Err(first_err) => match row.try_get_naive_datetime(column) {
            Ok(naive) => Ok(naive.map(|ts| DateTime::from_naive_utc_and_offset(ts, Utc))),
            Err(_) => Err(first_err),
        },
    }
}

fn read_required_timestamp<R>(row: &R, column: &str) -> Result<DateTime<Utc>>
where
    R: TimestampColumnAccessor + ?Sized,
{
    read_optional_timestamp(row, column)?
        .ok_or_else(|| anyhow!("column `{}` returned NULL timestamp", column))
}

fn build_lecture_sample(
    course_id: String,
    lecture_id: String,
    metadata: Option<LectureMetadata>,
) -> Value {
    let mut sample = Map::new();
    sample.insert("course_id".into(), Value::String(course_id.clone()));
    duplicate_string_field(&mut sample, "course_id", "courseId");
    sample.insert("lecture_id".into(), Value::String(lecture_id.clone()));
    duplicate_string_field(&mut sample, "lecture_id", "lectureId");

    if let Some(metadata) = metadata {
        insert_optional_string(&mut sample, "course_title", metadata.course_title);
        duplicate_string_field(&mut sample, "course_title", "courseTitle");
        insert_optional_string(&mut sample, "section_id", metadata.section_id.clone());
        duplicate_string_field(&mut sample, "section_id", "sectionId");
        insert_optional_string(&mut sample, "section_title", metadata.section_title);
        duplicate_string_field(&mut sample, "section_title", "sectionTitle");
        insert_optional_string(&mut sample, "lecture_title", metadata.lecture_title);
        duplicate_string_field(&mut sample, "lecture_title", "lectureTitle");
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
    duplicate_string_field(&mut sample, "task_id", "taskId");
    sample.insert("subtask_id".into(), Value::String(subtask_id.clone()));
    duplicate_string_field(&mut sample, "subtask_id", "subtaskId");
    sample.insert("subtask_type".into(), Value::String(subtask_type.clone()));
    duplicate_string_field(&mut sample, "subtask_type", "subtaskType");

    insert_optional_string(&mut sample, "course_id", course_id.clone());
    duplicate_string_field(&mut sample, "course_id", "courseId");
    insert_optional_string(&mut sample, "section_id", section_id.clone());
    duplicate_string_field(&mut sample, "section_id", "sectionId");
    insert_optional_string(&mut sample, "lecture_id", lecture_id.clone());
    duplicate_string_field(&mut sample, "lecture_id", "lectureId");

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
        duplicate_string_field(&mut sample, "lab_title", "labTitle");
        sample.insert("challenge_id".into(), Value::String(task_id.clone()));
        duplicate_string_field(&mut sample, "challenge_id", "challengeId");
        sample.insert(
            "coding_challenge_id".into(),
            Value::String(subtask_id.clone()),
        );
        duplicate_string_field(&mut sample, "coding_challenge_id", "codingChallengeId");
    } else {
        insert_optional_string(&mut sample, "task_title", challenge_title);
        duplicate_string_field(&mut sample, "task_title", "taskTitle");
        sample.insert("solve_id".into(), Value::String(task_id.clone()));
        duplicate_string_field(&mut sample, "solve_id", "solveId");
        sample.insert("solvable_id".into(), Value::String(task_id.clone()));
        duplicate_string_field(&mut sample, "solvable_id", "solvableId");
        sample.insert(
            "quizzes_from".into(),
            Value::String(derive_quizzes_from(
                course_id.as_deref(),
                &skill_ids,
                &subtask_type,
            )),
        );
        duplicate_string_field(&mut sample, "quizzes_from", "quizzesFrom");
        if subtask_type.contains("matching") {
            sample.insert("matching_id".into(), Value::String(subtask_id.clone()));
            duplicate_string_field(&mut sample, "matching_id", "matchingId");
        }
        sample.insert("query_subtask_id".into(), Value::String(subtask_id.clone()));
        duplicate_string_field(&mut sample, "query_subtask_id", "querySubTaskId");
    }

    Value::Object(sample)
}

async fn fetch_lecture_metadata(
    conn: &PgConnection<'_>,
    course_id: &str,
    lecture_id: &str,
) -> Result<Option<LectureMetadata>> {
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
            &[&course_id, &lecture_id],
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
            .unwrap_or(None)
            .and_then(normalize_db_string),
        section_id: row
            .try_get::<_, Option<Uuid>>("section_id")
            .unwrap_or(None)
            .map(|id| id.to_string())
            .or_else(|| {
                row.try_get::<_, Option<String>>("section_id")
                    .unwrap_or(None)
                    .and_then(normalize_db_string)
            }),
        section_title: row
            .try_get::<_, Option<String>>("section_title")
            .unwrap_or(None)
            .and_then(normalize_db_string),
        lecture_title: row
            .try_get::<_, Option<String>>("lecture_title")
            .unwrap_or(None)
            .and_then(normalize_db_string),
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

fn insert_optional_string(map: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        map.insert(key.to_owned(), Value::String(value));
    }
}

fn duplicate_string_field(map: &mut Map<String, Value>, source: &str, alias: &str) {
    if map.contains_key(alias) {
        return;
    }

    if let Some(Value::String(existing)) = map.get(source) {
        let alias_value = existing.clone();
        map.insert(alias.to_owned(), Value::String(alias_value));
    }
}

fn normalize_db_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, TimeZone};
    use serde_json::Value;
    use std::cell::RefCell;

    struct FakeTimestampRow {
        datetime: RefCell<Option<Result<Option<DateTime<Utc>>>>>,
        naive: RefCell<Option<Result<Option<NaiveDateTime>>>>,
    }

    impl FakeTimestampRow {
        fn new(
            datetime: Result<Option<DateTime<Utc>>>,
            naive: Result<Option<NaiveDateTime>>,
        ) -> Self {
            Self {
                datetime: RefCell::new(Some(datetime)),
                naive: RefCell::new(Some(naive)),
            }
        }
    }

    impl TimestampColumnAccessor for FakeTimestampRow {
        fn try_get_datetime_utc(&self, _column: &str) -> Result<Option<DateTime<Utc>>> {
            self.datetime
                .borrow_mut()
                .take()
                .expect("datetime accessor should be called once")
        }

        fn try_get_naive_datetime(&self, _column: &str) -> Result<Option<NaiveDateTime>> {
            self.naive
                .borrow_mut()
                .take()
                .expect("naive accessor should be called once")
        }
    }

    #[test]
    fn read_optional_timestamp_prefers_timezone_aware_value() {
        let expected = Utc.with_ymd_and_hms(2025, 11, 2, 13, 0, 0).unwrap();
        let row = FakeTimestampRow::new(Ok(Some(expected)), Err(anyhow!("naive unused")));

        let actual = read_optional_timestamp(&row, "completed").expect("timestamp read");

        assert_eq!(actual, Some(expected));
    }

    #[test]
    fn read_optional_timestamp_falls_back_to_naive_value() {
        let naive = NaiveDate::from_ymd_opt(2025, 11, 2)
            .unwrap()
            .and_hms_opt(12, 30, 0)
            .unwrap();
        let row = FakeTimestampRow::new(Err(anyhow!("type mismatch")), Ok(Some(naive)));

        let actual = read_optional_timestamp(&row, "completed").expect("timestamp read");

        assert_eq!(
            actual,
            Some(DateTime::from_naive_utc_and_offset(naive, Utc))
        );
    }

    #[test]
    fn read_required_timestamp_errors_on_null() {
        let row = FakeTimestampRow::new(Ok(None), Ok(None));

        let err = read_required_timestamp(&row, "completed").expect_err("null timestamp");

        assert!(
            err.to_string()
                .contains("column `completed` returned NULL timestamp"),
            "expected helpful error message"
        );
    }

    fn row(
        solved: Option<DateTime<Utc>>,
        subtask_type: &str,
        task_id: &str,
        subtask_id: &str,
        challenge_title: Option<&str>,
    ) -> SubtaskRow {
        SubtaskRow {
            solved_timestamp: solved,
            task_id: task_id.to_owned(),
            subtask_id: subtask_id.to_owned(),
            subtask_type: subtask_type.to_owned(),
            challenge_title: challenge_title.map(|title| title.to_owned()),
            category_id: None,
            skill_ids: Vec::new(),
            course_id: Some("course-1".to_owned()),
            section_id: Some("section-1".to_owned()),
            lecture_id: Some("lecture-1".to_owned()),
        }
    }

    #[test]
    fn build_subtask_activity_returns_none_without_matching_rows() {
        let solved = Utc.with_ymd_and_hms(2025, 11, 1, 9, 0, 0).unwrap();
        let rows = vec![row(
            Some(solved),
            "coding_challenge",
            "task",
            "sub",
            Some("Lab"),
        )];
        let day_start = Utc.with_ymd_and_hms(2025, 11, 1, 0, 0, 0).unwrap();
        let day_end = day_start + chrono::Duration::days(1);

        assert!(build_subtask_activity(&rows, &["matching"], false, day_start, day_end).is_none());
    }

    #[test]
    fn build_subtask_activity_tracks_first_and_last_for_practice() {
        let first = Utc.with_ymd_and_hms(2025, 11, 1, 8, 0, 0).unwrap();
        let mid = Utc.with_ymd_and_hms(2025, 11, 1, 10, 30, 0).unwrap();
        let last = Utc.with_ymd_and_hms(2025, 11, 1, 14, 15, 0).unwrap();

        let rows = vec![
            row(Some(first), "matching", "task-1", "sub-1", Some("Match A")),
            row(
                Some(mid),
                "coding_challenge",
                "task-ignored",
                "sub-ignored",
                None,
            ),
            row(
                Some(last),
                "multiple_choice_question",
                "task-2",
                "sub-2",
                Some("Quiz B"),
            ),
        ];
        let day_start = Utc.with_ymd_and_hms(2025, 11, 1, 0, 0, 0).unwrap();
        let day_end = day_start + chrono::Duration::days(1);

        let activity = build_subtask_activity(
            &rows,
            &["matching", "multiple_choice_question", "question"],
            false,
            day_start,
            day_end,
        )
        .expect("activity expected");

        assert_eq!(activity.first_detected_at, first);
        assert_eq!(activity.last_detected_at, last);

        let sample_value = activity.activity_sample.expect("practice sample expected");
        let sample = sample_value.as_object().expect("sample object");

        assert_eq!(
            sample.get("task_id").and_then(Value::as_str),
            Some("task-1")
        );
        assert_eq!(
            sample.get("matching_id").and_then(Value::as_str),
            Some("sub-1")
        );
        assert_eq!(
            sample.get("task_title").and_then(Value::as_str),
            Some("Match A")
        );
    }

    #[test]
    fn build_subtask_activity_builds_lab_sample() {
        let solved = Utc.with_ymd_and_hms(2025, 11, 2, 12, 0, 0).unwrap();
        let mut lab_row = row(
            Some(solved),
            "coding_challenge",
            "lab-task",
            "lab-sub",
            Some("FizzBuzz"),
        );
        lab_row.skill_ids = vec!["rust".to_owned()];
        let day_start = Utc.with_ymd_and_hms(2025, 11, 2, 0, 0, 0).unwrap();
        let day_end = day_start + chrono::Duration::days(1);

        let activity =
            build_subtask_activity(&[lab_row], &["coding_challenge"], true, day_start, day_end)
                .expect("lab activity");

        assert_eq!(activity.first_detected_at, solved);
        assert_eq!(activity.last_detected_at, solved);

        let sample_value = activity.activity_sample.expect("lab sample expected");
        let sample = sample_value.as_object().expect("sample object");

        assert_eq!(
            sample.get("coding_challenge_id").and_then(Value::as_str),
            Some("lab-sub")
        );
        assert_eq!(
            sample.get("lab_title").and_then(Value::as_str),
            Some("FizzBuzz")
        );
        assert_eq!(
            sample
                .get("skill_ids")
                .and_then(Value::as_array)
                .map(|ids| ids.len()),
            Some(1)
        );
    }

    #[test]
    fn build_subtask_activity_ignores_out_of_window_entries() {
        let day_start = Utc.with_ymd_and_hms(2025, 11, 3, 0, 0, 0).unwrap();
        let day_end = day_start + chrono::Duration::days(1);
        let solved = day_start - chrono::Duration::hours(1);

        let rows = vec![row(
            Some(solved),
            "matching",
            "task-out-of-window",
            "sub-out-of-window",
            None,
        )];

        assert!(
            build_subtask_activity(&rows, &["matching"], false, day_start, day_end).is_none(),
            "entries outside the day should be ignored"
        );
    }

    #[test]
    fn build_subtask_activity_prefers_today_entries() {
        let day_start = Utc.with_ymd_and_hms(2025, 11, 4, 0, 0, 0).unwrap();
        let day_end = day_start + chrono::Duration::days(1);
        let before = day_start - chrono::Duration::hours(2);
        let morning = day_start + chrono::Duration::hours(1);
        let evening = day_start + chrono::Duration::hours(5);

        let rows = vec![
            row(
                Some(before),
                "matching",
                "task-before",
                "sub-before",
                Some("Old Match"),
            ),
            row(
                Some(morning),
                "matching",
                "task-today",
                "sub-today",
                Some("Today Match"),
            ),
            row(
                Some(evening),
                "matching",
                "task-today-late",
                "sub-today-late",
                Some("Evening Match"),
            ),
        ];

        let activity = build_subtask_activity(&rows, &["matching"], false, day_start, day_end)
            .expect("today activity expected");

        assert_eq!(activity.first_detected_at, morning);
        assert_eq!(activity.last_detected_at, evening);

        let sample_value = activity.activity_sample.expect("practice sample expected");
        let sample = sample_value.as_object().expect("sample object");

        assert_eq!(
            sample.get("matching_id").and_then(Value::as_str),
            Some("sub-today"),
        );
    }

    #[test]
    fn build_subtask_activity_ignores_unsolved_entries() {
        let day_start = Utc.with_ymd_and_hms(2025, 11, 5, 0, 0, 0).unwrap();
        let day_end = day_start + chrono::Duration::days(1);

        let rows = vec![row(
            None,
            "matching",
            "task-pending",
            "sub-pending",
            Some("Pending Match"),
        )];

        assert!(
            build_subtask_activity(&rows, &["matching"], false, day_start, day_end).is_none(),
            "pending subtasks should not trigger detection",
        );
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

#[derive(Debug, Deserialize)]
struct RecommendationCourse {
    id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    image: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RecommendationSection {
    id: String,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RecommendationLecture {
    id: String,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RecommendationTask {
    id: String,
    subtask_id: String,
    subtask_type: String,
}

#[derive(Debug, Deserialize)]
struct NextLectureRecommendation {
    course: RecommendationCourse,
    section: RecommendationSection,
    lecture: RecommendationLecture,
}

#[derive(Debug, Deserialize)]
struct NextTaskRecommendation {
    course: RecommendationCourse,
    section: RecommendationSection,
    lecture: RecommendationLecture,
    task: RecommendationTask,
}

#[derive(Debug, Deserialize)]
struct NextLabRecommendation {
    course: RecommendationCourse,
    section: RecommendationSection,
    lecture: RecommendationLecture,
    task: RecommendationTask,
}

#[derive(Debug, Clone)]
pub struct DailyRewardActivityServiceImpl {
    skills: Option<ActivityPool>,
    challenges: Option<ActivityPool>,
    skills_recommendations: Option<SkillsRecommendationClient>,
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

#[derive(Debug, Clone)]
struct SubtaskRow {
    solved_timestamp: Option<DateTime<Utc>>,
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

#[derive(Debug, Default)]
struct ChallengeMetadata {
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

async fn detect_lecture(
    conn: &PgConnection<'_>,
    user_id: UserId,
    day_start: DateTime<Utc>,
    day_end: DateTime<Utc>,
) -> Result<Option<DailyRewardActivity>> {
    let day_start_naive = day_start.naive_utc();
    let day_end_naive = day_end.naive_utc();

    let first_row = conn
        .query_opt(
            "select course_id, lecture_id, completed \
             from skills_lecture_progress \
             where user_id::uuid = $1 \
               and completed >= $2 \
               and completed < $3 \
             order by completed asc \
             limit 1",
            &[&*user_id, &day_start_naive, &day_end_naive],
        )
        .await?;

    let Some(first_row) = first_row else {
        return Ok(None);
    };

    let course_id: String = first_row.get("course_id");
    let lecture_id: String = first_row.get("lecture_id");

    let first_completed =
        read_required_timestamp(&PgTimestampRow { row: &first_row }, "completed")?;
    let mut last_completed = first_completed;
    let maybe_last = conn
        .query_opt(
            "select max(completed) as completed \
             from skills_lecture_progress \
             where user_id::uuid = $1 \
               and completed >= $2 \
               and completed < $3",
            &[&*user_id, &day_start_naive, &day_end_naive],
        )
        .await?
        .map(|row| read_optional_timestamp(&PgTimestampRow { row: &row }, "completed"))
        .transpose()?
        .flatten();

    if let Some(value) = maybe_last {
        last_completed = value;
    }

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
    if allowed_types.is_empty() {
        return Ok(None);
    }

    let allowed: Vec<&str> = allowed_types.to_vec();
    let day_start_naive = day_start.naive_utc();
    let day_end_naive = day_end.naive_utc();

    let rows = conn
        .query(
            "select \
                cs.task_id, \
                cs.ty::text as subtask_type, \
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
               and cs.ty::text = any($4::text[]) \
             order by cus.solved_timestamp asc",
            &[&*user_id, &day_start_naive, &day_end_naive, &allowed],
        )
        .await?;

    let mut subtasks = Vec::with_capacity(rows.len());

    for row in rows {
        let solved_timestamp =
            read_optional_timestamp(&PgTimestampRow { row: &row }, "solved_timestamp")?;
        let task_id: Uuid = row.get("task_id");
        let subtask_id: Uuid = row.get("subtask_id");
        let challenge_title = row
            .try_get::<_, Option<String>>("challenge_title")
            .unwrap_or(None)
            .and_then(normalize_db_string);
        let category_id = row
            .try_get::<_, Option<Uuid>>("category_id")
            .unwrap_or(None);
        let skill_ids: Vec<String> = match row.try_get::<_, Vec<String>>("skill_ids") {
            Ok(ids) => ids,
            Err(_) => row
                .try_get::<_, Vec<Uuid>>("skill_ids")
                .map(|ids| ids.into_iter().map(|id| id.to_string()).collect())
                .unwrap_or_default(),
        };
        let course_id = row
            .try_get::<_, Option<String>>("course_id")
            .unwrap_or(None)
            .and_then(normalize_db_string);
        let section_id = row
            .try_get::<_, Option<String>>("section_id")
            .unwrap_or(None)
            .and_then(normalize_db_string);
        let lecture_id = row
            .try_get::<_, Option<String>>("lecture_id")
            .unwrap_or(None)
            .and_then(normalize_db_string);

        subtasks.push(SubtaskRow {
            solved_timestamp,
            task_id: task_id.to_string(),
            subtask_id: subtask_id.to_string(),
            subtask_type: row.get("subtask_type"),
            challenge_title,
            category_id,
            skill_ids,
            course_id,
            section_id,
            lecture_id,
        });
    }

    Ok(build_subtask_activity(
        &subtasks,
        allowed_types,
        is_lab,
        day_start,
        day_end,
    ))
}

fn build_subtask_activity(
    rows: &[SubtaskRow],
    allowed_types: &[&str],
    is_lab: bool,
    day_start: DateTime<Utc>,
    day_end: DateTime<Utc>,
) -> Option<DailyRewardActivity> {
    let mut first_detected_at: Option<DateTime<Utc>> = None;
    let mut last_detected_at: Option<DateTime<Utc>> = None;
    let mut sample = None;

    for row in rows {
        let Some(solved_timestamp) = row.solved_timestamp else {
            continue;
        };

        if solved_timestamp < day_start || solved_timestamp >= day_end {
            continue;
        }

        if !allowed_types
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(row.subtask_type.as_str()))
        {
            continue;
        }

        if first_detected_at.is_none() {
            first_detected_at = Some(solved_timestamp);
            sample = Some(build_subtask_sample(
                SubtaskSampleInput {
                    task_id: row.task_id.clone(),
                    subtask_id: row.subtask_id.clone(),
                    subtask_type: row.subtask_type.clone(),
                    challenge_title: row.challenge_title.clone(),
                    category_id: row.category_id,
                    skill_ids: row.skill_ids.clone(),
                    course_id: row.course_id.clone(),
                    section_id: row.section_id.clone(),
                    lecture_id: row.lecture_id.clone(),
                },
                is_lab,
            ));
        }

        last_detected_at = Some(match last_detected_at {
            Some(existing) if existing >= solved_timestamp => existing,
            _ => solved_timestamp,
        });
    }

    let first_detected_at = first_detected_at?;
    let last_detected_at = last_detected_at.unwrap_or(first_detected_at);

    Some(DailyRewardActivity {
        first_detected_at,
        last_detected_at,
        activity_sample: sample,
    })
}

impl DailyRewardActivityServiceImpl {
    pub async fn new(
        skills: Option<SkillsActivityConfig>,
        challenges: Option<ChallengesActivityConfig>,
        skills_recommendations: Option<SkillsRecommendationConfig>,
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

        let recommendations_client = match skills_recommendations {
            Some(ref cfg) => Some(
                SkillsRecommendationClient::new(cfg)
                    .context("Failed to initialise skills recommendations client")?,
            ),
            None => None,
        };

        Ok(Self {
            skills: skills_pool,
            challenges: challenges_pool,
            skills_recommendations: recommendations_client,
        })
    }
}

impl DailyRewardActivityService for DailyRewardActivityServiceImpl {
    async fn detect(
        &self,
        token: Option<&AccessToken>,
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
                        warn!(error = ?err, "Failed to detect lecture completion");
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
                            warn!(error = ?err, "Failed to detect practice completion");
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
                            warn!(error = ?err, "Failed to detect lab completion");
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

        if let (Some(token), Some(client)) = (token, &self.skills_recommendations) {
            match client.ext_lecture(token).await {
                Ok(Some(recommendation)) => {
                    let sample = self.build_lecture_recommendation_sample(&recommendation);
                    Self::apply_recommendation(&mut snapshot.lecture, Some(sample));
                }
                Ok(None) => Self::apply_recommendation(&mut snapshot.lecture, None),
                Err(err) => {
                    warn!(error = %err, "Failed to fetch lecture recommendation");
                    Self::mark_recommendation_error(&mut snapshot.lecture);
                }
            }

            match client.ext_task(token).await {
                Ok(Some(recommendation)) => match self
                    .build_task_recommendation_sample(&recommendation, false)
                    .await
                {
                    Ok(sample) => Self::apply_recommendation(&mut snapshot.practice, Some(sample)),
                    Err(err) => {
                        warn!(error = %err, "Failed to enrich practice recommendation");
                        Self::mark_recommendation_error(&mut snapshot.practice);
                    }
                },
                Ok(None) => Self::apply_recommendation(&mut snapshot.practice, None),
                Err(err) => {
                    warn!(error = %err, "Failed to fetch practice recommendation");
                    Self::mark_recommendation_error(&mut snapshot.practice);
                }
            }

            match client.ext_lab(token).await {
                Ok(Some(recommendation)) => {
                    match self.build_lab_recommendation_sample(&recommendation).await {
                        Ok(sample) => Self::apply_recommendation(&mut snapshot.lab, Some(sample)),
                        Err(err) => {
                            warn!(error = %err, "Failed to enrich lab recommendation");
                            Self::mark_recommendation_error(&mut snapshot.lab);
                        }
                    }
                }
                Ok(None) => Self::apply_recommendation(&mut snapshot.lab, None),
                Err(err) => {
                    warn!(error = %err, "Failed to fetch lab recommendation");
                    Self::mark_recommendation_error(&mut snapshot.lab);
                }
            }
        }

        Ok(snapshot)
    }
}

impl DailyRewardActivityServiceImpl {
    fn build_lecture_recommendation_sample(
        &self,
        recommendation: &NextLectureRecommendation,
    ) -> Value {
        let metadata = LectureMetadata {
            course_title: recommendation.course.title.clone(),
            section_id: Some(recommendation.section.id.clone()),
            section_title: recommendation.section.title.clone(),
            lecture_title: recommendation.lecture.title.clone(),
        };

        let mut sample = build_lecture_sample(
            recommendation.course.id.clone(),
            recommendation.lecture.id.clone(),
            Some(metadata),
        );

        if let Value::Object(ref mut map) = sample
            && let Some(image) = &recommendation.course.image
        {
            map.insert("course_image".into(), Value::String(image.clone()));
            duplicate_string_field(map, "course_image", "courseImage");
        }

        sample
    }

    async fn build_task_recommendation_sample(
        &self,
        recommendation: &NextTaskRecommendation,
        is_lab: bool,
    ) -> Result<Value> {
        self.build_subtask_recommendation_sample(
            &recommendation.task.id,
            &recommendation.task.subtask_id,
            &recommendation.task.subtask_type,
            (
                &recommendation.course,
                &recommendation.section,
                &recommendation.lecture,
            ),
            is_lab,
        )
        .await
    }

    async fn build_lab_recommendation_sample(
        &self,
        recommendation: &NextLabRecommendation,
    ) -> Result<Value> {
        self.build_subtask_recommendation_sample(
            &recommendation.task.id,
            &recommendation.task.subtask_id,
            &recommendation.task.subtask_type,
            (
                &recommendation.course,
                &recommendation.section,
                &recommendation.lecture,
            ),
            true,
        )
        .await
    }

    async fn build_subtask_recommendation_sample(
        &self,
        task_id: &str,
        subtask_id: &str,
        subtask_type: &str,
        context: (
            &RecommendationCourse,
            &RecommendationSection,
            &RecommendationLecture,
        ),
        is_lab: bool,
    ) -> Result<Value> {
        let (course, section, lecture) = context;
        let metadata = self.fetch_challenge_metadata(task_id).await?;
        let ChallengeMetadata {
            challenge_title,
            category_id,
            skill_ids,
            course_id,
            section_id,
            lecture_id,
        } = metadata.unwrap_or_default();

        let challenge_title = challenge_title.or_else(|| lecture.title.clone());
        let course_id = course_id.or_else(|| Some(course.id.clone()));
        let section_id = section_id.or_else(|| Some(section.id.clone()));
        let lecture_id = lecture_id.or_else(|| Some(lecture.id.clone()));

        let mut sample = build_subtask_sample(
            SubtaskSampleInput {
                task_id: task_id.to_owned(),
                subtask_id: subtask_id.to_owned(),
                subtask_type: subtask_type.to_owned(),
                challenge_title,
                category_id,
                skill_ids,
                course_id,
                section_id,
                lecture_id,
            },
            is_lab,
        );

        if let Value::Object(ref mut map) = sample {
            if let Some(image) = &course.image {
                map.insert("course_image".into(), Value::String(image.clone()));
                duplicate_string_field(map, "course_image", "courseImage");
            }
            if let Some(title) = &course.title {
                map.insert("course_title".into(), Value::String(title.clone()));
                duplicate_string_field(map, "course_title", "courseTitle");
            }
            if let Some(section_title) = &section.title {
                map.insert("section_title".into(), Value::String(section_title.clone()));
                duplicate_string_field(map, "section_title", "sectionTitle");
            }
            if let Some(lecture_title) = &lecture.title {
                map.insert("lecture_title".into(), Value::String(lecture_title.clone()));
                duplicate_string_field(map, "lecture_title", "lectureTitle");
            }
        }

        Ok(sample)
    }

    async fn fetch_challenge_metadata(&self, task_id: &str) -> Result<Option<ChallengeMetadata>> {
        let Some(challenges) = &self.challenges else {
            return Ok(None);
        };

        let task_uuid = match Uuid::parse_str(task_id) {
            Ok(uuid) => uuid,
            Err(err) => {
                warn!(error = %err, %task_id, "Skipping challenge metadata; invalid task_id");
                return Ok(None);
            }
        };

        let conn = match challenges.connection().await {
            Ok(conn) => conn,
            Err(err) => {
                warn!(
                    error = %err,
                    "Failed to acquire challenges connection for recommendation metadata"
                );
                return Ok(None);
            }
        };

        let row = conn
            .query_opt(
                "select \
                    cc.title as challenge_title, \
                    cc.category_id, \
                    cc.skill_ids, \
                    cct.course_id::text as course_id, \
                    cct.section_id::text as section_id, \
                    cct.lecture_id::text as lecture_id \
                 from challenges_challenges cc \
                 left join challenges_course_tasks cct on cct.task_id = cc.task_id \
                 where cc.task_id = $1 \
                 limit 1",
                &[&task_uuid],
            )
            .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        let challenge_title = row
            .try_get::<_, Option<String>>("challenge_title")
            .unwrap_or(None)
            .and_then(normalize_db_string);
        let category_id = row
            .try_get::<_, Option<Uuid>>("category_id")
            .unwrap_or(None);
        let skill_ids: Vec<String> = match row.try_get::<_, Vec<String>>("skill_ids") {
            Ok(ids) => ids,
            Err(_) => row
                .try_get::<_, Vec<Uuid>>("skill_ids")
                .map(|ids| ids.into_iter().map(|id| id.to_string()).collect())
                .unwrap_or_default(),
        };
        let course_id = row
            .try_get::<_, Option<String>>("course_id")
            .unwrap_or(None)
            .and_then(normalize_db_string);
        let section_id = row
            .try_get::<_, Option<String>>("section_id")
            .unwrap_or(None)
            .and_then(normalize_db_string);
        let lecture_id = row
            .try_get::<_, Option<String>>("lecture_id")
            .unwrap_or(None)
            .and_then(normalize_db_string);

        Ok(Some(ChallengeMetadata {
            challenge_title,
            category_id,
            skill_ids,
            course_id,
            section_id,
            lecture_id,
        }))
    }

    fn apply_recommendation(state: &mut DailyRewardActivityState, sample: Option<Value>) {
        if state.detected.is_some() {
            // The user already completed the activity; do not override with pending data.
            return;
        }

        match sample {
            Some(sample) => {
                state.pending_sample = Some(sample);
                state.unavailable_reason = None;
            }
            None => {
                if state.unavailable_reason.is_none() {
                    state.unavailable_reason = Some(DailyRewardUnavailableReason::NoRecommendation);
                }
            }
        }
    }

    fn mark_recommendation_error(state: &mut DailyRewardActivityState) {
        if state.detected.is_some() {
            return;
        }
        if state.unavailable_reason.is_none() {
            state.unavailable_reason = Some(DailyRewardUnavailableReason::Unknown);
        }
    }
}
