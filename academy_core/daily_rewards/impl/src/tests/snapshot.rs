use std::collections::HashMap;

use academy_core_daily_rewards_contracts::{DailyRewardStatus, DailyRewardUnavailableReason};
use academy_models::daily_rewards::{DailyRewardCategory, DailyRewardEntry};
use chrono::{NaiveDate, TimeZone, Utc};
use uuid::Uuid;

use crate::service::{RefreshedRewards, build_snapshot};

fn entry(
    category: DailyRewardCategory,
    coins: i32,
    date: NaiveDate,
    claimable_since: Option<chrono::DateTime<Utc>>,
    claimed_at: Option<chrono::DateTime<Utc>>,
) -> DailyRewardEntry {
    DailyRewardEntry {
        id: Uuid::new_v4(),
        user_id: Uuid::new_v4().into(),
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

#[test]
fn build_snapshot_assigns_statuses_and_totals() {
    let date = NaiveDate::from_ymd_opt(2025, 11, 1).unwrap();
    let ready_time = Utc.with_ymd_and_hms(2025, 11, 1, 7, 15, 0).unwrap();
    let claimed_time = Utc.with_ymd_and_hms(2025, 11, 1, 6, 0, 0).unwrap();

    let mut entries = HashMap::new();
    entries.insert(
        DailyRewardCategory::Arrival,
        entry(DailyRewardCategory::Arrival, 20, date, None, None),
    );
    entries.insert(
        DailyRewardCategory::Lecture,
        entry(
            DailyRewardCategory::Lecture,
            20,
            date,
            Some(claimed_time - chrono::Duration::minutes(5)),
            Some(claimed_time),
        ),
    );
    entries.insert(
        DailyRewardCategory::Practice,
        entry(DailyRewardCategory::Practice, 10, date, None, None),
    );
    entries.insert(
        DailyRewardCategory::Lab,
        entry(DailyRewardCategory::Lab, 30, date, Some(ready_time), None),
    );

    let mut unavailability = HashMap::new();
    unavailability.insert(DailyRewardCategory::Arrival, None);
    unavailability.insert(DailyRewardCategory::Lecture, None);
    unavailability.insert(
        DailyRewardCategory::Practice,
        Some(DailyRewardUnavailableReason::NoRecommendation),
    );
    unavailability.insert(DailyRewardCategory::Lab, None);

    let refreshed = RefreshedRewards {
        entries,
        unavailability,
    };

    let snapshot = build_snapshot(date, refreshed);

    let arrival = snapshot
        .rewards
        .iter()
        .find(|reward| reward.category == DailyRewardCategory::Arrival)
        .unwrap();
    let lecture = snapshot
        .rewards
        .iter()
        .find(|reward| reward.category == DailyRewardCategory::Lecture)
        .unwrap();
    let practice = snapshot
        .rewards
        .iter()
        .find(|reward| reward.category == DailyRewardCategory::Practice)
        .unwrap();
    let lab = snapshot
        .rewards
        .iter()
        .find(|reward| reward.category == DailyRewardCategory::Lab)
        .unwrap();

    assert_eq!(arrival.status, DailyRewardStatus::Pending);
    assert_eq!(lecture.status, DailyRewardStatus::Claimed);
    assert_eq!(practice.status, DailyRewardStatus::Unavailable);
    assert_eq!(lab.status, DailyRewardStatus::Ready);
    assert_eq!(snapshot.claim_totals.available_coins, 30);
    assert_eq!(snapshot.claim_totals.claimed_today, 20);
}

#[test]
fn build_snapshot_uses_entry_coin_amounts() {
    let date = NaiveDate::from_ymd_opt(2025, 11, 2).unwrap();
    let ready_time = Utc.with_ymd_and_hms(2025, 11, 2, 9, 0, 0).unwrap();
    let claimed_time = Utc.with_ymd_and_hms(2025, 11, 2, 8, 0, 0).unwrap();

    let mut entries = HashMap::new();
    entries.insert(
        DailyRewardCategory::Arrival,
        entry(
            DailyRewardCategory::Arrival,
            20,
            date,
            Some(claimed_time - chrono::Duration::minutes(15)),
            Some(claimed_time),
        ),
    );
    entries.insert(
        DailyRewardCategory::Lab,
        entry(DailyRewardCategory::Lab, 7, date, Some(ready_time), None),
    );

    let mut unavailability = HashMap::new();
    unavailability.insert(DailyRewardCategory::Arrival, None);
    unavailability.insert(DailyRewardCategory::Lab, None);

    let refreshed = RefreshedRewards {
        entries,
        unavailability,
    };

    let snapshot = build_snapshot(date, refreshed);

    let arrival = snapshot
        .rewards
        .iter()
        .find(|reward| reward.category == DailyRewardCategory::Arrival)
        .unwrap();
    let lab = snapshot
        .rewards
        .iter()
        .find(|reward| reward.category == DailyRewardCategory::Lab)
        .unwrap();

    assert_eq!(arrival.coins, 20);
    assert_eq!(lab.coins, 7);
    assert_eq!(snapshot.claim_totals.claimed_today, 20);
    assert_eq!(snapshot.claim_totals.available_coins, 7);
}
