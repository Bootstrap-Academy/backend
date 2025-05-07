use academy::commands::{
    admin::AdminCommand, email::EmailCommand, jwt::JwtCommand, migrate::MigrateCommand,
    serve::serve, tasks::TaskCommand,
};
use academy_utils::{academy_version, bin_name};
use anyhow::Context;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::CompleteEnv;
use sentry::integrations::tracing::EventFilter;
use tracing::Level;
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    CompleteEnv::with_factory(Cli::command).complete();

    let cli = Cli::parse();

    init_tracing();

    let config = academy_config::load().context("Failed to load config")?;

    let _sentry_guard = config.sentry.as_ref().map(|sentry_config| {
        sentry::init((
            sentry_config.dsn.as_str(),
            sentry::ClientOptions {
                release: Some(academy_version().into()),
                attach_stacktrace: true,
                ..Default::default()
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

fn init_tracing() {
    let fmt_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);

    #[cfg(tracing_pretty)]
    let fmt_layer = fmt_layer.pretty();

    tracing_subscriber::registry()
        .with(fmt_layer.with_filter(EnvFilter::from_default_env()))
        .with(
            sentry::integrations::tracing::layer().event_filter(|meta| match *meta.level() {
                Level::ERROR => EventFilter::Exception,
                Level::WARN => EventFilter::Event,
                Level::INFO | Level::DEBUG => EventFilter::Breadcrumb,
                Level::TRACE => EventFilter::Ignore,
            }),
        )
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli() {
        Cli::command().debug_assert();
    }
}
