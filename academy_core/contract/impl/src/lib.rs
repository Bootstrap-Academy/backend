use academy_auth_contracts::{AuthResultExt, AuthService};
use academy_cache_contracts::CacheService;
use academy_core_contract_contracts::*;
use academy_di::Build;
use academy_email_contracts::{ContentType, Email, EmailService};
use academy_models::{
    auth::AccessToken,
    contract::*,
    email_address::{EmailAddress, EmailAddressWithName},
};
use academy_persistence_contracts::{
    Database, Transaction, contract::ContractRepository, user::UserRepository,
};
use academy_shared_contracts::{hash::HashService, id::IdService, time::TimeService};
use academy_utils::trace_instrument;
use anyhow::Context;
use chrono::{DateTime, Utc};
use std::{net::IpAddr, sync::Arc, time::Duration};
use tracing::{error, trace, warn};
#[cfg(test)]
mod tests;
const DATETIME_FORMAT: &str = "%d.%m.%Y um %H:%M:%S Uhr";
const DATE_FORMAT: &str = "%d.%m.%Y";
#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct ContractFeatureServiceImpl<
    Db,
    Auth,
    Id,
    Time,
    Cache,
    Hash,
    EmailS,
    UserRepo,
    ContractRepo,
> {
    db: Db,
    auth: Auth,
    id: Id,
    time: Time,
    cache: Cache,
    hash: Hash,
    email: EmailS,
    user_repo: UserRepo,
    contract_repo: ContractRepo,
    config: ContractFeatureConfig,
}
#[derive(Debug, Clone)]
pub struct ContractFeatureConfig {
    pub internal_email: Arc<EmailAddressWithName>,
    pub rate_limit_window: Duration,
    pub rate_limit_per_ip: u64,
    pub rate_limit_per_email: u64,
}
impl<Db, Auth, Id, Time, Cache, Hash, EmailS, UserRepo, ContractRepo> ContractFeatureService
    for ContractFeatureServiceImpl<Db, Auth, Id, Time, Cache, Hash, EmailS, UserRepo, ContractRepo>
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    Id: IdService,
    Time: TimeService,
    Cache: CacheService,
    Hash: HashService,
    EmailS: EmailService,
    UserRepo: UserRepository<Db::Transaction>,
    ContractRepo: ContractRepository<Db::Transaction>,
{
    #[trace_instrument(skip(self, request))]
    async fn declare_cancellation(
        &self,
        client_ip: IpAddr,
        request: ContractCancellationRequest,
    ) -> Result<ContractDeclarationResult, ContractDeclareError> {
        let declaration = ContractDeclaration {
            id: request
                .request_key
                .as_ref()
                .map(|k| k.id)
                .unwrap_or_else(|| self.id.generate()),
            kind: ContractDeclarationKind::Cancellation,
            received_at: self.time.now(),
            name: request.name,
            email: request.email,
            user_id: None,
            contract: request.contract,
            contract_designation: request.contract_designation,
            cancellation_type: Some(request.cancellation_type),
            details: request.details,
            requested_end: request
                .requested_end
                .map(|time| DateTime::from_timestamp_micros(time.timestamp_micros()).unwrap()),
            effective_end: None,
            processed_at: None,
            processing_note: None,
            delivery: vec![],
            operational_evidence: None,
        };
        self.accept(
            client_ip,
            declaration,
            request.request_key,
            request.renewal_agreement_id,
        )
        .await
    }
    #[trace_instrument(skip(self, request))]
    async fn declare_withdrawal(
        &self,
        client_ip: IpAddr,
        request: ContractWithdrawalRequest,
    ) -> Result<ContractDeclarationResult, ContractDeclareError> {
        let declaration = ContractDeclaration {
            id: request
                .request_key
                .as_ref()
                .map(|k| k.id)
                .unwrap_or_else(|| self.id.generate()),
            kind: ContractDeclarationKind::Withdrawal,
            received_at: self.time.now(),
            name: request.name,
            email: request.email,
            user_id: None,
            contract: request.contract,
            contract_designation: request.contract_designation,
            cancellation_type: None,
            details: request.details,
            requested_end: None,
            effective_end: None,
            processed_at: None,
            processing_note: None,
            delivery: vec![],
            operational_evidence: None,
        };
        self.accept(client_ip, declaration, request.request_key, None)
            .await
    }
    #[trace_instrument(skip(self, key))]
    async fn lookup_receipt(
        &self,
        key: ContractRequestKey,
    ) -> Result<ContractDeclarationResult, ContractDeclareError> {
        let mut txn = self.db.begin_transaction().await?;
        let stored = self.contract_repo.receipt_access(&mut txn, key.id).await?;
        if stored.as_deref() != Some(&self.secret_hash(&key)) {
            return Err(ContractDeclareError::NotFound);
        }
        let declaration = self
            .contract_repo
            .get(&mut txn, key.id)
            .await?
            .ok_or(ContractDeclareError::NotFound)?;
        Ok(result(declaration))
    }
    async fn retry_confirmations(&self) -> anyhow::Result<()> {
        let mut txn = self.db.begin_transaction().await?;
        self.contract_repo.recover_schedules(&mut txn).await?;
        txn.commit().await?;
        self.deliver(None).await
    }
    #[trace_instrument(skip(self))]
    async fn list_declarations(
        &self,
        token: &AccessToken,
        query: ContractDeclarationListQuery,
    ) -> Result<ContractDeclarationListResult, ContractListError> {
        self.auth
            .authenticate(token)
            .await
            .map_auth_err()?
            .ensure_admin()
            .map_auth_err()?;
        let mut txn = self.db.begin_transaction().await?;
        let total = self.contract_repo.count(&mut txn, query.kind).await?;
        let declarations = self
            .contract_repo
            .list(&mut txn, query.kind, query.pagination)
            .await?;
        txn.commit().await?;
        Ok(ContractDeclarationListResult {
            total,
            declarations,
        })
    }
    #[trace_instrument(skip(self, update))]
    async fn set_declaration_processed(
        &self,
        token: &AccessToken,
        id: ContractDeclarationId,
        update: ContractDeclarationProcessingUpdate,
    ) -> Result<ContractDeclaration, ContractSetProcessedError> {
        self.auth
            .authenticate(token)
            .await
            .map_auth_err()?
            .ensure_admin()
            .map_auth_err()?;
        if update.note.is_none() || !update.identity_verified {
            return Err(ContractSetProcessedError::Invalid);
        }
        let mut txn = self.db.begin_transaction().await?;
        self.contract_repo.lock_request(&mut txn, id).await?;
        self.contract_repo.lock_processing(&mut txn, id).await?;
        let mut declaration = self
            .contract_repo
            .get(&mut txn, id)
            .await?
            .ok_or(ContractSetProcessedError::NotFound)?;
        if declaration.processed_at.is_some() {
            return Err(ContractSetProcessedError::Conflict);
        }
        if update.action == ContractProcessingAction::SchedulePremiumCancellation {
            if declaration.contract != ContractKind::Premium
                || declaration.cancellation_type != Some(ContractCancellationType::Ordinary)
            {
                return Err(ContractSetProcessedError::Invalid);
            }
            let user_id = update
                .verified_user_id
                .ok_or(ContractSetProcessedError::Invalid)?;
            let agreement_id = update
                .renewal_agreement_id
                .ok_or(ContractSetProcessedError::Invalid)?;
            // The original receipt and date, never the administrator's later approval time.
            if !self
                .contract_repo
                .schedule_cancellation(&mut txn, declaration.clone(), agreement_id, user_id)
                .await?
            {
                return Err(ContractSetProcessedError::Conflict);
            }
            declaration = self
                .contract_repo
                .get(&mut txn, id)
                .await?
                .ok_or(ContractSetProcessedError::NotFound)?;
        } else if declaration.kind == ContractDeclarationKind::Cancellation
            && update.effective_end.is_none()
        {
            return Err(ContractSetProcessedError::Invalid);
        }
        let declaration = self
            .contract_repo
            .set_processed(
                &mut txn,
                id,
                self.time.now(),
                if update.action == ContractProcessingAction::SchedulePremiumCancellation {
                    declaration.effective_end
                } else {
                    update.effective_end.or(declaration.effective_end)
                },
                match (declaration.processing_note, update.note) {
                    (Some(old), Some(note)) => Some(
                        format!("{}\n{}", *old, *note)
                            .try_into()
                            .map_err(|_| ContractSetProcessedError::Invalid)?,
                    ),
                    (_, note) => note,
                },
                update.action == ContractProcessingAction::RecordExternalResolution,
            )
            .await?
            .ok_or(ContractSetProcessedError::NotFound)?;
        txn.commit().await?;
        // Scheduling may still be pending. Processed is the documented operational action,
        // never a legal precondition and never a claim that SMTP reached an inbox.
        Ok(declaration)
    }
}
impl<Db, Auth, Id, Time, Cache, Hash, EmailS, UserRepo, ContractRepo>
    ContractFeatureServiceImpl<Db, Auth, Id, Time, Cache, Hash, EmailS, UserRepo, ContractRepo>
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    Id: IdService,
    Time: TimeService,
    Cache: CacheService,
    Hash: HashService,
    EmailS: EmailService,
    UserRepo: UserRepository<Db::Transaction>,
    ContractRepo: ContractRepository<Db::Transaction>,
{
    fn secret_hash(&self, key: &ContractRequestKey) -> String {
        hex::encode(self.hash.sha256(&(*key.secret).to_string()).0)
    }
    async fn accept(
        &self,
        client_ip: IpAddr,
        mut declaration: ContractDeclaration,
        key: Option<ContractRequestKey>,
        agreement_id: Option<academy_models::premium::PremiumRenewalId>,
    ) -> Result<ContractDeclarationResult, ContractDeclareError> {
        let mut txn = self.db.begin_transaction().await?;
        self.contract_repo
            .lock_request(&mut txn, declaration.id)
            .await?;
        if let Some(existing) = self.contract_repo.get(&mut txn, declaration.id).await? {
            let Some(key) = key else {
                return Err(ContractDeclareError::RequestConflict);
            };
            if self
                .contract_repo
                .receipt_access(&mut txn, key.id)
                .await?
                .as_deref()
                != Some(&self.secret_hash(&key))
                || !same_submission(&existing, &declaration)
                || !same_requested_agreement(&existing, agreement_id)
            {
                return Err(ContractDeclareError::RequestConflict);
            }
            return Ok(result(existing));
        }
        self.check_rate_limit(client_ip, &declaration.email).await?;
        declaration.user_id = self
            .user_repo
            .get_composite_by_email(&mut txn, &declaration.email)
            .await?
            .map(|u| u.user.id);
        self.contract_repo
            .create(&mut txn, declaration.clone())
            .await?;
        if let Some(key) = key {
            self.contract_repo
                .save_receipt_access(&mut txn, declaration.id, self.secret_hash(&key))
                .await?;
        }
        self.contract_repo
            .queue_delivery(
                &mut txn,
                ContractDeliveryAttempt {
                    requested_agreement_id: agreement_id,
                    declaration_id: declaration.id,
                    kind: "receipt".into(),
                    recipient: declaration.email.clone(),
                    subject: match declaration.kind {
                        ContractDeclarationKind::Cancellation => "Deine Kündigung ist angekommen",
                        ContractDeclarationKind::Withdrawal => "Dein Widerruf ist angekommen",
                    }
                    .into(),
                    body: match agreement_id {
                        Some(id) => format!(
                            "{}\nVon dir angegebene Abo-Nummer: {}\n",
                            receipt_body(&declaration),
                            *id
                        ),
                        None => receipt_body(&declaration),
                    },
                    generation: 0,
                },
            )
            .await?;
        self.contract_repo
            .queue_delivery(
                &mut txn,
                ContractDeliveryAttempt {
                    requested_agreement_id: agreement_id,
                    declaration_id: declaration.id,
                    kind: "internal".into(),
                    recipient: (*self.config.internal_email).clone().into_email_address(),
                    subject: format!(
                        "[Contract] Prüfung: {} ({})",
                        kind_label(declaration.kind),
                        contract_short_label(declaration.contract)
                    ),
                    body: internal_notification_body(&declaration),
                    generation: 0,
                },
            )
            .await?;
        // Knowledge of an email/name alone authorizes no account mutation. The optional
        // unguessable exact agreement reference adds contract-specific identification.
        if declaration.contract == ContractKind::Premium
            && declaration.cancellation_type == Some(ContractCancellationType::Ordinary)
            && let (Some(user_id), Some(agreement_id)) = (declaration.user_id, agreement_id)
        {
            self.contract_repo
                .schedule_cancellation(&mut txn, declaration.clone(), agreement_id, user_id)
                .await?;
        }
        txn.commit().await?;
        if let Err(err) = self.deliver(Some(declaration.id)).await {
            error!(error=%err,"Declaration confirmation pending; durable receipt retained");
        }
        let mut txn = self.db.begin_transaction().await?;
        let declaration = self
            .contract_repo
            .get(&mut txn, declaration.id)
            .await?
            .context("Committed declaration missing")?;
        Ok(result(declaration))
    }
    async fn deliver(&self, only: Option<ContractDeclarationId>) -> anyhow::Result<()> {
        let mut attempted = Vec::new();
        let mut failed = false;
        for _ in 0..if only.is_some() { 1 } else { 100 } {
            let mut txn = self.db.begin_transaction().await?;
            let Some(message) = self
                .contract_repo
                .claim_delivery(&mut txn, only, attempted.clone())
                .await?
            else {
                break;
            };
            txn.commit().await?;
            attempted.push(format!("{}:{}", *message.declaration_id, message.kind));
            // A separate guard transaction serializes terminal staff decisions with
            // resolution SMTP. The attempt and lease have already committed.
            let mut resolution_txn = if message.kind == "resolution" {
                let mut guard = self.db.begin_transaction().await?;
                if !self
                    .contract_repo
                    .lock_resolution_delivery(&mut guard, &message)
                    .await?
                {
                    guard.commit().await?;
                    continue;
                }
                Some(guard)
            } else {
                None
            };
            let accepted = matches!(
                tokio::time::timeout(
                    Duration::from_secs(30),
                    self.email.send(Email {
                        sender: None,
                        message_id: Some(format!(
                            "<contract-{}-{}@bootstrap.academy>",
                            *message.declaration_id, message.kind
                        )),
                        recipient: message.recipient.clone().with_name("".into()),
                        subject: message.subject.clone(),
                        body: message.body.clone(),
                        content_type: ContentType::Text,
                        reply_to: None,
                        attachments: vec![]
                    })
                )
                .await,
                Ok(Ok(true))
            );
            if !accepted {
                warn!(declaration_id=%*message.declaration_id,kind=%message.kind,"Declaration delivery failed; retry scheduled");
            }
            let ack: anyhow::Result<()> = async {
                let mut txn = match resolution_txn.take() {
                    Some(guard) => guard,
                    None => self.db.begin_transaction().await?,
                };
                self.contract_repo
                    .acknowledge_delivery(&mut txn, message, accepted)
                    .await?;
                txn.commit().await
            }
            .await;
            if ack.is_err() {
                failed = true;
                warn!("Declaration delivery acknowledgement failed; continuing fair pass");
            }
        }
        if failed {
            anyhow::bail!("Declaration acknowledgements failed; durable retry retained")
        }
        Ok(())
    }
    /// Return an error if the client IP address or the email address have
    /// exceeded the allowed number of declarations, otherwise count this
    /// attempt.
    async fn check_rate_limit(
        &self,
        client_ip: IpAddr,
        email: &EmailAddress,
    ) -> Result<(), ContractDeclareError> {
        let ip_key = self.rate_limit_key("ip", &client_ip.to_string());
        let email_key = self.rate_limit_key("email", &email.as_str().to_lowercase());

        let ip_count = self
            .cache
            .get::<u64>(&ip_key)
            .await
            .context("Failed to get rate limit counter from cache")?
            .unwrap_or(0);

        let email_count = self
            .cache
            .get::<u64>(&email_key)
            .await
            .context("Failed to get rate limit counter from cache")?
            .unwrap_or(0);

        if ip_count >= self.config.rate_limit_per_ip
            || email_count >= self.config.rate_limit_per_email
        {
            trace!("rate limit exceeded");
            return Err(ContractDeclareError::RateLimit);
        }

        let ttl = Some(self.config.rate_limit_window);

        self.cache
            .set(&ip_key, &(ip_count + 1), ttl)
            .await
            .context("Failed to save rate limit counter in cache")?;

        self.cache
            .set(&email_key, &(email_count + 1), ttl)
            .await
            .context("Failed to save rate limit counter in cache")?;

        Ok(())
    }

    fn rate_limit_key(&self, scope: &str, value: &str) -> String {
        let hash = self.hash.sha256(&value.to_owned());
        format!(
            "contract_declaration_rate_limit_{scope}_{}",
            hex::encode(hash.0)
        )
    }
}

fn result(declaration: ContractDeclaration) -> ContractDeclarationResult {
    let confirmation_email_sent = declaration
        .delivery
        .iter()
        .any(|d| d.kind == "receipt" && d.accepted_at.is_some());
    ContractDeclarationResult {
        declaration,
        confirmation_email_sent,
    }
}
fn same_submission(a: &ContractDeclaration, b: &ContractDeclaration) -> bool {
    a.kind == b.kind
        && a.name == b.name
        && a.email == b.email
        && a.contract == b.contract
        && a.contract_designation == b.contract_designation
        && a.cancellation_type == b.cancellation_type
        && a.details == b.details
        && a.requested_end == b.requested_end
}
fn same_requested_agreement(
    d: &ContractDeclaration,
    agreement: Option<academy_models::premium::PremiumRenewalId>,
) -> bool {
    let stored = d
        .operational_evidence
        .as_deref()
        .and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok());
    let original = stored
        .as_ref()
        .and_then(|v| v["messages"].as_array())
        .and_then(|messages| messages.iter().find(|m| m["kind"] == "receipt"))
        .and_then(|m| m["requested_agreement_id"].as_str());
    original == agreement.as_ref().map(|id| id.to_string()).as_deref()
}
fn receipt_body(d: &ContractDeclaration) -> String {
    let declaration = match d.kind {
        ContractDeclarationKind::Cancellation => "deine Kündigung",
        ContractDeclarationKind::Withdrawal => "dein Widerruf",
    };
    let mut body = format!(
        "Hallo,\n\n{declaration} ist am {} (Europe/Berlin) angekommen. Hier sind deine Angaben:\n\nVertrag: {}\nName: {}\nE-Mail: {}\n",
        format_datetime(d.received_at),
        contract_label(d.contract),
        *d.name,
        d.email.as_str(),
    );
    if let Some(designation) = designation(d) {
        body.push_str(&format!("Deine Vertragsbezeichnung: {designation}\n"));
    }
    if let Some(details) = details(d) {
        body.push_str(&format!("Deine Angaben oder Begründung: {details}\n"));
    }
    if d.kind == ContractDeclarationKind::Cancellation {
        body.push_str(&format!(
            "Art der Kündigung: {}\nGewünschtes Vertragsende: {}\n",
            d.cancellation_type
                .map(cancellation_type_label)
                .unwrap_or("–"),
            d.requested_end
                .map(format_datetime)
                .unwrap_or_else(|| "zum frühestmöglichen Zeitpunkt".into()),
        ));
    }
    body.push_str(&format!("\nDiese Mail bestätigt den Eingang deiner Erklärung.\n\nViele Grüße\nDein Bootstrap Academy Team\n\nReferenz für Rückfragen: {}\n", *d.id));
    body
}
fn details(declaration: &ContractDeclaration) -> Option<String> {
    Some(declaration.details.clone().into_inner()).filter(|details| !details.trim().is_empty())
}

fn designation(declaration: &ContractDeclaration) -> Option<String> {
    declaration
        .contract_designation
        .as_ref()
        .map(|designation| designation.clone().into_inner())
}

fn internal_notification_body(declaration: &ContractDeclaration) -> String {
    format!(
        "{urgent}\
         Art der Erklärung: {kind}\n\
         Eingegangen am: {received_at}\n\
         Name: {name}\n\
         E-Mail-Adresse: {email}\n\
         Konto: {account}\n\
         Vertrag: {contract}\n\
         Bezeichnung laut Erklärung: {designation}\n\
         Art der Kündigung: {cancellation_type}\n\
         Begründung/Angaben: {details}\n\
         Gewünschter Beendigungszeitpunkt: {requested_end}\n\
         Beendigungszeitpunkt: {effective_end}\n\
         ID der Erklärung: {id}\n",
        urgent = if is_extraordinary(declaration) {
            "DRINGEND: außerordentliche Kündigung. Der Beendigungszeitpunkt wurde nicht \
             automatisch ermittelt und ist gesondert in Textform zu bestätigen.\n\n"
        } else {
            ""
        },
        kind = kind_label(declaration.kind),
        received_at = format_datetime(declaration.received_at),
        name = *declaration.name,
        email = declaration.email.as_str(),
        account = match declaration.user_id {
            Some(user_id) => (*user_id).to_string(),
            None => "kein Konto gefunden".into(),
        },
        contract = contract_label(declaration.contract),
        designation = designation(declaration).unwrap_or_else(|| "-".into()),
        cancellation_type = declaration
            .cancellation_type
            .map(cancellation_type_label)
            .unwrap_or("-"),
        details = details(declaration).unwrap_or_else(|| "-".into()),
        requested_end = declaration
            .requested_end
            .map(format_date)
            .unwrap_or_else(|| "-".into()),
        effective_end = declaration
            .effective_end
            .map(format_date)
            .unwrap_or_else(|| "-".into()),
        id = *declaration.id,
    )
}

fn format_datetime(value: DateTime<Utc>) -> String {
    value
        .with_timezone(&chrono_tz::Europe::Berlin)
        .format(DATETIME_FORMAT)
        .to_string()
}

fn format_date(value: DateTime<Utc>) -> String {
    value
        .with_timezone(&chrono_tz::Europe::Berlin)
        .format(DATE_FORMAT)
        .to_string()
}

/// Whether the declaration is an extraordinary cancellation, which is examined
/// and answered by hand.
fn is_extraordinary(declaration: &ContractDeclaration) -> bool {
    declaration.cancellation_type == Some(ContractCancellationType::Extraordinary)
}

fn kind_label(kind: ContractDeclarationKind) -> &'static str {
    match kind {
        ContractDeclarationKind::Cancellation => "Kündigung",
        ContractDeclarationKind::Withdrawal => "Widerruf",
    }
}

fn contract_label(contract: ContractKind) -> &'static str {
    match contract {
        ContractKind::Premium => "Premium-Mitgliedschaft",
        ContractKind::Coins => "MorphCoins-Kauf",
        ContractKind::Other => "Sonstiger Vertrag",
    }
}

fn contract_short_label(contract: ContractKind) -> &'static str {
    match contract {
        ContractKind::Premium => "Premium",
        ContractKind::Coins => "Coins",
        ContractKind::Other => "Sonstiges",
    }
}

fn cancellation_type_label(cancellation_type: ContractCancellationType) -> &'static str {
    match cancellation_type {
        ContractCancellationType::Ordinary => "ordentliche Kündigung",
        ContractCancellationType::Extraordinary => "außerordentliche Kündigung",
    }
}
