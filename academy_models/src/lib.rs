use std::{ops::Deref, sync::LazyLock};

use macros::{nutype_string, sensitive_debug};
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub mod auth;
pub mod coin;
pub mod contact;
pub mod email_address;
mod macros;
pub mod mfa;
pub mod oauth2;
pub mod pagination;
pub mod paypal;
pub mod session;
pub mod url;
pub mod user;

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sha256Hash(#[serde(with = "academy_utils::serde::hex")] pub [u8; 32]);

impl std::fmt::Debug for Sha256Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        hex::encode(self.0).fmt(f)
    }
}

impl std::fmt::Display for Sha256Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        hex::encode(self.0).fmt(f)
    }
}

nutype_string!(SearchTerm(validate(len_char_max = 256)));

nutype_string!(VerificationCode(
    sensitive,
    sanitize(uppercase),
    validate(regex = VERIFICATION_CODE_REGEX),
));

pub static VERIFICATION_CODE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    hyphenated_code_regex(VerificationCode::CHUNK_COUNT, VerificationCode::CHUNK_SIZE)
});

impl VerificationCode {
    pub const CHUNK_COUNT: usize = 4;
    pub const CHUNK_SIZE: usize = 4;
}

fn hyphenated_code_regex(chunk_count: usize, chunk_size: usize) -> Regex {
    Regex::new(&format!(
        "^([A-Z0-9]{{{0}}}-){{{1}}}[A-Z0-9]{{{0}}}$",
        chunk_size,
        chunk_count - 1
    ))
    .unwrap()
}

nutype_string!(RecaptchaResponse(validate(len_char_max = 2048)));

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Sensitive<T>(pub T);
sensitive_debug!(Sensitive<T>);
impl<T> From<T> for Sensitive<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}
impl<T> Deref for Sensitive<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
