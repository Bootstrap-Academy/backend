use academy_auth_contracts::internal::AuthInternalService;
use academy_core_coin_contracts::coin::{CoinAddCoinsError, CoinService};
use academy_core_heart_contracts::heart::{HeartAddError, HeartService};
use academy_core_internal_contracts::{
    InternalAddCoinsError, InternalAddHeartsError, InternalGetHeartsError,
    InternalGetUserByEmailError, InternalGetUserError, InternalHasPremiumError, InternalService,
};
use academy_core_premium_contracts::premium::PremiumService;
use academy_di::Build;
use academy_models::{
    auth::InternalToken,
    coin::{Balance, TransactionDescription},
    email_address::EmailAddress,
    heart::Hearts,
    user::{UserComposite, UserId},
};
use academy_persistence_contracts::{user::UserRepository, Database, Transaction};
use academy_utils::trace_instrument;
use anyhow::Context;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Build, Default)]
pub struct InternalServiceImpl<Db, AuthInternal, UserRepo, Coin, Heart, Premium> {
    db: Db,
    auth_internal: AuthInternal,
    user_repo: UserRepo,
    coin: Coin,
    heart: Heart,
    premium: Premium,
}

impl<Db, AuthInternal, UserRepo, Coin, Heart, Premium> InternalService
    for InternalServiceImpl<Db, AuthInternal, UserRepo, Coin, Heart, Premium>
where
    Db: Database,
    AuthInternal: AuthInternalService,
    UserRepo: UserRepository<Db::Transaction>,
    Coin: CoinService<Db::Transaction>,
    Heart: HeartService<Db::Transaction>,
    Premium: PremiumService<Db::Transaction>,
{
    #[trace_instrument(skip(self))]
    async fn get_user(
        &self,
        token: &InternalToken,
        user_id: UserId,
    ) -> Result<UserComposite, InternalGetUserError> {
        self.auth_internal.authenticate(token, "auth")?;

        let mut txn = self.db.begin_transaction().await?;

        self.user_repo
            .get_composite(&mut txn, user_id)
            .await
            .context("Failed to get user from database")?
            .ok_or(InternalGetUserError::NotFound)
    }

    #[trace_instrument(skip(self))]
    async fn get_user_by_email(
        &self,
        token: &InternalToken,
        email: EmailAddress,
    ) -> Result<UserComposite, InternalGetUserByEmailError> {
        self.auth_internal.authenticate(token, "auth")?;

        let mut txn = self.db.begin_transaction().await?;

        self.user_repo
            .get_composite_by_email(&mut txn, &email)
            .await
            .context("Failed to get user from database")?
            .ok_or(InternalGetUserByEmailError::NotFound)
    }

    #[trace_instrument(skip(self))]
    async fn add_coins(
        &self,
        token: &InternalToken,
        user_id: UserId,
        coins: i64,
        description: Option<TransactionDescription>,
        include_in_credit_note: bool,
    ) -> Result<Balance, InternalAddCoinsError> {
        self.auth_internal.authenticate(token, "shop")?;

        let mut txn = self.db.begin_transaction().await?;

        let user_composite = self
            .user_repo
            .get_composite(&mut txn, user_id)
            .await?
            .ok_or(InternalAddCoinsError::UserNotFound)?;

        let withhold = coins >= 0 && !user_composite.can_receive_coins();

        let new_balance = self
            .coin
            .add_coins(
                &mut txn,
                user_id,
                coins,
                withhold,
                description,
                include_in_credit_note,
            )
            .await
            .map_err(|err| match err {
                CoinAddCoinsError::NotEnoughCoins => InternalAddCoinsError::NotEnoughCoins,
                CoinAddCoinsError::Other(err) => err.into(),
            })?;

        txn.commit().await?;

        Ok(new_balance)
    }

    #[trace_instrument(skip(self))]
    async fn get_hearts(
        &self,
        token: &InternalToken,
        user_id: UserId,
    ) -> Result<Hearts, InternalGetHeartsError> {
        self.auth_internal.authenticate(token, "shop")?;

        let mut txn = self.db.begin_transaction().await?;

        if !self.user_repo.exists(&mut txn, user_id).await? {
            return Err(InternalGetHeartsError::UserNotFound);
        }

        self.heart.get(&mut txn, user_id).await.map_err(Into::into)
    }

    #[trace_instrument(skip(self))]
    async fn add_hearts(
        &self,
        token: &InternalToken,
        user_id: UserId,
        hearts: i64,
    ) -> Result<Hearts, InternalAddHeartsError> {
        self.auth_internal.authenticate(token, "shop")?;

        let mut txn = self.db.begin_transaction().await?;

        if !self.user_repo.exists(&mut txn, user_id).await? {
            return Err(InternalAddHeartsError::UserNotFound);
        }

        let result = self
            .heart
            .add(&mut txn, user_id, hearts)
            .await
            .map_err(|err| match err {
                HeartAddError::NotEnoughHearts => InternalAddHeartsError::NotEnoughHearts,
                HeartAddError::Other(err) => err.into(),
            })?;

        txn.commit().await?;

        Ok(result)
    }

    #[trace_instrument(skip(self))]
    async fn has_premium(
        &self,
        token: &InternalToken,
        user_id: UserId,
    ) -> Result<bool, InternalHasPremiumError> {
        self.auth_internal.authenticate(token, "shop")?;

        let mut txn = self.db.begin_transaction().await?;

        if !self.user_repo.exists(&mut txn, user_id).await? {
            return Err(InternalHasPremiumError::UserNotFound);
        }

        let result = self.premium.get_active(&mut txn, user_id).await?.is_some();

        txn.commit().await?;

        Ok(result)
    }
}
