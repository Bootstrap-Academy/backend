use std::time::Duration;

use academy_auth_contracts::internal::{AuthInternalAuthenticateError, AuthInternalService};
use academy_config::MicroservicesConfig;
use academy_di::{Provide, provider};
use academy_extern_contracts::microservices::MicroservicesApiService;
use academy_extern_impl::microservices::{
    MicroservicesApiServiceConfig, MicroservicesApiServiceImpl,
};
use academy_models::{auth::InternalToken, url::Url, user::UserId};
use serde::Deserialize;
use uuid::Uuid;

#[tokio::test]
async fn ok() {
    let config = config();
    let base_url = config.skills_url.clone().unwrap();
    let sut = make_sut(MicroservicesApiServiceConfig::new(
        config.skills_url,
        config.challenges_url,
        config.events_url,
        config.timeout.into(),
    ));

    let user_id = UserId::from(Uuid::new_v4());

    sut.delete_user(user_id).await;

    assert_eq!(
        deleted_users(&base_url, user_id).await,
        [
            DeletedUser::new("challenges", user_id),
            DeletedUser::new("events", user_id),
            DeletedUser::new("skills", user_id),
        ]
    );
}

#[tokio::test]
async fn error_response() {
    let config = config();
    let base_url = config.skills_url.clone().unwrap();
    let sut = make_sut(MicroservicesApiServiceConfig::new(
        config.skills_url,
        None,
        None,
        config.timeout.into(),
    ));

    // deleting this user always fails in the testing server
    let user_id = UserId::from(Uuid::nil());

    // the error is logged, but not returned
    sut.delete_user(user_id).await;

    assert_eq!(
        deleted_users(&base_url, user_id).await,
        [DeletedUser::new("skills", user_id)]
    );
}

#[tokio::test]
async fn connection_error() {
    let sut = make_sut(MicroservicesApiServiceConfig::new(
        Some("http://127.0.0.1:1/".parse().unwrap()),
        None,
        None,
        Duration::from_secs(10),
    ));

    // the error is logged, but not returned
    sut.delete_user(UserId::from(Uuid::new_v4())).await;
}

#[tokio::test]
async fn disabled() {
    let config = config();
    let base_url = config.skills_url.clone().unwrap();
    let sut = make_sut(MicroservicesApiServiceConfig::new(
        None,
        None,
        None,
        config.timeout.into(),
    ));

    let user_id = UserId::from(Uuid::new_v4());

    sut.delete_user(user_id).await;

    assert!(deleted_users(&base_url, user_id).await.is_empty());
}

fn config() -> MicroservicesConfig {
    academy_config::load().unwrap().microservices
}

fn make_sut(config: MicroservicesApiServiceConfig) -> MicroservicesApiServiceImpl<AuthInternal> {
    provider! {
        Provider {
            auth_internal: AuthInternal,
            microservices_api_service_config: MicroservicesApiServiceConfig,
        }
    }

    let mut provider = Provider {
        _cache: Default::default(),
        auth_internal: AuthInternal,
        microservices_api_service_config: config,
    };

    provider.provide()
}

/// Issues a token which contains the audience it has been issued for, so that
/// the tokens received by the testing server can be matched against the
/// microservices they have been sent to.
#[derive(Debug, Clone)]
struct AuthInternal;

impl AuthInternalService for AuthInternal {
    fn issue_token(&self, audience: &str) -> anyhow::Result<InternalToken> {
        Ok(format!("internal token for {audience}").into())
    }

    fn authenticate(
        &self,
        _token: &InternalToken,
        _audience: &str,
    ) -> Result<(), AuthInternalAuthenticateError> {
        unimplemented!()
    }
}

/// Return all deletions of the given user which have been received by the
/// testing server, sorted by the name of the microservice.
async fn deleted_users(base_url: &Url, user_id: UserId) -> Vec<DeletedUser> {
    let mut deleted_users = reqwest::Client::new()
        .get(base_url.join("/deleted_users").unwrap())
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json::<Vec<DeletedUser>>()
        .await
        .unwrap()
        .into_iter()
        .filter(|deleted_user| deleted_user.user_id == *user_id)
        .collect::<Vec<_>>();

    deleted_users.sort_by(|a, b| a.service.cmp(&b.service));
    deleted_users
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct DeletedUser {
    service: String,
    token: String,
    user_id: Uuid,
}

impl DeletedUser {
    fn new(service: &str, user_id: UserId) -> Self {
        Self {
            service: service.into(),
            token: format!("internal token for {service}"),
            user_id: *user_id,
        }
    }
}
