use academy_config::Config;
use academy_core_session_contracts::session::SessionService;
use academy_di::Provide;
use academy_models::user::UserName;
use academy_persistence_contracts::{Database as _, Transaction, user::UserRepository};
use anyhow::{Context, anyhow};
use clap::Subcommand;

use crate::environment::{
    Provider,
    types::{self, Database, UserRepo},
};

#[derive(Debug, Subcommand)]
pub enum AdminSessionCommand {
    /// Impersonate a user
    #[command(aliases(["i", "login", "l"]))]
    Impersonate {
        /// The login name of the user to impersonate
        name: String,
    },
}

impl AdminSessionCommand {
    pub async fn invoke(self, config: Config) -> anyhow::Result<()> {
        match self {
            AdminSessionCommand::Impersonate { name } => impersonate(config, name).await,
        }
    }
}

async fn impersonate(config: Config, name: String) -> anyhow::Result<()> {
    let name = UserName::try_new(name)?;

    let mut provider = Provider::from_config(&config).await?;

    let db: Database = provider.provide();
    let mut txn = db.begin_transaction().await?;

    let user_repo: UserRepo = provider.provide();
    let user_composite = user_repo
        .get_composite_by_name(&mut txn, &name)
        .await?
        .ok_or_else(|| anyhow!("User does not exist"))?;

    let session_service: types::Session = provider.provide();
    let login = session_service
        .create(&mut txn, user_composite, None, false)
        .await
        .context("Failed to create session")?;

    eprintln!("{login:#?}");
    println!("{}", *login.access_token);

    txn.commit().await?;

    Ok(())
}
