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
}

nutype_string!(DeviceName(validate(len_char_max = DeviceName::MAX_LEN)));

impl DeviceName {
    const MAX_LEN: usize = 256;

    pub fn from_string_truncated(mut s: String) -> Self {
        s.truncate(Self::MAX_LEN);
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
}
