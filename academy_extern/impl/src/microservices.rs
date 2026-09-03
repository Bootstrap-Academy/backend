use std::{sync::Arc, time::Duration};

use academy_auth_contracts::internal::AuthInternalService;
use academy_di::Build;
use academy_extern_contracts::microservices::{MicroserviceExports, MicroservicesApiService};
use academy_models::{url::Url, user::UserId};
use academy_utils::trace_instrument;
use anyhow::{Context, anyhow, bail};
use futures::future::{join_all, try_join_all};
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
    export_timeout: Duration,
    /// Maximum size of the response body of a single export request.
    max_export_size: usize,
}

impl MicroservicesApiServiceConfig {
    pub fn new(
        skills_url: Option<Url>,
        challenges_url: Option<Url>,
        events_url: Option<Url>,
        timeout: Duration,
        export_timeout: Duration,
        max_export_size: usize,
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
            export_timeout,
            max_export_size,
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

    #[trace_instrument(skip(self))]
    async fn export_user(&self, user_id: UserId) -> anyhow::Result<MicroserviceExports> {
        try_join_all(
            self.config
                .services
                .iter()
                .map(|(audience, base_url)| async move {
                    self.export_user_from(audience, base_url, user_id)
                        .await
                        .map(|data| ((*audience).into(), data))
                        .with_context(|| format!("Failed to export user from {audience}"))
                }),
        )
        .await
        .map(MicroserviceExports::from_iter)
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

    /// Read the export of a single microservice.
    ///
    /// Neither the errors nor the logs of this function contain any of the
    /// exported data.
    async fn export_user_from(
        &self,
        audience: &str,
        base_url: &Url,
        user_id: UserId,
    ) -> anyhow::Result<serde_json::Value> {
        let token = self
            .auth_internal
            .issue_token(audience)
            .context("Failed to issue internal auth token")?;

        let url = base_url
            .join(&format!("_internal/users/{}/export", *user_id))
            .context("Failed to build export user url")?;

        let mut response = self
            .http
            .get(url)
            .bearer_auth(token.into_inner())
            .timeout(self.config.export_timeout)
            .send()
            .await
            .context("Failed to send export user request")?;

        let status = response.status();
        if !status.is_success() {
            bail!("Export user request failed with status {status}");
        }

        // The body is read in chunks so that a microservice cannot exhaust the
        // memory of the backend with an unbounded response.
        let max_export_size = self.config.max_export_size;
        if response
            .content_length()
            .is_some_and(|len| len > max_export_size as u64)
        {
            bail!("Export exceeds the maximum size of {max_export_size} bytes");
        }

        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .context("Failed to read the export response")?
        {
            if body.len() + chunk.len() > max_export_size {
                bail!("Export exceeds the maximum size of {max_export_size} bytes");
            }
            body.extend_from_slice(&chunk);
        }

        serde_json::from_slice(&body).map_err(|err| {
            // The message of serde_json quotes the input, which would put
            // exported data into the logs, so only the position is reported.
            anyhow!(
                "Failed to deserialize the export response at line {}, column {}",
                err.line(),
                err.column()
            )
        })
    }
}
