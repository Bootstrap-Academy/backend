use academy_auth_contracts::{AuthResultExt, AuthService};
use academy_core_coin_contracts::coin::{CoinAddCoinsError, CoinService};
use academy_core_heart_contracts::{
    heart::HeartService, HeartFeatureService, HeartGetError, HeartRefillError,
};
use academy_di::Build;
use academy_models::{
    auth::AccessToken,
    heart::{HeartConfig, Hearts},
    user::UserIdOrSelf,
};
use academy_persistence_contracts::{user::UserRepository, Database, Transaction};
use academy_utils::trace_instrument;
use chrono::NaiveTime;

pub mod heart;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct HeartFeatureServiceImpl<Db, Auth, UserRepo, Heart, Coin> {
    db: Db,
    auth: Auth,
    user_repo: UserRepo,
    heart: Heart,
    coin: Coin,
    config: HeartFeatureConfig,
}

#[derive(Debug, Clone)]
pub struct HeartFeatureConfig {
    pub hearts_max: u64,
    pub hearts_refill_price: u64,
    pub auto_refill_time: NaiveTime,
}

impl<Db, Auth, UserRepo, Heart, Coin> HeartFeatureService
    for HeartFeatureServiceImpl<Db, Auth, UserRepo, Heart, Coin>
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    UserRepo: UserRepository<Db::Transaction>,
    Heart: HeartService<Db::Transaction>,
    Coin: CoinService<Db::Transaction>,
{
    #[trace_instrument(skip(self))]
    fn get_config(&self) -> HeartConfig {
        HeartConfig {
            hearts_max: self.config.hearts_max,
            hearts_refill_price: self.config.hearts_refill_price,
        }
    }

    #[trace_instrument(skip(self))]
    async fn get(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
    ) -> Result<Hearts, HeartGetError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = user_id.unwrap_or(auth.user_id);
        auth.ensure_self_or_admin(user_id).map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        if !self.user_repo.exists(&mut txn, user_id).await? {
            return Err(HeartGetError::UserNotFound);
        }

        self.heart.get(&mut txn, user_id).await.map_err(Into::into)
    }

    #[trace_instrument(skip(self))]
    async fn refill(&self, token: &AccessToken) -> Result<Hearts, HeartRefillError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = auth.user_id;

        let mut txn = self.db.begin_transaction().await?;

        let hearts = self.heart.get(&mut txn, user_id).await?;
        if hearts.hearts >= self.config.hearts_max {
            return Ok(hearts);
        }

        self.coin
            .add_coins(
                &mut txn,
                user_id,
                -(self.config.hearts_refill_price as i64),
                false,
                Some("Hearts".try_into().unwrap()),
                false,
            )
            .await
            .map_err(|err| match err {
                CoinAddCoinsError::NotEnoughCoins => HeartRefillError::NotEnoughCoins,
                CoinAddCoinsError::Other(err) => err.into(),
            })?;

        let hearts = self
            .heart
            .add(&mut txn, user_id, self.config.hearts_max as _)
            .await
            .map_err(anyhow::Error::from)?;

        txn.commit().await?;

        Ok(hearts)
    }
}
