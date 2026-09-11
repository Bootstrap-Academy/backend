use std::{sync::Arc, time::Duration};

use academy_auth_contracts::internal::AuthInternalService;
use academy_di::Build;
use academy_extern_contracts::microservices::{MicroserviceExports, MicroservicesApiService};
use academy_models::{url::Url, user::UserId};
use academy_utils::trace_instrument;
use anyhow::{Context, anyhow, bail};
use futures::future::join_all;
use tracing::{error, instrument};

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
    async fn course_rights(
        &self,
        operation: &str,
        source_subject: UserId,
        body: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        if !matches!(operation, "list" | "original" | "deliver") {
            bail!("Unsupported course-right operation");
        }
        let (_, base) = self
            .config
            .services
            .iter()
            .find(|(aud, _)| *aud == SKILLS)
            .ok_or_else(|| anyhow!("Retained course rights unavailable"))?;
        let token = self.auth_internal.issue_token(SKILLS)?;
        let mut response = self
            .http
            .post(base.join(&format!(
                "_internal/users/{}/course-rights/{operation}",
                *source_subject
            ))?)
            .bearer_auth(token.into_inner())
            .json(&body)
            .timeout(self.config.timeout)
            .send()
            .await?;
        if !response.status().is_success() {
            bail!("Course-right operation unavailable");
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > self.config.max_export_size {
                bail!("Course-right response exceeds limit");
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    async fn event_rights(
        &self,
        operation: &str,
        source_subject: UserId,
        body: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        if !matches!(operation, "list" | "original" | "deliver" | "cancel") {
            bail!("Unsupported event-right operation");
        }
        let (_, base) = self
            .config
            .services
            .iter()
            .find(|(aud, _)| *aud == EVENTS)
            .ok_or_else(|| anyhow!("Retained event rights unavailable"))?;
        let token = self.auth_internal.issue_token(EVENTS)?;
        let mut response = self
            .http
            .post(base.join(&format!(
                "_internal/users/{}/event-rights/{operation}",
                *source_subject
            ))?)
            .bearer_auth(token.into_inner())
            .json(&body)
            .timeout(self.config.timeout)
            .send()
            .await?;
        if !response.status().is_success() {
            bail!("Event-right operation unavailable");
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > self.config.max_export_size {
                bail!("Event-right response exceeds limit");
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    async fn cancel_recipient_event(
        &self,
        user: UserId,
        event_id: uuid::Uuid,
    ) -> anyhow::Result<serde_json::Value> {
        let (_, base) = self
            .config
            .services
            .iter()
            .find(|(aud, _)| *aud == EVENTS)
            .ok_or_else(|| anyhow!("Events rights unavailable"))?;
        let token = self.auth_internal.issue_token(EVENTS)?;
        let response = self
            .http
            .request(
                reqwest::Method::DELETE,
                base.join(&format!(
                    "_internal/users/{}/rights/events/{event_id}",
                    *user
                ))?,
            )
            .bearer_auth(token.into_inner())
            .timeout(self.config.timeout)
            .send()
            .await?;
        let status = response.status().as_u16();
        let body = if matches!(status, 409 | 503) {
            response.json::<serde_json::Value>().await?
        } else {
            serde_json::Value::Null
        };
        recipient_event_response(status, body)
    }

    async fn moderation(
        &self,
        operation: &str,
        recipient: Option<UserId>,
        body: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let (_, base) = self
            .config
            .services
            .iter()
            .find(|(aud, _)| *aud == CHALLENGES)
            .ok_or_else(|| anyhow!("Challenges moderation unavailable"))?;
        let (method, path) = match operation {
            "inbox" => (
                reqwest::Method::GET,
                format!(
                    "recipients/{}/inbox",
                    *recipient.ok_or_else(|| anyhow!("Recipient required"))?
                ),
            ),
            "complain" => (
                reqwest::Method::POST,
                format!(
                    "recipients/{}/complaints",
                    *recipient.ok_or_else(|| anyhow!("Recipient required"))?
                ),
            ),
            "opened" => (
                reqwest::Method::POST,
                format!(
                    "recipients/{}/opened",
                    *recipient.ok_or_else(|| anyhow!("Recipient required"))?
                ),
            ),
            "claim" => (reqwest::Method::POST, "delivery/claim".into()),
            "ack" => (reqwest::Method::POST, "delivery/ack".into()),
            "maintenance" => (reqwest::Method::POST, "maintenance".into()),
            "minimizations" => (reqwest::Method::GET, "minimizations".into()),
            "minimization_ack" => (reqwest::Method::POST, "minimizations/ack".into()),
            "disposals" => (reqwest::Method::GET, "disposals".into()),
            "disposal_ack" => (reqwest::Method::POST, "disposals/ack".into()),
            _ => bail!("Unsupported moderation relay operation"),
        };
        let token = self.auth_internal.issue_token(CHALLENGES)?;
        let mut response = self
            .http
            .request(method, base.join(&format!("_internal/moderation/{path}"))?)
            .bearer_auth(token.into_inner())
            .json(&body)
            .timeout(self.config.timeout)
            .send()
            .await?;
        if !response.status().is_success() {
            bail!("Challenges moderation operation unavailable");
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > self.config.max_export_size {
                bail!("Moderation response exceeds limit");
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    #[trace_instrument(skip(self))]
    async fn delete_user(&self, user_id: UserId) {
        join_all(
            self.config
                .services
                .iter()
                .map(|(audience, base_url)| async move {
                    if let Err(err) = self.delete_user_in(audience, base_url, user_id).await {
                        error!(service=audience, error=%err, "Deletion delivery failed; durable work remains");
                    }
                }),
        )
        .await;
    }

    async fn delete_user_in_service(&self, service: &str, user_id: UserId) -> anyhow::Result<()> {
        let (audience, url) = self
            .config
            .services
            .iter()
            .find(|(name, _)| *name == service)
            .context("Deletion service has no configured URL")?;
        self.delete_user_in(audience, url, user_id).await
    }

    // Not `trace_instrument`, because that logs the return value, which is the
    // exported data of the user.
    #[instrument(skip(self))]
    async fn export_user(&self, user_id: UserId) -> MicroserviceExports {
        join_all(
            self.config
                .services
                .iter()
                .map(|(audience, base_url)| async move {
                    let export = match self.export_user_from(audience, base_url, user_id).await {
                        Ok(data) => Some(data),
                        Err(err) => {
                            // The error is logged rather than reported to the
                            // user, because it can contain internal urls. The
                            // export itself only records that this part is
                            // missing.
                            error!(service = audience, err = ?err, "Failed to export user");
                            None
                        }
                    };
                    ((*audience).into(), export)
                }),
        )
        .await
        .into_iter()
        .collect()
    }
}

impl<AuthInternal> MicroservicesApiServiceImpl<AuthInternal>
where
    AuthInternal: AuthInternalService,
{
    async fn delete_user_in(
        &self,
        audience: &str,
        base_url: &Url,
        user_id: UserId,
    ) -> anyhow::Result<()> {
        let token = self.auth_internal.issue_token(audience)?;
        let url = base_url.join(&format!("_internal/users/{}", *user_id))?;
        let response = self
            .http
            .delete(url)
            .bearer_auth(token.into_inner())
            .timeout(self.config.timeout)
            .send()
            .await?;
        if !response.status().is_success() {
            bail!("Deletion rejected with status {}", response.status());
        }
        Ok(())
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

// Deliberate refusal is distinct from a lost or uncertain service response.
fn recipient_event_response(
    status: u16,
    body: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    use academy_extern_contracts::microservices::RecipientEventError;
    match status {
        200..=299 => Ok(serde_json::json!(true)),
        404 => Err(RecipientEventError::NotFound.into()),
        403 => Err(RecipientEventError::Forbidden.into()),
        409 if body["detail"]["code"] == "ExactCancellationTargetRequired"
            && body["detail"]["cancellation_recorded"] == false =>
        {
            Err(RecipientEventError::ExactTargetRequired(body["detail"].clone()).into())
        }
        503 if body["detail"]["code"] == "EventSettlementPending"
            && body["detail"]["cancellation_committed"] == true =>
        {
            Err(RecipientEventError::Pending(body["detail"].clone()).into())
        }
        _ => bail!("Event cancellation temporarily unavailable"),
    }
}

#[cfg(test)]
mod ordinary_cancellation_tests {
    use super::recipient_event_response;
    use academy_extern_contracts::microservices::RecipientEventError;
    use serde_json::json;

    #[test]
    fn exact_refusal_preserves_the_original_detail_without_claiming_success() {
        let detail = json!({"code":"ExactCancellationTargetRequired", "cancellation_recorded":false,
            "retained_operation":"event-rights/cancel"});
        let error = recipient_event_response(409, json!({"detail":detail})).unwrap_err();
        assert!(matches!(error.downcast_ref::<RecipientEventError>(),
            Some(RecipientEventError::ExactTargetRequired(value)) if value == &detail));
    }

    #[test]
    fn ambiguous_or_mismatched_responses_do_not_claim_that_no_cancellation_was_recorded() {
        for (status, body) in [
            (
                409,
                json!({"detail":{"code":"ExactCancellationTargetRequired"}}),
            ),
            (
                409,
                json!({"detail":{"code":"ExactCancellationTargetRequired", "cancellation_recorded":true}}),
            ),
            (
                503,
                json!({"detail":{"code":"ExactCancellationTargetRequired", "cancellation_recorded":false}}),
            ),
            (
                409,
                json!({"detail":{"code":"OtherConflict", "cancellation_recorded":false}}),
            ),
        ] {
            let error = recipient_event_response(status, body).unwrap_err();
            assert!(error.downcast_ref::<RecipientEventError>().is_none());
        }
    }

    #[test]
    fn existing_committed_pending_and_resource_errors_keep_their_meaning() {
        let detail = json!({"code":"EventSettlementPending", "cancellation_committed":true,
            "pending_operations":1});
        let error = recipient_event_response(503, json!({"detail":detail})).unwrap_err();
        assert!(matches!(error.downcast_ref::<RecipientEventError>(),
            Some(RecipientEventError::Pending(value)) if value == &detail));
        assert!(matches!(
            recipient_event_response(404, json!(null))
                .unwrap_err()
                .downcast_ref::<RecipientEventError>(),
            Some(RecipientEventError::NotFound)
        ));
        assert!(matches!(
            recipient_event_response(403, json!(null))
                .unwrap_err()
                .downcast_ref::<RecipientEventError>(),
            Some(RecipientEventError::Forbidden)
        ));
    }
}
