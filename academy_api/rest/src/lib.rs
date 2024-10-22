use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use academy_core_coin_contracts::CoinFeatureService;
use academy_core_config_contracts::ConfigFeatureService;
use academy_core_contact_contracts::ContactFeatureService;
use academy_core_health_contracts::HealthFeatureService;
use academy_core_internal_contracts::InternalService;
use academy_core_mfa_contracts::MfaFeatureService;
use academy_core_oauth2_contracts::OAuth2FeatureService;
use academy_core_paypal_contracts::PaypalFeatureService;
use academy_core_session_contracts::SessionFeatureService;
use academy_core_user_contracts::UserFeatureService;
use academy_di::Build;
use academy_models::auth::{AccessToken, InternalToken};
use academy_utils::{academy_version, Apply};
use aide::{
    axum::ApiRouter,
    openapi::{Components, Info, OpenApi, ReferenceOr, SecurityScheme, Tag},
};
use anyhow::Context;
use axum::{
    response::{IntoResponse, Response},
    Extension, Json,
};
use extractors::auth::ApiTokenType;
use tokio::net::TcpListener;
use tracing::{debug, info};

mod docs;
mod errors;
mod extractors;
mod macros;
mod middlewares;
mod models;
mod routes;

#[derive(Debug, Clone, Build)]
pub struct RestServer<Health, Config, User, Session, Contact, Mfa, OAuth2, Coin, Paypal, Internal> {
    _config: RestServerConfig,
    health: Health,
    config: Config,
    user: User,
    session: Session,
    contact: Contact,
    mfa: Mfa,
    oauth2: OAuth2,
    coin: Coin,
    paypal: Paypal,
    internal: Internal,
}

#[derive(Debug, Clone)]
pub struct RestServerConfig {
    pub addr: SocketAddr,
    pub real_ip_config: Option<Arc<RestServerRealIpConfig>>,
}

#[derive(Debug, Clone)]
pub struct RestServerRealIpConfig {
    pub header: String,
    pub set_from: IpAddr,
}

impl<Health, Config, User, Session, Contact, Mfa, OAuth2, Coin, Paypal, Internal>
    RestServer<Health, Config, User, Session, Contact, Mfa, OAuth2, Coin, Paypal, Internal>
where
    Health: HealthFeatureService,
    Config: ConfigFeatureService,
    User: UserFeatureService,
    Session: SessionFeatureService,
    Contact: ContactFeatureService,
    Mfa: MfaFeatureService,
    OAuth2: OAuth2FeatureService,
    Coin: CoinFeatureService,
    Paypal: PaypalFeatureService,
    Internal: InternalService,
{
    pub async fn serve(self) -> anyhow::Result<()> {
        let RestServerConfig {
            addr,
            ref real_ip_config,
        } = self._config;
        let real_ip_config = real_ip_config.as_ref().map(Arc::clone);

        let mut api = OpenApi {
            info: Info {
                title: "Bootstrap Academy Backend".into(),
                version: academy_version().into(),
                description: Some(format!("GitHub: [{0}]({0})", env!("CARGO_PKG_REPOSITORY"))),
                ..Default::default()
            },
            tags: [
                routes::health::TAG,
                routes::config::TAG,
                routes::contact::TAG,
                routes::user::TAG,
                routes::session::TAG,
                routes::mfa::TAG,
                routes::oauth2::TAG,
                routes::coin::TAG,
                routes::paypal::TAG,
                routes::internal::TAG,
            ]
            .into_iter()
            .map(|tag| Tag {
                name: tag.into(),
                ..Default::default()
            })
            .collect(),
            components: Some(Components {
                security_schemes: {
                    let bearer = ReferenceOr::Item(SecurityScheme::Http {
                        scheme: "bearer".into(),
                        bearer_format: None,
                        description: None,
                        extensions: Default::default(),
                    });
                    [
                        (AccessToken::NAME.into(), bearer.clone()),
                        (InternalToken::NAME.into(), bearer),
                    ]
                    .into()
                },
                ..Default::default()
            }),
            ..Default::default()
        };

        let router = self
            .router()
            .route("/openapi.json", axum::routing::get(serve_api))
            .merge(docs::router())
            .apply(middlewares::panic_handler::add)
            .apply(middlewares::trace::add)
            .apply(middlewares::request_id::add)
            .apply(middlewares::client_ip::add(real_ip_config))
            .finish_api(&mut api)
            .layer(Extension(Arc::new(api)))
            .into_make_service_with_connect_info::<SocketAddr>();

        let listener = TcpListener::bind(addr)
            .await
            .with_context(|| format!("Failed to bind to {addr}"))?;

        let url = format!("http://{}", listener.local_addr()?);
        info!("Starting REST API server on {url}");
        debug!("Swagger UI is available on {url}/docs");
        debug!("Redoc is available on {url}/redoc");
        debug!("OpenAPI spec is available on {url}/openapi.json");

        axum::serve(listener, router)
            .await
            .context("Failed to start HTTP server")
    }

    fn router(self) -> ApiRouter<()> {
        ApiRouter::new()
            .merge(routes::health::router(self.health.into()))
            .merge(routes::config::router(self.config.into()))
            .merge(routes::user::router(self.user.into()))
            .merge(routes::session::router(self.session.into()))
            .merge(routes::contact::router(self.contact.into()))
            .merge(routes::mfa::router(self.mfa.into()))
            .merge(routes::oauth2::router(self.oauth2.into()))
            .merge(routes::coin::router(self.coin.into()))
            .merge(routes::paypal::router(self.paypal.into()))
            .merge(routes::internal::router(self.internal.into()))
    }
}

async fn serve_api(Extension(api): Extension<Arc<OpenApi>>) -> Response {
    Json(&*api).into_response()
}
