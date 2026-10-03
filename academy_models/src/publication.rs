//! Additive publication DTOs. Full account objects never cross this boundary.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::user::{UserDisplayName, UserId};

pub const SCOPE_VERSION: &str = "academy-verified-v1";
pub const NOTICE: &str = "Andere angemeldete Nutzer mit bestätigter E-Mail-Adresse sehen deinen Anzeigenamen, den Standard-Avatar, deine Gesamt-XP sowie Plätze und Punkte der Gesamt-, Aufgaben- und Sprachbestenlisten. Neue XP und Punkte werden mit angezeigt. Einzelne Skills, Bio, Tags, Lösungen und Projektstände bleiben privat. Du kannst jederzeit wieder privat stellen. Frühere Kopien können wir nicht zurückholen.\nOther signed-in users with a verified email address can see your display name, standard avatar, total XP, and ranks and scores in the overall, task and language leaderboards. Newly earned XP and scores update this view. Individual skills, bio, tags, solutions and project state stay private. You can make your profile private again at any time. We cannot retrieve earlier copies.";
// SHA-256 of the exact UTF-8 NOTICE above. Scope changes require a new version/hash.
pub const NOTICE_HASH: &str = "07434654b73f77ea6d552365142d459125f6d13e734c6a84bfa141cc9090a0a5";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProfileVisibility {
    #[default]
    Private,
    Shared,
}

impl ProfileVisibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Shared => "shared",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PublicationReceipt {
    pub request_id: Uuid,
    pub expected_revision: i64,
    pub visibility_revision: i64,
    pub profile_visibility: ProfileVisibility,
    pub recorded_at: i64,
    pub scope_version: Option<String>,
    pub notice_hash: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PublicationSettings {
    pub profile_visibility: ProfileVisibility,
    pub visibility_revision: i64,
    pub shared_scope_version: Option<String>,
    pub shared_notice_hash: Option<String>,
    pub shared_at: Option<i64>,
    pub withdrawn_at: Option<i64>,
    pub last_shared_receipt: Option<PublicationReceipt>,
    pub last_private_receipt: Option<PublicationReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PublicationEpoch {
    pub scope_version: String,
    pub publication_epoch: Uuid,
    pub epoch_revision: i64,
    pub policy_active: bool,
    pub publishing_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PublishedIdentity {
    pub user_id: UserId,
    pub display_name: UserDisplayName,
    /// Always null. Render the existing first-party standard avatar.
    pub avatar_url: Option<String>,
    pub visibility_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PublicationSnapshot {
    #[serde(flatten)]
    pub epoch: PublicationEpoch,
    pub participants: Vec<PublishedIdentity>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicationChoice {
    pub profile_visibility: ProfileVisibility,
    pub expected_revision: i64,
    pub request_id: Uuid,
    pub scope_version: Option<String>,
    pub notice_hash: Option<String>,
    /// Owner/revision-bound signed preview; never retained in a receipt.
    pub preview_token: Option<String>,
}

/// Support can withdraw an existing choice, never give consent for its owner.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicationWithdrawal {
    pub expected_revision: i64,
    pub request_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PublicationChoiceResult {
    pub current: PublicationSettings,
    pub receipt: PublicationReceipt,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PublicationPreview {
    pub profile: PublishedIdentity,
    pub publication: PublicationSettings,
    pub scope_version: String,
    pub notice_hash: String,
    pub notice: String,
    pub preview_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationPreviewClaims {
    pub purpose: String,
    pub user_id: UserId,
    pub visibility_revision: i64,
    pub scope_version: String,
    pub notice_hash: String,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PublicationConfig {
    pub enabled: bool,
}
