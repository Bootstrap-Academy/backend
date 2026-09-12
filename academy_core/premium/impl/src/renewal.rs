use academy_assets::email::{AGB_2026_09_R2_PDF, WIDERRUFSBELEHRUNG_2026_09_R1_PDF};
use academy_core_premium_contracts::{
    PremiumUpdateSubscriptionError, renewal::PremiumRenewalService,
};
use academy_di::Build;
use academy_email_contracts::{
    AttachmentContentType, ContentType, Email, EmailAttachment, EmailService,
};
use academy_models::{
    premium::{PremiumRenewalAgreement, PremiumRenewalConsent, PremiumRenewalOffer},
    user::UserId,
};
use academy_persistence_contracts::{
    Database, Transaction, premium::PremiumRepository, user::UserRepository,
};
use academy_shared_contracts::time::TimeService;
use sha2::{Digest, Sha256};

use crate::PremiumFeatureConfig;

pub const RENEWAL_TERMS_VERSION: &str = "2026-09-r2";
pub const RENEWAL_TEXT_VERSION: &str = "premium-renewal-2026-09-v2";

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct PremiumRenewalServiceImpl<Db, Time, UserRepo, PremiumRepo, EmailS> {
    db: Db,
    time: Time,
    user_repo: UserRepo,
    premium_repo: PremiumRepo,
    email: EmailS,
    config: PremiumFeatureConfig,
}

impl<Db, Time, UserRepo, PremiumRepo, EmailS> PremiumRenewalService
    for PremiumRenewalServiceImpl<Db, Time, UserRepo, PremiumRepo, EmailS>
where
    Db: Database,
    Time: TimeService,
    UserRepo: UserRepository<Db::Transaction>,
    PremiumRepo: PremiumRepository<Db::Transaction>,
    EmailS: EmailService,
{
    fn offer(&self) -> PremiumRenewalOffer {
        let price = self.config.monthly_price;
        let euros = price as f64 / 100.0;
        let text = format!(
            "Premium: Zugriff auf alle Kurse und Übungen ohne Verbrauch von Herzen; Webinare und Coachings sind nicht enthalten.\n\n\
             Monatliche automatische Verlängerung für {price} MorphCoins ({euros:.2} EUR einschließlich Umsatzsteuer) je Kalendermonat. \
             Die Verlängerung läuft auf unbestimmte Zeit. Du zahlst nur aus deinem vorhandenen MorphCoin-Guthaben. \
             Der Coin-Preis bleibt für diese Verlängerung fest. Es gibt keine automatische Zahlung über PayPal. Du musst keine Coins nachkaufen.\n\n\
             Die erste Abbuchung erfolgt nach Ablauf deiner bereits bezahlten Premium-Zeit und nach Versand dieser Vertragsbestätigung, \
             spätestens am folgenden Tag oder bei deiner nächsten Nutzung. Dein neuer Kalendermonat beginnt mit der Abbuchung. \
             Wenn du Premium zusätzlich kaufst, läuft diese Zeit zuerst ab. Deine automatische Verlängerung bleibt bestehen. \
             Die Vertragsbestätigung muss vor dem Ende deiner bei der Bestellung bereits bezahlten Premium-Zeit versandt sein. \
             Ein zusätzlicher Premium-Kauf verlängert diese Frist nicht. Wird sie verpasst, endet die automatische Verlängerung. Es wird auch später nichts dafür abgebucht; dafür müsstest du die Verlängerung neu bestellen. \
             Reicht dein Guthaben nicht aus, endet Premium ohne weitere Kosten und die automatische Verlängerung wird ausgeschaltet.\n\n\
             Du kannst jederzeit zum Ende deiner bezahlten Laufzeit kündigen, nach einer automatischen Verlängerung zum Ende des laufenden Monats: \
             auf der Seite Abonnement durch Ausschalten, unter https://bootstrap.academy/vertrag-kuendigen oder per E-Mail an hallo@bootstrap.academy. \
             Es wird immer nur ein Monat verlängert, kein ganzes Jahr. Deine gesetzlichen Widerrufs- und Mängelrechte bleiben bestehen.\n\n\
             Ich stimme dieser monatlichen kostenpflichtigen Verlängerung und den AGB {RENEWAL_TERMS_VERSION} für diese Vereinbarung ausdrücklich zu. \
             Meine übrigen bestehenden Verträge werden dadurch nicht geändert.\n\n\
             Ich verlange ausdrücklich und stimme zu, dass Sie vor Ablauf der Widerrufsfrist mit der Erbringung der Dienstleistung beginnen.\n\
             Mir ist bekannt, dass mein Widerrufsrecht mit vollständiger Erbringung der Dienstleistung erlischt.\n\n\
             Vertrags- und Erklärungssprache: Deutsch. Erklärungsversion: {RENEWAL_TEXT_VERSION}."
        );
        // The client acknowledges the exact offer, including document bytes;
        // changing price or an attachment invalidates outstanding confirmations.
        let mut hash = Sha256::new();
        for bytes in [
            text.as_bytes(),
            AGB_2026_09_R2_PDF,
            WIDERRUFSBELEHRUNG_2026_09_R1_PDF,
        ] {
            hash.update(bytes);
        }
        PremiumRenewalOffer {
            id: format!("{:x}", hash.finalize()),
            monthly_price: price,
            terms_version: RENEWAL_TERMS_VERSION.into(),
            text,
        }
    }

    async fn enable(
        &self,
        user_id: UserId,
        consent: PremiumRenewalConsent,
    ) -> Result<(), PremiumUpdateSubscriptionError> {
        let offer = self.offer();
        if !consent.accepted
            || !consent.withdrawal_consent
            || consent.offer_id != offer.id
            || offer.monthly_price == 0
        {
            return Err(PremiumUpdateSubscriptionError::RenewalConsentRequired);
        }
        let mut txn = self.db.begin_transaction().await?;
        // This read takes the per-user lock: cancellation, purchase, renewal and
        // consent creation are serialized. It never triggers a renewal charge.
        let paid = self
            .premium_repo
            .get_latest_by_user_id(&mut txn, user_id)
            .await?;
        if let Some(existing) = self
            .premium_repo
            .get_renewal_agreement(&mut txn, consent.request_id)
            .await?
        {
            if existing.user_id != user_id || existing.offer_id != offer.id {
                return Err(PremiumUpdateSubscriptionError::RenewalConsentRequired);
            }
            // A retried request never reactivates a subsequently cancelled row.
            txn.commit().await?;
            return Ok(());
        }
        let now = self.time.now();
        let Some(paid) = paid.filter(|p| now < p.until) else {
            return Err(PremiumUpdateSubscriptionError::NoPremium);
        };
        let user = self
            .user_repo
            .get_composite(&mut txn, user_id)
            .await?
            .ok_or(PremiumUpdateSubscriptionError::NoPremium)?
            .user;
        let recipient = user
            .email
            .ok_or(PremiumUpdateSubscriptionError::RenewalConsentRequired)?
            .with_name(user.name.to_string())
            .0
            .to_string();
        let document = format!(
            "Hallo,\n\nhier ist die Vertragsbestätigung für deine monatliche Premium-Verlängerung. Bei der Einrichtung wurde kein Guthaben abgebucht.\n\n\
             Dein vereinbartes Angebot und deine Erklärungen:\n{}\n\n\
             Die vereinbarten AGB und die Widerrufsbelehrung findest du im Anhang.\n\n\
             Viele Grüße\nDein Bootstrap Academy Team\nbootstrap academy GmbH\n\n\
             Empfänger: {recipient}\nAbo-Nummer: {}\nBestellt am: {}\n\
             Bereits bezahlter Premium-Zeitraum und Frist für diese Bestätigung: {}.",
            offer.text,
            *consent.request_id,
            now.format("%d.%m.%Y um %H:%M Uhr (UTC)"),
            paid.until.format("%d.%m.%Y um %H:%M Uhr (UTC)"),
        );
        self.premium_repo
            .create_renewal(
                &mut txn,
                &PremiumRenewalAgreement {
                    id: consent.request_id,
                    user_id,
                    received_at: now,
                    paid_period_id: Some(paid.id),
                    confirmation_deadline: Some(paid.until),
                    offer_id: offer.id,
                    monthly_price: offer.monthly_price,
                    recipient,
                    document,
                    terms_pdf: AGB_2026_09_R2_PDF.into(),
                    withdrawal_pdf: WIDERRUFSBELEHRUNG_2026_09_R1_PDF.into(),
                },
            )
            .await?;
        txn.commit().await?;
        // Committed outbox survives any SMTP failure. The regular task retries;
        // billing remains disabled until a successful delivery is recorded.
        if let Err(err) = self.deliver_pending().await {
            tracing::warn!(error = %err, "Premium confirmation remains queued");
        }
        Ok(())
    }

    async fn deliver_pending(&self) -> anyhow::Result<()> {
        let mut txn = self.db.begin_transaction().await?;
        for agreement in self
            .premium_repo
            .pending_renewal_confirmations(&mut txn)
            .await?
        {
            let sent = self
                .email
                .send(Email {
                    sender: None,
                    message_id: None,
                    recipient: agreement.recipient.parse()?,
                    subject: "Deine monatliche Premium-Verlängerung – Bootstrap Academy".into(),
                    body: agreement.document,
                    content_type: ContentType::Text,
                    reply_to: None,
                    attachments: vec![
                        EmailAttachment {
                            filename: "vereinbarte-agb.pdf".into(),
                            content_type: AttachmentContentType::Pdf,
                            content: agreement.terms_pdf,
                        },
                        EmailAttachment {
                            filename: "vereinbarte-widerrufsbelehrung.pdf".into(),
                            content_type: AttachmentContentType::Pdf,
                            content: agreement.withdrawal_pdf,
                        },
                    ],
                })
                .await
                .unwrap_or(false);
            self.premium_repo
                .record_renewal_delivery(&mut txn, agreement.id, sent)
                .await?;
        }
        txn.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use academy_demo::{UUID1, user::FOO};
    use academy_email_contracts::MockEmailService;
    use academy_models::premium::Premium;
    use academy_persistence_contracts::{
        MockDatabase, MockTransaction, premium::MockPremiumRepository, user::MockUserRepository,
    };
    use academy_shared_contracts::time::MockTimeService;
    use chrono::{TimeZone, Utc};

    type Sut = PremiumRenewalServiceImpl<
        MockDatabase,
        MockTimeService,
        MockUserRepository<MockTransaction>,
        MockPremiumRepository<MockTransaction>,
        MockEmailService,
    >;

    fn paid() -> Premium {
        Premium {
            id: UUID1.into(),
            user_id: FOO.user.id,
            since: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            until: Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap(),
        }
    }

    fn consent(sut: &Sut) -> PremiumRenewalConsent {
        PremiumRenewalConsent {
            request_id: UUID1.into(),
            offer_id: sut.offer().id,
            accepted: true,
            withdrawal_consent: true,
        }
    }

    #[tokio::test]
    async fn stale_offer_or_missing_declaration_never_writes() {
        let sut = Sut::default();
        for c in [
            PremiumRenewalConsent {
                accepted: false,
                ..consent(&sut)
            },
            PremiumRenewalConsent {
                withdrawal_consent: false,
                ..consent(&sut)
            },
            PremiumRenewalConsent {
                offer_id: "stale-price-or-document".into(),
                ..consent(&sut)
            },
        ] {
            assert!(matches!(
                sut.enable(FOO.user.id, c).await,
                Err(PremiumUpdateSubscriptionError::RenewalConsentRequired)
            ));
        }
    }

    #[tokio::test]
    async fn explicit_consent_creates_exact_evidence_independent_of_profile_terms_response() {
        for response in ["accepted", "declined", "none"] {
            let mut user = FOO.clone();
            user.user.terms_version =
                (response == "accepted").then(|| "2026-09-r1".try_into().unwrap());
            user.user.terms_declined_at = (response == "declined").then_some(paid().since);
            let mut db = MockDatabase::new();
            db.expect_begin_transaction().times(2).returning(|| {
                let mut txn = MockTransaction::new();
                txn.expect_commit()
                    .once()
                    .return_once(|| Box::pin(async { Ok(()) }));
                Box::pin(async { Ok(txn) })
            });
            let mut repo =
                MockPremiumRepository::new().with_get_latest_by_user_id(FOO.user.id, Some(paid()));
            repo.expect_get_renewal_agreement()
                .once()
                .return_once(|_, _| Box::pin(async { Ok(None) }));
            repo.expect_create_renewal()
                .once()
                .withf(|_, a| {
                    a.user_id == FOO.user.id
                        && a.paid_period_id == Some(paid().id)
                        && a.confirmation_deadline == Some(paid().until)
                        && a.monthly_price == 1000
                        && a.document.contains(
                            "Ich stimme dieser monatlichen kostenpflichtigen Verlängerung",
                        )
                        && a.document.contains("01.01.2027")
                        && a.terms_pdf == AGB_2026_09_R2_PDF
                        && a.withdrawal_pdf == WIDERRUFSBELEHRUNG_2026_09_R1_PDF
                })
                .return_once(|_, _| Box::pin(async { Ok(()) }));
            repo.expect_pending_renewal_confirmations()
                .once()
                .return_once(|_| Box::pin(async { Ok(vec![]) }));
            let sut = Sut {
                db,
                time: MockTimeService::new().with_now(paid().since),
                user_repo: MockUserRepository::new().with_get_composite(FOO.user.id, Some(user)),
                premium_repo: repo,
                ..Sut::default()
            };
            sut.enable(FOO.user.id, consent(&sut)).await.unwrap();
        }
    }

    #[tokio::test]
    async fn repeated_declaration_does_not_reactivate_after_cancellation() {
        let mut repo = MockPremiumRepository::new().with_get_latest_by_user_id(FOO.user.id, None);
        let offer = Sut::default().offer();
        repo.expect_get_renewal_agreement()
            .once()
            .return_once(move |_, _| {
                Box::pin(async move {
                    Ok(Some(PremiumRenewalAgreement {
                        id: UUID1.into(),
                        user_id: FOO.user.id,
                        received_at: paid().since,
                        paid_period_id: Some(paid().id),
                        confirmation_deadline: Some(paid().until),
                        offer_id: offer.id,
                        monthly_price: 1000,
                        recipient: "old@example.invalid".into(),
                        document: "original".into(),
                        terms_pdf: vec![1],
                        withdrawal_pdf: vec![2],
                    }))
                })
            });
        let sut = Sut {
            db: MockDatabase::build(true),
            premium_repo: repo,
            ..Sut::default()
        };
        sut.enable(FOO.user.id, consent(&sut)).await.unwrap();
    }

    #[tokio::test]
    async fn free_offer_cannot_create_automatic_payment_authority() {
        let mut sut = Sut::default();
        sut.config.monthly_price = 0;
        assert!(matches!(
            sut.enable(FOO.user.id, consent(&sut)).await,
            Err(PremiumUpdateSubscriptionError::RenewalConsentRequired)
        ));
        // All persistence mocks have no allowed calls: no agreement, debit or
        // confirmation work is created even with otherwise exact consent.
    }

    #[tokio::test]
    async fn expired_paid_access_cannot_create_new_renewal() {
        let mut repo =
            MockPremiumRepository::new().with_get_latest_by_user_id(FOO.user.id, Some(paid()));
        repo.expect_get_renewal_agreement()
            .once()
            .return_once(|_, _| Box::pin(async { Ok(None) }));
        let sut = Sut {
            db: MockDatabase::build(false),
            time: MockTimeService::new().with_now(paid().until),
            premium_repo: repo,
            ..Sut::default()
        };
        assert!(matches!(
            sut.enable(FOO.user.id, consent(&sut)).await,
            Err(PremiumUpdateSubscriptionError::NoPremium)
        ));
    }

    #[tokio::test]
    async fn confirmation_failure_and_retry_use_saved_content_and_record_attempts() {
        for sent in [false, true] {
            let mut repo = MockPremiumRepository::new();
            repo.expect_pending_renewal_confirmations()
                .once()
                .return_once(|_| {
                    Box::pin(async {
                        Ok(vec![PremiumRenewalAgreement {
                            id: UUID1.into(),
                            user_id: FOO.user.id,
                            received_at: paid().since,
                            paid_period_id: Some(paid().id),
                            confirmation_deadline: Some(paid().until),
                            offer_id: "old-offer".into(),
                            monthly_price: 750,
                            recipient: "old@example.invalid".into(),
                            document: "Saved 750-coin contract, not today's template".into(),
                            terms_pdf: academy_assets::email::AGB_2026_09_R1_PDF.to_vec(),
                            withdrawal_pdf: WIDERRUFSBELEHRUNG_2026_09_R1_PDF.to_vec(),
                        }])
                    })
                });
            repo.expect_record_renewal_delivery()
                .once()
                .withf(move |_, id, result| *id == UUID1.into() && *result == sent)
                .return_once(|_, _, _| Box::pin(async { Ok(()) }));
            let mut email = MockEmailService::new();
            email
                .expect_send()
                .once()
                .withf(|e| {
                    e.recipient.0.to_string() == "old@example.invalid"
                        && e.body.starts_with("Saved 750-coin")
                        && e.attachments[0].content == academy_assets::email::AGB_2026_09_R1_PDF
                        && e.attachments[1].content == WIDERRUFSBELEHRUNG_2026_09_R1_PDF
                })
                .return_once(move |_| Box::pin(async move { Ok(sent) }));
            let sut = Sut {
                db: MockDatabase::build(true),
                premium_repo: repo,
                email,
                ..Sut::default()
            };
            sut.deliver_pending().await.unwrap();
        }
    }

    #[test]
    fn changed_price_changes_offer_fingerprint() {
        let original = Sut::default().offer();
        let changed = Sut {
            config: PremiumFeatureConfig {
                monthly_price: 1500,
                ..Default::default()
            },
            ..Sut::default()
        }
        .offer();
        assert_ne!(original.id, changed.id);
        assert!(changed.text.contains("1500 MorphCoins (15.00 EUR"));
    }
    #[tokio::test]
    async fn closing_old_r1_activation_is_not_adopted_as_r2_or_replayed_through_new_offer() {
        let sut = Sut::default();
        let current = sut.offer();
        assert_eq!(current.terms_version, "2026-09-r2");
        let old_text = current.text.replace("2026-09-r2", "2026-09-r1");
        let mut hash = Sha256::new();
        for bytes in [
            old_text.as_bytes(),
            academy_assets::email::AGB_2026_09_R1_PDF,
            WIDERRUFSBELEHRUNG_2026_09_R1_PDF,
        ] {
            hash.update(bytes);
        }
        let old_id = format!("{:x}", hash.finalize());
        assert_ne!(current.id, old_id);
        let mut old_consent = consent(&sut);
        old_consent.offer_id = old_id;
        assert!(matches!(
            sut.enable(FOO.user.id, old_consent).await,
            Err(PremiumUpdateSubscriptionError::RenewalConsentRequired)
        ));
        // All mocks have zero allowed calls: this rejection neither loads nor
        // changes a historical agreement and cannot reactivate its renewal.
    }
}
