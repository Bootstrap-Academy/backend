use std::{net::IpAddr, sync::Arc, time::Duration};

use academy_auth_contracts::{AuthResultExt, AuthService};
use academy_cache_contracts::CacheService;
use academy_core_contract_contracts::{
    ContractCancellationRequest, ContractDeclarationListQuery, ContractDeclarationListResult,
    ContractDeclarationResult, ContractDeclareError, ContractFeatureService, ContractListError,
    ContractWithdrawalRequest,
};
use academy_di::Build;
use academy_email_contracts::{ContentType, Email, EmailService, template::TemplateEmailService};
use academy_models::{
    auth::AccessToken,
    contract::{
        ContractCancellationType, ContractDeclaration, ContractDeclarationKind, ContractKind,
    },
    email_address::{EmailAddress, EmailAddressWithName},
};
use academy_persistence_contracts::{
    Database, Transaction, contract::ContractRepository, premium::PremiumRepository,
    user::UserRepository,
};
use academy_shared_contracts::{hash::HashService, id::IdService, time::TimeService};
use academy_templates_contracts::{
    ContractCancellationConfirmationTemplate, ContractWithdrawalConfirmationTemplate,
};
use academy_utils::trace_instrument;
use anyhow::Context;
use chrono::{DateTime, Utc};
use tracing::{error, trace};

#[cfg(test)]
mod tests;

/// Format used to render a point in time in the `Europe/Berlin` time zone.
const DATETIME_FORMAT: &str = "%d.%m.%Y um %H:%M:%S Uhr";
/// Format used to render a date in the `Europe/Berlin` time zone.
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
    TemplateEmail,
    EmailS,
    UserRepo,
    PremiumRepo,
    ContractRepo,
> {
    db: Db,
    auth: Auth,
    id: Id,
    time: Time,
    cache: Cache,
    hash: Hash,
    template_email: TemplateEmail,
    email: EmailS,
    user_repo: UserRepo,
    premium_repo: PremiumRepo,
    contract_repo: ContractRepo,
    config: ContractFeatureConfig,
}

#[derive(Debug, Clone)]
pub struct ContractFeatureConfig {
    /// Internal recipient of the notifications about new declarations.
    pub internal_email: Arc<EmailAddressWithName>,
    /// Time window used for rate limiting declarations.
    pub rate_limit_window: Duration,
    /// Maximum number of declarations per IP address / email address within
    /// [`ContractFeatureConfig::rate_limit_window`].
    pub rate_limit_count: u64,
}

impl<Db, Auth, Id, Time, Cache, Hash, TemplateEmail, EmailS, UserRepo, PremiumRepo, ContractRepo>
    ContractFeatureService
    for ContractFeatureServiceImpl<
        Db,
        Auth,
        Id,
        Time,
        Cache,
        Hash,
        TemplateEmail,
        EmailS,
        UserRepo,
        PremiumRepo,
        ContractRepo,
    >
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    Id: IdService,
    Time: TimeService,
    Cache: CacheService,
    Hash: HashService,
    TemplateEmail: TemplateEmailService,
    EmailS: EmailService,
    UserRepo: UserRepository<Db::Transaction>,
    PremiumRepo: PremiumRepository<Db::Transaction>,
    ContractRepo: ContractRepository<Db::Transaction>,
{
    #[trace_instrument(skip(self))]
    async fn declare_cancellation(
        &self,
        client_ip: IpAddr,
        request: ContractCancellationRequest,
    ) -> Result<ContractDeclarationResult, ContractDeclareError> {
        self.check_rate_limit(client_ip, &request.email).await?;

        let received_at = self.time.now();
        let id = self.id.generate();
        let cancellation_type = request.cancellation_type;

        let mut txn = self.db.begin_transaction().await?;

        let user_id = self
            .user_repo
            .get_composite_by_email(&mut txn, &request.email)
            .await
            .context("Failed to get user from database")?
            .map(|user_composite| user_composite.user.id);

        // The end of the contract can only be determined for premium
        // memberships of a known account.
        let effective_end = match user_id.filter(|_| request.contract == ContractKind::Premium) {
            Some(user_id) => {
                trace!("disable premium subscription");
                self.premium_repo
                    .set_subscription(&mut txn, user_id, None)
                    .await
                    .context("Failed to disable premium subscription")?;

                // Read the repository directly instead of using the premium use
                // case, which would purchase a new period for an expired
                // membership and therefore debit coins.
                let latest = self
                    .premium_repo
                    .get_latest_by_user_id(&mut txn, user_id)
                    .await
                    .context("Failed to get premium membership from database")?;

                Some(match latest {
                    Some(premium) if received_at < premium.until => premium.until,
                    _ => received_at,
                })
            }
            None => None,
        };

        let declaration = ContractDeclaration {
            id,
            kind: ContractDeclarationKind::Cancellation,
            received_at,
            name: request.name,
            email: request.email,
            user_id,
            contract: request.contract,
            cancellation_type: Some(cancellation_type),
            details: request.details,
            requested_end: request.requested_end,
            effective_end,
            processed_at: None,
        };

        self.contract_repo
            .create(&mut txn, declaration.clone())
            .await
            .context("Failed to create contract declaration in database")?;

        // The declaration is durable before any email is attempted.
        txn.commit().await?;

        let template = ContractCancellationConfirmationTemplate {
            received_at: format_datetime(declaration.received_at),
            name: declaration.name.clone().into_inner(),
            email: declaration.email.as_str().into(),
            contract: contract_label(declaration.contract).into(),
            cancellation_type: cancellation_type_label(cancellation_type).into(),
            details: details(&declaration),
            requested_end: declaration.requested_end.map(format_date),
            effective_end: declaration.effective_end.map(format_date),
        };

        trace!("send confirmation email");
        let confirmation_email_sent = match self
            .template_email
            .send_contract_cancellation_confirmation_email(recipient(&declaration), &template)
            .await
        {
            Ok(sent) => {
                if !sent {
                    error!("Failed to send contract cancellation confirmation email");
                }
                sent
            }
            Err(err) => {
                error!("Failed to send contract cancellation confirmation email: {err:#}");
                false
            }
        };

        self.send_internal_notification(&declaration).await;

        Ok(ContractDeclarationResult {
            declaration,
            confirmation_email_sent,
        })
    }

    #[trace_instrument(skip(self))]
    async fn declare_withdrawal(
        &self,
        client_ip: IpAddr,
        request: ContractWithdrawalRequest,
    ) -> Result<ContractDeclarationResult, ContractDeclareError> {
        self.check_rate_limit(client_ip, &request.email).await?;

        let received_at = self.time.now();
        let id = self.id.generate();

        let mut txn = self.db.begin_transaction().await?;

        let user_id = self
            .user_repo
            .get_composite_by_email(&mut txn, &request.email)
            .await
            .context("Failed to get user from database")?
            .map(|user_composite| user_composite.user.id);

        let declaration = ContractDeclaration {
            id,
            kind: ContractDeclarationKind::Withdrawal,
            received_at,
            name: request.name,
            email: request.email,
            user_id,
            contract: request.contract,
            cancellation_type: None,
            details: request.details,
            requested_end: None,
            effective_end: None,
            processed_at: None,
        };

        self.contract_repo
            .create(&mut txn, declaration.clone())
            .await
            .context("Failed to create contract declaration in database")?;

        // The declaration is durable before any email is attempted.
        txn.commit().await?;

        let template = ContractWithdrawalConfirmationTemplate {
            received_at: format_datetime(declaration.received_at),
            name: declaration.name.clone().into_inner(),
            email: declaration.email.as_str().into(),
            contract: contract_label(declaration.contract).into(),
            details: details(&declaration),
        };

        trace!("send confirmation email");
        let confirmation_email_sent = match self
            .template_email
            .send_contract_withdrawal_confirmation_email(recipient(&declaration), &template)
            .await
        {
            Ok(sent) => {
                if !sent {
                    error!("Failed to send contract withdrawal confirmation email");
                }
                sent
            }
            Err(err) => {
                error!("Failed to send contract withdrawal confirmation email: {err:#}");
                false
            }
        };

        self.send_internal_notification(&declaration).await;

        Ok(ContractDeclarationResult {
            declaration,
            confirmation_email_sent,
        })
    }

    #[trace_instrument(skip(self))]
    async fn list_declarations(
        &self,
        token: &AccessToken,
        query: ContractDeclarationListQuery,
    ) -> Result<ContractDeclarationListResult, ContractListError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        auth.ensure_admin().map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        let total = self
            .contract_repo
            .count(&mut txn, query.kind)
            .await
            .context("Failed to count contract declarations in database")?;

        let declarations = self
            .contract_repo
            .list(&mut txn, query.kind, query.pagination)
            .await
            .context("Failed to get contract declarations from database")?;

        txn.commit().await?;

        Ok(ContractDeclarationListResult {
            total,
            declarations,
        })
    }
}

impl<Db, Auth, Id, Time, Cache, Hash, TemplateEmail, EmailS, UserRepo, PremiumRepo, ContractRepo>
    ContractFeatureServiceImpl<
        Db,
        Auth,
        Id,
        Time,
        Cache,
        Hash,
        TemplateEmail,
        EmailS,
        UserRepo,
        PremiumRepo,
        ContractRepo,
    >
where
    Cache: CacheService,
    Hash: HashService,
    EmailS: EmailService,
{
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

        if ip_count >= self.config.rate_limit_count || email_count >= self.config.rate_limit_count {
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

    /// Notify the support team about a new declaration.
    ///
    /// Failures are logged but never propagated: the stored declaration is what
    /// matters.
    async fn send_internal_notification(&self, declaration: &ContractDeclaration) {
        let email = Email {
            recipient: (*self.config.internal_email).clone(),
            subject: format!(
                "[Contract] {} ({})",
                kind_label(declaration.kind),
                contract_short_label(declaration.contract)
            ),
            body: internal_notification_body(declaration),
            content_type: ContentType::Text,
            reply_to: Some(recipient(declaration)),
            attachments: Vec::new(),
        };

        trace!("send internal notification email");
        match self.email.send(email).await {
            Ok(true) => {}
            Ok(false) => error!("Failed to send contract declaration notification email"),
            Err(err) => {
                error!("Failed to send contract declaration notification email: {err:#}")
            }
        }
    }
}

fn recipient(declaration: &ContractDeclaration) -> EmailAddressWithName {
    declaration
        .email
        .clone()
        .with_name(declaration.name.clone().into_inner())
}

fn details(declaration: &ContractDeclaration) -> Option<String> {
    Some(declaration.details.clone().into_inner()).filter(|details| !details.trim().is_empty())
}

fn internal_notification_body(declaration: &ContractDeclaration) -> String {
    format!(
        "Art der Erklärung: {kind}\n\
         Eingegangen am: {received_at}\n\
         Name: {name}\n\
         E-Mail-Adresse: {email}\n\
         Konto: {account}\n\
         Vertrag: {contract}\n\
         Art der Kündigung: {cancellation_type}\n\
         Begründung/Angaben: {details}\n\
         Gewünschter Beendigungszeitpunkt: {requested_end}\n\
         Beendigungszeitpunkt: {effective_end}\n\
         ID der Erklärung: {id}\n",
        kind = kind_label(declaration.kind),
        received_at = format_datetime(declaration.received_at),
        name = *declaration.name,
        email = declaration.email.as_str(),
        account = match declaration.user_id {
            Some(user_id) => (*user_id).to_string(),
            None => "kein Konto gefunden".into(),
        },
        contract = contract_label(declaration.contract),
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
