use academy_core_user_contracts::export::{AccountDataExport, UserExportService};
use academy_di::Build;
use academy_models::user::UserId;
use academy_persistence_contracts::{
    coin::CoinRepository, contract::ContractRepository, oauth2::OAuth2Repository,
    paypal::PaypalRepository, premium::PremiumRepository, session::SessionRepository,
    user::UserRepository, withdrawal::WithdrawalRepository,
};
use academy_utils::trace_instrument;
use anyhow::Context;

#[derive(Debug, Clone, Copy, Build, Default)]
pub struct UserExportServiceImpl<
    UserRepo,
    SessionRepo,
    OAuth2Repo,
    CoinRepo,
    PremiumRepo,
    PaypalRepo,
    ContractRepo,
    WithdrawalRepo,
> {
    user_repo: UserRepo,
    session_repo: SessionRepo,
    oauth2_repo: OAuth2Repo,
    coin_repo: CoinRepo,
    premium_repo: PremiumRepo,
    paypal_repo: PaypalRepo,
    contract_repo: ContractRepo,
    withdrawal_repo: WithdrawalRepo,
}

impl<
    Txn,
    UserRepo,
    SessionRepo,
    OAuth2Repo,
    CoinRepo,
    PremiumRepo,
    PaypalRepo,
    ContractRepo,
    WithdrawalRepo,
> UserExportService<Txn>
    for UserExportServiceImpl<
        UserRepo,
        SessionRepo,
        OAuth2Repo,
        CoinRepo,
        PremiumRepo,
        PaypalRepo,
        ContractRepo,
        WithdrawalRepo,
    >
where
    Txn: Send + Sync + 'static,
    UserRepo: UserRepository<Txn>,
    SessionRepo: SessionRepository<Txn>,
    OAuth2Repo: OAuth2Repository<Txn>,
    CoinRepo: CoinRepository<Txn>,
    PremiumRepo: PremiumRepository<Txn>,
    PaypalRepo: PaypalRepository<Txn>,
    ContractRepo: ContractRepository<Txn>,
    WithdrawalRepo: WithdrawalRepository<Txn>,
{
    #[trace_instrument(skip(self, txn))]
    async fn export(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> anyhow::Result<Option<AccountDataExport>> {
        let Some(user) = self
            .user_repo
            .get_composite(txn, user_id)
            .await
            .context("Failed to get user from database")?
        else {
            return Ok(None);
        };

        Ok(Some(AccountDataExport {
            user,
            sessions: self
                .session_repo
                .list_by_user(txn, user_id)
                .await
                .context("Failed to get sessions from database")?,
            oauth2_links: self
                .oauth2_repo
                .list_links_by_user(txn, user_id)
                .await
                .context("Failed to get OAuth2 links from database")?,
            balance: self
                .coin_repo
                .get_balance(txn, user_id)
                .await
                .context("Failed to get balance from database")?,
            transactions: self
                .coin_repo
                .get_all_transactions(txn, user_id)
                .await
                .context("Failed to get transactions from database")?,
            premium: self
                .premium_repo
                .get_latest_by_user_id(txn, user_id)
                .await
                .context("Failed to get premium from database")?,
            premium_subscription: self
                .premium_repo
                .get_subscription(txn, user_id)
                .await
                .context("Failed to get premium subscription from database")?,
            invoices: self
                .paypal_repo
                .list_coin_orders_by_user_id(txn, user_id)
                .await
                .context("Failed to get coin orders from database")?,
            contract_declarations: self
                .contract_repo
                .list_by_user_id(txn, user_id)
                .await
                .context("Failed to get contract declarations from database")?,
            withdrawal_consents: self
                .withdrawal_repo
                .list_by_user_id(txn, user_id)
                .await
                .context("Failed to get withdrawal consents from database")?,
        }))
    }
}
