use std::net::IpAddr;

use academy_testing::{internal, oauth2, paypal, recaptcha, vat};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use url::Url;

const _: () = {
    assert!(!env!("CARGO_PKG_HOMEPAGE").is_empty());
    assert!(!env!("CARGO_PKG_REPOSITORY").is_empty());
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Command::Recaptcha { host, port, secret } => {
            recaptcha::start_server(host, port, secret).await?
        }
        Command::OAuth2 {
            host,
            port,
            client_id,
            client_secret,
            redirect_url,
        } => oauth2::start_server(host, port, client_id, client_secret, redirect_url).await?,
        Command::Vat { host, port } => vat::start_server(host, port).await?,
        Command::Internal { host, port } => internal::start_server(host, port).await?,
        Command::Paypal {
            host,
            port,
            client_id,
            client_secret,
        } => paypal::start_server(host, port, client_id, client_secret).await?,
        Command::Completion { shell } => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                env!("CARGO_BIN_NAME"),
                &mut std::io::stdout(),
            );
        }
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
    /// Start the recaptcha testing server
    Recaptcha {
        #[arg(long, default_value = "127.0.0.1")]
        host: IpAddr,
        #[arg(long, default_value = "8001")]
        port: u16,
        #[arg(long, default_value = "test-secret")]
        secret: String,
    },
    /// Start the oauth2 testing server
    #[clap(name = "oauth2")]
    OAuth2 {
        #[arg(long, default_value = "127.0.0.1")]
        host: IpAddr,
        #[arg(long, default_value = "8002")]
        port: u16,
        #[arg(long, default_value = "client-id")]
        client_id: String,
        #[arg(long, default_value = "client-secret")]
        client_secret: String,
        #[arg(long, default_value = "http://localhost/oauth2/callback")]
        redirect_url: Url,
    },
    /// Start the vat api testing server
    Vat {
        #[arg(long, default_value = "127.0.0.1")]
        host: IpAddr,
        #[arg(long, default_value = "8003")]
        port: u16,
    },
    /// Start the internal api testing server
    Internal {
        #[arg(long, default_value = "127.0.0.1")]
        host: IpAddr,
        #[arg(long, default_value = "8004")]
        port: u16,
    },
    /// Start the paypal testing server
    Paypal {
        #[arg(long, default_value = "127.0.0.1")]
        host: IpAddr,
        #[arg(long, default_value = "8005")]
        port: u16,
        #[arg(long, default_value = "test-client")]
        client_id: String,
        #[arg(long, default_value = "test-secret")]
        client_secret: String,
    },
    /// Generate shell completions
    Completion {
        /// The shell to generate completions for
        #[clap(value_enum)]
        shell: Shell,
    },
}
