use academy_di::Build;
use academy_models::{
    user::UserId,
    withdrawal::{WithdrawalConsent, WithdrawalSubject},
};
use academy_persistence_contracts::withdrawal::WithdrawalRepository;
use academy_utils::trace_instrument;
use clorinde::{
    client::Params,
    queries::{self, withdrawal::CreateParams},
};
use futures::{StreamExt, TryStreamExt};

use crate::PostgresTransaction;

#[derive(Debug, Clone, Build)]
pub struct PostgresWithdrawalRepository;

impl WithdrawalRepository<PostgresTransaction> for PostgresWithdrawalRepository {
    #[trace_instrument(skip(self, txn))]
    async fn create(
        &self,
        txn: &mut PostgresTransaction,
        consent: &WithdrawalConsent,
    ) -> anyhow::Result<()> {
        let params = CreateParams {
            id: *consent.id,
            user_id: *consent.user_id,
            subject: consent.subject.as_str(),
            reference: consent.reference.as_deref(),
            text_version: &*consent.text_version,
            consented_at: consent.consented_at.into(),
        };

        queries::withdrawal::create()
            .params(txn.txn(), &params)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn list_by_user_id(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Vec<WithdrawalConsent>> {
        queries::withdrawal::list_by_user_id()
            .bind(txn.txn(), &user_id)
            .iter()
            .await?
            .map(|row| row.map_err(Into::into).and_then(decode_consent))
            .try_collect()
            .await
    }
}

fn decode_consent(value: queries::withdrawal::Consent) -> anyhow::Result<WithdrawalConsent> {
    Ok(WithdrawalConsent {
        id: value.id.into(),
        user_id: value.user_id.into(),
        subject: value.subject.parse::<WithdrawalSubject>()?,
        reference: value.reference.map(TryInto::try_into).transpose()?,
        text_version: value.text_version.try_into()?,
        consented_at: value.consented_at.into(),
    })
}
