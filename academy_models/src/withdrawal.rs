//! Declarations a consumer gives before placing an order so that the right of
//! withdrawal expires early (§ 356 Abs. 5 Nr. 2, Abs. 6 Nr. 2 BGB).
//!
//! The wording of the declarations is fixed by the withdrawal instruction
//! published at <https://bootstrap.academy/docs/right-of-withdrawal> and must
//! be repeated verbatim in the confirmation of the contract (§ 312f Abs. 3
//! BGB).

use chrono::{DateTime, Utc};

use crate::{
    macros::{id, nutype_string},
    user::UserId,
};

id!(WithdrawalConsentId);

nutype_string!(WithdrawalTextVersion(validate(
    len_char_min = 1,
    len_char_max = 32
)));

nutype_string!(WithdrawalReference(validate(
    len_char_min = 1,
    len_char_max = 256
)));

/// Declaration for services, taken verbatim from part A of the withdrawal
/// instruction.
pub const WITHDRAWAL_CONSENT_SERVICE: &str = "Ich verlange ausdrücklich und stimme zu, dass Sie \
                                              vor Ablauf der Widerrufsfrist mit der Erbringung \
                                              der Dienstleistung beginnen. Mir ist bekannt, dass \
                                              mein Widerrufsrecht mit vollständiger Erbringung \
                                              der Dienstleistung erlischt.";

/// Declaration for digital content, taken verbatim from part B of the
/// withdrawal instruction.
pub const WITHDRAWAL_CONSENT_DIGITAL_CONTENT: &str = "Ich stimme ausdrücklich zu, dass Sie vor Ablauf der Widerrufsfrist mit der Ausführung des \
     Vertrags beginnen. Mir ist bekannt, dass mein Widerrufsrecht mit Beginn der Ausführung des \
     Vertrags erlischt.";

/// Purchase the declarations are given for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WithdrawalSubject {
    /// Morphcoin purchase (digital content, part B).
    Coins,
    /// Premium membership (service, part A).
    Premium,
    /// Heart refill (digital content, part B).
    Hearts,
    /// Unlocking a paid course (digital content, part B).
    Course,
    /// Booking a webinar (service, part A).
    Webinar,
    /// Booking a coaching (service, part A).
    Coaching,
}

impl WithdrawalSubject {
    /// Identifier of this subject in the API and in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Coins => "coins",
            Self::Premium => "premium",
            Self::Hearts => "hearts",
            Self::Course => "course",
            Self::Webinar => "webinar",
            Self::Coaching => "coaching",
        }
    }

    /// The declaration the consumer gives for this subject, verbatim.
    pub fn consent_text(self) -> &'static str {
        match self {
            Self::Coins | Self::Hearts | Self::Course => WITHDRAWAL_CONSENT_DIGITAL_CONTENT,
            Self::Premium | Self::Webinar | Self::Coaching => WITHDRAWAL_CONSENT_SERVICE,
        }
    }
}

impl std::str::FromStr for WithdrawalSubject {
    type Err = InvalidWithdrawalSubjectError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "coins" => Ok(Self::Coins),
            "premium" => Ok(Self::Premium),
            "hearts" => Ok(Self::Hearts),
            "course" => Ok(Self::Course),
            "webinar" => Ok(Self::Webinar),
            "coaching" => Ok(Self::Coaching),
            _ => Err(InvalidWithdrawalSubjectError),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("The withdrawal consent subject is invalid.")]
pub struct InvalidWithdrawalSubjectError;

/// The declarations as they arrive from the client.
///
/// An order is only accepted if the consumer actively gave the declarations
/// and the client stated which version of the withdrawal instruction they were
/// taken from.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WithdrawalConsentDeclaration {
    /// Whether the consumer gave the declarations.
    pub given: bool,
    /// Version of the withdrawal instruction the declarations were taken from.
    pub text_version: Option<WithdrawalTextVersion>,
}

impl WithdrawalConsentDeclaration {
    /// Return the accepted text version, or `None` if the declarations have
    /// not been given.
    pub fn text_version(&self) -> Option<&WithdrawalTextVersion> {
        self.given.then_some(self.text_version.as_ref()).flatten()
    }
}

/// A recorded consent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WithdrawalConsent {
    pub id: WithdrawalConsentId,
    pub user_id: UserId,
    pub subject: WithdrawalSubject,
    /// Identifier of the purchased item, if the purchase has one.
    pub reference: Option<WithdrawalReference>,
    pub text_version: WithdrawalTextVersion,
    pub consented_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_round_trip() {
        for subject in [
            WithdrawalSubject::Coins,
            WithdrawalSubject::Premium,
            WithdrawalSubject::Hearts,
            WithdrawalSubject::Course,
            WithdrawalSubject::Webinar,
            WithdrawalSubject::Coaching,
        ] {
            assert_eq!(
                subject.as_str().parse::<WithdrawalSubject>().unwrap(),
                subject
            );
        }
    }

    #[test]
    fn declaration_requires_both_the_tick_and_the_version() {
        let version = WithdrawalTextVersion::try_new("2026-09").unwrap();

        assert_eq!(
            WithdrawalConsentDeclaration {
                given: true,
                text_version: Some(version.clone()),
            }
            .text_version(),
            Some(&version)
        );
        assert_eq!(
            WithdrawalConsentDeclaration {
                given: false,
                text_version: Some(version),
            }
            .text_version(),
            None
        );
        assert_eq!(
            WithdrawalConsentDeclaration {
                given: true,
                text_version: None,
            }
            .text_version(),
            None
        );
    }
}
