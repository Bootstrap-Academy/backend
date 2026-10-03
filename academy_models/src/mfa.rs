use std::sync::LazyLock;

use academy_utils::patch::Patch;
use chrono::{DateTime, Utc};
use nutype::nutype;
use regex::Regex;

use crate::{
    hyphenated_code_regex,
    macros::{id, nutype_string, sensitive_debug, sha256hash},
    user::UserId,
};

id!(TotpDeviceId);

#[derive(Debug, Clone, PartialEq, Eq, Patch)]
pub struct TotpDevice {
    #[no_patch]
    pub id: TotpDeviceId,
    #[no_patch]
    pub user_id: UserId,
    pub enabled: bool,
    #[no_patch]
    pub created_at: DateTime<Utc>,
}

#[nutype(
    validate(regex = TOTP_CODE_REGEX),
    derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, AsRef, Deref, TryFrom, Serialize, Deserialize, JsonSchema)
)]
pub struct TotpCode(String);

// Second-factor credentials remain redacted in both debug and release builds.
impl std::fmt::Debug for TotpCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TotpCode([REDACTED])")
    }
}

impl AsRef<[u8]> for TotpCode {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}
pub static TOTP_CODE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new("^[0-9]{6}$").unwrap());

#[nutype(validate(predicate = |x| x.len() >= 16), derive(Clone, PartialEq, Eq, Deref, TryFrom))]
pub struct TotpSecret(Vec<u8>);
sensitive_debug!(TotpSecret);

#[nutype(
    validate(greater_or_equal = 16),
    derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        Deref,
        TryFrom,
        Serialize,
        Deserialize
    )
)]
pub struct TotpSecretLength(usize);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TotpSetup {
    /// The base32 encoded totp secret.
    pub secret: TotpSecretBase32,
}

nutype_string!(TotpSecretBase32(sensitive));

nutype_string!(MfaRecoveryCode(
    sensitive,
    sanitize(uppercase),
    validate(regex = MFA_RECOVERY_CODE_REGEX),
));

pub static MFA_RECOVERY_CODE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    hyphenated_code_regex(MfaRecoveryCode::CHUNK_COUNT, MfaRecoveryCode::CHUNK_SIZE)
});

impl MfaRecoveryCode {
    pub const CHUNK_COUNT: usize = 4;
    pub const CHUNK_SIZE: usize = 6;
}

sha256hash!(MfaRecoveryCodeHash);

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MfaAuthentication {
    pub totp_code: Option<TotpCode>,
    pub recovery_code: Option<MfaRecoveryCode>,
}
