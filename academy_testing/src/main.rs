use std::net::IpAddr;

use academy_testing::{microservices, oauth2, paypal, recaptcha, vat};
use academy_utils::{academy_version, bin_name};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::CompleteEnv;
use url::Url;

const _: () = {
    assert!(!env!("CARGO_PKG_HOMEPAGE").is_empty());
    assert!(!env!("CARGO_PKG_REPOSITORY").is_empty());
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    CompleteEnv::with_factory(Cli::command).complete();

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
        Command::Paypal {
            host,
            port,
            client_id,
            client_secret,
        } => paypal::start_server(host, port, client_id, client_secret).await?,
        Command::Microservices { host, port } => microservices::start_server(host, port).await?,
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
    /// Start the recaptcha testing server
    Recaptcha {
        #[arg(long, default_value = "127.0.0.1")]
        host: IpAddr,
        #[arg(long)]
        port: u16,
        #[arg(long, default_value = "test-secret")]
        secret: String,
    },
    /// Start the oauth2 testing server
    #[clap(name = "oauth2")]
    OAuth2 {
        #[arg(long, default_value = "127.0.0.1")]
        host: IpAddr,
        #[arg(long)]
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
        #[arg(long)]
        port: u16,
    },
    /// Start the paypal testing server
    Paypal {
        #[arg(long, default_value = "127.0.0.1")]
        host: IpAddr,
        #[arg(long)]
        port: u16,
        #[arg(long, default_value = "test-client")]
        client_id: String,
        #[arg(long, default_value = "test-secret")]
        client_secret: String,
    },
    /// Start the microservices testing server
    Microservices {
        #[arg(long, default_value = "127.0.0.1")]
        host: IpAddr,
        #[arg(long)]
        port: u16,
    },
}
