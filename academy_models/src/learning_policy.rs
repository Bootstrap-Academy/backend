//! Explicit, terms-bound rollout. Measurement never changes a learner's contract.
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::user::{TermsVersion, User, UserId};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LearningMode {
    #[default]
    Legacy,
    Shadow,
    Daily,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LearningPolicyConfig {
    pub mode: LearningMode,
    /// A deliberately chosen version; never changes the registration terms.
    pub terms_version: Option<TermsVersion>,
    /// Acceptance must have occurred on or after the approved transition date.
    pub accepted_since: Option<DateTime<Utc>>,
    /// Explicit pilot cohort. An empty cohort never activates existing users.
    pub user_ids: Vec<UserId>,
    /// Optional, explicitly selected new-registration cohort.
    pub registered_since: Option<DateTime<Utc>>,
    pub daily_documents: Option<LearningDocumentBundleConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearningDocumentBundleConfig {
    pub terms_version: TermsVersion,
    pub terms_pdf_path: std::path::PathBuf,
    pub terms_sha256: String,
    pub withdrawal_pdf_path: std::path::PathBuf,
    pub withdrawal_sha256: String,
}

impl LearningPolicyConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.mode == LearningMode::Daily {
            anyhow::ensure!(
                self.terms_version.is_some() && self.accepted_since.is_some(),
                "daily learning requires a terms version and acceptance start"
            );
            anyhow::ensure!(
                !self.user_ids.is_empty() || self.registered_since.is_some(),
                "daily learning requires an explicitly selected cohort"
            );
        }
        Ok(())
    }

    pub fn mode_for(&self, user: &User) -> LearningMode {
        match self.mode {
            LearningMode::Legacy => LearningMode::Legacy,
            LearningMode::Shadow => LearningMode::Shadow,
            LearningMode::Daily => {
                let in_cohort = self.user_ids.contains(&user.id)
                    || self
                        .registered_since
                        .is_some_and(|since| user.created_at >= since);
                let accepted = self
                    .terms_version
                    .as_ref()
                    .is_some_and(|version| user.terms_version.as_ref() == Some(version))
                    && self
                        .accepted_since
                        .zip(user.terms_accepted_at)
                        .is_some_and(|(since, at)| at >= since && at <= Utc::now());
                if in_cohort && accepted {
                    LearningMode::Daily
                } else {
                    LearningMode::Legacy
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LearningPolicy {
    pub mode: LearningMode,
    pub premium: bool,
    pub single_course_sales: bool,
    pub heart_sales: bool,
}

impl LearningPolicy {
    pub fn new(mode: LearningMode, premium: bool) -> Self {
        Self {
            mode,
            premium,
            single_course_sales: mode != LearningMode::Daily,
            heart_sales: mode != LearningMode::Daily,
        }
    }
}
