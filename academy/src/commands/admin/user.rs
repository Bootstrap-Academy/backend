use academy_config::Config;
use academy_core_user_contracts::user::{UserCreateCommand, UserService};
use academy_di::Provide;
use academy_persistence_contracts::{Database as _, Transaction};
use anyhow::Context;
use clap::Subcommand;
use tracing::info;

use crate::environment::{
    Provider,
    types::{self, Database},
};

#[derive(Debug, Subcommand)]
pub enum AdminUserCommand {
    /// Create a new user account
    #[command(aliases(["c", "new", "n", "+"]))]
    Create {
        /// Grant the new user admin privileges
        #[arg(long)]
        admin: bool,
        /// Disable the new user account
        #[arg(long)]
        disabled: bool,
        /// Mark the email address of the new user as verified
        #[arg(long)]
        verified: bool,
        /// The login and display name of the new user
        name: String,
        /// The email address of the new user
        email: String,
        /// The password of the new user
        password: String,
    },
}

impl AdminUserCommand {
    pub async fn invoke(self, config: Config) -> anyhow::Result<()> {
        match self {
            AdminUserCommand::Create {
                admin,
                name,
                email,
                password,
                disabled,
                verified,
            } => create(config, name, email, password, admin, !disabled, verified).await,
        }
    }
}

async fn create(
    config: Config,
    name: String,
    email: String,
    password: String,
    admin: bool,
    enabled: bool,
    email_verified: bool,
) -> anyhow::Result<()> {
    let mut provider = Provider::from_config(&config).await?;

    let db: Database = provider.provide();
    let mut txn = db.begin_transaction().await?;

    let user_service: types::User = provider.provide();
    let user = user_service
        .create(
            &mut txn,
            UserCreateCommand {
                name: name.clone().try_into()?,
                display_name: name.try_into()?,
                email: email.parse()?,
                password: Some(password.try_into()?),
                admin,
                enabled,
                email_verified,
                oauth2_registration: None,
                terms_version: None,
                age_confirmed: false,
            },
        )
        .await
        .context("Failed to create user")?;

    txn.commit().await?;

    info!("User has been created:\n{user:#?}");

    Ok(())
}
