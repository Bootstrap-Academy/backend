use std::borrow::Cow;

use academy_models::pagination::{PaginationLimit, PaginationSlice};
use schemars::{gen::SchemaGenerator, schema::Schema, JsonSchema};
use serde::Deserialize;

use crate::const_schema;

pub mod coin;
pub mod contact;
pub mod oauth2;
pub mod session;
pub mod user;

const_schema! {
    pub OkResponse(true);
}

#[derive(Deserialize, JsonSchema)]
pub struct ApiPaginationSlice {
    /// The number of items to select.
    #[serde(default)]
    pub limit: PaginationLimit,
    /// The number of items to skip.
    #[serde(default)]
    pub offset: u64,
}

impl From<ApiPaginationSlice> for PaginationSlice {
    fn from(value: ApiPaginationSlice) -> Self {
        Self {
            limit: value.limit,
            offset: value.offset,
        }
    }
}

/// [`Option`]-like enum that deserializes the empty string to `None`
pub enum StringOption<T> {
    Some(T),
    None,
}

// no `#[derive(Default)]` to avoid the `T: Default` bound
impl<T> Default for StringOption<T> {
    fn default() -> Self {
        Self::None
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for StringOption<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        enum Inner<T> {
            #[serde(rename = "")]
            Empty,
            #[serde(untagged)]
            Some(T),
        }

        match Option::<Inner<T>>::deserialize(deserializer)? {
            Some(Inner::Some(x)) => Ok(Self::Some(x)),
            Some(Inner::Empty) | None => Ok(Self::None),
        }
    }
}

impl<T: JsonSchema> JsonSchema for StringOption<T> {
    fn schema_name() -> String {
        <Option<T> as JsonSchema>::schema_name()
    }

    fn json_schema(gen: &mut SchemaGenerator) -> Schema {
        <Option<T> as JsonSchema>::json_schema(gen)
    }

    fn is_referenceable() -> bool {
        <Option<T> as JsonSchema>::is_referenceable()
    }

    fn schema_id() -> Cow<'static, str> {
        <Option<T> as JsonSchema>::schema_id()
    }
}

impl<T> From<StringOption<T>> for Option<T> {
    fn from(value: StringOption<T>) -> Self {
        match value {
            StringOption::Some(x) => Some(x),
            StringOption::None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use academy_models::user::UserPassword;

    use super::*;

    #[test]
    fn string_option() {
        let x = serde_json::Value::String("test".into());
        let y = Option::<UserPassword>::from(
            serde_json::from_value::<StringOption<UserPassword>>(x).unwrap(),
        );
        assert_eq!(y.unwrap().into_inner(), "test");

        let x = serde_json::Value::String("".into());
        let y = Option::<UserPassword>::from(
            serde_json::from_value::<StringOption<UserPassword>>(x).unwrap(),
        );
        assert_eq!(y, None);

        let x = serde_json::Value::Null;
        let y = Option::<UserPassword>::from(
            serde_json::from_value::<StringOption<UserPassword>>(x).unwrap(),
        );
        assert_eq!(y, None);
    }
}
