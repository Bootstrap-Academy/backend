use crate::{
    extractors::auth::ApiToken,
    middlewares::client_ip::ClientIp,
    models::{StringOption, user_export::ApiAccountDataExport},
};
use academy_core_moderation_contracts::{
    ModerationFeatureService, RecipientAccessError, RecipientCredentials,
};
use academy_core_session_contracts::{SessionCreateCommand, SessionCreateError};
use academy_models::{
    RecaptchaResponse,
    auth::{AccessToken, InternalToken},
    mfa::{MfaAuthentication, MfaRecoveryCode, TotpCode},
    user::{UserNameOrEmailAddress, UserPassword},
};
use aide::axum::{ApiRouter, routing};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

pub const TAG: &str = "Moderation";
pub fn router(service: Arc<impl ModerationFeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route(
            "/auth/_internal/ordinary-authority",
            routing::post(authority),
        )
        .api_route(
            "/auth/_internal/moderation/rule-evidence/{user}",
            routing::get(rule_evidence),
        )
        .api_route("/auth/moderation/admin/{operation}", routing::post(admin))
        .api_route("/auth/moderation/access/password", routing::post(password))
        .api_route("/auth/moderation/access/recovery", routing::post(recovery))
        .api_route("/auth/moderation/access/revoke", routing::post(revoke))
        .api_route(
            "/auth/moderation/access/oauth/begin",
            routing::post(oauth_begin),
        )
        .api_route(
            "/auth/moderation/access/oauth/finish",
            routing::post(oauth_finish),
        )
        .api_route("/auth/moderation/inbox", routing::get(inbox))
        .api_route(
            "/auth/moderation/complaints/{source}",
            routing::post(complain),
        )
        .api_route("/auth/moderation/export", routing::get(export))
        .api_route("/auth/moderation/opened/{source}", routing::post(opened))
        .api_route(
            "/auth/moderation/purchases/{id}/documents/{kind}",
            routing::get(purchase_document),
        )
        .api_route(
            "/auth/moderation/finance/{kind}/{number}/{month}",
            routing::get(finance_document),
        )
        .api_route(
            "/auth/moderation/finance-access",
            routing::post(finance_access),
        )
        .api_route("/auth/moderation/account", routing::delete(erase))
        .api_route(
            "/auth/moderation/events/{id}",
            routing::delete(cancel_event),
        )
        .with_state(service)
        .with_path_items(|op| op.tag(TAG))
}
#[derive(Deserialize, JsonSchema)]
struct AuthorityRequest {
    access_token: String,
}
async fn authority(
    State(s): State<Arc<impl ModerationFeatureService>>,
    internal: ApiToken<InternalToken>,
    Json(r): Json<AuthorityRequest>,
) -> Response {
    match s
        .ordinary_authority(&internal.0, &AccessToken::new(r.access_token))
        .await
    {
        Ok(v) => response(v),
        Err(e)
            if matches!(
                e.downcast_ref::<academy_models::auth::AuthenticateError>(),
                Some(academy_models::auth::AuthenticateError::InvalidToken)
            ) || e
                .downcast_ref::<academy_auth_contracts::internal::AuthInternalAuthenticateError>()
                .is_some() =>
        {
            error(
                StatusCode::UNAUTHORIZED,
                "Ordinary or internal token invalid or restricted",
            )
        }
        Err(_) => error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Current ordinary authority could not be checked",
        ),
    }
}
async fn admin(
    State(s): State<Arc<impl ModerationFeatureService>>,
    token: ApiToken,
    Path(operation): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    match s.admin(&token.0, &operation, body).await {
        Ok(v) => response(v),
        Err(_) => error(
            StatusCode::CONFLICT,
            "Decision incomplete, access denied or stale revision; reload the case",
        ),
    }
}
#[derive(Deserialize, JsonSchema)]
struct PasswordRequest {
    name_or_email: UserNameOrEmailAddress,
    password: UserPassword,
    #[serde(default)]
    mfa_code: StringOption<TotpCode>,
    #[serde(default)]
    recovery_code: StringOption<MfaRecoveryCode>,
    #[serde(default)]
    recaptcha_response: StringOption<RecaptchaResponse>,
}
async fn password(
    State(s): State<Arc<impl ModerationFeatureService>>,
    Extension(ClientIp(ip)): Extension<ClientIp>,
    Json(r): Json<PasswordRequest>,
) -> Response {
    let cmd = SessionCreateCommand {
        name_or_email: r.name_or_email,
        password: r.password,
        mfa: MfaAuthentication {
            totp_code: r.mfa_code.into(),
            recovery_code: r.recovery_code.into(),
        },
        device_name: None,
    };
    match s
        .password_access(ip, cmd, r.recaptcha_response.into())
        .await
    {
        Ok(v) => response(v),
        Err(SessionCreateError::MfaFailed) => {
            error(StatusCode::UNAUTHORIZED, "MFA required or invalid")
        }
        Err(SessionCreateError::Recaptcha) => {
            error(StatusCode::FORBIDDEN, "CAPTCHA required or invalid")
        }
        Err(SessionCreateError::TooManyFailedAttempts(d)) => (
            [(
                header::RETRY_AFTER,
                d.as_secs().saturating_add(1).to_string(),
            )],
            error(StatusCode::TOO_MANY_REQUESTS, "Try again later"),
        )
            .into_response(),
        Err(SessionCreateError::Other(_)) => error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Recipient proof temporarily unavailable",
        ),
        Err(_) => error(StatusCode::UNAUTHORIZED, "Recipient proof invalid"),
    }
}
fn credentials(headers: &HeaderMap) -> RecipientCredentials {
    RecipientCredentials {
        ordinary: headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(AccessToken::from),
        capability: headers
            .get("x-moderation-capability")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned),
    }
}
async fn inbox(
    State(s): State<Arc<impl ModerationFeatureService>>,
    headers: HeaderMap,
) -> Response {
    match s.inbox(credentials(&headers)).await {
        Ok(v) => response(v),
        Err(e) => recipient_error(e),
    }
}
async fn recovery(
    State(s): State<Arc<impl ModerationFeatureService>>,
    Extension(ClientIp(ip)): Extension<ClientIp>,
    Json(body): Json<Value>,
) -> Response {
    match s.recovery(ip, body).await {
        Ok(v) => response(v),
        Err(e) => recipient_error(e),
    }
}
async fn revoke(
    State(s): State<Arc<impl ModerationFeatureService>>,
    headers: HeaderMap,
) -> Response {
    match s.revoke(credentials(&headers)).await {
        Ok(v) => response(v),
        Err(_) => error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Access revocation unavailable; clear the local credential",
        ),
    }
}
async fn complain(
    State(s): State<Arc<impl ModerationFeatureService>>,
    headers: HeaderMap,
    Path(source): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    match s.complain(credentials(&headers), &source, body).await {
        Ok(v) => response(json!({"receipt":v,"status":"pending_human_review"})),
        Err(e) => recipient_error(e),
    }
}
async fn export(
    State(s): State<Arc<impl ModerationFeatureService>>,
    headers: HeaderMap,
) -> Response {
    match s.export(credentials(&headers)).await {
        Ok(v) => response(
            json!({"account":v.account.map(ApiAccountDataExport::from),"retained":v.retained,"complete":v.services.values().all(Option::is_some)&&v.moderation["challenges_available"]==true&&v.retained["complete"]==true,"services":v.services,"moderation":v.moderation}),
        ),
        Err(e) => recipient_error(e),
    }
}
fn response(value: Value) -> Response {
    (
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::REFERRER_POLICY, "no-referrer"),
        ],
        Json(value),
    )
        .into_response()
}
async fn opened(
    State(s): State<Arc<impl ModerationFeatureService>>,
    headers: HeaderMap,
    Path(source): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    match s.opened(credentials(&headers), &source, body).await {
        Ok(v) => response(v),
        Err(e) => recipient_error(e),
    }
}
async fn finance_document(
    State(s): State<Arc<impl ModerationFeatureService>>,
    headers: HeaderMap,
    Path((kind, number, month)): Path<(String, u64, u32)>,
) -> Response {
    match s
        .finance_document(credentials(&headers), &kind, number, month)
        .await
    {
        Ok(bytes) => (
            [
                (header::CACHE_CONTROL, "no-store"),
                (header::REFERRER_POLICY, "no-referrer"),
                (header::CONTENT_TYPE, "application/pdf"),
                (header::CONTENT_DISPOSITION, "attachment"),
            ],
            bytes,
        )
            .into_response(),
        Err(e) => recipient_error(e),
    }
}
async fn finance_access(
    State(s): State<Arc<impl ModerationFeatureService>>,
    headers: HeaderMap,
) -> Response {
    match s.finance_access(credentials(&headers)).await {
        Ok(v) => response(v),
        Err(e) => recipient_error(e),
    }
}
async fn erase(
    State(s): State<Arc<impl ModerationFeatureService>>,
    headers: HeaderMap,
) -> Response {
    match s.erase(credentials(&headers)).await {
        Ok(v) => response(v),
        Err(e) => recipient_error(e),
    }
}
async fn cancel_event(
    State(s): State<Arc<impl ModerationFeatureService>>,
    headers: HeaderMap,
    Path(id): Path<uuid::Uuid>,
) -> Response {
    match s.cancel_event(credentials(&headers), id).await {
        Ok(v) => response(v),
        Err(e) => recipient_error(e),
    }
}
async fn purchase_document(
    State(s): State<Arc<impl ModerationFeatureService>>,
    headers: HeaderMap,
    Path((id, kind)): Path<(uuid::Uuid, String)>,
) -> Response {
    match s.purchase_document(credentials(&headers), id, &kind).await {
        Ok(bytes) => (
            [
                (header::CACHE_CONTROL, "no-store"),
                (header::REFERRER_POLICY, "no-referrer"),
                (
                    header::CONTENT_TYPE,
                    if matches!(
                        kind.as_str(),
                        "confirmation"
                            | "fulfillment"
                            | "timing"
                            | "fulfillment-original"
                            | "timing-original"
                    ) {
                        "text/plain; charset=utf-8"
                    } else {
                        "application/pdf"
                    },
                ),
                (header::CONTENT_DISPOSITION, "attachment"),
            ],
            bytes,
        )
            .into_response(),
        Err(e) => recipient_error(e),
    }
}
fn error(status: StatusCode, message: &str) -> Response {
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({"detail":message})),
    )
        .into_response()
}

#[derive(Deserialize, JsonSchema)]
struct OAuthBegin {
    provider: academy_models::oauth2::OAuth2ProviderId,
    redirect_uri: academy_models::url::Url,
}
#[derive(Deserialize, JsonSchema)]
struct OAuthFinish {
    state: academy_models::oauth2::OAuth2State,
    code: academy_models::oauth2::OAuth2AuthorizationCode,
}
async fn oauth_begin(
    State(s): State<Arc<impl ModerationFeatureService>>,
    Json(r): Json<OAuthBegin>,
) -> Response {
    match s.oauth_begin(r.provider, r.redirect_uri).await {
        Ok(v) => response(v),
        Err(_) => error(StatusCode::BAD_REQUEST, "Rights authorization unavailable"),
    }
}
async fn oauth_finish(
    State(s): State<Arc<impl ModerationFeatureService>>,
    Json(r): Json<OAuthFinish>,
) -> Response {
    match s
        .oauth_finish(academy_models::oauth2::OAuth2Callback {
            state: r.state,
            code: r.code,
        })
        .await
    {
        Ok(v) => response(v),
        Err(e) => recipient_error(e),
    }
}

async fn rule_evidence(
    State(s): State<Arc<impl ModerationFeatureService>>,
    internal: ApiToken<InternalToken>,
    Path(user): Path<academy_models::user::UserId>,
) -> Response {
    match s.rule_evidence(&internal.0, user).await {
        Ok(v) => response(v),
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "Rule evidence unavailable"),
    }
}

// Failed infrastructure must not be mistaken for an invalid credential: clients
// keep a still-valid proof and exact mutation intent for a later retry.
fn recipient_error(e: anyhow::Error) -> Response {
    if let Some(RecipientAccessError::ExactTargetRequired(original)) =
        e.downcast_ref::<RecipientAccessError>()
    {
        let mut detail = original.clone();
        detail["retry_same_request"] = json!(false);
        detail["message"] = json!(
            "This event-only request recorded no cancellation declaration. Select the original booking right and confirm an exact declaration using retained-rights access. Keep any previously issued receipt."
        );
        detail["next_steps"] = json!({
            "rights_access_page": "/moderation/access",
            "open_rights_record_if_needed": {
                "method": "POST", "path": "/shop/claims/recipient/open",
                "required_fields": ["command_id"], "declares_cancellation": false,
                "requires_original_account_or_evidenced_case": true,
                "inventory_completion_confirmed": false,
                "creates_replacement_account": false
            },
            "list_original_rights": {
                "method": "POST", "path": "/shop/claims/recipient/event_rights", "body": {}
            },
            "unavailable_original_rights": "An unavailable or empty original-right listing does not establish that no rights exist. Preserve original documents and seek resolution; do not substitute the event ID or create a replacement account.",
            "declare_selected_right": {
                "method": "POST", "path": "/shop/claims/recipient/event_cancel",
                "required_fields": ["command_id", "right_id", "cancel_identified_contract", "original_text"],
                "optional_fields": ["source_subject"],
                "command_id_policy": "Use a separate stable UUID for this declaration, distinct from case opening; retry its exact command and body if needed.",
                "confirmation_required": true,
                "accepted_proof_headers": ["x-moderation-capability", "x-commercial-claim-key"],
                "proof_scope": "current rights access for the original subject; case-only moderation and learning access do not suffice",
                "ordinary_bearer_alone_sufficient": false
            }
        });
        return (
            StatusCode::CONFLICT,
            [
                (header::CACHE_CONTROL, "no-store"),
                (header::REFERRER_POLICY, "no-referrer"),
            ],
            Json(json!({"detail": detail})),
        )
            .into_response();
    }
    if let Some(RecipientAccessError::Pending(body)) = e.downcast_ref::<RecipientAccessError>() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::CACHE_CONTROL, "no-store")],
            Json(json!({"detail":body})),
        )
            .into_response();
    }
    if matches!(
        e.downcast_ref::<RecipientAccessError>(),
        Some(RecipientAccessError::NotFound)
    ) {
        return error(
            StatusCode::NOT_FOUND,
            "Resource unavailable for this recipient",
        );
    }
    if matches!(
        e.downcast_ref::<RecipientAccessError>(),
        Some(RecipientAccessError::Forbidden)
    ) {
        return error(
            StatusCode::FORBIDDEN,
            "Current cancellation timing does not permit this action; statutory declaration channels remain available",
        );
    }
    if matches!(e.downcast_ref::<RecipientAccessError>(),Some(RecipientAccessError::Invalid))
  || matches!(e.downcast_ref::<academy_models::auth::AuthenticateError>(),Some(academy_models::auth::AuthenticateError::InvalidToken))
  || e.downcast_ref::<academy_core_oauth2_contracts::RecipientProofInvalid>().is_some()
  || matches!(e.downcast_ref::<academy_core_oauth2_contracts::login::OAuth2LoginServiceError>(),Some(academy_core_oauth2_contracts::login::OAuth2LoginServiceError::InvalidCode|academy_core_oauth2_contracts::login::OAuth2LoginServiceError::InvalidProvider)) {
  return error(StatusCode::UNAUTHORIZED,"Recipient proof invalid, expired or revoked");
 }
    if matches!(
        e.downcast_ref::<RecipientAccessError>(),
        Some(RecipientAccessError::Malformed)
    ) {
        return error(
            StatusCode::BAD_REQUEST,
            "Enter the case source, identifier and contact address",
        );
    }
    if matches!(
        e.downcast_ref::<RecipientAccessError>(),
        Some(RecipientAccessError::Conflict)
    ) {
        return error(
            StatusCode::CONFLICT,
            "Request conflicts with current state or required fields; retain the exact request",
        );
    }
    if matches!(
        e.downcast_ref::<RecipientAccessError>(),
        Some(RecipientAccessError::Scope)
    ) {
        return error(
            StatusCode::FORBIDDEN,
            "This proof does not authorize the requested resource",
        );
    }
    if matches!(
        e.downcast_ref::<academy_core_finance_contracts::FinanceDownloadError>(),
        Some(academy_core_finance_contracts::FinanceDownloadError::NotFound)
    ) {
        return error(
            StatusCode::NOT_FOUND,
            "Financial document unavailable for this recipient",
        );
    }
    if matches!(
        e.downcast_ref::<academy_core_purchase_contracts::PurchaseError>(),
        Some(academy_core_purchase_contracts::PurchaseError::NotFound)
    ) {
        return error(
            StatusCode::NOT_FOUND,
            "Document unavailable for this recipient",
        );
    }
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "Request not confirmed; keep your proof and exact request for retry",
    )
}

#[cfg(test)]
mod ordinary_cancellation_tests {
    use super::recipient_error;
    use academy_core_moderation_contracts::RecipientAccessError;
    use axum::{
        body::to_bytes,
        http::{StatusCode, header},
    };
    use serde_json::{Value, json};

    #[tokio::test]
    async fn deliberate_refusal_reaches_recipient_as_actionable_conflict_without_a_new_declaration()
    {
        let original = json!({"code":"ExactCancellationTargetRequired", "cancellation_recorded":false,
            "retained_operation":"event-rights/cancel"});
        let response =
            recipient_error(RecipientAccessError::ExactTargetRequired(original.clone()).into());
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(response.headers()[header::REFERRER_POLICY], "no-referrer");
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 16_384).await.unwrap()).unwrap();
        for field in ["code", "cancellation_recorded", "retained_operation"] {
            assert_eq!(body["detail"][field], original[field]);
        }
        assert_eq!(body["detail"]["retry_same_request"], false);
        let next = &body["detail"]["next_steps"];
        assert_eq!(next["rights_access_page"], "/moderation/access");
        assert_eq!(
            next["open_rights_record_if_needed"]["path"],
            "/shop/claims/recipient/open"
        );
        assert_eq!(
            next["open_rights_record_if_needed"]["required_fields"],
            json!(["command_id"])
        );
        assert_eq!(
            next["open_rights_record_if_needed"]["declares_cancellation"],
            false
        );
        assert_eq!(
            next["open_rights_record_if_needed"]["requires_original_account_or_evidenced_case"],
            true
        );
        assert_eq!(
            next["open_rights_record_if_needed"]["inventory_completion_confirmed"],
            false
        );
        assert_eq!(
            next["open_rights_record_if_needed"]["creates_replacement_account"],
            false
        );
        assert_eq!(
            next["list_original_rights"]["path"],
            "/shop/claims/recipient/event_rights"
        );
        assert_eq!(
            next["declare_selected_right"]["path"],
            "/shop/claims/recipient/event_cancel"
        );
        assert_eq!(
            next["declare_selected_right"]["ordinary_bearer_alone_sufficient"],
            false
        );
        assert_eq!(
            next["declare_selected_right"]["confirmation_required"],
            true
        );
        assert!(
            next["declare_selected_right"]["command_id_policy"]
                .as_str()
                .unwrap()
                .contains("distinct from case opening")
        );
        assert_eq!(
            next["declare_selected_right"]["optional_fields"],
            json!(["source_subject"])
        );
        assert_eq!(
            next["declare_selected_right"]["accepted_proof_headers"],
            json!(["x-moderation-capability", "x-commercial-claim-key"])
        );
        assert!(body["detail"].get("command_id").is_none());
        assert!(body["detail"].get("right_id").is_none());
    }

    #[tokio::test]
    async fn uncertain_service_result_keeps_retry_semantics_and_committed_pending_keeps_its_receipt()
     {
        let response = recipient_error(anyhow::anyhow!("synthetic service unavailable"));
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 16_384).await.unwrap()).unwrap();
        assert!(body["detail"].is_string());
        assert!(body["detail"].get("cancellation_recorded").is_none());
        let pending = json!({"code":"EventSettlementPending", "cancellation_committed":true, "pending_operations":1});
        let response = recipient_error(RecipientAccessError::Pending(pending.clone()).into());
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 16_384).await.unwrap()).unwrap();
        assert_eq!(body["detail"], pending);
    }
}
