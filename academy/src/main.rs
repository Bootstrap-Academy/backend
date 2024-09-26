use academy::commands::{
    admin::AdminCommand, email::EmailCommand, jwt::JwtCommand, migrate::MigrateCommand,
    serve::serve, tasks::TaskCommand,
};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use sentry::integrations::tracing::EventFilter;
use tracing::Level;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if let Command::Completion { shell } = cli.command {
        clap_complete::generate(
            shell,
            &mut Cli::command(),
            env!("CARGO_BIN_NAME"),
            &mut std::io::stdout(),
        );
        return Ok(());
    }

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(EnvFilter::from_default_env()))
        .with(
            sentry::integrations::tracing::layer().event_filter(|meta| match *meta.level() {
                Level::ERROR => EventFilter::Exception,
                Level::WARN => EventFilter::Event,
                Level::INFO | Level::DEBUG | Level::TRACE => EventFilter::Breadcrumb,
            }),
        )
        .init();

    let config = academy_config::load()?;

    let _sentry_guard = config.sentry.as_ref().map(|sentry_config| {
        sentry::init((
            sentry_config.dsn.as_str(),
            sentry::ClientOptions {
                release: Some(env!("CARGO_PKG_VERSION").into()),
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
            if verbose {
                println!("{config:#?}");
            }
        }
        Command::Completion { .. } => unreachable!(),
    }

    Ok(())
}

#[derive(Debug, Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the backend server
    #[command(aliases(["run", "start", "r", "s"]))]
    Serve,
    /// Run database migrations
    #[command(aliases(["mig", "m"]))]
    Migrate {
        #[command(subcommand)]
        command: MigrateCommand,
    },
    #[command(aliases(["a"]))]
    Admin {
        #[command(subcommand)]
        command: AdminCommand,
    },
    #[command(aliases(["j"]))]
    Jwt {
        #[command(subcommand)]
        command: JwtCommand,
    },
    #[command(aliases(["e"]))]
    Email {
        #[command(subcommand)]
        command: EmailCommand,
    },
    #[command(aliases(["t"]))]
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    /// Validate config files
    CheckConfig {
        #[arg(short, long)]
        verbose: bool,
    },
    /// Generate shell completions
    Completion {
        /// The shell to generate completions for
        #[clap(value_enum)]
        shell: Shell,
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
