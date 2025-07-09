use academy_config::Config;
use clap::Subcommand;
use coin::AdminCoinCommand;
use invoice::AdminInvoiceCommand;
use user::AdminUserCommand;

use crate::commands::admin::session::AdminSessionCommand;

mod coin;
mod invoice;
mod session;
mod user;

#[derive(Debug, Subcommand)]
pub enum AdminCommand {
    /// Manage user accounts
    #[command(aliases(["u"]))]
    User {
        #[command(subcommand)]
        command: AdminUserCommand,
    },
    /// Manage sessions
    #[command(aliases(["s"]))]
    Session {
        #[command(subcommand)]
        command: AdminSessionCommand,
    },
    /// Manage invoices
    #[command(aliases(["i"]))]
    Invoice {
        #[command(subcommand)]
        command: AdminInvoiceCommand,
    },
    /// Manage coins
    #[command(aliases(["c"]))]
    Coin {
        #[command(subcommand)]
        command: AdminCoinCommand,
    },
}

impl AdminCommand {
    pub async fn invoke(self, config: Config) -> anyhow::Result<()> {
        match self {
            AdminCommand::User { command } => command.invoke(config).await,
            AdminCommand::Session { command } => command.invoke(config).await,
            AdminCommand::Invoice { command } => command.invoke(config).await,
            AdminCommand::Coin { command } => command.invoke(config).await,
        }
    }
}
