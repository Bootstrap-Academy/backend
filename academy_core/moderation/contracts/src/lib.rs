use academy_core_session_contracts::{SessionCreateCommand, SessionCreateError};
use academy_core_user_contracts::export::AccountDataExport;
use academy_models::{
    RecaptchaResponse,
    auth::{AccessToken, InternalToken},
    user::UserId,
};
use serde_json::Value;
use std::{future::Future, net::IpAddr};

pub mod commercial;

/// This capability is deliberately opaque: it has no JWT uid/rt/data and is
/// never an ordinary service bearer token. Do not log its contents.
pub struct RecipientCredentials {
    pub ordinary: Option<AccessToken>,
    pub capability: Option<String>,
}
pub struct RightsExport {
    pub account: Option<AccountDataExport>,
    pub retained: Value,
    pub services: std::collections::BTreeMap<String, Option<Value>>,
    pub moderation: Value,
}
pub trait ModerationFeatureService: Send + Sync + 'static {
    fn recovery(
        &self,
        ip: IpAddr,
        body: Value,
    ) -> impl Future<Output = anyhow::Result<Value>> + Send;
    fn revoke(
        &self,
        credentials: RecipientCredentials,
    ) -> impl Future<Output = anyhow::Result<Value>> + Send;
    fn rule_evidence(
        &self,
        internal: &InternalToken,
        user: UserId,
    ) -> impl Future<Output = anyhow::Result<Value>> + Send;

    fn oauth_begin(
        &self,
        provider: academy_models::oauth2::OAuth2ProviderId,
        redirect: academy_models::url::Url,
    ) -> impl Future<Output = anyhow::Result<Value>> + Send;
    fn oauth_finish(
        &self,
        callback: academy_models::oauth2::OAuth2Callback,
    ) -> impl Future<Output = anyhow::Result<Value>> + Send;

    fn ordinary_authority(
        &self,
        internal: &InternalToken,
        access: &AccessToken,
    ) -> impl Future<Output = anyhow::Result<Value>> + Send;
    fn admin(
        &self,
        access: &AccessToken,
        operation: &str,
        body: Value,
    ) -> impl Future<Output = anyhow::Result<Value>> + Send;
    fn password_access(
        &self,
        ip: IpAddr,
        cmd: SessionCreateCommand,
        captcha: Option<RecaptchaResponse>,
    ) -> impl Future<Output = Result<Value, SessionCreateError>> + Send;
    fn issue_verified_access(
        &self,
        user: UserId,
    ) -> impl Future<Output = anyhow::Result<Value>> + Send;
    fn inbox(
        &self,
        credentials: RecipientCredentials,
    ) -> impl Future<Output = anyhow::Result<Value>> + Send;
    fn opened(
        &self,
        credentials: RecipientCredentials,
        source: &str,
        body: Value,
    ) -> impl Future<Output = anyhow::Result<Value>> + Send;
    fn purchase_document(
        &self,
        credentials: RecipientCredentials,
        id: uuid::Uuid,
        kind: &str,
    ) -> impl Future<Output = anyhow::Result<Vec<u8>>> + Send;
    fn erase(
        &self,
        credentials: RecipientCredentials,
    ) -> impl Future<Output = anyhow::Result<Value>> + Send;
    fn cancel_event(
        &self,
        credentials: RecipientCredentials,
        id: uuid::Uuid,
    ) -> impl Future<Output = anyhow::Result<Value>> + Send;
    fn finance_document(
        &self,
        credentials: RecipientCredentials,
        kind: &str,
        number: u64,
        month: u32,
    ) -> impl Future<Output = anyhow::Result<Vec<u8>>> + Send;
    fn finance_access(
        &self,
        credentials: RecipientCredentials,
    ) -> impl Future<Output = anyhow::Result<Value>> + Send;
    fn complain(
        &self,
        credentials: RecipientCredentials,
        source: &str,
        body: Value,
    ) -> impl Future<Output = anyhow::Result<Value>> + Send;
    fn export(
        &self,
        credentials: RecipientCredentials,
    ) -> impl Future<Output = anyhow::Result<RightsExport>> + Send;
    fn retry(&self) -> impl Future<Output = anyhow::Result<()>> + Send;
}

#[derive(Debug, thiserror::Error)]
pub enum RecipientAccessError {
    #[error("Exact original cancellation target required; no declaration recorded")]
    ExactTargetRequired(Value),
    #[error("Recipient proof is missing, invalid, expired or revoked")]
    Invalid,
    #[error("This proof does not authorize the requested recipient resource")]
    Scope,
    #[error("Command conflicts with case state or required fields")]
    Conflict,
    #[error("Requested retained resource is unavailable")]
    NotFound,
    #[error("Cancellation timing does not permit this action")]
    Forbidden,
    #[error("Cancellation committed with pending work")]
    Pending(Value),
    #[error("Malformed recipient request")]
    Malformed,
}
