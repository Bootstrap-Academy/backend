use std::future;

use academy_auth_contracts::{Authentication, MockAuthService};
use academy_cache_contracts::MockCacheService;
use academy_core_coin_contracts::coin::MockCoinService;
use academy_core_daily_rewards_contracts::{
    DailyRewardActivity, DailyRewardActivitySnapshot, DailyRewardActivityState,
    DailyRewardCategory, DailyRewardFeatureService, DailyRewardGetResponse, DailyRewardStatus,
    MockDailyRewardActivityService,
};
use academy_models::{
    Sha256Hash,
    auth::AccessToken,
    daily_rewards::DailyRewardEntry,
    session::{SessionId, SessionRefreshTokenHash},
    user::UserId,
};
use academy_persistence_contracts::{
    MockDatabase, MockTransaction,
    daily_rewards::{DailyRewardMarkReady, MockDailyRewardRepository},
};
use academy_shared_contracts::{id::MockIdService, time::MockTimeService};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use mockall::predicate;
use serde_json::json;
use uuid::Uuid;

use crate::{DailyRewardFeatureConfig, DailyRewardFeatureServiceImpl};

fn make_entry(
    user_id: UserId,
    category: DailyRewardCategory,
    date: NaiveDate,
    coins: i32,
    claimable_since: Option<DateTime<Utc>>,
) -> DailyRewardEntry {
    DailyRewardEntry {
        id: Uuid::new_v4(),
        user_id,
        date_utc: date,
        category,
        coins,
        first_detected_at: claimable_since,
        last_detected_at: claimable_since,
        claimable_since,
        claimed_at: None,
        activity_sample: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn mock_auth(user_id: UserId) -> MockAuthService<MockTransaction> {
    let mut auth = MockAuthService::new();
    auth.expect_authenticate()
        .once()
        .with(predicate::eq(AccessToken::new("token")))
        .return_once(move |_| {
            Box::pin(future::ready(Ok(Authentication {
                user_id,
                session_id: SessionId::from(Uuid::new_v4()),
                refresh_token_hash: SessionRefreshTokenHash::new(Sha256Hash::default()),
                admin: false,
                email_verified: true,
            })))
        });
    auth
}

#[tokio::test]
async fn get_today_marks_practice_and_lab_ready_when_detected() {
    let user_id = UserId::from(Uuid::new_v4());
    let now = Utc.with_ymd_and_hms(2025, 11, 4, 15, 0, 0).unwrap();
    let date = now.date_naive();
    let day_start = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let day_end = day_start + chrono::Duration::days(1);

    let practice_entry = make_entry(user_id, DailyRewardCategory::Practice, date, 10, None);
    let lab_entry = make_entry(user_id, DailyRewardCategory::Lab, date, 30, None);

    let arrival_ready_at = now - chrono::Duration::hours(3);
    let arrival_entry = make_entry(
        user_id,
        DailyRewardCategory::Arrival,
        date,
        20,
        Some(arrival_ready_at),
    );

    let lecture_entry = make_entry(
        user_id,
        DailyRewardCategory::Lecture,
        date,
        20,
        Some(now - chrono::Duration::hours(1)),
    );

    let base_entries = vec![
        arrival_entry.clone(),
        lecture_entry.clone(),
        practice_entry.clone(),
        lab_entry.clone(),
    ];

    let practice_sample = json!({ "task_id": "practice-task" });
    let lab_sample = json!({ "task_id": "lab-task" });

    let practice_first = now - chrono::Duration::minutes(20);
    let practice_last = now - chrono::Duration::minutes(5);
    let lab_first = now - chrono::Duration::minutes(10);
    let lab_last = now - chrono::Duration::minutes(2);

    let activity_snapshot = DailyRewardActivitySnapshot {
        lecture: DailyRewardActivityState::default(),
        practice: DailyRewardActivityState {
            detected: Some(DailyRewardActivity {
                first_detected_at: practice_first,
                last_detected_at: practice_last,
                activity_sample: Some(practice_sample.clone()),
            }),
            pending_sample: None,
            unavailable_reason: None,
        },
        lab: DailyRewardActivityState {
            detected: Some(DailyRewardActivity {
                first_detected_at: lab_first,
                last_detected_at: lab_last,
                activity_sample: Some(lab_sample.clone()),
            }),
            pending_sample: None,
            unavailable_reason: None,
        },
    };

    let mut repo = MockDailyRewardRepository::new();
    {
        let entries = base_entries.clone();
        repo.expect_list_by_user_and_date()
            .once()
            .with(
                predicate::always(),
                predicate::eq(user_id),
                predicate::eq(date),
            )
            .return_once(move |_, _, _| Box::pin(future::ready(Ok(entries))));
    }

    {
        let mut updated_practice = practice_entry.clone();
        let practice_expect_sample = practice_sample.clone();
        repo.expect_mark_ready()
            .with(
                predicate::always(),
                predicate::function(move |params: &DailyRewardMarkReady| {
                    params.user_id == user_id
                        && params.category == DailyRewardCategory::Practice
                        && params.date_utc == date
                        && params.first_detected_at == Some(practice_first)
                        && params.last_detected_at == Some(practice_last)
                        && params.claimable_since == Some(practice_first)
                        && params.activity_sample == Some(practice_expect_sample.clone())
                }),
            )
            .return_once(move |_, params| {
                updated_practice.first_detected_at = params.first_detected_at;
                updated_practice.last_detected_at = params.last_detected_at;
                updated_practice.claimable_since = params.claimable_since;
                updated_practice.activity_sample = params.activity_sample.clone();
                Box::pin(future::ready(Ok(updated_practice)))
            });
    }

    {
        let mut updated_lab = lab_entry.clone();
        let lab_expect_sample = lab_sample.clone();
        repo.expect_mark_ready()
            .with(
                predicate::always(),
                predicate::function(move |params: &DailyRewardMarkReady| {
                    params.user_id == user_id
                        && params.category == DailyRewardCategory::Lab
                        && params.date_utc == date
                        && params.first_detected_at == Some(lab_first)
                        && params.last_detected_at == Some(lab_last)
                        && params.claimable_since == Some(lab_first)
                        && params.activity_sample == Some(lab_expect_sample.clone())
                }),
            )
            .return_once(move |_, params| {
                updated_lab.first_detected_at = params.first_detected_at;
                updated_lab.last_detected_at = params.last_detected_at;
                updated_lab.claimable_since = params.claimable_since;
                updated_lab.activity_sample = params.activity_sample.clone();
                Box::pin(future::ready(Ok(updated_lab)))
            });
    }

    repo.expect_upsert_entry().never();
    repo.expect_mark_claimed().never();

    let mut activity = MockDailyRewardActivityService::new();
    activity
        .expect_detect()
        .once()
        .withf(move |token, user, start, end| {
            token.is_some() && user == &user_id && start == &day_start && end == &day_end
        })
        .return_once(move |_, _, _, _| Box::pin(future::ready(Ok(activity_snapshot))));

    let db = MockDatabase::build(true);
    let auth = mock_auth(user_id);
    let coin = MockCoinService::new();
    let cache = MockCacheService::new();
    let time = MockTimeService::new().with_now(now);

    let sut = DailyRewardFeatureServiceImpl::new_for_tests(
        db,
        auth,
        repo,
        coin,
        cache,
        activity,
        MockIdService::new(),
        time,
        DailyRewardFeatureConfig::default(),
    );

    let response: DailyRewardGetResponse = sut
        .get_today(&AccessToken::new("token"))
        .await
        .expect("get_today succeeds");

    let practice_reward = response
        .snapshot
        .rewards
        .iter()
        .find(|reward| reward.category == DailyRewardCategory::Practice)
        .expect("practice reward present");
    let lab_reward = response
        .snapshot
        .rewards
        .iter()
        .find(|reward| reward.category == DailyRewardCategory::Lab)
        .expect("lab reward present");

    assert_eq!(practice_reward.status, DailyRewardStatus::Ready);
    assert_eq!(lab_reward.status, DailyRewardStatus::Ready);
    assert_eq!(practice_reward.claimable_since, Some(practice_first));
    assert_eq!(lab_reward.claimable_since, Some(lab_first));
    assert_eq!(
        practice_reward.activity_sample,
        Some(practice_sample.clone())
    );
    assert_eq!(lab_reward.activity_sample, Some(lab_sample.clone()));
}
