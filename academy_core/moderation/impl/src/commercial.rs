use super::*;
use academy_core_moderation_contracts::commercial::{
    CommercialCredentials, CommercialFeatureService,
};
use academy_core_purchase_contracts::PurchaseError;
use academy_persistence_contracts::moderation::CommercialPurchaseReadError;

mod retention_page;

// These two operations expose only their versioned public fields. In particular,
// callers cannot supply the transient session proof injected below.
fn validate_hold_body(operation: &str, body: &Value) -> anyhow::Result<()> {
    fn exact(value: &Value, keys: &[&str]) -> bool {
        value
            .as_object()
            .is_some_and(|o| o.len() == keys.len() && keys.iter().all(|k| o.contains_key(*k)))
    }
    fn canonical_uuid(value: &Value) -> bool {
        value
            .as_str()
            .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok_and(|u| u.to_string() == s))
    }
    fn hold(kind: &Value, record: &Value) -> bool {
        match kind.as_str() {
            Some("financial_document") => record.as_str().is_some_and(|s| !s.is_empty()),
            Some("contract_declaration" | "renewal_agreement" | "legacy_renewal") => {
                canonical_uuid(record)
            }
            _ => false,
        }
    }
    let valid = body["version"] == json!(1)
        && if operation == "hold_queue" {
            exact(body, &["version", "limit", "cursor"])
                && body["limit"]
                    .as_u64()
                    .is_some_and(|n| (1..=100).contains(&n))
                && (body["cursor"].is_null() || {
                    let c = &body["cursor"];
                    exact(
                        c,
                        &[
                            "review_due_at",
                            "kind",
                            "case_id",
                            "record_id",
                            "incarnation_id",
                        ],
                    ) && c["review_due_at"].as_str().is_some_and(|s| !s.is_empty())
                        && canonical_uuid(&c["case_id"])
                        && canonical_uuid(&c["incarnation_id"])
                        && hold(&c["kind"], &c["record_id"])
                })
        } else {
            exact(
                body,
                &[
                    "version",
                    "command_id",
                    "case_id",
                    "subject",
                    "hold",
                    "expected",
                    "decision",
                    "review_scope",
                    "assessment",
                    "next_review_at",
                ],
            ) && ["command_id", "case_id", "subject"]
                .iter()
                .all(|k| canonical_uuid(&body[k]))
                && exact(&body["hold"], &["kind", "record_id"])
                && hold(&body["hold"]["kind"], &body["hold"]["record_id"])
                && exact(&body["expected"], &["incarnation_id", "review_version"])
                && canonical_uuid(&body["expected"]["incarnation_id"])
                && body["expected"]["review_version"]
                    .as_str()
                    .is_some_and(|s| s.parse::<i64>().is_ok_and(|n| n >= 0 && n.to_string() == s))
                && body["decision"] == "keep"
                && body["review_scope"] == "entire_existing_hold"
                && body["assessment"]
                    .as_str()
                    .is_some_and(|s| s.trim().chars().count() >= 20)
                && body["next_review_at"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty())
        };
    ensure!(valid, RecipientAccessError::Malformed);
    Ok(())
}

fn purchase_read_error(error: CommercialPurchaseReadError) -> PurchaseError {
    match error {
        CommercialPurchaseReadError::NotFound => PurchaseError::NotFound,
        CommercialPurchaseReadError::Unavailable => PurchaseError::Unavailable,
        CommercialPurchaseReadError::Other(error) => PurchaseError::Other(error),
    }
}

// The new bounded read has a fixed projection. Reader corruption is unavailable,
// not client-malformed input or observed missing history. Keep JSON and native
// timestamp text unchanged; SQL validates the selected historical request.
fn validate_determination_status(value: &Value, selected: &Value) -> anyhow::Result<()> {
    fn exact(value: &Value, keys: &[&str]) -> bool {
        value.as_object().is_some_and(|object| {
            object.len() == keys.len() && keys.iter().all(|key| object.contains_key(*key))
        })
    }
    fn uuid(value: &Value) -> bool {
        value
            .as_str()
            .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok())
    }
    fn same_uuid(a: &Value, b: &Value) -> bool {
        a.as_str()
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
            .zip(b.as_str().and_then(|s| uuid::Uuid::parse_str(s).ok()))
            .is_some_and(|(a, b)| a == b)
    }
    fn decimal(value: &Value, signed: bool) -> bool {
        value.as_str().is_some_and(|s| {
            s.parse::<i64>()
                .is_ok_and(|n| (signed || n >= 0) && n.to_string() == s)
        })
    }
    let o = &value["obligation"];
    let j = &value["journal"];
    let valid = exact(
        value,
        &[
            "protocol",
            "case_id",
            "subject",
            "observed_at",
            "obligation",
            "journal",
        ],
    ) && value["protocol"] == json!(1)
        && same_uuid(&value["case_id"], &selected["case_id"])
        && same_uuid(&value["subject"], &selected["subject"])
        && value["observed_at"].as_str().is_some_and(|s| !s.is_empty())
        && exact(
            o,
            &[
                "id",
                "source",
                "source_key",
                "component",
                "status",
                "units",
                "cash_units",
                "original_json",
                "determination_json",
            ],
        )
        && same_uuid(&o["id"], &selected["obligation_id"])
        && ["source", "source_key", "component", "original_json"]
            .iter()
            .all(|key| o[key].is_string())
        && matches!(
            o["status"].as_str(),
            Some("pending_evidence" | "established" | "historical_wallet_application" | "rejected")
        )
        && ["units", "cash_units"]
            .iter()
            .all(|key| o[key].is_null() || decimal(&o[key], false))
        && (o["determination_json"].is_null() || o["determination_json"].is_string())
        && (j.is_null()
            || (exact(
                j,
                &[
                    "id",
                    "case_id",
                    "obligation_id",
                    "actor",
                    "command_id",
                    "kind",
                    "request_json",
                    "result_json",
                    "recorded_at",
                ],
            ) && decimal(&j["id"], true)
                && same_uuid(&j["case_id"], &selected["case_id"])
                && same_uuid(&j["obligation_id"], &selected["obligation_id"])
                && same_uuid(&j["command_id"], &selected["command_id"])
                && uuid(&j["actor"])
                && j["kind"] == "determine"
                && j["request_json"].is_string()
                && j["result_json"].is_string()
                && j["recorded_at"].as_str().is_some_and(|s| !s.is_empty())));
    ensure!(valid, "Unavailable determination status projection");
    Ok(())
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
    async fn commercial_op(
        &self,
        op: &str,
        actor: Option<UserId>,
        body: Value,
    ) -> anyhow::Result<Value> {
        let mut tx = self.db.begin_transaction().await?;
        let result = self
            .repo
            .commercial_operation(&mut tx, op, actor, &body)
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

    async fn commercial_principal(
        &self,
        credentials: CommercialCredentials,
    ) -> anyhow::Result<UserId> {
        if let Some(key) = credentials.claim_key {
            ensure!(
                (43..=256).contains(&key.len()),
                RecipientAccessError::Invalid
            );
            let record = self
                .commercial_op(
                    "authenticate",
                    None,
                    json!({"hash":self.hash.sha256(&key).to_string()}),
                )
                .await?;
            ensure!(!record.is_null(), RecipientAccessError::Invalid);
            return Ok(serde_json::from_value(record["subject"].clone())?);
        }
        let principal = self.principal(credentials.recipient).await?;
        ensure!(principal.rights, RecipientAccessError::Scope);
        Ok(principal.subject)
    }

    async fn commercial_original_document(
        &self,
        user: UserId,
        kind: &str,
        id: &str,
        variant: &str,
    ) -> anyhow::Result<Vec<u8>> {
        if kind == "purchase" {
            let offer = uuid::Uuid::parse_str(id).map_err(|_| RecipientAccessError::Malformed)?;
            let mut tx = self.db.begin_transaction().await?;
            let owner = self
                .repo
                .commercial_purchase_owner(&mut tx, user, offer)
                .await
                .map_err(purchase_read_error)?;
            tx.commit().await?;
            let bytes = self
                .purchase
                .recipient_document(owner, offer, variant)
                .await?;
            ensure!(!bytes.is_empty(), PurchaseError::Unavailable);
            return Ok(bytes);
        }
        let number = id.parse().map_err(|_| RecipientAccessError::Malformed)?;
        let (kind, month) = match kind {
            "invoice" => (academy_models::finance::FinancialDocumentKind::Invoice, 0),
            "final-statement" => (
                academy_models::finance::FinancialDocumentKind::FinalStatement,
                0,
            ),
            "credit-note" => (
                academy_models::finance::FinancialDocumentKind::CreditNote,
                variant
                    .parse()
                    .map_err(|_| RecipientAccessError::Malformed)?,
            ),
            _ => return Err(RecipientAccessError::Malformed.into()),
        };
        self.finance
            .download_recipient_original(user, kind, number, month)
            .await
            .map_err(Into::into)
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
> CommercialFeatureService
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
    async fn commercial_learning(&self, key: &str, operation: &str, body: Value) -> anyhow::Result<Value> {
        ensure!((43..=256).contains(&key.len()) && body.is_object(), RecipientAccessError::Malformed);
        let authority=self.commercial_op("learning_authority",None,json!({"hash":self.hash.sha256(&key.to_owned()).to_string()})).await?;
        ensure!(!authority.is_null(),RecipientAccessError::Invalid);
        let subject: UserId=serde_json::from_value(authority["subject"].clone())?;
        match operation {
            "offer" => Ok(serde_json::to_value(self.purchase.retained_offer(subject,body["kind"].as_str().ok_or(RecipientAccessError::Malformed)?).await?)?),
            "accept" => Ok(serde_json::to_value(self.purchase.retained_accept(subject,serde_json::from_value(body).map_err(|_|RecipientAccessError::Malformed)?).await?)?),
            "status" => Ok(serde_json::to_value(self.purchase.retained_get(subject,serde_json::from_value(body["order_id"].clone()).map_err(|_|RecipientAccessError::Malformed)?).await?)?),
            "resources" => Ok(self.purchase.retained_resources(subject).await?),
            _ => Err(RecipientAccessError::Malformed.into()),
        }
    }
    async fn commercial_recipient(
        &self,
        credentials: CommercialCredentials,
        operation: &str,
        mut body: Value,
    ) -> anyhow::Result<Value> {
        ensure!(body.is_object(), RecipientAccessError::Malformed);
        ensure!(
            matches!(operation, "event_cancel" | "event_rights" | "event_successor" | "resource_rights" | "premium_continue" | "course_rights" | "course_successor" | "open" | "access" | "export" | "elect" | "revoke" | "learning_start" | "learning_access" | "learning_summary" | "learning_revoke" | "learning_erase" | "purchase_authorize" | "restore_credit" | "request_wallet_cash" | "return_wallet_cash"),
            RecipientAccessError::Malformed
        );
        body.as_object_mut().unwrap().remove("_claim_hash");
        body.as_object_mut().unwrap().remove("_moderation_hash");
        if matches!(operation, "event_cancel" | "event_successor" | "resource_rights" | "premium_continue" | "course_successor" | "access" | "elect" | "revoke" | "learning_start" | "learning_access" | "learning_summary" | "learning_revoke" | "learning_erase" | "purchase_authorize" | "restore_credit" | "request_wallet_cash" | "return_wallet_cash") {
            if let Some(key) = credentials.claim_key.as_ref() {
                body["_claim_hash"] = json!(self.hash.sha256(key).to_string());
            } else if let Some(key) = credentials.recipient.capability.as_ref() {
                body["_moderation_hash"] = json!(self.hash.sha256(key).to_string());
            } else {
                // Ordinary sessions include administrative impersonation and do
                // not establish a new personal financial credential. The fresh
                // password/MFA or existing-link OAuth rights proof does.
                return Err(RecipientAccessError::Scope.into());
            }
        }
        let user = self.commercial_principal(credentials).await?;
        if matches!(operation, "course_rights" | "course_successor") {
            let source_subject: UserId = if let Some(value) = body.get("source_subject") {
                serde_json::from_value(value.clone()).map_err(|_| RecipientAccessError::Malformed)?
            } else { user };
            let owned = self.commercial_op("owned_service_subject", Some(user), json!({"subject":source_subject})).await?;
            ensure!(!owned.is_null(), RecipientAccessError::Scope);
            if operation == "course_rights" {
                return self.services.course_rights("list", source_subject, json!({})).await;
            }
            let right_id = body.get("right_id").and_then(Value::as_str).ok_or(RecipientAccessError::Malformed)?;
            let original = self.services.course_rights("original", source_subject, json!({"right_id":right_id})).await?;
            body["source_subject"] = json!(source_subject);
            body["original_scope"] = original;
            let grant = self.commercial_op("course_successor", Some(user), body).await?;
            let delivered = match self.services.course_rights("deliver", source_subject, json!({"grant_id":grant["id"]})).await {
                Ok(result) => result,
                Err(_) => json!({"grant_id":grant["id"],"subject":grant["successor"],"right_id":grant["original_contract"],
                    "state":"uncertain","reason":"Current delivery result unavailable; exact original command can be retried","new_purchase":false}),
            };
            return self.commercial_op("course_successor_outcome", Some(user), json!({"grant_id":grant["id"],"outcome":delivered})).await;
        }
        if operation == "event_cancel" {
            if body.get("source_subject").is_none() { body["source_subject"] = json!(user); }
            // Commit the actual declaration before service lookup/processing;
            // response loss cannot restart its original receipt chronology.
            let receipt = self.commercial_op("event_cancel", Some(user), body).await?;
            let source_subject: UserId = serde_json::from_value(receipt["source_subject"].clone())?;
            let outcome = match self.services.event_rights("cancel", source_subject, json!({"command_id":receipt["command_id"]})).await {
                Ok(result) => result,
                Err(_) => json!({"command_id":receipt["command_id"],"source_subject":source_subject,"right_id":receipt["right_id"],
                    "state":"uncertain","financial_satisfaction":false,
                    "reason":"Declaration retained; exact service processing can be retried"}),
            };
            return self.commercial_op("event_cancellation_outcome", Some(user), json!({"command_id":receipt["command_id"],"outcome":outcome})).await;
        }
        if matches!(operation, "event_rights" | "event_successor") {
            let source_subject: UserId = if let Some(value) = body.get("source_subject") {
                serde_json::from_value(value.clone()).map_err(|_| RecipientAccessError::Malformed)?
            } else { user };
            let owned = self.commercial_op("owned_service_subject", Some(user), json!({"subject":source_subject})).await?;
            ensure!(!owned.is_null(), RecipientAccessError::Scope);
            if operation == "event_rights" {
                return self.services.event_rights("list", source_subject, json!({})).await;
            }
            let right_id = body.get("right_id").and_then(Value::as_str).ok_or(RecipientAccessError::Malformed)?;
            let original = self.services.event_rights("original", source_subject, json!({"right_id":right_id})).await?;
            body["source_subject"] = json!(source_subject);
            body["original_scope"] = original;
            let grant = self.commercial_op("event_successor", Some(user), body).await?;
            let delivered = match self.services.event_rights("deliver", source_subject, json!({"grant_id":grant["id"]})).await {
                Ok(result) => result,
                Err(_) => json!({"grant_id":grant["id"],"subject":grant["successor"],"right_id":grant["original_contract"],
                    "state":"uncertain","reason":"Current delivery result unavailable; exact original command can be retried","new_purchase":false}),
            };
            return self.commercial_op("event_successor_outcome", Some(user), json!({"grant_id":grant["id"],"outcome":delivered})).await;
        }
        if operation == "learning_erase" {
            let admitted = self.commercial_op("learning_erasure_target", Some(user), body).await?;
            let subject: UserId = serde_json::from_value(admitted["subject"].clone())?;
            if admitted["already_erased"] != true {
                // The existing deletion service commits its permanent original
                // intake before waiting for the actual subject/user lock. It
                // owns finance preservation, erasure and durable service fanout.
                self.user_feature.recipient_delete(subject).await?;
            }
            return self.commercial_op("erasure", Some(subject), json!({})).await;
        }
        if matches!(operation, "access" | "revoke" | "learning_start" | "learning_access") {
            // The client generates and retains this credential before sending its
            // exact command. A lost response never loses a server-only secret.
            let key = body
                .get("key")
                .and_then(Value::as_str)
                .ok_or(RecipientAccessError::Malformed)?;
            ensure!(
                (43..=256).contains(&key.len()),
                RecipientAccessError::Malformed
            );
            let hash = self.hash.sha256(&key.to_owned()).to_string();
            body.as_object_mut().unwrap().remove("key");
            body["hash"] = json!(hash);
        }
        self.commercial_op(operation, Some(user), body).await
    }

    async fn commercial_admin(
        &self,
        access: &AccessToken,
        operation: &str,
        mut body: Value,
    ) -> anyhow::Result<Value> {
        let auth = self.auth.authenticate(access).await?;
        auth.ensure_admin()?;
        ensure!(body.is_object(), RecipientAccessError::Malformed);
        if operation == "retention_page" {
            retention_page::validate_body(&body)?;
        }
        if matches!(operation, "hold_review" | "hold_queue") {
            validate_hold_body(operation, &body)?;
        }
        if operation == "cash_capacity" {
            let object = body.as_object().unwrap();
            ensure!(
                object.len() == 2
                    && object.contains_key("case_id")
                    && object.contains_key("subject"),
                RecipientAccessError::Malformed
            );
            for field in ["case_id", "subject"] {
                let value = body[field]
                    .as_str()
                    .ok_or(RecipientAccessError::Malformed)?;
                uuid::Uuid::parse_str(value).map_err(|_| RecipientAccessError::Malformed)?;
            }
        }
        if operation == "determination_status" {
            let object = body.as_object().unwrap();
            ensure!(
                object.len() == 4
                    && ["case_id", "subject", "obligation_id", "command_id"]
                        .iter()
                        .all(|key| object.contains_key(*key)),
                RecipientAccessError::Malformed
            );
            for key in ["case_id", "subject", "obligation_id", "command_id"] {
                if key == "command_id" && body[key].is_null() {
                    continue;
                }
                let value = body[key].as_str().ok_or(RecipientAccessError::Malformed)?;
                uuid::Uuid::parse_str(value).map_err(|_| RecipientAccessError::Malformed)?;
            }
        }
        body["_staff_session"] = json!(auth.session_id);
        body["_staff_refresh_hash"] = json!(auth.refresh_token_hash.to_string());
        ensure!(
            matches!(
                operation,
                "queue"
                    | "hold_review"
                    | "hold_queue"
                    | "cash_capacity"
                    | "retention_queue"
                    | "retention_page"
                    | "statement_review"
                    | "archive_review"
                    | "detail"
                    | "determine"
                    | "determination_status"
                    | "review"
                    | "minimize_contact"
                    | "release_document"
                    | "release_record"
            ),
            RecipientAccessError::Malformed
        );
        if operation == "retention_page" {
            // This read retains its separate initial and SQL pre-page authority
            // observations; other operations keep their own final-auth rules.
            let selected = body.clone();
            let result = self
                .commercial_op("admin_retention_page", Some(auth.user_id), body)
                .await?;
            return retention_page::unwrap(result, &selected);
        }
        if operation == "determination_status" {
            // Capture query, projection, commit and missing-target errors before
            // checking the same original credential independently once more.
            let result = async {
                let selected = body.clone();
                let result = self
                    .commercial_op("admin_determination_status", Some(auth.user_id), body)
                    .await?;
                ensure!(!result.is_null(), RecipientAccessError::NotFound);
                validate_determination_status(&result, &selected)?;
                Ok(result)
            }
            .await;
            let current = self.auth.authenticate(access).await?;
            current.ensure_admin()?;
            ensure!(
                current.user_id == auth.user_id
                    && current.session_id == auth.session_id
                    && current.refresh_token_hash == auth.refresh_token_hash,
                RecipientAccessError::Invalid
            );
            return result;
        }
        if operation == "cash_capacity" {
            let result = self
                .commercial_op("admin_cash_capacity", Some(auth.user_id), body)
                .await?;
            ensure!(!result.is_null(), RecipientAccessError::NotFound);
            return Ok(result);
        }
        if operation == "detail" {
            let subject = serde_json::from_value(body["subject"].clone())
                .map_err(|_| RecipientAccessError::Malformed)?;
            return self.commercial_op("export", Some(subject), json!({})).await;
        }
        self.commercial_op(operation, Some(auth.user_id), body)
            .await
    }

    async fn commercial_internal(
        &self,
        token: &InternalToken,
        operation: &str,
        mut body: Value,
    ) -> anyhow::Result<Value> {
        self.internal.authenticate(token, "shop")?;
        ensure!(
            matches!(operation, "event_cancellation_pending" | "event_cancellation_outcome" | "event_cancellation_authority" | "event_successor_authority" | "course_successor_authority" | "erasure" | "register_event" | "inventory" | "learning_authority" | "learning_authority_digest"),
            RecipientAccessError::Malformed
        );
        ensure!(body.is_object(), RecipientAccessError::Malformed);
        if operation == "learning_authority" {
            let key=body.get("key").and_then(Value::as_str).ok_or(RecipientAccessError::Malformed)?;
            ensure!((43..=256).contains(&key.len()),RecipientAccessError::Malformed);
            body=json!({"hash":self.hash.sha256(&key.to_owned()).to_string()});
        }
        if operation == "learning_authority_digest" {
            // Scoped streaming links retain only a digest of the short-lived
            // learning credential. This is an internal transport entrypoint;
            // neither a claimant credential nor ordinary authority is issued.
            let hash=body.get("hash").and_then(Value::as_str).ok_or(RecipientAccessError::Malformed)?;
            ensure!(hash.len()==64 && hash.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),RecipientAccessError::Malformed);
            body=json!({"hash":hash});
        }
        let actor = if operation == "erasure" {
            serde_json::from_value(body["subject"].clone())
                .map_err(|_| RecipientAccessError::Malformed)?
        } else {
            UserId::from(uuid::Uuid::nil())
        };
        self.commercial_op(if operation=="learning_authority_digest" {"learning_authority"} else {operation}, Some(actor), body).await
    }

    async fn commercial_document(
        &self,
        credentials: CommercialCredentials,
        kind: &str,
        id: &str,
        variant: &str,
    ) -> anyhow::Result<Vec<u8>> {
        let user = self.commercial_principal(credentials).await?;
        self.commercial_original_document(user, kind, id, variant)
            .await
    }

    async fn commercial_admin_document(
        &self,
        access: &AccessToken,
        case_id: uuid::Uuid,
        kind: &str,
        id: &str,
        variant: &str,
    ) -> anyhow::Result<Vec<u8>> {
        let admitted = self.auth.authenticate(access).await?;
        admitted.ensure_admin()?;
        // Capture every reader outcome: archive waits and even reader errors
        // must finish before the same captured staff credential is rechecked.
        let result = async {
            let mut tx = self.db.begin_transaction().await?;
            let subject = self
                .repo
                .commercial_case_subject(&mut tx, case_id)
                .await?
                .ok_or(RecipientAccessError::NotFound)?;
            tx.commit().await?;
            self.commercial_original_document(subject, kind, id, variant)
                .await
        }
        .await;
        let current = self.auth.authenticate(access).await?;
        current.ensure_admin()?;
        ensure!(
            current.user_id == admitted.user_id
                && current.session_id == admitted.session_id
                && current.refresh_token_hash == admitted.refresh_token_hash,
            RecipientAccessError::Invalid
        );
        result
    }

    async fn commercial_document_inventory(
        &self,
        credentials: CommercialCredentials,
    ) -> anyhow::Result<academy_models::commercial_document::DocumentInventory> {
        let claimant = self.commercial_principal(credentials).await?;
        // Authentication uses its own transaction. No data query may precede the
        // repository's snapshot settings in this new metadata transaction.
        let mut tx = self.db.begin_transaction().await?;
        let result = self
            .repo
            .commercial_document_inventory(&mut tx, claimant)
            .await?;
        tx.commit().await?;
        Ok(result)
    }
    async fn commercial_purchase_status(
        &self,
        credentials: CommercialCredentials,
        offer: uuid::Uuid,
    ) -> anyhow::Result<academy_models::purchase::PurchaseStatus> {
        let claimant = self.commercial_principal(credentials).await?;
        let mut tx = self.db.begin_transaction().await?;
        let status = self
            .repo
            .commercial_purchase_status(&mut tx, claimant, offer)
            .await
            .map_err(purchase_read_error)?;
        tx.commit().await?;
        Ok(status)
    }
}

#[cfg(test)]
#[path = "commercial_tests.rs"]
mod tests;
