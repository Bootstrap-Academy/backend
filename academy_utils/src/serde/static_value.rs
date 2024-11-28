/// Define a unit struct which always serializes into and deserializes from a
/// static value.
///
/// #### Example
/// ```rust
/// # use academy_utils::static_value;
/// # use serde::{Serialize, Deserialize};
/// static_value!(Foo("foo"));
///
/// #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
/// struct Test {
///     value: Foo,
/// }
///
/// let test = Test { value: Foo };
/// let serialized = serde_json::to_string(&test).unwrap();
/// assert_eq!(serialized, r#"{"value":"foo"}"#);
/// let deserialized = serde_json::from_str::<Test>(&serialized).unwrap();
/// assert_eq!(deserialized, test);
///
/// let invalid = r#"{"value":"invalid"}"#;
/// let err = serde_json::from_str::<Test>(&invalid).unwrap_err();
/// assert_eq!(
///     err.to_string(),
///     r#"Expected "foo", got "invalid" instead at line 1 column 19"#
/// );
/// ```
#[macro_export]
macro_rules! static_value {
    ($vis:vis $ident:ident($expr:expr)) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        $vis struct $ident;

        impl ::serde::Serialize for $ident {
            fn serialize<S>(&self, serializer: S) -> ::core::result::Result<S::Ok, S::Error>
            where
                S: ::serde::Serializer,
            {
                ( $expr ).serialize(serializer)
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $ident {
            fn deserialize<D>(deserializer: D) -> ::core::result::Result<Self, D::Error>
            where
                D: ::serde::Deserializer<'de>,
            {
                fn eq<T: ::core::cmp::PartialEq>(a: &T, b: &T) -> bool {
                    a == b
                }

                let expected = $expr;
                let deserialized = ::serde::Deserialize::deserialize(deserializer)?;
                eq(&expected, &deserialized)
                    .then_some(Self)
                    .ok_or_else(|| ::serde::de::Error::custom(format!("Expected {expected:?}, got {deserialized:?} instead")))
            }
        }
    };
}
