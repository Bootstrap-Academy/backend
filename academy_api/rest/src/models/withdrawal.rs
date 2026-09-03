use academy_models::withdrawal::{
    WithdrawalConsent, WithdrawalConsentDeclaration, WithdrawalSubject, WithdrawalTextVersion,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Purchase the declarations under § 356 Abs. 5/6 BGB are given for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApiWithdrawalSubject {
    Coins,
    Premium,
    Hearts,
    Course,
    Webinar,
    Coaching,
}

// No doc comment: this struct is flattened into request bodies and a doc
// comment here would become the description of the whole request.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct ApiWithdrawalConsentDeclaration {
    /// Whether the consumer gave the declarations under
    /// § 356 Abs. 5 Nr. 2 / Abs. 6 Nr. 2 BGB shown next to the order button.
    /// An order without them is rejected.
    #[serde(default)]
    pub withdrawal_consent: bool,
    /// Version of the withdrawal instruction the declarations were taken from.
    #[serde(default)]
    pub withdrawal_text_version: Option<WithdrawalTextVersion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ApiWithdrawalConsent {
    pub id: uuid::Uuid,
    pub subject: ApiWithdrawalSubject,
    pub reference: Option<String>,
    pub text_version: String,
    pub consented_at: i64,
}

impl From<ApiWithdrawalSubject> for WithdrawalSubject {
    fn from(value: ApiWithdrawalSubject) -> Self {
        match value {
            ApiWithdrawalSubject::Coins => Self::Coins,
            ApiWithdrawalSubject::Premium => Self::Premium,
            ApiWithdrawalSubject::Hearts => Self::Hearts,
            ApiWithdrawalSubject::Course => Self::Course,
            ApiWithdrawalSubject::Webinar => Self::Webinar,
            ApiWithdrawalSubject::Coaching => Self::Coaching,
        }
    }
}

impl From<WithdrawalSubject> for ApiWithdrawalSubject {
    fn from(value: WithdrawalSubject) -> Self {
        match value {
            WithdrawalSubject::Coins => Self::Coins,
            WithdrawalSubject::Premium => Self::Premium,
            WithdrawalSubject::Hearts => Self::Hearts,
            WithdrawalSubject::Course => Self::Course,
            WithdrawalSubject::Webinar => Self::Webinar,
            WithdrawalSubject::Coaching => Self::Coaching,
        }
    }
}

impl From<ApiWithdrawalConsentDeclaration> for WithdrawalConsentDeclaration {
    fn from(value: ApiWithdrawalConsentDeclaration) -> Self {
        Self {
            given: value.withdrawal_consent,
            text_version: value.withdrawal_text_version,
        }
    }
}

impl From<WithdrawalConsent> for ApiWithdrawalConsent {
    fn from(value: WithdrawalConsent) -> Self {
        Self {
            id: *value.id,
            subject: value.subject.into(),
            reference: value.reference.map(|reference| reference.into_inner()),
            text_version: value.text_version.into_inner(),
            consented_at: value.consented_at.timestamp(),
        }
    }
}
