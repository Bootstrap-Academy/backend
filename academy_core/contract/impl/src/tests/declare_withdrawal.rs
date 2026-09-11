use super::*;
use academy_core_contract_contracts::ContractFeatureService;
use academy_demo::UUID1;
use academy_models::contract::ContractDeliveryAttempt;
#[tokio::test]
async fn rejected_and_exceptional_smtp_leave_durable_retry() {
    for throws in [false, true] {
        let message = ContractDeliveryAttempt {
            requested_agreement_id: None,
            declaration_id: UUID1.into(),
            kind: "receipt".into(),
            recipient: unknown_email(),
            subject: "Immutable subject".into(),
            body: "Immutable original receipt".into(),
            generation: 7,
        };
        let mut repo = MockContractRepository::new();
        repo.expect_recover_schedules()
            .once()
            .return_once(|_| Box::pin(async { Ok(()) }));
        let copy = message.clone();
        repo.expect_claim_delivery()
            .once()
            .return_once(|_, _, _| Box::pin(async { Ok(Some(copy)) }));
        repo.expect_claim_delivery()
            .once()
            .return_once(|_, _, _| Box::pin(async { Ok(None) }));
        repo.expect_acknowledge_delivery()
            .once()
            .withf(move |_, m, accepted| m == &message && !accepted)
            .return_once(|_, _, _| Box::pin(async { Ok(()) }));
        let mut email = MockEmailService::new();
        email
            .expect_send()
            .once()
            .withf(|m| {
                m.body == "Immutable original receipt"
                    && m.message_id
                        .as_ref()
                        .is_some_and(|id| id.contains("-receipt@"))
            })
            .return_once(move |_| {
                Box::pin(async move {
                    if throws {
                        Err(anyhow::anyhow!("SMTP failed"))
                    } else {
                        Ok(false)
                    }
                })
            });
        let mut db = MockDatabase::new();
        db.expect_begin_transaction().times(4).returning(|| {
            let mut txn = MockTransaction::new();
            txn.expect_commit()
                .times(0..=1)
                .returning(|| Box::pin(async { Ok(()) }));
            Box::pin(async { Ok(txn) })
        });
        let sut = Sut {
            db,
            contract_repo: repo,
            email,
            ..Sut::default()
        };
        sut.retry_confirmations().await.unwrap();
    }
}
