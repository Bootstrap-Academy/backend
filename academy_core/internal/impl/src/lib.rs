use academy_auth_contracts::internal::AuthInternalService;
use academy_core_coin_contracts::coin::{CoinAddCoinsError, CoinService};
use academy_core_heart_contracts::heart::{HeartAddError, HeartService};
use academy_core_internal_contracts::{
    InternalAddCoinsError, InternalAddHeartsError, InternalGetHeartsError,
    InternalGetUserByEmailError, InternalGetUserError, InternalHasPremiumError,
    InternalHeartOperationError, InternalService,
};
use academy_core_premium_contracts::premium::PremiumService;
use academy_di::Build;
use academy_models::{
    auth::InternalToken,
    coin::{Balance, CoinOperation, CoinOperationClaim, TransactionDescription},
    email_address::EmailAddress,
    heart::{
        HeartOperation, HeartOperationClaim, HeartOperationOutcome, HeartOperationReceipt, Hearts,
    },
    user::{UserComposite, UserId},
};
use academy_persistence_contracts::{
    Database, Transaction, coin::CoinRepository, heart::HeartRepository, user::UserRepository,
};
use academy_utils::trace_instrument;
use anyhow::Context;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Build, Default)]
pub struct InternalServiceImpl<
    Db,
    AuthInternal,
    UserRepo,
    Coin,
    Heart,
    Premium,
    CoinRepo,
    HeartRepo,
> {
    db: Db,
    coin_repo: CoinRepo,
    heart_repo: HeartRepo,
    auth_internal: AuthInternal,
    user_repo: UserRepo,
    coin: Coin,
    heart: Heart,
    premium: Premium,
}

impl<Db, AuthInternal, UserRepo, Coin, Heart, Premium, CoinRepo, HeartRepo> InternalService
    for InternalServiceImpl<Db, AuthInternal, UserRepo, Coin, Heart, Premium, CoinRepo, HeartRepo>
where
    Db: Database,
    CoinRepo: CoinRepository<Db::Transaction>,
    HeartRepo: HeartRepository<Db::Transaction>,
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
            .get_internal_composite(&mut txn, user_id)
            .await
            .context("Failed to get user from database")?
            .ok_or(InternalGetUserError::NotFound)
    }

    #[trace_instrument(skip(self, email))]
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
        if coins > 0 {
            return Err(InternalAddCoinsError::CreditNotAuthorized);
        }

        let mut txn = self.db.begin_transaction().await?;

        // New limited-subject purchases use exact-order acceptance. Generic
        // zero-value compatibility requests keep their existing recipient semantics.
        let user_composite = if coins < 0 {
            self.user_repo.get_composite(&mut txn, user_id).await?
        } else {
            self.user_repo
                .get_purchase_composite(&mut txn, user_id)
                .await?
        }
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
    async fn apply_coin_operation(
        &self,
        token: &InternalToken,
        operation: CoinOperation,
    ) -> Result<Balance, InternalAddCoinsError> {
        self.auth_internal.authenticate(token, "shop")?;
        let mut txn = self.db.begin_transaction().await?;
        match self.coin_repo.claim_operation(&mut txn, &operation).await? {
            CoinOperationClaim::Completed(balance) => return Ok(balance),
            CoinOperationClaim::Conflict => return Err(InternalAddCoinsError::OperationConflict),
            CoinOperationClaim::CreditNotAuthorized => {
                return Err(InternalAddCoinsError::CreditNotAuthorized);
            }
            CoinOperationClaim::New => (),
        }
        // Completed receipts above remain replayable without a current user.
        // Only new ordinary recipients can use this generic debit interface.
        let user = if operation.coins < 0 {
            self.user_repo
                .get_composite(&mut txn, operation.user_id)
                .await?
        } else {
            self.user_repo
                .get_purchase_composite(&mut txn, operation.user_id)
                .await?
        }
        .ok_or(InternalAddCoinsError::UserNotFound)?;
        let balance = self
            .coin
            .add_coins(
                &mut txn,
                operation.user_id,
                operation.coins,
                operation.coins >= 0 && !user.can_receive_coins(),
                operation.description,
                operation.include_in_credit_note,
            )
            .await
            .map_err(|err| match err {
                CoinAddCoinsError::NotEnoughCoins => InternalAddCoinsError::NotEnoughCoins,
                CoinAddCoinsError::Other(err) => err.into(),
            })?;
        self.coin_repo
            .complete_operation(&mut txn, operation.id, balance)
            .await?;
        txn.commit().await?;
        Ok(balance)
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

    #[tracing::instrument(skip(self, token, operation))]
    async fn apply_heart_operation(
        &self,
        token: &InternalToken,
        operation: HeartOperation,
    ) -> Result<HeartOperationReceipt, InternalHeartOperationError> {
        self.auth_internal.authenticate(token, "shop")?;
        let mut txn = self.db.begin_transaction().await?;
        match self
            .heart_repo
            .claim_operation(&mut txn, &operation)
            .await?
        {
            HeartOperationClaim::Completed(receipt) => return Ok(receipt),
            HeartOperationClaim::Conflict => {
                return Err(InternalHeartOperationError::OperationConflict);
            }
            HeartOperationClaim::New => (),
        }
        if operation.half_hearts != 2 || operation.reason != "incorrect_challenge_attempt" {
            return Err(InternalHeartOperationError::InvalidRequest);
        }
        if !self
            .heart_repo
            .lock_user(&mut txn, operation.user_id)
            .await?
        {
            return Err(InternalHeartOperationError::UserNotFound);
        }

        // The shared user lock also serializes premium changes, paid refills,
        // ordinary consumption and erasure. Refill is evaluated exactly once.
        let premium = self
            .premium
            .get_active(&mut txn, operation.user_id)
            .await?
            .is_some();
        let current = self.heart.get(&mut txn, operation.user_id).await?;
        let (outcome, charged_half_hearts) = if premium {
            (HeartOperationOutcome::Premium, 0)
        } else if current.hearts < 2 {
            // A concurrent attempt may have used the last heart. No partial
            // half-heart debit and no debt payable by a later refill.
            (HeartOperationOutcome::Insufficient, 0)
        } else {
            (HeartOperationOutcome::Charged, 2)
        };
        let hearts = current.hearts - charged_half_hearts;
        if charged_half_hearts != 0 {
            self.heart_repo
                .set(&mut txn, operation.user_id, Hearts { hearts, ..current })
                .await?;
        }
        let receipt = HeartOperationReceipt {
            operation_id: operation.id,
            user_id: operation.user_id,
            charged_half_hearts,
            hearts,
            outcome,
        };
        self.heart_repo
            .complete_operation(&mut txn, &operation, receipt)
            .await?;
        txn.commit().await?;
        Ok(receipt)
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
