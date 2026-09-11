use academy_auth_contracts::{AuthService, internal::AuthInternalService};
use academy_core_finance_contracts::FinanceFeatureService;
use academy_core_moderation_contracts::{
    ModerationFeatureService, RecipientAccessError, RecipientCredentials, RightsExport,
};
use academy_core_oauth2_contracts::OAuth2FeatureService;
use academy_core_purchase_contracts::PurchaseFeatureService;
use academy_core_session_contracts::{
    SessionCreateCommand, SessionCreateError, SessionFeatureService,
};
use academy_core_user_contracts::UserFeatureService;
use academy_core_user_contracts::export::UserExportService;
use academy_di::Build;
use academy_email_contracts::{Email, EmailService};
use academy_extern_contracts::microservices::MicroservicesApiService;
use academy_models::{
    RecaptchaResponse,
    auth::{AccessToken, InternalToken},
    email_address::EmailAddressWithName,
    user::UserId,
};
use academy_persistence_contracts::{Database, Transaction, moderation::ModerationRepository};
use academy_shared_contracts::{hash::HashService, secret::SecretService};
use anyhow::{anyhow, ensure};
use serde_json::{Value, json};
use std::net::IpAddr;

mod commercial;

#[derive(Debug, Clone, Build)]
pub struct ModerationFeatureServiceImpl<
    Db,
    Auth,
    Internal,
    Repo,
    Session,
    Export,
    Services,
    EmailS,
    Hash,
    Secret,
    OAuth,
    Purchase,
    Finance,
    UserFeature,
> {
    db: Db,
    auth: Auth,
    internal: Internal,
    repo: Repo,
    session: Session,
    export: Export,
    services: Services,
    email: EmailS,
    hash: Hash,
    secret: Secret,
    oauth: OAuth,
    purchase: Purchase,
    finance: Finance,
    user_feature: UserFeature,
}
struct Principal {
    subject: UserId,
    case_id: Option<String>,
    case_source: Option<String>,
    rights: bool,
}
impl<
    Db,
    Auth,
    Internal,
    Repo,
    Session,
    Export,
    Services,
    EmailS,
    Hash,
    Secret,
    OAuth,
    Purchase,
    Finance,
    UserFeature,
>
    ModerationFeatureServiceImpl<
        Db,
        Auth,
        Internal,
        Repo,
        Session,
        Export,
        Services,
        EmailS,
        Hash,
        Secret,
        OAuth,
        Purchase,
        Finance,
        UserFeature,
    >
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    Internal: AuthInternalService,
    Repo: ModerationRepository<Db::Transaction>,
    Session: SessionFeatureService,
    Export: UserExportService<Db::Transaction>,
    Services: MicroservicesApiService,
    EmailS: EmailService,
    Hash: HashService,
    Secret: SecretService,
    OAuth: OAuth2FeatureService,
    Purchase: PurchaseFeatureService,
    Finance: FinanceFeatureService,
    UserFeature: UserFeatureService,
{
    async fn op(
        &self,
        operation: &str,
        actor: Option<UserId>,
        body: Value,
    ) -> anyhow::Result<Value> {
        let mut tx = self.db.begin_transaction().await?;
        let result = self
            .repo
            .operation(&mut tx, operation, actor, &body)
            .await
            .map_err(|e| -> anyhow::Error {
                if e.downcast_ref::<academy_persistence_contracts::moderation::ModerationConflict>()
                    .is_some()
                {
                    RecipientAccessError::Conflict.into()
                } else {
                    e
                }
            })?;
        tx.commit().await?;
        Ok(result)
    }
    async fn principal(&self, credentials: RecipientCredentials) -> anyhow::Result<Principal> {
        if let Some(cap) = credentials.capability {
            ensure!(
                cap.len() >= 32 && cap.len() <= 256,
                RecipientAccessError::Invalid
            );
            let hash = self.hash.sha256(&cap).to_string();
            let record = self.op("capability", None, json!({"hash":hash})).await?;
            ensure!(!record.is_null(), RecipientAccessError::Invalid);
            return Ok(Principal {
                subject: serde_json::from_value(record["subject"].clone())?,
                case_id: record["case_id"].as_str().map(str::to_owned),
                case_source: record["source"].as_str().map(str::to_owned),
                rights: record["scope"] == "rights",
            });
        }
        let token = credentials.ordinary.ok_or(RecipientAccessError::Invalid)?;
        let auth = self.auth.authenticate(&token).await?;
        Ok(Principal {
            subject: auth.user_id,
            case_id: None,
            case_source: None,
            rights: true,
        })
    }
    async fn collect_inbox(&self, p: &Principal) -> anyhow::Result<Value> {
        let mut backend = self.op("inbox", Some(p.subject), Value::Null).await?;
        let challenges = self
            .services
            .moderation("inbox", Some(p.subject), Value::Null)
            .await;
        filter_case(&mut backend, p.case_id.as_deref());
        let (mut challenges, available) = match challenges {
            Ok(v) => (v, true),
            Err(_) => (json!([]), false),
        };
        filter_case(&mut challenges, p.case_id.as_deref());
        if p.case_source.as_deref().is_some_and(|s| s != "backend") {
            backend = json!([]);
        }
        if p.case_source.as_deref().is_some_and(|s| s != "challenges") {
            challenges = json!([]);
        }
        Ok(
            json!({"backend":backend,"challenges":challenges,"challenges_available":available,"recipient_id":p.subject,"scope":if p.rights{"rights"}else{"case"},"case_id":p.case_id,"case_source":p.case_source}),
        )
    }
}
impl<
    Db,
    Auth,
    Internal,
    Repo,
    Session,
    Export,
    Services,
    EmailS,
    Hash,
    Secret,
    OAuth,
    Purchase,
    Finance,
    UserFeature,
> ModerationFeatureService
    for ModerationFeatureServiceImpl<
        Db,
        Auth,
        Internal,
        Repo,
        Session,
        Export,
        Services,
        EmailS,
        Hash,
        Secret,
        OAuth,
        Purchase,
        Finance,
        UserFeature,
    >
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    Internal: AuthInternalService,
    Repo: ModerationRepository<Db::Transaction>,
    Session: SessionFeatureService,
    Export: UserExportService<Db::Transaction>,
    Services: MicroservicesApiService,
    EmailS: EmailService,
    Hash: HashService,
    Secret: SecretService,
    OAuth: OAuth2FeatureService,
    Purchase: PurchaseFeatureService,
    Finance: FinanceFeatureService,
    UserFeature: UserFeatureService,
{
    async fn recovery(&self, ip: IpAddr, mut body: Value) -> anyhow::Result<Value> {
        ensure!(
            body.is_object()
                && ["backend", "challenges"].contains(&body["source"].as_str().unwrap_or_default())
                && body["case_id"]
                    .as_str()
                    .is_some_and(|v| v.parse::<uuid::Uuid>().is_ok())
                && body["contact"].as_str().is_some_and(|v| v.len() <= 320),
            RecipientAccessError::Malformed
        );
        let secret = self.secret.generate(64).0;
        body["hash"] = json!(self.hash.sha256(&secret).to_string());
        body["ip_hash"] = json!(
            self.hash
                .sha256(&format!("moderation-recovery:{ip}"))
                .to_string()
        );
        body["link"] = json!(format!(
            "https://bootstrap.academy/moderation/access#capability={secret}"
        ));
        self.op("recovery_request", None, body).await?;
        Ok(
            json!({"status":"If the case and recorded contact match, a case access link will be queued. Otherwise use hallo@bootstrap.academy for proportionate identity verification. No account is created."}),
        )
    }
    async fn revoke(&self, credentials: RecipientCredentials) -> anyhow::Result<Value> {
        if let Some(secret) = credentials.capability {
            self.op(
                "revoke_capability",
                None,
                json!({"hash":self.hash.sha256(&secret).to_string()}),
            )
            .await?;
        }
        Ok(json!(true))
    }
    async fn oauth_begin(
        &self,
        provider: academy_models::oauth2::OAuth2ProviderId,
        redirect: academy_models::url::Url,
    ) -> anyhow::Result<Value> {
        let result = self.oauth.begin_recipient(provider, redirect).await?;
        Ok(json!({"state":result.state,"authorize_url":result.authorize_url}))
    }
    async fn oauth_finish(
        &self,
        callback: academy_models::oauth2::OAuth2Callback,
    ) -> anyhow::Result<Value> {
        let user = self.oauth.prove_recipient(callback).await?;
        self.issue_verified_access(user).await
    }
    async fn rule_evidence(&self, internal: &InternalToken, user: UserId) -> anyhow::Result<Value> {
        self.internal.authenticate(internal, "auth")?;
        self.op("rule_evidence", Some(user), Value::Null).await
    }
    async fn ordinary_authority(
        &self,
        internal: &InternalToken,
        access: &AccessToken,
    ) -> anyhow::Result<Value> {
        self.internal.authenticate(internal, "auth")?;
        let a = self.auth.authenticate(access).await?;
        // Admin scope is based on current account state AND stored session MFA.
        Ok(
            json!({"id":a.user_id,"admin":a.admin&&a.mfa_verified,"email_verified":a.email_verified}),
        )
    }
    async fn admin(
        &self,
        access: &AccessToken,
        operation: &str,
        body: Value,
    ) -> anyhow::Result<Value> {
        let auth = self.auth.authenticate(access).await?;
        auth.ensure_admin()?;
        ensure!(
            [
                "open",
                "queue",
                "case",
                "decide",
                "escalate",
                "handling",
                "delivery_queue",
                "delivery_contact",
                "retention"
            ]
            .contains(&operation),
            "Unsupported case operation"
        );
        // No shared cache write precedes this commit. Session deletion is durable;
        // ordinary authentication checks it even if invalidation transport is down.
        self.op(operation, Some(auth.user_id), body).await
    }
    async fn password_access(
        &self,
        ip: IpAddr,
        cmd: SessionCreateCommand,
        captcha: Option<RecaptchaResponse>,
    ) -> Result<Value, SessionCreateError> {
        let subject = self.session.prove_recipient(ip, cmd, captcha).await?;
        self.issue_verified_access(subject)
            .await
            .map_err(Into::into)
    }
    async fn issue_verified_access(&self, user: UserId) -> anyhow::Result<Value> {
        let secret = self.secret.generate(64).0;
        let hash = self.hash.sha256(&secret).to_string();
        let mut result = self
            .op(
                "issue_capability",
                Some(user),
                json!({"hash":hash,"scope":"rights"}),
            )
            .await?;
        result["capability"] = json!(secret);
        Ok(result)
    }
    async fn inbox(&self, credentials: RecipientCredentials) -> anyhow::Result<Value> {
        self.collect_inbox(&self.principal(credentials).await?)
            .await
    }
    async fn opened(
        &self,
        credentials: RecipientCredentials,
        source: &str,
        body: Value,
    ) -> anyhow::Result<Value> {
        let p = self.principal(credentials).await?;
        let inbox = self.collect_inbox(&p).await?;
        ensure!(
            inbox[source]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|m| m["id"] == body["id"])),
            RecipientAccessError::Scope
        );
        match source {
            "backend" => self.op("opened", Some(p.subject), body).await,
            "challenges" => {
                self.services
                    .moderation("opened", Some(p.subject), body)
                    .await
            }
            _ => Err(anyhow!("Unsupported case owner")),
        }
    }
    async fn purchase_document(
        &self,
        credentials: RecipientCredentials,
        id: uuid::Uuid,
        kind: &str,
    ) -> anyhow::Result<Vec<u8>> {
        let p = self.principal(credentials).await?;
        ensure!(p.rights, RecipientAccessError::Scope);
        Ok(self
            .purchase
            .recipient_document(p.subject, id, kind)
            .await?)
    }
    async fn erase(&self, credentials: RecipientCredentials) -> anyhow::Result<Value> {
        let p = self.principal(credentials).await?;
        ensure!(p.rights, RecipientAccessError::Scope);
        self.user_feature.recipient_delete(p.subject).await?;
        Ok(
            json!({"status":"Account erasure committed; retained rights and service erasure work follow their documented workflows."}),
        )
    }
    async fn cancel_event(
        &self,
        credentials: RecipientCredentials,
        id: uuid::Uuid,
    ) -> anyhow::Result<Value> {
        let p = self.principal(credentials).await?;
        ensure!(p.rights, RecipientAccessError::Scope);
        self.services
            .cancel_recipient_event(p.subject, id)
            .await
            .map_err(recipient_event_error)
    }

    async fn finance_document(
        &self,
        credentials: RecipientCredentials,
        kind: &str,
        number: u64,
        month: u32,
    ) -> anyhow::Result<Vec<u8>> {
        let p = self.principal(credentials).await?;
        ensure!(p.rights, RecipientAccessError::Scope);
        let kind = match kind {
            "invoice" => academy_models::finance::FinancialDocumentKind::Invoice,
            "credit-note" if number <= i32::MAX as u64 && (1..=12).contains(&month) => {
                academy_models::finance::FinancialDocumentKind::CreditNote
            }
            _ => return Err(RecipientAccessError::Malformed.into()),
        };
        Ok(self
            .finance
            .download_recipient_original(p.subject, kind, number, month)
            .await?)
    }
    async fn finance_access(&self, credentials: RecipientCredentials) -> anyhow::Result<Value> {
        let p = self.principal(credentials).await?;
        ensure!(p.rights, RecipientAccessError::Scope);
        Ok(json!({"token":self.finance.recipient_download_token(p.subject).await?}))
    }
    async fn complain(
        &self,
        credentials: RecipientCredentials,
        source: &str,
        body: Value,
    ) -> anyhow::Result<Value> {
        let p = self.principal(credentials).await?;
        if let Some(case) = &p.case_id {
            let inbox = self.collect_inbox(&p).await?;
            ensure!(
                inbox[source].as_array().is_some_and(|rows| rows
                    .iter()
                    .any(|m| m["case_id"] == *case && m["decision_id"] == body["decision_id"])),
                RecipientAccessError::Scope
            );
        }
        match source {
            "backend" => self.op("complain", Some(p.subject), body).await,
            "challenges" => {
                self.services
                    .moderation("complain", Some(p.subject), body)
                    .await
            }
            _ => Err(anyhow!("Unsupported case owner")),
        }
    }
    async fn export(&self, credentials: RecipientCredentials) -> anyhow::Result<RightsExport> {
        let p = self.principal(credentials).await?;
        ensure!(p.rights, RecipientAccessError::Scope);
        let mut tx = self.db.begin_transaction().await?;
        let account = self.export.export(&mut tx, p.subject).await?;
        let retained = self.export.retained(&mut tx, p.subject).await?;
        // Materialize the read-only result and release its connection before remote
        // exports or another local transaction; works with a single available slot.
        tx.rollback().await?;
        let services = self.services.export_user(p.subject).await;
        let moderation = self.collect_inbox(&p).await?;
        Ok(RightsExport {
            account,
            retained,
            services,
            moderation,
        })
    }
    async fn retry(&self) -> anyhow::Result<()> {
        for _ in 0..25 {
            if self.op("maintenance", None, Value::Null).await? == json!(0) {
                break;
            }
        }
        for _ in 0..25 {
            match self
                .services
                .moderation("maintenance", None, Value::Null)
                .await
            {
                Ok(v) if v != json!(0) => {}
                _ => break,
            }
        }
        if let Ok(Value::Array(records)) = self
            .services
            .moderation("minimizations", None, Value::Null)
            .await
        {
            for mut record in records {
                record["source"] = json!("challenges");
                if self
                    .op("accept_minimization", None, record.clone())
                    .await
                    .is_ok()
                {
                    let _ = self
                        .services
                        .moderation("minimization_ack", None, json!({"id":record["id"]}))
                        .await;
                }
            }
        }
        if let Ok(Value::Array(disposals)) = self
            .services
            .moderation("disposals", None, Value::Null)
            .await
        {
            for record in disposals {
                if self
                    .op(
                        "accept_disposal",
                        None,
                        json!({"source":"challenges","case_id":record["case_id"],"review_due_at":record["review_due_at"]}),
                    )
                    .await
                    .is_ok()
                {
                    let _ = self
                        .services
                        .moderation("disposal_ack", None, json!({"case_id":record["case_id"]}))
                        .await;
                }
            }
        }

        // Bounded pull assigns the owner server-side; no arbitrary relay destination.
        for source in ["backend", "challenges"] {
            let pending = if source == "backend" {
                self.op("claim", None, Value::Null).await
            } else {
                self.services.moderation("claim", None, Value::Null).await
            };
            let Ok(Value::Array(messages)) = pending else {
                continue;
            };
            for mut message in messages {
                message["source"] = json!(source);
                let ok = self
                    .op("accept_delivery", None, message.clone())
                    .await
                    .is_ok();
                let ack = json!({"id":message["id"],"generation":message["generation"],"ok":ok});
                if source == "backend" {
                    let _ = self.op("ack", None, ack).await;
                } else {
                    let _ = self.services.moderation("ack", None, ack).await;
                }
            }
        }
        let messages = self.op("claim_email", None, Value::Null).await?;
        for claimed in messages
            .as_array()
            .ok_or_else(|| anyhow!("Invalid delivery work"))?
        {
            let message = self.op("admit_email", None, json!({"source":claimed["source"],"id":claimed["id"],"generation":claimed["generation"]})).await?;
            if message.is_null() {
                continue;
            }
            let status = if let Some(contact) = message["contact"].as_str() {
                match contact.parse::<EmailAddressWithName>() {
                    Ok(recipient) => {
                        let statement = &message["body"];
                        let mut email=Email{sender:self.email.sender().clone(),recipient,subject:"Bootstrap Academy: Meldung oder Moderationsentscheidung".into(),
       body:format!("Information zu deinem Vorgang {}\n\n{}\n\nAutomatisierung: {}\nRechtsbehelfe: {}\n\nDer Vorgang und menschliche Beschwerden sind unter https://bootstrap.academy/moderation erreichbar, auch bei gesperrtem Konto. Kontakt: hallo@bootstrap.academy.",message["case_id"].as_str().unwrap_or_default(),statement["rationale"].as_str().or_else(||statement["text"].as_str()).unwrap_or("Neue Nachricht in deinem Vorgang."),statement["automation"].as_str().unwrap_or("Keine zusätzliche automatische Entscheidung durch den Versand."),statement["redress"].as_str().unwrap_or("Menschliche Überprüfung kostenlos über den Vorgang oder hallo@bootstrap.academy; gesetzliche Rechte bleiben unberührt.")),
       message_id:format!("moderation-{}-{}@bootstrap.academy",message["source"].as_str().unwrap_or_default(),message["id"].as_str().unwrap_or_default()).into(),
       content_type:academy_email_contracts::ContentType::Text,reply_to:None,attachments:vec![]};
                        for (label, key) in [
                            ("Maßnahme", "outcome"),
                            ("Grundlage", "ground"),
                            ("Regelfassung", "rule_version"),
                            ("Umfang", "scope"),
                            ("Ende", "ends_at"),
                            ("Menschliche Prüfung", "review_assessment"),
                        ] {
                            if !statement[key].is_null() {
                                email.body.push_str(&format!(
                                    "\n{label}: {}",
                                    statement[key].as_str().unwrap_or_default()
                                ));
                            }
                        }
                        if let Some(link) = statement["recovery_link"].as_str() {
                            email
                                .body
                                .push_str(&format!("\n\nZugang zu diesem Vorgang: {link}"));
                        }
                        match tokio::time::timeout(
                            std::time::Duration::from_secs(30),
                            self.email.send(email),
                        )
                        .await
                        {
                            Ok(Ok(true)) => "transport_accepted",
                            Ok(Ok(false)) => "retry",
                            Ok(Err(_)) | Err(_) => "uncertain",
                        }
                    }
                    Err(_) => "no_contact",
                }
            } else {
                "no_contact"
            };
            self.op("ack_email",None,json!({"source":message["source"],"id":message["id"],"generation":message["generation"],"attempt_id":message["attempt_id"],"status":status})).await?;
        }
        Ok(())
    }
}
fn filter_case(messages: &mut Value, case: Option<&str>) {
    if let (Some(case), Some(rows)) = (case, messages.as_array_mut()) {
        rows.retain(|m| m["case_id"] == case);
    }
}


fn recipient_event_error(error: anyhow::Error) -> anyhow::Error {
    use academy_extern_contracts::microservices::RecipientEventError;
    match error.downcast_ref::<RecipientEventError>() {
        Some(RecipientEventError::ExactTargetRequired(body)) => RecipientAccessError::ExactTargetRequired(body.clone()).into(),
        Some(RecipientEventError::NotFound) => RecipientAccessError::NotFound.into(),
        Some(RecipientEventError::Forbidden) => RecipientAccessError::Forbidden.into(),
        Some(RecipientEventError::Pending(body)) => RecipientAccessError::Pending(body.clone()).into(),
        None => error,
    }
}

#[cfg(test)]
mod ordinary_cancellation_tests {
    use super::recipient_event_error;
    use academy_core_moderation_contracts::RecipientAccessError;
    use academy_extern_contracts::microservices::RecipientEventError;
    use serde_json::json;

    #[test]
    fn core_preserves_exact_refusal_and_keeps_it_distinct_from_pending_and_unavailability() {
        let detail = json!({"code":"ExactCancellationTargetRequired", "cancellation_recorded":false,
            "retained_operation":"event-rights/cancel"});
        let mapped = recipient_event_error(RecipientEventError::ExactTargetRequired(detail.clone()).into());
        assert!(matches!(mapped.downcast_ref::<RecipientAccessError>(),
            Some(RecipientAccessError::ExactTargetRequired(value)) if value == &detail));
        let pending = json!({"cancellation_committed":true});
        let mapped = recipient_event_error(RecipientEventError::Pending(pending.clone()).into());
        assert!(matches!(mapped.downcast_ref::<RecipientAccessError>(),
            Some(RecipientAccessError::Pending(value)) if value == &pending));
        let unknown = recipient_event_error(anyhow::anyhow!("unknown service outcome"));
        assert!(unknown.downcast_ref::<RecipientAccessError>().is_none());
    }
}
