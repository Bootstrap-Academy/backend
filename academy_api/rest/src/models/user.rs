use std::borrow::Cow;

use academy_models::{
    SearchTerm,
    email_address::EmailAddress,
    url::Url,
    user::{
        TermsVersion, UserBio, UserCity, UserComposite, UserCountry, UserDisplayName, UserFilter,
        UserFirstName, UserId, UserIdOrSelf, UserLastName, UserName, UserPassword, UserStreet,
        UserTags, UserVatId, UserZipCode,
    },
};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use crate::const_schema;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ApiUser {
    /// User ID
    pub id: UserId,
    /// Unique user name (used for login, case insensitive)
    pub name: UserName,
    /// Display name (not necessarily unique)
    pub display_name: UserDisplayName,
    /// Email address (always set for new accounts, may be null for old
    /// accounts migrated from the coding challenges platform)
    pub email: Option<EmailAddress>,
    /// Whether the email address has been verified
    pub email_verified: bool,
    /// Timestamp of creation
    pub registration: i64,
    /// Timestamp of last successful login
    pub last_login: Option<i64>,
    /// Timestamp of last `name` change
    pub last_name_change: Option<i64>,
    /// Whether the user account is enabled (disabled users cannot login)
    pub enabled: bool,
    /// Whether the user is an administrator
    pub admin: bool,
    /// Whether the user has set a password (if not, login is only possible via
    /// OAuth2)
    pub password: bool,
    /// Whether the user has enabled MFA
    pub mfa_enabled: bool,
    /// Bio of the user profile
    pub description: UserBio,
    /// Tags of the user profile
    pub tags: UserTags,
    /// Version of the terms and conditions the user accepted (read-only, null
    /// for accounts created before acceptance was recorded)
    pub terms_version: Option<TermsVersion>,
    /// Timestamp at which the user accepted the terms and conditions
    /// (read-only)
    pub terms_accepted_at: Option<i64>,
    /// Timestamp at which the user most recently declined to accept the
    /// version of the terms and conditions the platform currently asks for
    /// (read-only, null once the user accepts)
    pub terms_declined_at: Option<i64>,
    /// Whether the user represents a business instead of a private person
    pub business: Option<bool>,
    /// First name of the user
    pub first_name: Option<UserFirstName>,
    /// Last name of the user
    pub last_name: Option<UserLastName>,
    /// Street of the user's address
    pub street: Option<UserStreet>,
    /// Zip code of the user's address
    pub zip_code: Option<UserZipCode>,
    /// City of the user's address
    pub city: Option<UserCity>,
    /// Country of the user's address
    pub country: Option<UserCountry>,
    /// Vat ID of the user
    pub vat_id: Option<UserVatId>,
    /// Whether the user can buy coins
    pub can_buy_coins: bool,
    /// Whether the user can receive coins
    pub can_receive_coins: bool,
    /// URL of the user's avatar (always null; the platform renders a
    /// first-party default avatar instead of using an external service)
    pub avatar_url: Option<Url>,
}

impl From<UserComposite> for ApiUser {
    fn from(user_composite: UserComposite) -> Self {
        let can_buy_coins = user_composite.can_buy_coins();
        let can_receive_coins = user_composite.can_receive_coins();

        let UserComposite {
            user,
            profile,
            details,
            invoice_info,
        } = user_composite;

        Self {
            id: user.id,
            name: user.name,
            email: user.email,
            email_verified: user.email_verified,
            registration: user.created_at.timestamp(),
            last_login: user.last_login.map(|x| x.timestamp()),
            last_name_change: user.last_name_change.map(|x| x.timestamp()),
            enabled: user.enabled,
            admin: user.admin,
            terms_version: user.terms_version,
            terms_accepted_at: user.terms_accepted_at.map(|x| x.timestamp()),
            terms_declined_at: user.terms_declined_at.map(|x| x.timestamp()),

            display_name: profile.display_name,
            description: profile.bio,
            tags: profile.tags,

            mfa_enabled: details.mfa_enabled,
            password: details.password_login,

            business: invoice_info.business,
            first_name: invoice_info.first_name,
            last_name: invoice_info.last_name,
            street: invoice_info.street,
            zip_code: invoice_info.zip_code,
            city: invoice_info.city,
            country: invoice_info.country,
            vat_id: invoice_info.vat_id,

            avatar_url: None,
            can_buy_coins,
            can_receive_coins,
        }
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct ApiUserFilter {
    /// Filter by `name` and `display_name`
    pub name: Option<SearchTerm>,
    /// Filter by `email`
    pub email: Option<SearchTerm>,
    /// Filter by `enabled`
    pub enabled: Option<bool>,
    /// Filter by `admin`
    pub admin: Option<bool>,
    /// Filter by `mfa_enabled`
    pub mfa_enabled: Option<bool>,
    /// Filter by `email_verified`
    pub email_verified: Option<bool>,
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

impl JsonSchema for ApiUserIdOrSelf {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("UserIdOrSelf")
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        const_schema! {
            Me("me");
            Slf("self");
        }

        json_schema!({
            "oneOf": [
                Me::json_schema(generator),
                Slf::json_schema(generator),
                UserId::json_schema(generator),
            ],
        })
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PathUserId {
    pub user_id: UserId,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PathUserIdOrSelf {
    pub user_id: ApiUserIdOrSelf,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
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
