use std::borrow::Cow;

use academy_models::pagination::{PaginationLimit, PaginationSlice};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::Deserialize;

use crate::const_schema;

pub mod coin;
pub mod contact;
pub mod contract;
pub mod heart;
pub mod oauth2;
pub mod premium;
pub mod session;
pub mod user;
pub mod user_export;
pub mod withdrawal;

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
#[derive(Default)]
pub enum StringOption<T> {
    Some(T),
    #[default]
    None,
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
    fn schema_name() -> Cow<'static, str> {
        <Option<T> as JsonSchema>::schema_name()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        <Option<T> as JsonSchema>::json_schema(generator)
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

    #[test]
    fn string_option_missing_field() {
        #[derive(Deserialize)]
        struct Request {
            #[serde(default)]
            password: StringOption<UserPassword>,
        }

        let request =
            serde_json::from_value::<Request>(serde_json::json!({})).expect("field is optional");
        assert_eq!(Option::<UserPassword>::from(request.password), None);
    }
}
