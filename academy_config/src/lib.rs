use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    path::Path,
};

use academy_models::{email_address::EmailAddressWithName, mfa::TotpSecretLength, url::Url};
use anyhow::Context;
use config::{File, FileFormat};
use duration::Duration;
use serde::Deserialize;

pub mod duration;

const DEFAULT_CONFIG: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../config.toml"));
const DEV_CONFIG_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../config.dev.toml");

pub const ENVIRONMENT_VARIABLE: &str = "ACADEMY_CONFIG";

pub fn load() -> anyhow::Result<Config> {
    load_paths(&parse_env_var()?, &[])
}

pub fn load_with_overrides(overrides: &[&str]) -> anyhow::Result<Config> {
    load_paths(&parse_env_var()?, overrides)
}

pub fn load_dev_config() -> anyhow::Result<Config> {
    load_paths(&[DEV_CONFIG_PATH], &[])
}

fn parse_env_var() -> anyhow::Result<Vec<String>> {
    let env_var = std::env::var(ENVIRONMENT_VARIABLE)
        .with_context(|| format!("Failed to load environment variable {ENVIRONMENT_VARIABLE}"))?;
    Ok(env_var.split(':').rev().map(Into::into).collect())
}

fn load_paths(paths: &[impl AsRef<Path>], overrides: &[&str]) -> anyhow::Result<Config> {
    let mut builder =
        config::Config::builder().add_source(File::from_str(DEFAULT_CONFIG, FileFormat::Toml));

    for path in paths {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file at {}", path.display()))?;
        let source = File::from_str(&content, FileFormat::Toml);
        builder = builder.add_source(source);
    }

    for content in overrides {
        let source = File::from_str(content, FileFormat::Toml);
        builder = builder.add_source(source);
    }

    let mut config = builder
        .build()?
        .try_deserialize::<Config>()
        .context("Failed to load config")?;

    config
        .recaptcha
        .take_if(|recaptcha| recaptcha.enable == Some(false));

    config.sentry.take_if(|sentry| sentry.enable == Some(false));

    if let Some(oauth2) = &mut config.oauth2 {
        oauth2.providers.retain(|_, p| p.enable != Some(false));
    }
    config
        .oauth2
        .take_if(|oauth2| oauth2.enable == Some(false) || oauth2.providers.is_empty());

    Ok(config)
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub http: HttpConfig,
    pub database: DatabaseConfig,
    pub cache: CacheConfig,
    pub email: EmailConfig,
    pub jwt: JwtConfig,
    pub internal: InternalConfig,
    pub health: HealthConfig,
    pub user: UserConfig,
    pub session: SessionConfig,
    pub totp: TotpConfig,
    pub contact: ContactConfig,
    pub recaptcha: Option<RecaptchaConfig>,
    pub vat: VatConfig,
    pub paypal: PaypalConfig,
    pub coin: CoinConfig,
    pub sentry: Option<SentryConfig>,
    pub oauth2: Option<OAuth2Config>,
}

#[derive(Debug, Deserialize)]
pub struct HttpConfig {
    pub address: SocketAddr,
    pub real_ip: Option<HttpRealIpConfig>,
}

#[derive(Debug, Deserialize)]
pub struct HttpRealIpConfig {
    pub header: String,
    pub set_from: IpAddr,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout: Duration,
    pub idle_timeout: Option<Duration>,
    pub max_lifetime: Option<Duration>,
}

#[derive(Debug, Deserialize)]
pub struct CacheConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout: Duration,
    pub idle_timeout: Option<Duration>,
    pub max_lifetime: Option<Duration>,
}

#[derive(Debug, Deserialize)]
pub struct EmailConfig {
    pub smtp_url: String,
    pub from: EmailAddressWithName,
}

#[derive(Debug, Deserialize)]
pub struct JwtConfig {
    pub secret: String,
}

#[derive(Debug, Deserialize)]
pub struct InternalConfig {
    pub jwt_ttl: Duration,
}

#[derive(Debug, Deserialize)]
pub struct HealthConfig {
    pub database_cache_ttl: Duration,
    pub cache_cache_ttl: Duration,
    pub email_cache_ttl: Duration,
}

#[derive(Debug, Deserialize)]
pub struct UserConfig {
    pub name_change_rate_limit: Duration,
    pub verification_code_ttl: Duration,
    pub verification_redirect_url: String,
    pub password_reset_code_ttl: Duration,
    pub password_reset_redirect_url: String,
    pub newsletter_code_ttl: Duration,
    pub newsletter_redirect_url: String,
}

#[derive(Debug, Deserialize)]
pub struct SessionConfig {
    pub access_token_ttl: Duration,
    pub refresh_token_ttl: Duration,
    pub refresh_token_length: usize,
    pub login_fails_before_captcha: u64,
}

#[derive(Debug, Deserialize)]
pub struct TotpConfig {
    pub secret_length: TotpSecretLength,
}

#[derive(Debug, Deserialize)]
pub struct ContactConfig {
    pub email: EmailAddressWithName,
}

#[derive(Debug, Deserialize)]
pub struct RecaptchaConfig {
    pub enable: Option<bool>,
    pub siteverify_endpoint_override: Option<Url>,
    pub sitekey: String,
    pub secret: String,
    pub min_score: f64,
}

#[derive(Debug, Deserialize)]
pub struct VatConfig {
    pub validate_endpoint_override: Option<Url>,
}

#[derive(Debug, Deserialize)]
pub struct PaypalConfig {
    pub base_url_override: Option<Url>,
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Deserialize)]
pub struct CoinConfig {
    pub purchase_min: u64,
    pub purchase_max: u64,
}

#[derive(Debug, Deserialize)]
pub struct SentryConfig {
    pub enable: Option<bool>,
    pub dsn: Url,
}

#[derive(Debug, Deserialize)]
pub struct OAuth2Config {
    pub enable: Option<bool>,
    pub registration_token_ttl: Duration,
    pub providers: HashMap<String, OAuth2ProviderConfig>,
}

#[derive(Debug, Deserialize)]
pub struct OAuth2ProviderConfig {
    pub enable: Option<bool>,
    pub name: String,
    pub client_id: String,
    pub client_secret: String,
    pub auth_url: Url,
    pub token_url: Url,
    pub userinfo_url: Url,
    pub userinfo_id_key: String,
    pub userinfo_name_key: String,
    pub scopes: Vec<String>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn load_dev_config() {
        super::load_dev_config().unwrap();
    }
}
