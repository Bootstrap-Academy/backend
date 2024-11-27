use academy_config::Config;
use clap::Subcommand;
use invoice::AdminInvoiceCommand;
use user::AdminUserCommand;

mod invoice;
mod user;

#[derive(Debug, Subcommand)]
pub enum AdminCommand {
    /// Manage user accounts
    #[command(aliases(["u"]))]
    User {
        #[command(subcommand)]
        command: AdminUserCommand,
    },
    /// Manage invoices
    #[command(aliases(["i"]))]
    Invoice {
        #[command(subcommand)]
        command: AdminInvoiceCommand,
    },
}

impl AdminCommand {
    pub async fn invoke(self, config: Config) -> anyhow::Result<()> {
        match self {
            AdminCommand::User { command } => command.invoke(config).await,
            AdminCommand::Invoice { command } => command.invoke(config).await,
        }
    }
}
