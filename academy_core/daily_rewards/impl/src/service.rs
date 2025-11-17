use std::{collections::HashMap, time::Duration};

use academy_auth_contracts::{AuthResultExt, AuthService};
use academy_cache_contracts::CacheService;
use academy_core_coin_contracts::coin::CoinService;
use academy_core_daily_rewards_contracts::{
    DailyRewardActivityService, DailyRewardActivitySnapshot, DailyRewardActivityState,
    DailyRewardClaimAllError, DailyRewardClaimAllResponse, DailyRewardClaimError,
    DailyRewardClaimResponse, DailyRewardClaimSkip, DailyRewardClaimSkipReason,
    DailyRewardClaimSuccess, DailyRewardFeatureService, DailyRewardGetError,
    DailyRewardGetResponse, DailyRewardItem, DailyRewardStatus, DailyRewardUnavailableReason,
    DailyRewardsSnapshot,
};
use academy_di::Build;
use academy_models::{
    auth::AccessToken,
    coin::TransactionDescription,
    daily_rewards::{DailyRewardCategory, DailyRewardEntry},
    user::UserId,
};
use academy_persistence_contracts::{
    Database, Transaction,
    daily_rewards::{
        DailyRewardEntryUpsert, DailyRewardMarkClaimed, DailyRewardMarkClaimedError,
        DailyRewardMarkReady, DailyRewardRepository,
    },
};
use academy_shared_contracts::{id::IdService, time::TimeService};
use academy_utils::trace_instrument;
use anyhow::{Result, anyhow};
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct DailyRewardCoinsConfig {
    pub arrival: i32,
    pub lecture: i32,
    pub practice: i32,
    pub lab: i32,
}

impl DailyRewardCoinsConfig {
    fn get(&self, category: DailyRewardCategory) -> i32 {
        match category {
            DailyRewardCategory::Arrival => self.arrival,
            DailyRewardCategory::Lecture => self.lecture,
            DailyRewardCategory::Practice => self.practice,
            DailyRewardCategory::Lab => self.lab,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DailyRewardFeatureConfig {
    pub enable: bool,
    pub coins: DailyRewardCoinsConfig,
    pub cache_ttl: Option<Duration>,
}

#[derive(Debug, Clone, Build)]
pub struct DailyRewardFeatureServiceImpl<Db, Auth, Repo, Coin, Cache, Activity, Id, Time> {
    db: Db,
    auth: Auth,
    repo: Repo,
    coin: Coin,
    cache: Cache,
    activity: Activity,
    id: Id,
    time: Time,
    config: DailyRewardFeatureConfig,
}

pub(crate) struct RefreshedRewards {
    pub(crate) entries: HashMap<DailyRewardCategory, DailyRewardEntry>,
    pub(crate) unavailability: HashMap<DailyRewardCategory, Option<DailyRewardUnavailableReason>>,
}

impl<Db, Auth, Repo, Coin, Cache, Activity, Id, Time> DailyRewardFeatureService
    for DailyRewardFeatureServiceImpl<Db, Auth, Repo, Coin, Cache, Activity, Id, Time>
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    Repo: DailyRewardRepository<Db::Transaction>,
    Coin: CoinService<Db::Transaction>,
    Cache: CacheService,
    Activity: DailyRewardActivityService,
    Id: IdService,
    Time: TimeService,
{
    #[trace_instrument(skip(self))]
    async fn get_today(
        &self,
        token: &AccessToken,
    ) -> Result<DailyRewardGetResponse, DailyRewardGetError> {
        if !self.config.enable {
            return Err(DailyRewardGetError::FeatureDisabled);
        }

        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = auth.user_id;

        let now = self.time.now();
        let date = now.date_naive();
        let cache_ttl = self.config.cache_ttl;
        let cache_key = cache_ttl.map(|_| cache_key(user_id, date));

        let mut cached_snapshot = None;

        if let Some(cache_key) = cache_key.as_ref() {
            match self.cache.get::<DailyRewardsSnapshot>(cache_key).await {
                Ok(Some(snapshot)) => {
                    cached_snapshot = Some(snapshot);
                }
                Ok(None) => {}
                Err(err) => {
                    warn!(
                        error = %err,
                        user_id = ?user_id,
                        date = %date,
                        "Failed to retrieve daily rewards snapshot from cache"
                    );
                }
            }
        }

        let day_start = start_of_day_utc(date);
        let day_end = day_start + chrono::Duration::days(1);

        let mut txn = self
            .db
            .begin_transaction()
            .await
            .map_err(DailyRewardGetError::Other)?;

        let refreshed = match self
            .refresh_entries(&mut txn, Some(token), user_id, date, day_start, day_end)
            .await
        {
            Ok(refreshed) => refreshed,
            Err(err) => {
                if let Some(snapshot) = cached_snapshot {
                    warn!(
                        error = %err,
                        user_id = ?user_id,
                        date = %date,
                        "Failed to refresh daily rewards; returning cached snapshot"
                    );
                    if let Err(rollback_err) = txn.rollback().await {
                        warn!(
                            error = %rollback_err,
                            user_id = ?user_id,
                            date = %date,
                            "Failed to rollback transaction after refresh failure"
                        );
                    }
                    self.emit_view_event(user_id, &snapshot);
                    return Ok(DailyRewardGetResponse { snapshot });
                }
                return Err(DailyRewardGetError::Other(err));
            }
        };

        txn.commit().await.map_err(DailyRewardGetError::Other)?;

        let snapshot = build_snapshot(date, refreshed);

        if let (Some(ttl), Some(cache_key)) = (cache_ttl, cache_key.as_ref())
            && let Err(err) = self.cache.set(cache_key, &snapshot, Some(ttl)).await
        {
            warn!(
                error = %err,
                user_id = ?user_id,
                date = %date,
                "Failed to store daily rewards snapshot in cache"
            );
        }

        self.emit_view_event(user_id, &snapshot);

        Ok(DailyRewardGetResponse { snapshot })
    }

    #[trace_instrument(skip(self))]
    async fn claim(
        &self,
        token: &AccessToken,
        category: DailyRewardCategory,
    ) -> Result<DailyRewardClaimResponse, DailyRewardClaimError> {
        if !self.config.enable {
            return Err(DailyRewardClaimError::FeatureDisabled);
        }

        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = auth.user_id;

        let now = self.time.now();
        let date = now.date_naive();
        let day_start = start_of_day_utc(date);
        let day_end = day_start + chrono::Duration::days(1);
        let coins = self.config.coins.get(category);

        let mut txn = self
            .db
            .begin_transaction()
            .await
            .map_err(DailyRewardClaimError::Other)?;

        let refreshed = self
            .refresh_entries(&mut txn, Some(token), user_id, date, day_start, day_end)
            .await
            .map_err(DailyRewardClaimError::Other)?;

        let entry = refreshed.entries.get(&category).ok_or_else(|| {
            DailyRewardClaimError::Other(anyhow!("Reward entry missing for category {category}"))
        })?;

        if entry.claimed_at.is_some() {
            return Err(DailyRewardClaimError::AlreadyClaimed);
        }

        if entry.claimable_since.is_none() {
            if refreshed
                .unavailability
                .get(&category)
                .and_then(|r| *r)
                .is_some()
            {
                return Err(DailyRewardClaimError::Unavailable);
            }
            return Err(DailyRewardClaimError::NotReady);
        }

        let mark_params = DailyRewardMarkClaimed {
            user_id,
            date_utc: date,
            category,
            claimed_at: now,
        };

        let updated_entry = self
            .repo
            .mark_claimed(&mut txn, mark_params)
            .await
            .map_err(|err| map_claim_error(err, category))?;

        let description = TransactionDescription::try_from(format!("Daily reward - {}", category))
            .map(Some)
            .map_err(|err| DailyRewardClaimError::Other(err.into()))?;

        self.coin
            .add_coins(&mut txn, user_id, coins as i64, false, description, false)
            .await
            .map_err(|err| DailyRewardClaimError::Other(err.into()))?;

        let claimed_entry = updated_entry.clone();
        let claimed_at = claimed_entry.claimed_at.unwrap_or(now);

        txn.commit().await.map_err(DailyRewardClaimError::Other)?;

        self.invalidate_cache(user_id, date).await;

        self.emit_claim_event(user_id, category, coins, claimed_at, &claimed_entry);

        Ok(DailyRewardClaimResponse {
            success: DailyRewardClaimSuccess {
                category,
                coins,
                claimed_at,
            },
        })
    }

    #[trace_instrument(skip(self))]
    async fn claim_all(
        &self,
        token: &AccessToken,
    ) -> Result<DailyRewardClaimAllResponse, DailyRewardClaimAllError> {
        if !self.config.enable {
            return Err(DailyRewardClaimAllError::FeatureDisabled);
        }

        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = auth.user_id;

        let now = self.time.now();
        let date = now.date_naive();
        let day_start = start_of_day_utc(date);
        let day_end = day_start + chrono::Duration::days(1);

        let mut txn = self
            .db
            .begin_transaction()
            .await
            .map_err(DailyRewardClaimAllError::Other)?;

        let refreshed = self
            .refresh_entries(&mut txn, Some(token), user_id, date, day_start, day_end)
            .await
            .map_err(DailyRewardClaimAllError::Other)?;

        let mut claimed = Vec::new();
        let mut skipped = Vec::new();
        let mut claimed_entries = Vec::new();

        for category in DailyRewardCategory::ALL {
            let coins = self.config.coins.get(category);
            let entry = match refreshed.entries.get(&category) {
                Some(entry) => entry,
                None => {
                    skipped.push(DailyRewardClaimSkip {
                        category,
                        reason: DailyRewardClaimSkipReason::Error,
                    });
                    continue;
                }
            };

            if entry.claimed_at.is_some() {
                skipped.push(DailyRewardClaimSkip {
                    category,
                    reason: DailyRewardClaimSkipReason::AlreadyClaimed,
                });
                continue;
            }

            if entry.claimable_since.is_none() {
                let reason = refreshed
                    .unavailability
                    .get(&category)
                    .and_then(|r| *r)
                    .map(|_| DailyRewardClaimSkipReason::Unavailable)
                    .unwrap_or(DailyRewardClaimSkipReason::Pending);

                skipped.push(DailyRewardClaimSkip { category, reason });
                continue;
            }

            let mark_params = DailyRewardMarkClaimed {
                user_id,
                date_utc: date,
                category,
                claimed_at: now,
            };

            match self.repo.mark_claimed(&mut txn, mark_params).await {
                Ok(updated_entry) => {
                    let description =
                        TransactionDescription::try_from(format!("Daily reward - {}", category))
                            .map(Some)
                            .map_err(|err| DailyRewardClaimAllError::Other(err.into()))?;

                    if let Err(err) = self
                        .coin
                        .add_coins(&mut txn, user_id, coins as i64, false, description, false)
                        .await
                    {
                        return Err(DailyRewardClaimAllError::Other(err.into()));
                    }

                    let claimed_at = updated_entry.claimed_at.unwrap_or(now);
                    claimed.push(DailyRewardClaimSuccess {
                        category,
                        coins,
                        claimed_at,
                    });
                    claimed_entries.push((category, coins, claimed_at, updated_entry));
                }
                Err(DailyRewardMarkClaimedError::NotReady) => {
                    skipped.push(DailyRewardClaimSkip {
                        category,
                        reason: DailyRewardClaimSkipReason::Pending,
                    });
                }
                Err(DailyRewardMarkClaimedError::AlreadyClaimed) => {
                    skipped.push(DailyRewardClaimSkip {
                        category,
                        reason: DailyRewardClaimSkipReason::AlreadyClaimed,
                    });
                }
                Err(DailyRewardMarkClaimedError::NotFound) => {
                    skipped.push(DailyRewardClaimSkip {
                        category,
                        reason: DailyRewardClaimSkipReason::Error,
                    });
                }
                Err(DailyRewardMarkClaimedError::Other(err)) => {
                    return Err(DailyRewardClaimAllError::Other(err));
                }
            }
        }

        txn.commit()
            .await
            .map_err(DailyRewardClaimAllError::Other)?;

        if !claimed_entries.is_empty() {
            self.invalidate_cache(user_id, date).await;
        }

        for (category, coins, claimed_at, entry) in &claimed_entries {
            self.emit_claim_event(user_id, *category, *coins, *claimed_at, entry);
        }
        self.emit_claim_all_event(user_id, &claimed, &skipped);

        Ok(DailyRewardClaimAllResponse { claimed, skipped })
    }
}

impl<Db, Auth, Repo, Coin, Cache, Activity, Id, Time>
    DailyRewardFeatureServiceImpl<Db, Auth, Repo, Coin, Cache, Activity, Id, Time>
where
    Db: Database,
    Repo: DailyRewardRepository<Db::Transaction>,
    Cache: CacheService,
    Activity: DailyRewardActivityService,
    Id: IdService,
    Time: TimeService,
{
    fn emit_view_event(&self, user_id: UserId, snapshot: &DailyRewardsSnapshot) {
        let ready_categories = snapshot
            .rewards
            .iter()
            .filter(|reward| matches!(reward.status, DailyRewardStatus::Ready))
            .map(|reward| reward.category.as_str())
            .collect::<Vec<_>>();
        let unavailable_categories = snapshot
            .rewards
            .iter()
            .filter(|reward| matches!(reward.status, DailyRewardStatus::Unavailable))
            .map(|reward| reward.category.as_str())
            .collect::<Vec<_>>();

        info!(
            event = "daily_reward.viewed",
            user_id = ?user_id,
            date = %snapshot.date_utc,
            available_coins = snapshot.claim_totals.available_coins,
            claimed_today = snapshot.claim_totals.claimed_today,
            ready_categories = ?ready_categories,
            unavailable_categories = ?unavailable_categories,
            feature_enabled = snapshot.feature_enabled,
        );
    }

    fn emit_claim_event(
        &self,
        user_id: UserId,
        category: DailyRewardCategory,
        coins: i32,
        claimed_at: DateTime<Utc>,
        entry: &DailyRewardEntry,
    ) {
        info!(
            event = "daily_reward.claimed",
            user_id = ?user_id,
            category = %category,
            coins,
            claimed_at = %claimed_at,
            date = %entry.date_utc,
            first_detected_at = ?entry.first_detected_at,
            last_detected_at = ?entry.last_detected_at,
            claimable_since = ?entry.claimable_since,
            activity_sample = ?entry.activity_sample,
        );
    }

    fn emit_claim_all_event(
        &self,
        user_id: UserId,
        claimed: &[DailyRewardClaimSuccess],
        skipped: &[DailyRewardClaimSkip],
    ) {
        let claimed_categories = claimed
            .iter()
            .map(|success| success.category.as_str())
            .collect::<Vec<_>>();
        let skipped_details = skipped
            .iter()
            .map(|skip| (skip.category.as_str(), skip.reason))
            .collect::<Vec<_>>();
        let total_claimed_coins: i32 = claimed.iter().map(|success| success.coins).sum();

        info!(
            event = "daily_reward.claim_all",
            user_id = ?user_id,
            total_claimed_coins,
            claimed_categories = ?claimed_categories,
            skipped = ?skipped_details,
        );
    }

    fn emit_category_ready_event(
        &self,
        user_id: UserId,
        category: DailyRewardCategory,
        entry: &DailyRewardEntry,
    ) {
        info!(
            event = "daily_reward.category_ready",
            user_id = ?user_id,
            category = %category,
            coins = entry.coins,
            date = %entry.date_utc,
            first_detected_at = ?entry.first_detected_at,
            last_detected_at = ?entry.last_detected_at,
            claimable_since = ?entry.claimable_since,
            activity_sample = ?entry.activity_sample,
        );
    }

    async fn invalidate_cache(&self, user_id: UserId, date: NaiveDate) {
        if self.config.cache_ttl.is_none() {
            return;
        }

        let cache_key = cache_key(user_id, date);
        if let Err(err) = self.cache.remove(&cache_key).await {
            warn!(
                error = %err,
                user_id = ?user_id,
                date = %date,
                "Failed to remove daily rewards snapshot from cache"
            );
        }
    }

    pub async fn rebuild_snapshot(
        &self,
        user_id: UserId,
        date: NaiveDate,
    ) -> Result<DailyRewardsSnapshot> {
        let cache_ttl = self.config.cache_ttl;
        let cache_key = cache_ttl.map(|_| cache_key(user_id, date));

        let day_start = start_of_day_utc(date);
        let day_end = day_start + chrono::Duration::days(1);

        let mut txn = self.db.begin_transaction().await?;
        let refreshed = self
            .refresh_entries(&mut txn, None, user_id, date, day_start, day_end)
            .await?;
        txn.commit().await?;

        self.invalidate_cache(user_id, date).await;

        let snapshot = build_snapshot(date, refreshed);

        if let (Some(ttl), Some(cache_key)) = (cache_ttl, cache_key.as_ref()) {
            match self.cache.set(cache_key, &snapshot, Some(ttl)).await {
                Ok(()) => {}
                Err(err) => {
                    warn!(
                        error = %err,
                        user_id = ?user_id,
                        date = %date,
                        "Failed to store rebuilt daily rewards snapshot in cache"
                    );
                }
            }
        }

        Ok(snapshot)
    }
}

#[cfg(test)]
impl<Db, Auth, Repo, Coin, Cache, Activity, Id, Time>
    DailyRewardFeatureServiceImpl<Db, Auth, Repo, Coin, Cache, Activity, Id, Time>
where
    Db: Database,
    Repo: DailyRewardRepository<Db::Transaction>,
    Cache: CacheService,
    Activity: DailyRewardActivityService,
    Id: IdService,
    Time: TimeService,
{
    #[allow(
        clippy::too_many_arguments,
        reason = "Test helper wiring all dependencies explicitly for clarity"
    )]
    pub(crate) fn new_for_tests(
        db: Db,
        auth: Auth,
        repo: Repo,
        coin: Coin,
        cache: Cache,
        activity: Activity,
        id: Id,
        time: Time,
        config: DailyRewardFeatureConfig,
    ) -> Self {
        Self {
            db,
            auth,
            repo,
            coin,
            cache,
            activity,
            id,
            time,
            config,
        }
    }
}

fn start_of_day_utc(date: NaiveDate) -> DateTime<Utc> {
    date.and_time(NaiveTime::MIN).and_utc()
}

fn cache_key(user_id: UserId, date: NaiveDate) -> String {
    format!("daily_rewards:{}:{}", user_id.into_inner(), date)
}

fn map_claim_error(
    err: DailyRewardMarkClaimedError,
    category: DailyRewardCategory,
) -> DailyRewardClaimError {
    match err {
        DailyRewardMarkClaimedError::NotReady => DailyRewardClaimError::NotReady,
        DailyRewardMarkClaimedError::AlreadyClaimed => DailyRewardClaimError::AlreadyClaimed,
        DailyRewardMarkClaimedError::NotFound => {
            DailyRewardClaimError::Other(anyhow!("Reward entry not found for {category}"))
        }
        DailyRewardMarkClaimedError::Other(err) => DailyRewardClaimError::Other(err),
    }
}

pub(crate) fn build_snapshot(date: NaiveDate, refreshed: RefreshedRewards) -> DailyRewardsSnapshot {
    let mut rewards = Vec::new();
    let mut available_total = 0;
    let mut claimed_total = 0;

    for category in DailyRewardCategory::ALL {
        if let Some(entry) = refreshed.entries.get(&category) {
            let unavailable_reason = refreshed.unavailability.get(&category).copied().flatten();
            let status = determine_status(entry, unavailable_reason);

            if entry.claimable_since.is_some() && entry.claimed_at.is_none() {
                available_total += entry.coins;
            }

            if entry
                .claimed_at
                .is_some_and(|claimed_at| claimed_at.date_naive() == date)
            {
                claimed_total += entry.coins;
            }

            rewards.push(DailyRewardItem {
                category,
                coins: entry.coins,
                status,
                claimable_since: entry.claimable_since,
                last_detected_at: entry.last_detected_at,
                claimed_at: entry.claimed_at,
                activity_sample: entry.activity_sample.clone(),
                unavailable_reason,
            });
        }
    }

    DailyRewardsSnapshot {
        date_utc: date,
        feature_enabled: true,
        rewards,
        claim_totals: academy_core_daily_rewards_contracts::DailyRewardClaimTotals {
            available_coins: available_total,
            claimed_today: claimed_total,
        },
    }
}

fn determine_status(
    entry: &DailyRewardEntry,
    unavailable: Option<DailyRewardUnavailableReason>,
) -> DailyRewardStatus {
    if entry.claimed_at.is_some() {
        DailyRewardStatus::Claimed
    } else if entry.claimable_since.is_some() {
        DailyRewardStatus::Ready
    } else if unavailable.is_some() {
        DailyRewardStatus::Unavailable
    } else {
        DailyRewardStatus::Pending
    }
}

impl<Db, Auth, Repo, Coin, Cache, Activity, Id, Time>
    DailyRewardFeatureServiceImpl<Db, Auth, Repo, Coin, Cache, Activity, Id, Time>
where
    Db: Database,
    Repo: DailyRewardRepository<Db::Transaction>,
    Cache: CacheService,
    Activity: DailyRewardActivityService,
    Id: IdService,
    Time: TimeService,
{
    async fn refresh_entries(
        &self,
        txn: &mut Db::Transaction,
        token: Option<&AccessToken>,
        user_id: UserId,
        date: NaiveDate,
        day_start: DateTime<Utc>,
        day_end: DateTime<Utc>,
    ) -> Result<RefreshedRewards> {
        let mut entries = self.repo.list_by_user_and_date(txn, user_id, date).await?;

        let mut map: HashMap<DailyRewardCategory, DailyRewardEntry> = entries
            .drain(..)
            .map(|entry| (entry.category, entry))
            .collect();

        for category in DailyRewardCategory::ALL {
            let coins = self.config.coins.get(category);
            let maybe_entry = map.get(&category).cloned();
            let entry = match maybe_entry {
                Some(entry) if entry.claimed_at.is_some() => entry,
                Some(entry) if entry.coins == coins => entry,
                Some(entry) => {
                    let params = DailyRewardEntryUpsert {
                        id: entry.id,
                        user_id,
                        date_utc: date,
                        category,
                        coins,
                    };
                    self.repo.upsert_entry(txn, params).await?
                }
                None => {
                    let new_id: uuid::Uuid = self.id.generate();
                    let params = DailyRewardEntryUpsert {
                        id: new_id,
                        user_id,
                        date_utc: date,
                        category,
                        coins,
                    };
                    self.repo.upsert_entry(txn, params).await?
                }
            };
            map.insert(category, entry);
        }

        if let Some(entry) = map.get(&DailyRewardCategory::Arrival).cloned() {
            let was_ready = entry.claimable_since.is_some();
            if entry.claimable_since.is_none() && entry.claimed_at.is_none() {
                let now = self.time.now();
                let params = DailyRewardMarkReady {
                    user_id,
                    date_utc: date,
                    category: DailyRewardCategory::Arrival,
                    first_detected_at: Some(now),
                    last_detected_at: Some(now),
                    claimable_since: Some(now),
                    activity_sample: None,
                };
                let updated = self.repo.mark_ready(txn, params).await?;
                if !was_ready && updated.claimable_since.is_some() {
                    self.emit_category_ready_event(user_id, DailyRewardCategory::Arrival, &updated);
                }
                map.insert(DailyRewardCategory::Arrival, updated);
            }
        }

        let activity = self
            .activity
            .detect(token, user_id, day_start, day_end)
            .await
            .unwrap_or_else(|err| {
                warn!(error = %err, "Failed to fetch activity data");
                DailyRewardActivitySnapshot::default()
            });

        if let (true, Some(entry)) = (
            apply_activity(
                txn,
                user_id,
                date,
                DailyRewardCategory::Lecture,
                &activity.lecture,
                &self.repo,
                &mut map,
            )
            .await?,
            map.get(&DailyRewardCategory::Lecture),
        ) {
            self.emit_category_ready_event(user_id, DailyRewardCategory::Lecture, entry);
        }
        if let (true, Some(entry)) = (
            apply_activity(
                txn,
                user_id,
                date,
                DailyRewardCategory::Practice,
                &activity.practice,
                &self.repo,
                &mut map,
            )
            .await?,
            map.get(&DailyRewardCategory::Practice),
        ) {
            self.emit_category_ready_event(user_id, DailyRewardCategory::Practice, entry);
        }
        if let (true, Some(entry)) = (
            apply_activity(
                txn,
                user_id,
                date,
                DailyRewardCategory::Lab,
                &activity.lab,
                &self.repo,
                &mut map,
            )
            .await?,
            map.get(&DailyRewardCategory::Lab),
        ) {
            self.emit_category_ready_event(user_id, DailyRewardCategory::Lab, entry);
        }

        apply_pending_sample(
            txn,
            user_id,
            date,
            DailyRewardCategory::Lecture,
            &activity.lecture,
            &self.repo,
            &mut map,
        )
        .await?;
        apply_pending_sample(
            txn,
            user_id,
            date,
            DailyRewardCategory::Practice,
            &activity.practice,
            &self.repo,
            &mut map,
        )
        .await?;
        apply_pending_sample(
            txn,
            user_id,
            date,
            DailyRewardCategory::Lab,
            &activity.lab,
            &self.repo,
            &mut map,
        )
        .await?;

        let mut unavailability = HashMap::new();
        unavailability.insert(
            DailyRewardCategory::Lecture,
            activity.lecture.unavailable_reason,
        );
        unavailability.insert(
            DailyRewardCategory::Practice,
            activity.practice.unavailable_reason,
        );
        unavailability.insert(DailyRewardCategory::Lab, activity.lab.unavailable_reason);
        unavailability.insert(DailyRewardCategory::Arrival, None);

        Ok(RefreshedRewards {
            entries: map,
            unavailability,
        })
    }
}

async fn apply_activity<Repo, Txn>(
    txn: &mut Txn,
    user_id: UserId,
    date: NaiveDate,
    category: DailyRewardCategory,
    state: &DailyRewardActivityState,
    repo: &Repo,
    map: &mut HashMap<DailyRewardCategory, DailyRewardEntry>,
) -> Result<bool>
where
    Repo: DailyRewardRepository<Txn>,
    Txn: Transaction,
{
    let Some(activity) = &state.detected else {
        return Ok(false);
    };

    let was_ready = map
        .get(&category)
        .and_then(|entry| entry.claimable_since)
        .is_some();

    let params = DailyRewardMarkReady {
        user_id,
        date_utc: date,
        category,
        first_detected_at: Some(activity.first_detected_at),
        last_detected_at: Some(activity.last_detected_at),
        claimable_since: Some(activity.first_detected_at),
        activity_sample: activity.activity_sample.clone(),
    };

    let updated = repo.mark_ready(txn, params).await?;
    let is_ready = updated.claimable_since.is_some();
    map.insert(category, updated);
    Ok(!was_ready && is_ready)
}

async fn apply_pending_sample<Repo, Txn>(
    txn: &mut Txn,
    user_id: UserId,
    date: NaiveDate,
    category: DailyRewardCategory,
    state: &DailyRewardActivityState,
    repo: &Repo,
    map: &mut HashMap<DailyRewardCategory, DailyRewardEntry>,
) -> Result<()>
where
    Repo: DailyRewardRepository<Txn>,
    Txn: Transaction,
{
    if state.detected.is_some() {
        return Ok(());
    }

    let Some(sample) = state.pending_sample.clone() else {
        return Ok(());
    };

    let Some(entry) = map.get(&category) else {
        return Ok(());
    };

    if entry.claimable_since.is_some() || entry.claimed_at.is_some() {
        // The reward is already ready or claimed; keep the existing activity sample.
        return Ok(());
    }

    let params = DailyRewardMarkReady {
        user_id,
        date_utc: date,
        category,
        first_detected_at: None,
        last_detected_at: None,
        claimable_since: None,
        activity_sample: Some(sample),
    };

    let updated = repo.mark_ready(txn, params).await?;
    map.insert(category, updated);
    Ok(())
}

#[cfg(test)]
mod get_today_cache_tests {
    use super::*;
    use academy_auth_contracts::{Authentication, MockAuthService};
    use academy_cache_contracts::MockCacheService;
    use academy_core_coin_contracts::coin::MockCoinService;
    use academy_core_daily_rewards_contracts::{
        DailyRewardClaimTotals, DailyRewardsSnapshot, MockDailyRewardActivityService,
    };
    use academy_models::{
        Sha256Hash,
        auth::AccessToken,
        session::{SessionId, SessionRefreshTokenHash},
        user::UserId,
    };
    use academy_persistence_contracts::{
        MockDatabase, MockTransaction, daily_rewards::MockDailyRewardRepository,
    };
    use academy_shared_contracts::{id::MockIdService, time::MockTimeService};
    use chrono::NaiveDate;
    use mockall::predicate;
    use std::future;

    #[tokio::test]
    async fn returns_cached_snapshot_without_refreshing() {
        let user_id = UserId::from(uuid::Uuid::new_v4());
        let date = NaiveDate::from_ymd_opt(2025, 11, 1).unwrap();
        let now = date.and_hms_opt(12, 0, 0).unwrap().and_utc();
        let token = AccessToken::new("token");

        let snapshot = DailyRewardsSnapshot {
            date_utc: date,
            feature_enabled: true,
            rewards: Vec::new(),
            claim_totals: DailyRewardClaimTotals {
                available_coins: 0,
                claimed_today: 0,
            },
        };

        let mut auth = MockAuthService::new();
        auth.expect_authenticate()
            .once()
            .with(predicate::eq(token.clone()))
            .return_once(move |_| {
                Box::pin(future::ready(Ok(Authentication {
                    user_id,
                    session_id: SessionId::from(uuid::Uuid::new_v4()),
                    refresh_token_hash: SessionRefreshTokenHash::new(Sha256Hash::default()),
                    admin: false,
                    email_verified: true,
                })))
            });

        let cache_key = cache_key(user_id, date);
        let mut cache = MockCacheService::new();
        {
            let expected_snapshot = snapshot.clone();
            cache
                .expect_get()
                .once()
                .with(predicate::eq(cache_key.clone()))
                .return_once(move |_| Box::pin(future::ready(Ok(Some(expected_snapshot)))));
            cache.expect_set::<DailyRewardsSnapshot>().never();
        }

        let db = MockDatabase::build_expect_rollback();

        let mut repo = MockDailyRewardRepository::new();
        repo.expect_list_by_user_and_date()
            .once()
            .return_once(move |_, _, _| {
                Box::pin(future::ready(Err(anyhow::anyhow!(
                    "failed to load entries"
                ))))
            });
        repo.expect_upsert_entry().never();
        repo.expect_mark_ready().never();
        repo.expect_mark_claimed().never();

        let mut activity = MockDailyRewardActivityService::new();
        activity.expect_detect().never();

        let coin = MockCoinService::<MockTransaction>::new();

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
            DailyRewardFeatureConfig {
                enable: true,
                coins: DailyRewardCoinsConfig {
                    arrival: 5,
                    lecture: 20,
                    practice: 10,
                    lab: 30,
                },
                cache_ttl: Some(Duration::from_secs(60)),
            },
        );

        let response = sut.get_today(&token).await.unwrap();
        assert_eq!(response.snapshot.date_utc, date);
        assert!(response.snapshot.rewards.is_empty());
        assert_eq!(response.snapshot.claim_totals.available_coins, 0);
        assert_eq!(response.snapshot.claim_totals.claimed_today, 0);
    }
}

#[cfg(test)]
mod refresh_entries_tests {
    use super::*;
    use crate::DailyRewardFeatureConfig;
    use academy_auth_contracts::MockAuthService;
    use academy_cache_contracts::MockCacheService;
    use academy_core_coin_contracts::coin::MockCoinService;
    use academy_core_daily_rewards_contracts::DailyRewardActivitySnapshot;
    use academy_core_daily_rewards_contracts::MockDailyRewardActivityService;
    use academy_persistence_contracts::{
        MockDatabase, MockTransaction, daily_rewards::MockDailyRewardRepository,
    };
    use academy_shared_contracts::{id::MockIdService, time::MockTimeService};
    use chrono::TimeZone;
    use mockall::predicate;
    use std::future;

    fn claimed_entry(
        user_id: UserId,
        date: NaiveDate,
        claimed_at: DateTime<Utc>,
        coins: i32,
    ) -> DailyRewardEntry {
        let detected_at = claimed_at - chrono::Duration::minutes(5);
        DailyRewardEntry {
            id: uuid::Uuid::new_v4(),
            user_id,
            date_utc: date,
            category: DailyRewardCategory::Arrival,
            coins,
            first_detected_at: Some(detected_at),
            last_detected_at: Some(detected_at),
            claimable_since: Some(detected_at),
            claimed_at: Some(claimed_at),
            activity_sample: None,
            created_at: claimed_at,
            updated_at: claimed_at,
        }
    }

    fn entry(
        user_id: UserId,
        category: DailyRewardCategory,
        date: NaiveDate,
        coins: i32,
    ) -> DailyRewardEntry {
        DailyRewardEntry {
            id: uuid::Uuid::new_v4(),
            user_id,
            date_utc: date,
            category,
            coins,
            first_detected_at: None,
            last_detected_at: None,
            claimable_since: None,
            claimed_at: None,
            activity_sample: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn refresh_entries_keeps_claimed_coin_amounts() {
        let user_id = UserId::from(uuid::Uuid::new_v4());
        let date = NaiveDate::from_ymd_opt(2025, 11, 1).unwrap();
        let day_start = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
        let day_end = day_start + chrono::Duration::days(1);
        let claimed_at = Utc.with_ymd_and_hms(2025, 11, 1, 8, 0, 0).unwrap();

        let arrival_entry = claimed_entry(user_id, date, claimed_at, 20);
        let lecture_entry = entry(user_id, DailyRewardCategory::Lecture, date, 20);
        let practice_entry = entry(user_id, DailyRewardCategory::Practice, date, 10);
        let lab_entry = entry(user_id, DailyRewardCategory::Lab, date, 30);

        let entries = vec![
            arrival_entry.clone(),
            lecture_entry,
            practice_entry,
            lab_entry,
        ];

        let mut repo = MockDailyRewardRepository::new();
        {
            let entries = entries.clone();
            repo.expect_list_by_user_and_date()
                .once()
                .with(
                    predicate::always(),
                    predicate::eq(user_id),
                    predicate::eq(date),
                )
                .return_once(move |_, _, _| Box::pin(future::ready(Ok(entries))));
        }
        repo.expect_upsert_entry().never();

        let mut activity = MockDailyRewardActivityService::new();
        let expected_day_start = day_start;
        let expected_day_end = day_end;
        let expected_user = user_id;
        activity
            .expect_detect()
            .once()
            .withf(move |token, user, start, end| {
                token.is_none()
                    && user == &expected_user
                    && start == &expected_day_start
                    && end == &expected_day_end
            })
            .return_once(|_, _, _, _| {
                Box::pin(future::ready(Ok(DailyRewardActivitySnapshot::default())))
            });

        let sut = DailyRewardFeatureServiceImpl::new_for_tests(
            MockDatabase::new(),
            MockAuthService::<MockTransaction>::new(),
            repo,
            MockCoinService::<MockTransaction>::new(),
            MockCacheService::new(),
            activity,
            MockIdService::new(),
            MockTimeService::new(),
            DailyRewardFeatureConfig {
                enable: true,
                coins: DailyRewardCoinsConfig {
                    arrival: 5,
                    lecture: 20,
                    practice: 10,
                    lab: 30,
                },
                cache_ttl: None,
            },
        );

        let mut txn = MockTransaction::new();

        let refreshed = sut
            .refresh_entries(&mut txn, None, user_id, date, day_start, day_end)
            .await
            .unwrap();

        let stored_arrival = refreshed
            .entries
            .get(&DailyRewardCategory::Arrival)
            .expect("arrival entry missing");
        assert_eq!(stored_arrival.coins, 20);
        assert_eq!(stored_arrival.claimed_at, Some(claimed_at));
    }
}
