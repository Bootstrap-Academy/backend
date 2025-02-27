use std::str::FromStr;

use schemars::{
    gen::SchemaGenerator,
    schema::{Schema, SchemaObject},
    JsonSchema,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmailAddress(pub lettre::Address);

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmailAddressWithName(pub lettre::message::Mailbox);

impl std::fmt::Debug for EmailAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("EmailAddress")
            .field(&AsRef::<str>::as_ref(&self.0))
            .finish()
    }
}

impl std::fmt::Debug for EmailAddressWithName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("EmailAddressWithName")
            .field(&self.0.to_string())
            .finish()
    }
}

impl std::fmt::Display for EmailAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl EmailAddress {
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }

    pub fn with_name(self, name: String) -> EmailAddressWithName {
        EmailAddressWithName(lettre::message::Mailbox {
            name: Some(name),
            email: self.0,
        })
    }
}

impl EmailAddressWithName {
    pub fn into_email_address(self) -> EmailAddress {
        EmailAddress(self.0.email)
    }
}

impl FromStr for EmailAddress {
    type Err = <lettre::Address as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self)
    }
}

impl FromStr for EmailAddressWithName {
    type Err = <lettre::message::Mailbox as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self)
    }
}

impl TryFrom<&str> for EmailAddress {
    type Error = <Self as FromStr>::Err;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl JsonSchema for EmailAddress {
    fn schema_name() -> String {
        "EmailAddress".into()
    }

    fn json_schema(_gen: &mut SchemaGenerator) -> Schema {
        SchemaObject {
            format: Some("email".into()),
            ..Default::default()
        }
        .into()
    }
}
