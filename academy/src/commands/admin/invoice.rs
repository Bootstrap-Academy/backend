use academy_config::Config;
use academy_core_finance_contracts::invoice::FinanceInvoiceService;
use academy_di::Provide;
use academy_persistence_contracts::{Database, paypal::PaypalRepository};
use clap::Subcommand;
use futures::TryStreamExt;
use indicatif::ProgressBar;

use crate::{
    cache, database, email,
    environment::{ConfigProvider, Provider, types},
};

#[derive(Debug, Subcommand)]
pub enum AdminInvoiceCommand {
    /// Generate missing invoice pdf files
    #[command(aliases(["g"]))]
    Generate,
}

impl AdminInvoiceCommand {
    pub async fn invoke(self, config: Config) -> anyhow::Result<()> {
        match self {
            AdminInvoiceCommand::Generate => generate(config).await,
        }
    }
}

async fn generate(config: Config) -> anyhow::Result<()> {
    let database = database::connect(&config.database).await?;
    let cache = cache::connect(&config.cache).await?;
    let email_service = email::connect(&config.email).await?;
    let config_provider = ConfigProvider::new(&config)?;
    let mut provider = Provider::new(config_provider, database, cache, email_service);

    let db: types::Database = provider.provide();
    let mut txn = db.begin_transaction().await?;

    let finance_invoice_service: types::FinanceInvoice = provider.provide();
    let paypal_repo: types::PaypalRepo = provider.provide();

    let cnt = paypal_repo.count_coin_orders(&mut txn).await?;
    let bar = ProgressBar::new(cnt);
    let mut stream = std::pin::pin!(paypal_repo.stream_coin_orders(&mut txn));
    let mut txn = db.begin_transaction().await?;
    while let Some(coin_order) = stream.try_next().await? {
        finance_invoice_service
            .get_invoice_pdf(&mut txn, None, coin_order.invoice_number)
            .await?;
        bar.inc(1);
    }

    Ok(())
}
