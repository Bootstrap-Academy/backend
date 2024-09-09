use academy_core_internal_contracts::{
    auth::InternalAuthService, InternalGetUserByEmailError, InternalGetUserError, InternalService,
};
use academy_di::Build;
use academy_models::{
    email_address::EmailAddress,
    user::{UserComposite, UserId},
};
use academy_persistence_contracts::{user::UserRepository, Database};

pub mod auth;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Build, Default)]
pub struct InternalServiceImpl<Db, InternalAuth, UserRepo> {
    db: Db,
    internal_auth: InternalAuth,
    user_repo: UserRepo,
}

impl<Db, InternalAuth, UserRepo> InternalService for InternalServiceImpl<Db, InternalAuth, UserRepo>
where
    Db: Database,
    InternalAuth: InternalAuthService,
    UserRepo: UserRepository<Db::Transaction>,
{
    async fn get_user(
        &self,
        token: &str,
        user_id: UserId,
    ) -> Result<UserComposite, InternalGetUserError> {
        self.internal_auth.authenticate(token, "auth")?;

        let mut txn = self.db.begin_transaction().await?;

        self.user_repo
            .get_composite(&mut txn, user_id)
            .await?
            .ok_or(InternalGetUserError::NotFound)
    }

    async fn get_user_by_email(
        &self,
        token: &str,
        email: EmailAddress,
    ) -> Result<UserComposite, InternalGetUserByEmailError> {
        self.internal_auth.authenticate(token, "auth")?;

        let mut txn = self.db.begin_transaction().await?;

        self.user_repo
            .get_composite_by_email(&mut txn, &email)
            .await?
            .ok_or(InternalGetUserByEmailError::NotFound)
    }
}