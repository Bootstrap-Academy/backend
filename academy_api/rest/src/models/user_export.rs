use std::collections::BTreeMap;

use academy_core_user_contracts::export::{AccountDataExport, UserDataExport};
use academy_models::{
    coin::{Transaction, TransactionDescription, TransactionId},
    oauth2::{
        OAuth2Link, OAuth2LinkId, OAuth2ProviderId, OAuth2RemoteUserId, OAuth2RemoteUserName,
    },
    paypal::{PaypalCoinOrder, PaypalOrderId},
    session::{DeviceName, Session, SessionId},
    withdrawal::{
        WithdrawalConsent, WithdrawalConsentId, WithdrawalReference, WithdrawalTextVersion,
    },
};
use schemars::JsonSchema;
use serde::Serialize;

use super::{
    coin::ApiBalance,
    contract::{ApiContractDeclaration, ApiTimestamp},
    premium::ApiPremiumPlan,
    user::ApiUser,
    withdrawal::ApiWithdrawalSubject,
};

/// Everything the platform stores about a single user.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ApiUserDataExport {
    /// The data stored by this service
    pub account: ApiAccountDataExport,
    /// The data stored by the microservices, keyed by the name of the service
    pub services: BTreeMap<String, serde_json::Value>,
}

/// Everything this service stores about a single user.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ApiAccountDataExport {
    /// The account, the profile and the invoice information, in the same
    /// representation as `GET /auth/users/{user_id}`
    pub user: ApiUser,
    /// The sessions of the user, without any tokens
    pub sessions: Vec<ApiExportSession>,
    /// The OAuth2 accounts the user has linked
    pub oauth2_links: Vec<ApiExportOAuth2Link>,
    /// The current Morphcoin balance of the user
    pub balance: ApiBalance,
    /// The Morphcoin transactions of the user, oldest first
    pub transactions: Vec<ApiExportTransaction>,
    /// The premium membership of the user, if any
    pub premium: Option<ApiExportPremium>,
    /// The invoices issued to the user, oldest first
    pub invoices: Vec<ApiExportInvoice>,
    /// The cancellations and withdrawals the user has declared, oldest first
    pub contract_declarations: Vec<ApiContractDeclaration>,
    /// The declarations the user gave before placing an order, oldest first
    pub withdrawal_consents: Vec<ApiExportWithdrawalConsent>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ApiExportSession {
    pub id: SessionId,
    pub device_name: Option<DeviceName>,
    pub created_at: ApiTimestamp,
    pub last_update: ApiTimestamp,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ApiExportOAuth2Link {
    pub id: OAuth2LinkId,
    pub provider_id: OAuth2ProviderId,
    /// Identifier of the user at the OAuth2 provider
    pub remote_user_id: OAuth2RemoteUserId,
    /// Display name of the user at the OAuth2 provider
    pub remote_user_name: OAuth2RemoteUserName,
    pub created_at: ApiTimestamp,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ApiExportTransaction {
    pub id: TransactionId,
    /// Number of Morphcoins added to (positive) or taken from (negative) the
    /// balance
    pub coins: i64,
    pub description: Option<TransactionDescription>,
    pub created_at: ApiTimestamp,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ApiExportPremium {
    pub since: ApiTimestamp,
    pub until: ApiTimestamp,
    /// The plan the membership is renewed with, if it is renewed
    pub subscription: Option<ApiPremiumPlan>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ApiExportInvoice {
    /// Order id at the payment provider
    pub order_id: PaypalOrderId,
    pub invoice_number: u64,
    /// Number of Morphcoins that were purchased
    pub coins: u64,
    pub created_at: ApiTimestamp,
    /// Point in time at which the payment was captured, unset for an order
    /// that was never paid
    pub captured_at: Option<ApiTimestamp>,
    /// Point in time at which the declarations under § 356 Abs. 6 Nr. 2 BGB
    /// were given
    pub withdrawal_consent_at: Option<ApiTimestamp>,
    /// Version of the withdrawal instruction the declarations were taken from
    pub withdrawal_text_version: Option<WithdrawalTextVersion>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ApiExportWithdrawalConsent {
    pub id: WithdrawalConsentId,
    pub subject: ApiWithdrawalSubject,
    /// Identifier of the purchased item, if the purchase has one
    pub reference: Option<WithdrawalReference>,
    pub text_version: WithdrawalTextVersion,
    pub consented_at: ApiTimestamp,
}

impl From<UserDataExport> for ApiUserDataExport {
    fn from(value: UserDataExport) -> Self {
        Self {
            account: value.account.into(),
            services: value.services,
        }
    }
}

impl From<AccountDataExport> for ApiAccountDataExport {
    fn from(value: AccountDataExport) -> Self {
        let premium_subscription = value.premium_subscription;

        Self {
            user: value.user.into(),
            sessions: value.sessions.into_iter().map(Into::into).collect(),
            oauth2_links: value.oauth2_links.into_iter().map(Into::into).collect(),
            balance: value.balance.into(),
            transactions: value.transactions.into_iter().map(Into::into).collect(),
            premium: value.premium.map(|premium| ApiExportPremium {
                since: premium.since.into(),
                until: premium.until.into(),
                subscription: premium_subscription.map(Into::into),
            }),
            invoices: value.invoices.into_iter().map(Into::into).collect(),
            contract_declarations: value
                .contract_declarations
                .into_iter()
                .map(Into::into)
                .collect(),
            withdrawal_consents: value
                .withdrawal_consents
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }
}

impl From<Session> for ApiExportSession {
    fn from(value: Session) -> Self {
        Self {
            id: value.id,
            device_name: value.device_name,
            created_at: value.created_at.into(),
            last_update: value.updated_at.into(),
        }
    }
}

impl From<OAuth2Link> for ApiExportOAuth2Link {
    fn from(value: OAuth2Link) -> Self {
        Self {
            id: value.id,
            provider_id: value.provider_id,
            remote_user_id: value.remote_user.id,
            remote_user_name: value.remote_user.name,
            created_at: value.created_at.into(),
        }
    }
}

impl From<Transaction> for ApiExportTransaction {
    fn from(value: Transaction) -> Self {
        Self {
            id: value.id,
            coins: value.coins,
            description: value.description,
            created_at: value.created_at.into(),
        }
    }
}

impl From<PaypalCoinOrder> for ApiExportInvoice {
    fn from(value: PaypalCoinOrder) -> Self {
        Self {
            order_id: value.id,
            invoice_number: value.invoice_number,
            coins: value.coins,
            created_at: value.created_at.into(),
            captured_at: value.captured_at.map(Into::into),
            withdrawal_consent_at: value.withdrawal_consent_at.map(Into::into),
            withdrawal_text_version: value.withdrawal_text_version,
        }
    }
}

impl From<WithdrawalConsent> for ApiExportWithdrawalConsent {
    fn from(value: WithdrawalConsent) -> Self {
        Self {
            id: value.id,
            subject: value.subject.into(),
            reference: value.reference,
            text_version: value.text_version,
            consented_at: value.consented_at.into(),
        }
    }
}
