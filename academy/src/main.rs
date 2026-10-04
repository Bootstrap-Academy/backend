use academy::commands::{
    admin::AdminCommand, email::EmailCommand, jwt::JwtCommand, migrate::MigrateCommand,
    serve::serve, tasks::TaskCommand,
};
use academy::telemetry;
use academy_utils::{academy_version, bin_name};
use anyhow::Context;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::CompleteEnv;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    CompleteEnv::with_factory(Cli::command).complete();

    let cli = Cli::parse();

    telemetry::init_tracing();

    let config = academy_config::load().context("Failed to load config")?;

    let _sentry_guard = config.sentry.as_ref().map(|sentry_config| {
        sentry::init((
            sentry_config.dsn.as_str(),
            sentry::ClientOptions {
                release: Some(academy_version().into()),
                ..telemetry::options()
            },
        ))
    });

    match cli.command {
        Command::Serve => serve(config).await?,
        Command::Migrate { command } => command.invoke(config).await?,
        Command::Admin { command } => command.invoke(config).await?,
        Command::Jwt { command } => command.invoke(config).await?,
        Command::Email { command } => command.invoke(config).await?,
        Command::Task { command } => command.invoke(config).await?,
        Command::CheckConfig { verbose } => {
            verbose.then(|| println!("{config:#?}"));
        }
    }

    Ok(())
}

#[derive(Debug, Parser)]
#[command(name = bin_name!(), version = academy_version())]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the REST API server to serve the Bootstrap Academy backend
    #[command(aliases(["run", "start", "r", "s"]))]
    Serve,
    /// Manage database and migrations
    #[command(aliases(["mig", "m"]))]
    Migrate {
        #[command(subcommand)]
        command: MigrateCommand,
    },
    /// Perform administrative actions
    #[command(aliases(["a"]))]
    Admin {
        #[command(subcommand)]
        command: AdminCommand,
    },
    /// Issue JSON Web Tokens
    #[command(aliases(["j"]))]
    Jwt {
        #[command(subcommand)]
        command: JwtCommand,
    },
    /// Test email deliverability
    #[command(aliases(["e"]))]
    Email {
        #[command(subcommand)]
        command: EmailCommand,
    },
    /// Invoke scheduled tasks
    #[command(aliases(["t"]))]
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    /// Validate configuration
    CheckConfig {
        /// Print a debug representation of the config
        #[arg(short, long)]
        verbose: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli() {
        Cli::command().debug_assert();
    }
}
