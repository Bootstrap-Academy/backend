use chrono::{DateTime, Utc};

use crate::{
    macros::{id, nutype_string},
    user::UserId,
};

id!(AdminAuditLogEntryId);

/// Number of months administrative audit log entries are kept before
/// `academy task prune-database` removes them.
pub const ADMIN_AUDIT_LOG_RETENTION_MONTHS: u32 = 12;

/// A single state changing request that was authenticated with an
/// administrator's access token.
///
/// Only request metadata is recorded, never request bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminAuditLogEntry {
    pub id: AdminAuditLogEntryId,
    /// Time at which the request was answered
    pub at: DateTime<Utc>,
    /// The administrator whose access token authenticated the request
    pub admin_user_id: UserId,
    /// HTTP method of the request
    pub method: RequestMethod,
    /// Path of the request, without the query string
    pub path: RequestPath,
    /// The user the request acted on, as far as the route identifies one
    pub target_user_id: Option<UserId>,
    /// HTTP status code of the response
    pub status: u16,
    /// Value of the `X-Request-Id` response header
    pub request_id: RequestId,
}

/// Filter for the administrative audit log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AdminAuditLogFilter {
    /// Only return entries recorded for this administrator.
    pub admin_user_id: Option<UserId>,
    /// Only return entries whose request acted on this user.
    pub target_user_id: Option<UserId>,
}

nutype_string!(RequestMethod(validate(
    len_char_max = RequestMethod::MAX_LEN
)));
nutype_string!(RequestPath(validate(len_char_max = RequestPath::MAX_LEN)));
nutype_string!(RequestId(validate(len_char_max = RequestId::MAX_LEN)));

macro_rules! truncated {
    ($ident:ident($max_len:expr)) => {
        impl $ident {
            pub const MAX_LEN: usize = $max_len;

            /// Create the value, dropping everything beyond
            /// [`Self::MAX_LEN`] characters.
            pub fn from_string_truncated(s: String) -> Self {
                match s.char_indices().nth(Self::MAX_LEN) {
                    Some((idx, _)) => Self::try_new(s[..idx].to_owned()).unwrap(),
                    None => Self::try_new(s).unwrap(),
                }
            }
        }
    };
}

truncated!(RequestMethod(16));
truncated!(RequestPath(256));
truncated!(RequestId(64));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_string_truncated() {
        let path = RequestPath::from_string_truncated("/auth/users/me".into());
        assert_eq!(path.into_inner(), "/auth/users/me");

        let long = "/".to_owned() + &"a".repeat(RequestPath::MAX_LEN);
        let path = RequestPath::from_string_truncated(long);
        assert_eq!(path.chars().count(), RequestPath::MAX_LEN);
    }

    /// Truncation must not split a multi byte character.
    #[test]
    fn from_string_truncated_multibyte() {
        let input = "ä".repeat(RequestPath::MAX_LEN + 10);
        let path = RequestPath::from_string_truncated(input);
        assert_eq!(path.chars().count(), RequestPath::MAX_LEN);
    }
}
