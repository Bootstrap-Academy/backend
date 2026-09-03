use academy_core_withdrawal_contracts::consent::WithdrawalConsentService;
use academy_di::Build;
use academy_models::{
    user::UserId,
    withdrawal::{
        WithdrawalConsent, WithdrawalReference, WithdrawalSubject, WithdrawalTextVersion,
    },
};
use academy_persistence_contracts::withdrawal::WithdrawalRepository;
use academy_shared_contracts::{id::IdService, time::TimeService};
use academy_utils::trace_instrument;

#[derive(Debug, Clone, Build, Default)]
pub struct WithdrawalConsentServiceImpl<Id, Time, WithdrawalRepo> {
    id: Id,
    time: Time,
    withdrawal_repo: WithdrawalRepo,
}

impl<Txn, Id, Time, WithdrawalRepo> WithdrawalConsentService<Txn>
    for WithdrawalConsentServiceImpl<Id, Time, WithdrawalRepo>
where
    Txn: Send + Sync + 'static,
    Id: IdService,
    Time: TimeService,
    WithdrawalRepo: WithdrawalRepository<Txn>,
{
    #[trace_instrument(skip(self, txn))]
    async fn record(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        subject: WithdrawalSubject,
        reference: Option<WithdrawalReference>,
        text_version: WithdrawalTextVersion,
    ) -> anyhow::Result<WithdrawalConsent> {
        let consent = WithdrawalConsent {
            id: self.id.generate(),
            user_id,
            subject,
            reference,
            text_version,
            consented_at: self.time.now(),
        };

        self.withdrawal_repo.create(txn, &consent).await?;

        Ok(consent)
    }
}

#[cfg(test)]
mod tests {
    use academy_demo::{UUID1, user::FOO};
    use academy_persistence_contracts::withdrawal::MockWithdrawalRepository;
    use academy_shared_contracts::{id::MockIdService, time::MockTimeService};

    use super::*;

    #[tokio::test]
    async fn record() {
        // Arrange
        let expected = WithdrawalConsent {
            id: UUID1.into(),
            user_id: FOO.user.id,
            subject: WithdrawalSubject::Course,
            reference: Some("html".try_into().unwrap()),
            text_version: "2026-09".try_into().unwrap(),
            consented_at: FOO.user.created_at,
        };

        let id = MockIdService::new().with_generate(expected.id);
        let time = MockTimeService::new().with_now(expected.consented_at);
        let withdrawal_repo = MockWithdrawalRepository::new().with_create(expected.clone());

        let sut = WithdrawalConsentServiceImpl {
            id,
            time,
            withdrawal_repo,
        };

        // Act
        let result = sut
            .record(
                &mut (),
                expected.user_id,
                expected.subject,
                expected.reference.clone(),
                expected.text_version.clone(),
            )
            .await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }
}
