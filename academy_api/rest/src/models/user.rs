use academy_models::{
    user::{
        UserBio, UserComposite, UserDisplayName, UserFilter, UserId, UserIdOrSelf, UserName,
        UserPassword, UserTags,
    },
    SearchTerm,
};
use email_address::EmailAddress;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiUser {
    pub id: UserId,
    pub name: UserName,
    pub display_name: UserDisplayName,
    pub email: Option<EmailAddress>,
    pub email_verified: bool,
    pub created_at: i64,
    pub last_login: Option<i64>,
    pub last_name_change: Option<i64>,
    pub enabled: bool,
    pub admin: bool,
    pub password: bool,
    pub mfa_enabled: bool,
    pub description: UserBio,
    pub tags: UserTags,
    pub newsletter: bool,
}

impl From<UserComposite> for ApiUser {
    fn from(
        UserComposite {
            user,
            profile,
            details,
        }: UserComposite,
    ) -> Self {
        Self {
            id: user.id,
            name: user.name,
            email: user.email,
            email_verified: user.email_verified,
            created_at: user.created_at.timestamp(),
            last_login: user.last_login.map(|x| x.timestamp()),
            last_name_change: user.last_name_change.map(|x| x.timestamp()),
            enabled: user.enabled,
            admin: user.admin,
            newsletter: user.newsletter,

            display_name: profile.display_name,
            description: profile.bio,
            tags: profile.tags,

            mfa_enabled: details.mfa_enabled,
            password: details.password_login,
        }
    }
}

#[derive(Deserialize)]
pub struct ApiUserFilter {
    pub name: Option<SearchTerm>,
    pub email: Option<SearchTerm>,
    pub enabled: Option<bool>,
    pub admin: Option<bool>,
    pub mfa_enabled: Option<bool>,
    pub email_verified: Option<bool>,
    pub newsletter: Option<bool>,
}

impl From<ApiUserFilter> for UserFilter {
    fn from(value: ApiUserFilter) -> Self {
        Self {
            name: value.name,
            email: value.email,
            enabled: value.enabled,
            admin: value.admin,
            mfa_enabled: value.mfa_enabled,
            email_verified: value.email_verified,
            newsletter: value.newsletter,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiUserIdOrSelf {
    UserId(UserId),
    Slf,
}

impl<'de> Deserialize<'de> for ApiUserIdOrSelf {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        pub enum UserIdOrString {
            UserId(UserId),
            String(String),
        }

        let result = UserIdOrString::deserialize(deserializer)?;
        match result {
            UserIdOrString::UserId(user_id) => Ok(ApiUserIdOrSelf::UserId(user_id)),
            UserIdOrString::String(s) if matches!(s.to_lowercase().as_str(), "me" | "self") => {
                Ok(ApiUserIdOrSelf::Slf)
            }
            _ => Err(serde::de::Error::custom("Invalid user id")),
        }
    }
}

impl From<ApiUserIdOrSelf> for UserIdOrSelf {
    fn from(value: ApiUserIdOrSelf) -> Self {
        match value {
            ApiUserIdOrSelf::UserId(user_id) => Self::UserId(user_id),
            ApiUserIdOrSelf::Slf => Self::Slf,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub enum ApiUserPasswordOrEmpty {
    #[serde(rename = "")]
    Empty,
    #[serde(untagged)]
    Password(UserPassword),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_api_user_id_or_self() {
        enum Kind {
            UserId,
            Slf,
            Invalid,
        }

        for (input, kind) in [
            ("3b1c09f9-4971-4376-89e0-ccc478fcd213", Kind::UserId),
            ("self", Kind::Slf),
            ("me", Kind::Slf),
            ("SELF", Kind::Slf),
            ("ME", Kind::Slf),
            ("asdf", Kind::Invalid),
            ("", Kind::Invalid),
        ] {
            let result =
                serde_json::from_value::<ApiUserIdOrSelf>(serde_json::Value::String(input.into()));
            match kind {
                Kind::UserId => assert_eq!(
                    result.unwrap(),
                    ApiUserIdOrSelf::UserId(UserId::new(input.parse().unwrap()))
                ),
                Kind::Slf => assert_eq!(result.unwrap(), ApiUserIdOrSelf::Slf),
                Kind::Invalid => assert!(result.is_err()),
            }
        }
    }

    #[test]
    fn deserialize_api_user_password_or_empty() {
        let result =
            serde_json::from_value::<ApiUserPasswordOrEmpty>(serde_json::Value::String("".into()));
        assert_eq!(result.unwrap(), ApiUserPasswordOrEmpty::Empty);

        let result =
            serde_json::from_value::<ApiUserPasswordOrEmpty>(serde_json::Value::String("a".into()));
        assert_eq!(
            result.unwrap(),
            ApiUserPasswordOrEmpty::Password("a".try_into().unwrap())
        );

        let input = "a".repeat(UserPassword::MAX_LENGTH + 1);
        let result =
            serde_json::from_value::<ApiUserPasswordOrEmpty>(serde_json::Value::String(input));
        assert!(result.is_err());
    }
}
