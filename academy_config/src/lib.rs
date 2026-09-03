use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
};

use academy_assets::CONFIG_TOML;
use academy_models::{email_address::EmailAddressWithName, mfa::TotpSecretLength, url::Url};
use anyhow::Context;
use chrono::NaiveTime;
use config::{File, FileFormat};
use duration::Duration;
use regex::bytes::RegexSet;
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer};

pub mod duration;

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
        config::Config::builder().add_source(File::from_str(CONFIG_TOML, FileFormat::Toml));

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
    pub heart: HeartConfig,
    pub premium: PremiumConfig,
    pub render: RenderConfig,
    pub microservices: MicroservicesConfig,
    pub finance: FinanceConfig,
    pub sentry: Option<SentryConfig>,
    pub oauth2: Option<OAuth2Config>,
}

#[derive(Debug, Deserialize)]
pub struct HttpConfig {
    pub address: SocketAddr,
    pub real_ip: Option<HttpRealIpConfig>,
    #[serde(deserialize_with = "deserialize_regex_set")]
    pub allowed_origins: RegexSet,
}

fn deserialize_regex_set<'de, D>(deserializer: D) -> Result<RegexSet, D::Error>
where
    D: Deserializer<'de>,
{
    let regexes = Vec::<String>::deserialize(deserializer)?;
    RegexSet::new(regexes).map_err(serde::de::Error::custom)
}

#[derive(Debug, Deserialize)]
pub struct HttpRealIpConfig {
    pub header: String,
    pub set_from: Option<IpAddr>,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout: Duration,
    pub idle_timeout: Option<Duration>,
    pub max_lifetime: Option<Duration>,
    pub run_migrations: bool,
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
    pub download_token_ttl: Duration,
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
pub struct HeartConfig {
    pub max: u64,
    pub refill_price: u64,
    pub auto_refill_time: NaiveTime,
}

#[derive(Debug, Deserialize)]
pub struct PremiumConfig {
    pub monthly_price: u64,
    pub yearly_price: u64,
}

#[derive(Debug, Deserialize)]
pub struct RenderConfig {
    pub daemon_url: Url,
}

#[derive(Debug, Deserialize)]
pub struct MicroservicesConfig {
    #[serde(default, deserialize_with = "deserialize_optional_url")]
    pub skills_url: Option<Url>,
    #[serde(default, deserialize_with = "deserialize_optional_url")]
    pub challenges_url: Option<Url>,
    #[serde(default, deserialize_with = "deserialize_optional_url")]
    pub events_url: Option<Url>,
    pub timeout: Duration,
}

/// Deserialize an optional [`Url`], treating an empty string like a missing
/// value.
fn deserialize_optional_url<'de, D>(deserializer: D) -> Result<Option<Url>, D::Error>
where
    D: Deserializer<'de>,
{
    let Some(url) = Option::<String>::deserialize(deserializer)? else {
        return Ok(None);
    };

    let url = url.trim();
    if url.is_empty() {
        return Ok(None);
    }

    url.parse().map(Some).map_err(serde::de::Error::custom)
}

#[derive(Debug, Deserialize)]
pub struct FinanceConfig {
    pub vat_percent: Decimal,
    pub invoices_archive: PathBuf,
    pub credit_notes_archive: PathBuf,
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
    /// The minimal set of properties that have no default value and therefore
    /// need to be set by the deployment.
    const MINIMAL_OVERRIDES: [&str; 20] = [
        "http.address = \"0.0.0.0:8000\"",
        "database.url = \"\"",
        "cache.url = \"\"",
        "email.smtp_url = \"\"",
        "email.from = \"Test <test@example.com>\"",
        "jwt.secret = \"\"",
        "contact.email = \"test@example.com\"",
        "recaptcha.sitekey = \"\"",
        "recaptcha.secret = \"\"",
        "paypal.client_id = \"\"",
        "paypal.client_secret = \"\"",
        "render.daemon_url = \"http://localhost:8001\"",
        "finance.invoices_archive = \"\"",
        "finance.credit_notes_archive = \"\"",
        "oauth2.providers.github.client_id = \"\"",
        "oauth2.providers.github.client_secret = \"\"",
        "oauth2.providers.discord.client_id = \"\"",
        "oauth2.providers.discord.client_secret = \"\"",
        "oauth2.providers.google.client_id = \"\"",
        "oauth2.providers.google.client_secret = \"\"",
    ];

    #[test]
    fn load_dev_config() {
        super::load_dev_config().unwrap();
    }

    #[test]
    fn load_minimal_config() {
        let overrides = MINIMAL_OVERRIDES;

        super::load_paths(&[] as &[&str], &overrides).unwrap();

        for i in 0..overrides.len() {
            let filtered_overrides = overrides
                .into_iter()
                .take(i)
                .chain(overrides.into_iter().skip(i + 1))
                .collect::<Vec<_>>();
            assert!(
                super::load_paths(&[] as &[&str], &filtered_overrides).is_err(),
                "override \"{}\" is not needed",
                overrides[i]
            );
        }
    }

    #[test]
    fn microservices_disabled_by_default() {
        let config = super::load_paths(&[] as &[&str], &MINIMAL_OVERRIDES).unwrap();

        assert!(config.microservices.skills_url.is_none());
        assert!(config.microservices.challenges_url.is_none());
        assert!(config.microservices.events_url.is_none());
    }

    #[test]
    fn empty_microservice_url() {
        let overrides = MINIMAL_OVERRIDES
            .into_iter()
            .chain([
                "microservices.skills_url = \"\"",
                "microservices.challenges_url = \"http://localhost:8001/\"",
            ])
            .collect::<Vec<_>>();

        let config = super::load_paths(&[] as &[&str], &overrides).unwrap();

        assert!(config.microservices.skills_url.is_none());
        assert_eq!(
            config.microservices.challenges_url.unwrap().as_str(),
            "http://localhost:8001/"
        );
    }
}
