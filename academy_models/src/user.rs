use std::{borrow::Cow, sync::LazyLock};

use academy_utils::patch::Patch;
use chrono::{DateTime, Utc};
use nutype::nutype;
use regex::Regex;
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};

use crate::{
    SearchTerm,
    email_address::EmailAddress,
    macros::{id, nutype_string},
};

pub static USER_NAME_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^[a-zA-Z0-9_-]{1,32}$").unwrap());

id!(UserId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UserIdOrSelf {
    UserId(UserId),
    Slf,
}

impl From<UserId> for UserIdOrSelf {
    fn from(value: UserId) -> Self {
        Self::UserId(value)
    }
}

impl UserIdOrSelf {
    pub fn unwrap_or(self, self_user_id: UserId) -> UserId {
        match self {
            UserIdOrSelf::UserId(user_id) => user_id,
            UserIdOrSelf::Slf => self_user_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserComposite {
    pub user: User,
    pub profile: UserProfile,
    pub details: UserDetails,
    pub invoice_info: UserInvoiceInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Patch)]
pub struct User {
    #[no_patch]
    pub id: UserId,
    pub name: UserName,
    pub email: Option<EmailAddress>,
    pub email_verified: bool,
    #[no_patch]
    pub created_at: DateTime<Utc>,
    pub last_login: Option<DateTime<Utc>>,
    pub last_name_change: Option<DateTime<Utc>>,
    pub enabled: bool,
    pub admin: bool,
    pub newsletter: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Patch)]
pub struct UserProfile {
    pub display_name: UserDisplayName,
    pub bio: UserBio,
    pub tags: UserTags,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserDetails {
    pub mfa_enabled: bool,
    pub password_login: bool,
    pub oauth2_login: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Patch, Default)]
pub struct UserInvoiceInfo {
    pub business: Option<bool>,
    pub first_name: Option<UserFirstName>,
    pub last_name: Option<UserLastName>,
    pub street: Option<UserStreet>,
    pub zip_code: Option<UserZipCode>,
    pub city: Option<UserCity>,
    pub country: Option<UserCountry>,
    pub vat_id: Option<UserVatId>,
}

impl UserComposite {
    pub fn can_receive_coins(&self) -> bool {
        self.user.email_verified
            && self.invoice_info.business.is_some()
            && self.invoice_info.first_name.is_some()
            && self.invoice_info.last_name.is_some()
            && self.invoice_info.street.is_some()
            && self.invoice_info.zip_code.is_some()
            && self.invoice_info.city.is_some()
            && self.invoice_info.country.is_some()
            && (self.invoice_info.business != Some(true) || self.invoice_info.vat_id.is_some())
    }

    pub fn can_buy_coins(&self) -> bool {
        self.can_receive_coins()
            || (self.user.email_verified
                && self.invoice_info.business == Some(false)
                && self.invoice_info.country.is_some())
    }
}

impl UserInvoiceInfo {
    pub fn into_details(self, name_fallback: Option<String>, email: Option<String>) -> Vec<String> {
        let name = match (self.first_name, self.last_name) {
            (Some(first_name), Some(last_name)) => Some(format!("{} {}", *first_name, *last_name)),
            _ => name_fallback,
        };

        let city = match (self.zip_code, self.city) {
            (Some(zip_code), Some(city)) => Some(format!("{} {}", *zip_code, *city)),
            _ => None,
        };

        let vat_id = self
            .business
            .is_some_and(|x| x)
            .then(|| Some(format!("USt.-IdNr.: {}", *self.vat_id?)))
            .flatten();

        [
            name,
            self.street.map(UserStreet::into_inner),
            city,
            self.country.map(UserCountry::into_inner),
            email,
            vat_id,
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

nutype_string!(UserName(validate(regex = USER_NAME_REGEX)));
nutype_string!(UserDisplayName(validate(
    len_char_min = 1,
    len_char_max = 64
)));

nutype_string!(UserPassword(
    sensitive,
    validate(len_char_min = 1, len_char_max = UserPassword::MAX_LENGTH)
));
impl UserPassword {
    pub const MAX_LENGTH: usize = 4096;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum UserNameOrEmailAddress {
    Name(UserName),
    Email(EmailAddress),
}

nutype_string!(UserBio(
    validate(len_char_max = 1024),
    derive(Default),
    default = ""
));

nutype_string!(UserTag(validate(len_char_min = 1, len_char_max = 64)));

#[nutype(
    validate(predicate = |x| x.len() <= 8),
    derive(Debug, Clone, PartialEq, Eq, Deref, Default, TryFrom, Serialize, Deserialize),
    default = Vec::new(),
)]
pub struct UserTags(Vec<UserTag>);

impl JsonSchema for UserTags {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("UserTags")
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        Vec::<UserTag>::json_schema(generator)
    }
}

nutype_string!(UserFirstName(validate(len_char_max = 128)));
nutype_string!(UserLastName(validate(len_char_max = 128)));
nutype_string!(UserStreet(validate(len_char_max = 256)));
nutype_string!(UserZipCode(validate(len_char_max = 16)));
nutype_string!(UserCity(validate(len_char_max = 64)));
nutype_string!(UserCountry(validate(len_char_max = 64)));
nutype_string!(UserVatId(validate(len_char_max = 64)));

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UserFilter {
    pub name: Option<SearchTerm>,
    pub email: Option<SearchTerm>,
    pub enabled: Option<bool>,
    pub admin: Option<bool>,
    pub mfa_enabled: Option<bool>,
    pub email_verified: Option<bool>,
    pub newsletter: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_name_or_email() {
        enum Kind {
            Name,
            Email,
            Invalid,
        }
        use Kind::{Email, Invalid, Name};

        for (input, kind) in [
            ("foobar", Name),
            ("foo@bar.com", Email),
            ("@", Invalid),
            ("", Invalid),
            ("foo bar", Invalid),
        ] {
            let result = serde_json::from_value::<UserNameOrEmailAddress>(
                serde_json::Value::String(input.into()),
            );

            match kind {
                Name => assert_eq!(
                    result.unwrap(),
                    UserNameOrEmailAddress::Name(input.try_into().unwrap())
                ),
                Email => assert_eq!(
                    result.unwrap(),
                    UserNameOrEmailAddress::Email(input.parse().unwrap())
                ),
                Invalid => assert!(result.is_err()),
            }
        }
    }
}
