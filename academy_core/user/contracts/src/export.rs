use std::{collections::BTreeMap, future::Future};

use academy_models::{
    coin::{Balance, Transaction},
    contract::ContractDeclaration,
    heart::Hearts,
    oauth2::OAuth2Link,
    paypal::PaypalCoinOrder,
    premium::{Premium, PremiumPlan},
    session::Session,
    user::{UserComposite, UserId},
    withdrawal::WithdrawalConsent,
};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait UserExportService<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Collect everything the monolith stores about the given user.
    ///
    /// Returns [`None`] if the user does not exist.
    fn export(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<Option<AccountDataExport>>> + Send;
}

/// Everything the platform stores about a single user (Art. 15 and 20 GDPR).
#[derive(Debug, Clone, PartialEq)]
pub struct UserDataExport {
    /// The data stored by the monolith.
    pub account: AccountDataExport,
    /// The data stored by the microservices, keyed by the name of the service.
    ///
    /// A service that could not be read is part of this map as
    /// [`ServiceDataExport::Unavailable`], so that a missing part of the export
    /// is always named.
    pub services: BTreeMap<String, ServiceDataExport>,
}

impl UserDataExport {
    /// Whether every microservice could be read, so that this export is
    /// everything the platform stores about the user.
    pub fn is_complete(&self) -> bool {
        self.services
            .values()
            .all(|export| matches!(export, ServiceDataExport::Data(_)))
    }
}

/// The data a single microservice stores about a user.
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceDataExport {
    /// The data the microservice returned.
    Data(serde_json::Value),
    /// The microservice could not be read, so this part of the export is
    /// missing. The reason is logged; it is deliberately not reported to the
    /// user, because it can contain internal urls.
    Unavailable,
}

impl From<Option<serde_json::Value>> for ServiceDataExport {
    fn from(value: Option<serde_json::Value>) -> Self {
        match value {
            Some(data) => Self::Data(data),
            None => Self::Unavailable,
        }
    }
}

/// Everything the monolith stores about a single user.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountDataExport {
    /// The account itself, including the profile and the invoice information.
    pub user: UserComposite,
    /// The sessions of the user, without any tokens.
    pub sessions: Vec<Session>,
    /// The OAuth2 accounts the user has linked.
    pub oauth2_links: Vec<OAuth2Link>,
    /// The current Morphcoin balance of the user.
    pub balance: Balance,
    /// The hearts of the user, unset if they never spent one.
    pub hearts: Option<Hearts>,
    /// The Morphcoin transactions of the user, oldest first.
    pub transactions: Vec<Transaction>,
    /// The most recent premium membership of the user, if any.
    pub premium: Option<Premium>,
    /// The plan the premium membership is renewed with, if any.
    pub premium_subscription: Option<PremiumPlan>,
    /// The coin orders of the user, which are the invoices issued to them.
    pub invoices: Vec<PaypalCoinOrder>,
    /// The cancellations and withdrawals the user has declared.
    pub contract_declarations: Vec<ContractDeclaration>,
    /// The declarations the user gave before placing an order.
    pub withdrawal_consents: Vec<WithdrawalConsent>,
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockUserExportService<Txn> {
    pub fn with_export(mut self, user_id: UserId, result: Option<AccountDataExport>) -> Self {
        self.expect_export()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }
}
