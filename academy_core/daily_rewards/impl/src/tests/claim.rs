use std::future;

use academy_auth_contracts::{Authentication, MockAuthService};
use academy_core_coin_contracts::coin::MockCoinService;
use academy_core_daily_rewards_contracts::{
    DailyRewardActivitySnapshot, DailyRewardClaimError, DailyRewardFeatureService,
    MockDailyRewardActivityService,
};
use academy_models::{
    Sha256Hash,
    auth::AccessToken,
    coin::Balance,
    coin::TransactionDescription,
    daily_rewards::{DailyRewardCategory, DailyRewardEntry},
    session::{SessionId, SessionRefreshTokenHash},
    user::UserId,
};
use academy_persistence_contracts::{
    MockDatabase, MockTransaction,
    daily_rewards::{DailyRewardMarkClaimed, MockDailyRewardRepository},
};
use academy_shared_contracts::{id::MockIdService, time::MockTimeService};
use chrono::{DateTime, Duration as ChronoDuration, NaiveDate, TimeZone, Utc};
use mockall::predicate;
use uuid::Uuid;

use crate::DailyRewardFeatureServiceImpl;

fn make_entry(
    category: DailyRewardCategory,
    coins: i32,
    date: NaiveDate,
    claimable_since: Option<DateTime<Utc>>,
    claimed_at: Option<DateTime<Utc>>,
) -> DailyRewardEntry {
    DailyRewardEntry {
        id: Uuid::new_v4(),
        user_id: UserId::from(Uuid::new_v4()),
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

fn activity_service() -> MockDailyRewardActivityService {
    let mut activity = MockDailyRewardActivityService::new();
    activity
        .expect_detect()
        .once()
        .return_once(|_, _, _| Box::pin(future::ready(Ok(DailyRewardActivitySnapshot::default()))));
    activity
}

fn base_entries(user_id: UserId, date: NaiveDate, now: DateTime<Utc>) -> [DailyRewardEntry; 4] {
    [
        DailyRewardEntry {
            user_id,
            ..make_entry(
                DailyRewardCategory::Arrival,
                20,
                date,
                Some(now - ChronoDuration::hours(1)),
                None,
            )
        },
        DailyRewardEntry {
            user_id,
            ..make_entry(
                DailyRewardCategory::Lecture,
                20,
                date,
                Some(now - ChronoDuration::minutes(10)),
                None,
            )
        },
        DailyRewardEntry {
            user_id,
            ..make_entry(DailyRewardCategory::Practice, 10, date, None, None)
        },
        DailyRewardEntry {
            user_id,
            ..make_entry(DailyRewardCategory::Lab, 30, date, None, None)
        },
    ]
}

#[tokio::test]
async fn claim_success() {
    // Arrange
    let user_id = UserId::from(Uuid::new_v4());
    let date = NaiveDate::from_ymd_opt(2025, 11, 1).unwrap();
    let now = Utc.with_ymd_and_hms(2025, 11, 1, 12, 0, 0).unwrap();

    let entries_array = base_entries(user_id, date, now);
    let lecture_entry = entries_array[1].clone();
    let entries_vec = Vec::from(entries_array);

    let mut repo = MockDailyRewardRepository::new();
    repo.expect_list_by_user_and_date()
        .once()
        .with(
            predicate::always(),
            predicate::eq(user_id),
            predicate::eq(date),
        )
        .return_once(move |_, _, _| Box::pin(future::ready(Ok(entries_vec))));

    let lecture_for_claim = lecture_entry.clone();
    repo.expect_mark_claimed()
        .once()
        .with(
            predicate::always(),
            predicate::function(move |params: &DailyRewardMarkClaimed| {
                params.user_id == user_id
                    && params.category == DailyRewardCategory::Lecture
                    && params.date_utc == date
            }),
        )
        .return_once(move |_, params| {
            let mut updated = lecture_for_claim;
            updated.claimed_at = Some(params.claimed_at);
            Box::pin(future::ready(Ok(updated)))
        });

    let description =
        TransactionDescription::try_from("Daily reward - lecture".to_string()).unwrap();
    let coin = MockCoinService::new().with_add_coins(
        user_id,
        20,
        false,
        Some(description),
        false,
        Ok(Balance::default()),
    );

    let db = MockDatabase::build(true);
    let auth = auth_service(user_id);
    let activity = activity_service();
    let time = MockTimeService::new().with_now(now);

    let sut = DailyRewardFeatureServiceImpl::new_for_tests(
        db,
        auth,
        repo,
        coin,
        activity,
        MockIdService::new(),
        time,
        Default::default(),
    );

    // Act
    let result = sut
        .claim(&AccessToken::new("token"), DailyRewardCategory::Lecture)
        .await
        .unwrap();

    // Assert
    assert_eq!(result.success.category, DailyRewardCategory::Lecture);
    assert_eq!(result.success.coins, 20);
    assert_eq!(result.success.claimed_at, now);
}

#[tokio::test]
async fn claim_already_claimed() {
    // Arrange
    let user_id = UserId::from(Uuid::new_v4());
    let date = NaiveDate::from_ymd_opt(2025, 11, 1).unwrap();
    let now = Utc.with_ymd_and_hms(2025, 11, 1, 12, 0, 0).unwrap();

    let mut entries = base_entries(user_id, date, now);
    entries[1].claimed_at = Some(now - ChronoDuration::minutes(5));
    let entries_vec = Vec::from(entries);

    let mut repo = MockDailyRewardRepository::new();
    repo.expect_list_by_user_and_date()
        .once()
        .with(
            predicate::always(),
            predicate::eq(user_id),
            predicate::eq(date),
        )
        .return_once(move |_, _, _| Box::pin(future::ready(Ok(entries_vec))));

    let db = MockDatabase::build(false);
    let auth = auth_service(user_id);
    let activity = activity_service();
    let time = MockTimeService::new().with_now(now);

    let coin = MockCoinService::new();
    let sut = DailyRewardFeatureServiceImpl::new_for_tests(
        db,
        auth,
        repo,
        coin,
        activity,
        MockIdService::new(),
        time,
        Default::default(),
    );

    // Act
    let result = sut
        .claim(&AccessToken::new("token"), DailyRewardCategory::Lecture)
        .await;

    // Assert
    assert!(matches!(result, Err(DailyRewardClaimError::AlreadyClaimed)));
}
