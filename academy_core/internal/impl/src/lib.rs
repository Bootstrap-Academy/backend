use academy_auth_contracts::internal::AuthInternalService;
use academy_core_coin_contracts::coin::{CoinAddCoinsError, CoinService};
use academy_core_internal_contracts::{
    InternalAddCoinsError, InternalGetUserByEmailError, InternalGetUserError, InternalService,
};
use academy_di::Build;
use academy_models::{
    auth::InternalToken,
    coin::{Balance, TransactionDescription},
    email_address::EmailAddress,
    user::{UserComposite, UserId},
};
use academy_persistence_contracts::{user::UserRepository, Database, Transaction};
use academy_utils::trace_instrument;
use anyhow::Context;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Build, Default)]
pub struct InternalServiceImpl<Db, AuthInternal, UserRepo, Coin> {
    db: Db,
    auth_internal: AuthInternal,
    user_repo: UserRepo,
    coin: Coin,
}

impl<Db, AuthInternal, UserRepo, Coin> InternalService
    for InternalServiceImpl<Db, AuthInternal, UserRepo, Coin>
where
    Db: Database,
    AuthInternal: AuthInternalService,
    UserRepo: UserRepository<Db::Transaction>,
    Coin: CoinService<Db::Transaction>,
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
}
