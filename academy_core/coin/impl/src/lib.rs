use academy_auth_contracts::{AuthResultExt, AuthService};
use academy_core_coin_contracts::{
    coin::CoinService, CoinAddCoinsError, CoinFeatureService, CoinGetBalanceError,
};
use academy_di::Build;
use academy_models::{
    auth::AccessToken,
    coin::{Balance, TransactionDescription},
    user::UserIdOrSelf,
};
use academy_persistence_contracts::{
    coin::CoinRepository, user::UserRepository, Database, Transaction,
};
use academy_utils::trace_instrument;

pub mod coin;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Default, Build)]
pub struct CoinFeatureServiceImpl<Db, Auth, UserRepo, CoinRepo, Coin> {
    db: Db,
    auth: Auth,
    user_repo: UserRepo,
    coin_repo: CoinRepo,
    coin: Coin,
}

impl<Db, Auth, UserRepo, CoinRepo, Coin> CoinFeatureService
    for CoinFeatureServiceImpl<Db, Auth, UserRepo, CoinRepo, Coin>
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    UserRepo: UserRepository<Db::Transaction>,
    CoinRepo: CoinRepository<Db::Transaction>,
    Coin: CoinService<Db::Transaction>,
{
    #[trace_instrument(skip(self))]
    async fn get_balance(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
    ) -> Result<Balance, CoinGetBalanceError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = user_id.unwrap_or(auth.user_id);
        auth.ensure_self_or_admin(user_id).map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        if !self.user_repo.exists(&mut txn, user_id).await? {
            return Err(CoinGetBalanceError::UserNotFound);
        }

        let balance = self.coin_repo.get_balance(&mut txn, user_id).await?;

        Ok(balance)
    }

    #[trace_instrument(skip(self))]
    async fn add_coins(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
        coins: i64,
        description: Option<TransactionDescription>,
        include_in_credit_note: bool,
    ) -> Result<Balance, CoinAddCoinsError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = user_id.unwrap_or(auth.user_id);
        auth.ensure_admin().map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        if !self.user_repo.exists(&mut txn, user_id).await? {
            return Err(CoinAddCoinsError::UserNotFound);
        }

        let new_balance = self
            .coin
            .add_coins(
                &mut txn,
                user_id,
                coins,
                false,
                description,
                include_in_credit_note,
            )
            .await
            .map_err(|err| {
                use academy_core_coin_contracts::coin::CoinAddCoinsError as E;
                match err {
                    E::NotEnoughCoins => CoinAddCoinsError::NotEnoughCoins,
                    E::Other(err) => err.into(),
                }
            })?;

        txn.commit().await?;

        Ok(new_balance)
    }
}
