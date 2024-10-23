use std::{collections::HashMap, sync::Arc};

use academy_api_rest::{RestServerConfig, RestServerRealIpConfig};
use academy_auth_impl::AuthServiceConfig;
use academy_config::Config;
use academy_core_contact_impl::ContactFeatureConfig;
use academy_core_health_impl::HealthFeatureConfig;
use academy_core_oauth2_impl::OAuth2FeatureConfig;
use academy_core_paypal_impl::PaypalFeatureConfig;
use academy_core_session_impl::SessionFeatureConfig;
use academy_core_user_impl::UserFeatureConfig;
use academy_di::provider;
use academy_extern_impl::{
    paypal::PaypalApiServiceConfig, recaptcha::RecaptchaApiServiceConfig, vat::VatApiServiceConfig,
};
use academy_models::oauth2::OAuth2Provider;
use academy_shared_impl::{
    captcha::{CaptchaServiceConfig, RecaptchaCaptchaServiceConfig},
    jwt::JwtServiceConfig,
    totp::TotpServiceConfig,
};
use types::{Cache, Database, Email};

pub mod types;

provider! {
    /// The default provider, capable of providing all the dependencies
    pub Provider {
        database: Database,
        cache: Cache,
        email: Email,
        ..config: ConfigProvider {
            // API
            RestServerConfig,

            // Extern
            RecaptchaApiServiceConfig,
            VatApiServiceConfig,
            PaypalApiServiceConfig,

            // Shared
            CaptchaServiceConfig,
            JwtServiceConfig,
            OAuth2FeatureConfig,
            TotpServiceConfig,

            // Auth
            AuthServiceConfig,

            // Core
            ContactFeatureConfig,
            HealthFeatureConfig,
            SessionFeatureConfig,
            UserFeatureConfig,
            PaypalFeatureConfig,
        }
    }
}

impl Provider {
    pub fn new(config: ConfigProvider, database: Database, cache: Cache, email: Email) -> Self {
        Self {
            _cache: Default::default(),
            database,
            cache,
            email,
            config,
        }
    }
}

provider! {
    /// Reduced provider, capable of providing services that only depend on the configuration
    pub ConfigProvider {
        // API
        rest_server_config: RestServerConfig,

        // Extern
        recaptcha_api_service_config: RecaptchaApiServiceConfig,
        vat_api_service_config: VatApiServiceConfig,
        paypal_api_service_config: PaypalApiServiceConfig,

        // Shared
        captcha_service_config: CaptchaServiceConfig,
        jwt_service_config: JwtServiceConfig,
        oauth2_service_config: OAuth2FeatureConfig,
        totp_service_config: TotpServiceConfig,

        // Auth
        auth_service_config: AuthServiceConfig,

        // Core
        contact_feature_config: ContactFeatureConfig,
        health_feature_config: HealthFeatureConfig,
        session_feature_config: SessionFeatureConfig,
        user_feature_config: UserFeatureConfig,
        paypal_feature_config: PaypalFeatureConfig,
    }
}

impl ConfigProvider {
    pub fn new(config: &Config) -> anyhow::Result<Self> {
        // API
        let rest_server_config = RestServerConfig {
            addr: config.http.address,
            real_ip_config: config.http.real_ip.as_ref().map(|real_ip_config| {
                Arc::new(RestServerRealIpConfig {
                    header: real_ip_config.header.clone(),
                    set_from: real_ip_config.set_from,
                })
            }),
        };

        // Extern
        let recaptcha_api_service_config = RecaptchaApiServiceConfig::new(
            config
                .recaptcha
                .as_ref()
                .and_then(|recaptcha| recaptcha.siteverify_endpoint_override.clone()),
        );

        let vat_api_service_config =
            VatApiServiceConfig::new(config.vat.validate_endpoint_override.clone());

        let paypal_api_service_config = PaypalApiServiceConfig::new(
            config.paypal.base_url_override.clone(),
            config.paypal.client_id.clone(),
            config.paypal.client_secret.clone(),
        );

        // Shared
        let captcha_service_config = match config.recaptcha.as_ref() {
            Some(recaptcha) => CaptchaServiceConfig::Recaptcha(RecaptchaCaptchaServiceConfig {
                sitekey: recaptcha.sitekey.clone().into(),
                secret: recaptcha.secret.clone().into(),
                min_score: recaptcha.min_score,
            }),
            None => CaptchaServiceConfig::Disabled,
        };

        let jwt_service_config = JwtServiceConfig::new(&config.jwt.secret)?;

        let oauth2_service_config = OAuth2FeatureConfig {
            registration_token_ttl: config
                .oauth2
                .as_ref()
                .map(|oauth2| oauth2.registration_token_ttl.0)
                .unwrap_or_default(),
            providers: config
                .oauth2
                .iter()
                .flat_map(|oauth2| oauth2.providers.iter())
                .map(|(id, provider)| {
                    (
                        id.clone().into(),
                        OAuth2Provider {
                            name: provider.name.clone().into(),
                            client_id: provider.client_id.clone(),
                            client_secret: Some(provider.client_secret.clone().into()),
                            auth_url: provider.auth_url.clone(),
                            token_url: provider.token_url.clone(),
                            userinfo_url: provider.userinfo_url.clone(),
                            userinfo_id_key: provider.userinfo_id_key.clone(),
                            userinfo_name_key: provider.userinfo_name_key.clone(),
                            scopes: provider.scopes.clone(),
                        },
                    )
                })
                .collect::<HashMap<_, _>>()
                .into(),
        };

        let totp_service_config = TotpServiceConfig {
            secret_length: config.totp.secret_length,
        };

        // Auth
        let auth_service_config = AuthServiceConfig {
            access_token_ttl: config.session.access_token_ttl.into(),
            refresh_token_ttl: config.session.refresh_token_ttl.into(),
            refresh_token_length: config.session.refresh_token_length,
            internal_token_ttl: config.internal.jwt_ttl.into(),
        };

        // Core
        let contact_feature_config = ContactFeatureConfig {
            email: config.contact.email.clone().into(),
        };

        let health_feature_config = HealthFeatureConfig {
            database_cache_ttl: config.health.database_cache_ttl.into(),
            cache_cache_ttl: config.health.cache_cache_ttl.into(),
            email_cache_ttl: config.health.email_cache_ttl.into(),
        };

        let session_feature_config = SessionFeatureConfig {
            login_fails_before_captcha: config.session.login_fails_before_captcha,
        };

        let user_feature_config = UserFeatureConfig {
            name_change_rate_limit: config.user.name_change_rate_limit.into(),
            verification_redirect_url: config.user.verification_redirect_url.clone().into(),
            verification_verification_code_ttl: config.user.verification_code_ttl.into(),
            password_reset_redirect_url: config.user.password_reset_redirect_url.clone().into(),
            password_reset_verification_code_ttl: config.user.password_reset_code_ttl.into(),
            newsletter_subscription_redirect_url: config
                .user
                .newsletter_redirect_url
                .clone()
                .into(),
            newsletter_subscription_verification_code_ttl: config.user.newsletter_code_ttl.into(),
        };

        let paypal_feature_config = PaypalFeatureConfig {
            purchase_range: config.coin.purchase_min..=config.coin.purchase_max,
        };

        Ok(Self {
            _cache: Default::default(),

            // API
            rest_server_config,

            // Extern
            recaptcha_api_service_config,
            vat_api_service_config,
            paypal_api_service_config,

            // Shared
            jwt_service_config,
            totp_service_config,
            captcha_service_config,
            oauth2_service_config,

            // Auth
            auth_service_config,

            // Core
            contact_feature_config,
            health_feature_config,
            session_feature_config,
            user_feature_config,
            paypal_feature_config,
        })
    }
}

#[cfg(test)]
mod tests {
    use academy_cache_valkey::ValkeyCache;
    use academy_di::Provide;
    use academy_email_impl::EmailServiceImpl;
    use academy_persistence_postgres::PostgresDatabase;
    use types::RestServer;

    use super::*;

    #[tokio::test]
    async fn provide_rest_server() {
        let config = academy_config::load_dev_config().unwrap();
        let config_provider = ConfigProvider::new(&config).unwrap();

        let database = PostgresDatabase::dummy().await;
        let cache = ValkeyCache::dummy().await;
        let email = EmailServiceImpl::dummy().await;

        let mut provider = Provider::new(config_provider, database, cache, email);
        let _: RestServer = provider.provide();
    }
}
