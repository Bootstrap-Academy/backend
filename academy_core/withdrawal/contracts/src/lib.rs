use std::future::Future;

use academy_models::{
    auth::{AccessToken, AuthError},
    withdrawal::{
        WithdrawalConsent, WithdrawalConsentDeclaration, WithdrawalReference, WithdrawalSubject,
    },
};
use thiserror::Error;

pub mod consent;

pub trait WithdrawalFeatureService: Send + Sync + 'static {
    /// Record the declarations the authenticated user gave before placing an
    /// order (§ 356 Abs. 5 Nr. 2, Abs. 6 Nr. 2 BGB).
    ///
    /// Purchases that are completed by the backend itself record the
    /// declarations as part of the purchase. This use case exists for the
    /// purchases that are completed by one of the other services, which the
    /// client calls directly.
    fn record_consent(
        &self,
        token: &AccessToken,
        subject: WithdrawalSubject,
        reference: Option<WithdrawalReference>,
        declaration: WithdrawalConsentDeclaration,
    ) -> impl Future<Output = Result<WithdrawalConsent, WithdrawalRecordConsentError>> + Send;
}

#[derive(Debug, Error)]
pub enum WithdrawalRecordConsentError {
    #[error("The user did not give the withdrawal declarations.")]
    ConsentMissing,
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
