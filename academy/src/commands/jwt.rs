use std::time::Duration;

use academy_config::Config;
use academy_di::Provide;
use academy_shared_contracts::jwt::JwtService;
use anyhow::Context;
use clap::Subcommand;

use crate::environment::{types::Jwt, ConfigProvider};

#[derive(Debug, Subcommand)]
pub enum JwtCommand {
    /// Sign a JWT
    Sign {
        /// The time to live in seconds
        #[arg(long, default_value = "3600")]
        ttl: u64,
        /// The JSON data to sign
        data: String,
    },
}

impl JwtCommand {
    pub async fn invoke(self, config: Config) -> anyhow::Result<()> {
        match self {
            JwtCommand::Sign { ttl, data } => sign(&config, &data, Duration::from_secs(ttl)),
        }
    }
}

fn sign(config: &Config, data: &str, ttl: Duration) -> anyhow::Result<()> {
    let mut provider = ConfigProvider::new(config)?;
    let jwt_service: Jwt = provider.provide();

    let data = serde_json::from_str::<serde_json::Value>(data)
        .context("Failed to parse the payload as json")?;
    let jwt = jwt_service.sign(data, ttl)?;
    println!("{jwt}");

    Ok(())
}
