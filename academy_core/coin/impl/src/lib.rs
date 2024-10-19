use academy_auth_contracts::{AuthResultExt, AuthService};
use academy_core_coin_contracts::{CoinFeatureService, CoinGetBalanceError};
use academy_di::Build;
use academy_models::{auth::AccessToken, coin::Balance, user::UserIdOrSelf};
use academy_persistence_contracts::{coin::CoinRepository, user::UserRepository, Database};
use academy_utils::trace_instrument;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Default, Build)]
pub struct CoinFeatureServiceImpl<Db, Auth, UserRepo, CoinRepo> {
    db: Db,
    auth: Auth,
    user_repo: UserRepo,
    coin_repo: CoinRepo,
}

impl<Db, Auth, UserRepo, CoinRepo> CoinFeatureService
    for CoinFeatureServiceImpl<Db, Auth, UserRepo, CoinRepo>
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    UserRepo: UserRepository<Db::Transaction>,
    CoinRepo: CoinRepository<Db::Transaction>,
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
            return Err(CoinGetBalanceError::NotFound);
        }

        let balance = self
            .coin_repo
            .get_balance(&mut txn, user_id)
            .await?
            .unwrap_or_default();

        Ok(balance)
    }
}
