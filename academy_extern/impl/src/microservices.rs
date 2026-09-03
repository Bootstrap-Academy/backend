use std::{sync::Arc, time::Duration};

use academy_auth_contracts::internal::AuthInternalService;
use academy_di::Build;
use academy_extern_contracts::microservices::MicroservicesApiService;
use academy_models::{url::Url, user::UserId};
use academy_utils::trace_instrument;
use futures::future::join_all;
use tracing::error;

use crate::http::HttpClient;

/// The audiences of the internal auth tokens expected by the microservices.
const SKILLS: &str = "skills";
const CHALLENGES: &str = "challenges";
const EVENTS: &str = "events";

#[derive(Debug, Clone, Build)]
pub struct MicroservicesApiServiceImpl<AuthInternal> {
    auth_internal: AuthInternal,
    config: MicroservicesApiServiceConfig,
    #[di(default)]
    http: HttpClient,
}

#[derive(Debug, Clone)]
pub struct MicroservicesApiServiceConfig {
    /// The base urls of all enabled microservices and the audiences of the
    /// tokens they expect. Microservices without a base url are disabled.
    services: Arc<[(&'static str, Url)]>,
    timeout: Duration,
}

impl MicroservicesApiServiceConfig {
    pub fn new(
        skills_url: Option<Url>,
        challenges_url: Option<Url>,
        events_url: Option<Url>,
        timeout: Duration,
    ) -> Self {
        Self {
            services: [
                (SKILLS, skills_url),
                (CHALLENGES, challenges_url),
                (EVENTS, events_url),
            ]
            .into_iter()
            .filter_map(|(audience, url)| url.map(|url| (audience, url)))
            .collect(),
            timeout,
        }
    }
}

impl<AuthInternal> MicroservicesApiService for MicroservicesApiServiceImpl<AuthInternal>
where
    AuthInternal: AuthInternalService,
{
    #[trace_instrument(skip(self))]
    async fn delete_user(&self, user_id: UserId) {
        join_all(
            self.config
                .services
                .iter()
                .map(|(audience, base_url)| self.delete_user_in(audience, base_url, user_id)),
        )
        .await;
    }
}

impl<AuthInternal> MicroservicesApiServiceImpl<AuthInternal>
where
    AuthInternal: AuthInternalService,
{
    /// Send the delete request to a single microservice.
    ///
    /// Any error is logged instead of being returned, so that one microservice
    /// cannot affect the other microservices or the caller.
    async fn delete_user_in(&self, audience: &str, base_url: &Url, user_id: UserId) {
        let token = match self.auth_internal.issue_token(audience) {
            Ok(token) => token,
            Err(err) => {
                error!(service = audience, %err, "Failed to issue internal auth token");
                return;
            }
        };

        let url = match base_url.join(&format!("_internal/users/{}", *user_id)) {
            Ok(url) => url,
            Err(err) => {
                error!(service = audience, %err, "Failed to build delete user url");
                return;
            }
        };

        let response = self
            .http
            .delete(url)
            .bearer_auth(token.into_inner())
            .timeout(self.config.timeout)
            .send()
            .await;

        match response {
            Ok(response) if response.status().is_success() => {}
            Ok(response) => {
                let status = response.status();
                let body = response.text().await;
                error!(service = audience, %status, ?body, "Failed to delete user");
            }
            Err(err) => {
                error!(service = audience, %err, "Failed to send delete user request");
            }
        }
    }
}
