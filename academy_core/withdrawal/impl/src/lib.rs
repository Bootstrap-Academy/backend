use academy_auth_contracts::{AuthResultExt, AuthService};
use academy_core_withdrawal_contracts::{
    WithdrawalFeatureService, WithdrawalRecordConsentError, consent::WithdrawalConsentService,
};
use academy_di::Build;
use academy_models::{
    auth::AccessToken,
    withdrawal::{
        WithdrawalConsent, WithdrawalConsentDeclaration, WithdrawalReference, WithdrawalSubject,
    },
};
use academy_persistence_contracts::{Database, Transaction};
use academy_utils::trace_instrument;

pub mod consent;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct WithdrawalFeatureServiceImpl<Db, Auth, WithdrawalConsentS> {
    db: Db,
    auth: Auth,
    withdrawal_consent: WithdrawalConsentS,
}

impl<Db, Auth, WithdrawalConsentS> WithdrawalFeatureService
    for WithdrawalFeatureServiceImpl<Db, Auth, WithdrawalConsentS>
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    WithdrawalConsentS: WithdrawalConsentService<Db::Transaction>,
{
    #[trace_instrument(skip(self))]
    async fn record_consent(
        &self,
        token: &AccessToken,
        subject: WithdrawalSubject,
        reference: Option<WithdrawalReference>,
        declaration: WithdrawalConsentDeclaration,
    ) -> Result<WithdrawalConsent, WithdrawalRecordConsentError> {
        let text_version = declaration
            .text_version()
            .ok_or(WithdrawalRecordConsentError::ConsentMissing)?
            .clone();

        let auth = self.auth.authenticate(token).await.map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        let consent = self
            .withdrawal_consent
            .record(&mut txn, auth.user_id, subject, reference, text_version)
            .await?;

        txn.commit().await?;

        Ok(consent)
    }
}
