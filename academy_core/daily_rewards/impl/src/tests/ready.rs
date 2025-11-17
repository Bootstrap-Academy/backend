use std::{
    collections::HashMap,
    future,
    sync::{Arc, Mutex},
};

use academy_auth_contracts::{Authentication, MockAuthService};
use academy_cache_contracts::MockCacheService;
use academy_core_coin_contracts::coin::MockCoinService;
use academy_core_daily_rewards_contracts::{
    DailyRewardActivity, DailyRewardActivitySnapshot, DailyRewardActivityState,
    DailyRewardFeatureService, DailyRewardStatus, MockDailyRewardActivityService,
};
use academy_models::{
    Sha256Hash,
    auth::AccessToken,
    daily_rewards::{DailyRewardCategory, DailyRewardEntry},
    session::{SessionId, SessionRefreshTokenHash},
    user::UserId,
};
use academy_persistence_contracts::{
    MockDatabase, MockTransaction,
    daily_rewards::{DailyRewardMarkReady, MockDailyRewardRepository},
};
use academy_shared_contracts::{id::MockIdService, time::MockTimeService};
use chrono::{Duration as ChronoDuration, NaiveDate, TimeZone, Utc};
use mockall::predicate;
use serde_json::json;
use uuid::Uuid;

use crate::DailyRewardFeatureServiceImpl;

fn auth_service(user_id: UserId) -> MockAuthService<MockTransaction> {
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

fn entry(
    user_id: UserId,
    date: NaiveDate,
    category: DailyRewardCategory,
    coins: i32,
    claimable_since: Option<chrono::DateTime<Utc>>,
    claimed_at: Option<chrono::DateTime<Utc>>,
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
        claimed_at,
        activity_sample: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

#[tokio::test]
async fn get_today_marks_practice_and_lab_ready() {
    let user_id = UserId::from(Uuid::new_v4());
    let date = NaiveDate::from_ymd_opt(2025, 11, 2).unwrap();
    let now = Utc.with_ymd_and_hms(2025, 11, 2, 12, 0, 0).unwrap();
    let practice_ready_at = now - ChronoDuration::minutes(15);
    let lab_ready_at = now - ChronoDuration::minutes(5);

    let arrival_entry = entry(
        user_id,
        date,
        DailyRewardCategory::Arrival,
        20,
        Some(now - ChronoDuration::hours(1)),
        None,
    );
    let lecture_entry = entry(user_id, date, DailyRewardCategory::Lecture, 20, None, None);
    let practice_entry = entry(user_id, date, DailyRewardCategory::Practice, 10, None, None);
    let lab_entry = entry(user_id, date, DailyRewardCategory::Lab, 30, None, None);

    let entries_map: HashMap<DailyRewardCategory, DailyRewardEntry> = [
        (DailyRewardCategory::Arrival, arrival_entry),
        (DailyRewardCategory::Lecture, lecture_entry),
        (DailyRewardCategory::Practice, practice_entry),
        (DailyRewardCategory::Lab, lab_entry),
    ]
    .into_iter()
    .collect();
    let shared_entries = Arc::new(Mutex::new(entries_map));

    let mut repo = MockDailyRewardRepository::new();
    let list_entries = Arc::clone(&shared_entries);
    repo.expect_list_by_user_and_date()
        .once()
        .with(
            predicate::always(),
            predicate::eq(user_id),
            predicate::eq(date),
        )
        .return_once(move |_, _, _| {
            let entries = list_entries
                .lock()
                .unwrap()
                .values()
                .cloned()
                .collect::<Vec<_>>();
            Box::pin(future::ready(Ok(entries)))
        });

    repo.expect_upsert_entry().never();

    let mark_ready_entries = Arc::clone(&shared_entries);
    repo.expect_mark_ready()
        .times(2)
        .returning(move |_, params: DailyRewardMarkReady| {
            let mut guard = mark_ready_entries.lock().unwrap();
            let mut entry = guard.get(&params.category).cloned().unwrap();
            entry.first_detected_at = params.first_detected_at;
            entry.last_detected_at = params.last_detected_at;
            entry.claimable_since = params.claimable_since;
            entry.activity_sample = params.activity_sample.clone();
            guard.insert(params.category, entry.clone());
            Box::pin(future::ready(Ok(entry)))
        });

    let mut activity = MockDailyRewardActivityService::new();
    activity
        .expect_detect()
        .once()
        .withf(move |token, detected_user, _, _| token.is_some() && *detected_user == user_id)
        .return_once(move |_, _, _, _| {
            let practice_activity = DailyRewardActivity {
                first_detected_at: practice_ready_at,
                last_detected_at: practice_ready_at,
                activity_sample: Some(json!({"taskId": "practice"})),
            };
            let lab_activity = DailyRewardActivity {
                first_detected_at: lab_ready_at,
                last_detected_at: lab_ready_at,
                activity_sample: Some(json!({"taskId": "lab"})),
            };
            let snapshot = DailyRewardActivitySnapshot {
                practice: DailyRewardActivityState {
                    detected: Some(practice_activity),
                    pending_sample: None,
                    unavailable_reason: None,
                },
                lab: DailyRewardActivityState {
                    detected: Some(lab_activity),
                    pending_sample: None,
                    unavailable_reason: None,
                },
                ..Default::default()
            };
            Box::pin(future::ready(Ok(snapshot)))
        });

    let db = MockDatabase::build(true);
    let auth = auth_service(user_id);
    let coin = MockCoinService::new();
    let cache = MockCacheService::new();
    let ids = MockIdService::new();
    let time = MockTimeService::new().with_now(now);

    let sut = DailyRewardFeatureServiceImpl::new_for_tests(
        db,
        auth,
        repo,
        coin,
        cache,
        activity,
        ids,
        time,
        Default::default(),
    );

    let response = sut
        .get_today(&AccessToken::new("token"))
        .await
        .expect("get_today should succeed");

    let practice_reward = response
        .snapshot
        .rewards
        .iter()
        .find(|reward| reward.category == DailyRewardCategory::Practice)
        .expect("practice reward present");
    assert_eq!(practice_reward.status, DailyRewardStatus::Ready);
    assert_eq!(practice_reward.claimable_since, Some(practice_ready_at));

    let lab_reward = response
        .snapshot
        .rewards
        .iter()
        .find(|reward| reward.category == DailyRewardCategory::Lab)
        .expect("lab reward present");
    assert_eq!(lab_reward.status, DailyRewardStatus::Ready);
    assert_eq!(lab_reward.claimable_since, Some(lab_ready_at));

    assert_eq!(response.snapshot.claim_totals.available_coins, 60);
}
