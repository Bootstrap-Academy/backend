use academy_utils::patch::Patch;
use chrono::{DateTime, Utc};

use crate::{
    macros::{id, nutype_string, sha256hash},
    user::UserId,
};

id!(SessionId);

#[derive(Debug, Clone, PartialEq, Eq, Patch)]
pub struct Session {
    #[no_patch]
    pub id: SessionId,
    #[no_patch]
    pub user_id: UserId,
    pub device_name: Option<DeviceName>,
    #[no_patch]
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Whether the second factor was verified when this session was created.
    ///
    /// Sessions that were not established with a second factor do not grant
    /// administrative privileges, even if the account is an administrator.
    #[no_patch]
    pub mfa_verified: bool,
    /// How the session came into existence. Recorded once and kept across
    /// refreshes.
    #[no_patch]
    pub origin: SessionOrigin,
}

impl Session {
    /// Whether this session belongs to the account owner's own sign-in.
    ///
    /// Decisions only the owner may make, such as sharing the profile, require
    /// such a session. Sessions from before origins were recorded count as the
    /// owner's own sign-in only if they carry a device name: every way of
    /// signing in to someone else's account (the admin API and the CLI) has
    /// always created sessions without one, and a session's device name is
    /// never changed later.
    pub fn is_owner_sign_in(&self) -> bool {
        match self.origin {
            SessionOrigin::SignIn => true,
            SessionOrigin::Impersonation { .. } => false,
            SessionOrigin::Legacy => self.device_name.is_some(),
        }
    }
}

/// How a session came into existence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionOrigin {
    /// The account owner signed in (password, OAuth or registration).
    SignIn,
    /// Someone else signed in to the account: an administrator through the API
    /// (`admin` is set), or an operator with server access through the CLI.
    Impersonation { admin: Option<UserId> },
    /// Created before origins were recorded, or by a version of the backend
    /// that does not record them.
    Legacy,
}

impl SessionOrigin {
    /// The administrator who signed in to the account, if known.
    pub fn impersonated_by(self) -> Option<UserId> {
        match self {
            Self::Impersonation { admin } => admin,
            Self::SignIn | Self::Legacy => None,
        }
    }
}

nutype_string!(DeviceName(validate(len_char_max = DeviceName::MAX_LEN)));

impl DeviceName {
    const MAX_LEN: usize = 256;

    pub fn from_string_truncated(mut s: String) -> Self {
        if let Some((end, _)) = s.char_indices().nth(Self::MAX_LEN) {
            s.truncate(end);
        }
        Self::try_new(s).unwrap()
    }
}

sha256hash!(SessionRefreshTokenHash);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_name_from_string_truncated() {
        // Arrange
        let input = std::iter::once('A')
            .chain(std::iter::repeat('B'))
            .take(DeviceName::MAX_LEN + 20)
            .collect();
        let expected = std::iter::once('A')
            .chain(std::iter::repeat('B'))
            .take(DeviceName::MAX_LEN)
            .collect::<String>();

        // Act
        let result = DeviceName::from_string_truncated(input);

        // Assert
        assert_eq!(result.into_inner(), expected);
    }

    #[test]
    fn only_the_owners_sign_in_counts_as_owner() {
        let session = |origin, device_name: Option<&str>| Session {
            id: uuid::Uuid::nil().into(),
            user_id: uuid::Uuid::nil().into(),
            device_name: device_name.map(|name| DeviceName::try_new(name.to_owned()).unwrap()),
            created_at: DateTime::UNIX_EPOCH,
            updated_at: DateTime::UNIX_EPOCH,
            mfa_verified: false,
            origin,
        };
        let admin = Some(uuid::Uuid::max().into());
        for (origin, device_name, expected) in [
            (SessionOrigin::SignIn, Some("Firefox"), true),
            (SessionOrigin::SignIn, None, true),
            (SessionOrigin::Impersonation { admin }, None, false),
            (
                SessionOrigin::Impersonation { admin },
                Some("Firefox"),
                false,
            ),
            (SessionOrigin::Impersonation { admin: None }, None, false),
            (SessionOrigin::Legacy, Some("Firefox"), true),
            (SessionOrigin::Legacy, None, false),
        ] {
            assert_eq!(
                session(origin, device_name).is_owner_sign_in(),
                expected,
                "{origin:?} {device_name:?}"
            );
        }
        assert_eq!(
            SessionOrigin::Impersonation { admin }.impersonated_by(),
            admin
        );
        assert_eq!(SessionOrigin::Legacy.impersonated_by(), None);
    }

    #[test]
    fn device_name_truncation_preserves_unicode_characters() {
        for input in [
            format!("{}é", "A".repeat(255)),
            "ä".repeat(DeviceName::MAX_LEN + 20),
            "🙂".repeat(DeviceName::MAX_LEN + 20),
        ] {
            let expected = input.chars().take(DeviceName::MAX_LEN).collect::<String>();
            assert_eq!(
                DeviceName::from_string_truncated(input).into_inner(),
                expected
            );
        }
    }
}
