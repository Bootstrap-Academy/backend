use academy_config::Config;
use academy_core_coin_contracts::coin::CoinService;
use academy_di::Provide;
use academy_persistence_contracts::{Database, Transaction};
use clap::Subcommand;
use uuid::Uuid;

use crate::{
    cache, database, email,
    environment::{ConfigProvider, Provider, types},
};

#[derive(Debug, Subcommand)]
pub enum AdminCoinCommand {
    /// Add morphcoins to the given user's balance
    #[command(aliases(["a"]))]
    Add {
        /// Whether to whithhold the new coins
        #[arg(long)]
        withhold: bool,
        /// Whether to whithhold the new coins
        #[arg(long)]
        no_credit_note: bool,
        /// The user's id
        user_id: Uuid,
        /// The number of coins to add to the user's balance (can be negative)
        coins: i64,
        /// An optional description for the transaction
        description: Option<String>,
    },
}

impl AdminCoinCommand {
    pub async fn invoke(self, config: Config) -> anyhow::Result<()> {
        match self {
            AdminCoinCommand::Add {
                withhold,
                no_credit_note,
                user_id,
                coins,
                description,
            } => {
                add(
                    config,
                    user_id,
                    coins,
                    withhold,
                    description,
                    no_credit_note,
                )
                .await
            }
        }
    }
}

async fn add(
    config: Config,
    user_id: Uuid,
    coins: i64,
    withhold: bool,
    description: Option<String>,
    no_credit_note: bool,
) -> anyhow::Result<()> {
    let database = database::connect(&config.database).await?;
    let cache = cache::connect(&config.cache).await?;
    let email_service = email::connect(&config.email).await?;
    let config_provider = ConfigProvider::new(&config)?;
    let mut provider = Provider::new(config_provider, database, cache, email_service);

    let db: types::Database = provider.provide();
    let mut txn = db.begin_transaction().await?;

    let coin_service: types::Coin = provider.provide();
    let balance = coin_service
        .add_coins(
            &mut txn,
            user_id.into(),
            coins,
            withhold,
            description.map(TryInto::try_into).transpose()?,
            (coins > 0) && !no_credit_note,
        )
        .await?;

    println!(
        "New balance: {} (withheld: {})",
        balance.coins, balance.withheld_coins
    );

    txn.commit().await?;

    Ok(())
}
