/// Define unit structs that serialize into constant values
#[macro_export]
macro_rules! const_schema {
    ($($vis:vis $ident:ident($expr:expr));* $(;)*) => { $(
        $vis struct $ident;
        impl $ident {
            pub fn value() -> ::serde_json::Value { ($expr).into() }
        }
        impl ::schemars::JsonSchema for $ident {
            fn schema_name() -> ::std::string::String { ::core::stringify!($ident).into() }
            fn is_referenceable() -> ::core::primitive::bool { false }
            fn json_schema(_gen: &mut ::schemars::r#gen::SchemaGenerator) -> ::schemars::schema::Schema {
                ::schemars::schema::SchemaObject {
                    const_value: ::core::option::Option::Some(Self::value()),
                    ..::core::default::Default::default()
                }.into()
            }
        }
        impl ::serde::Serialize for $ident {
            fn serialize<S>(&self, serializer: S) -> ::core::result::Result<S::Ok, S::Error>
            where S: ::serde::Serializer,
            { ::serde::Serialize::serialize(&Self::value(), serializer) }
        }
        impl ::core::default::Default for $ident {
            fn default() -> Self { Self }
        }
    )* };
}

/// Define unit structs that implement [`ApiErrorCode`]
#[macro_export]
macro_rules! error_code {
    ($($(#[doc=$doc:literal])* $vis:vis $ident:ident($status:ident, $detail:literal));* $(;)*) => {
        $crate::const_schema! { $(
            $vis $ident($detail);
        )* }

        $(
            impl $crate::errors::ApiErrorCode for $ident {
                const DESCRIPTION: &str = ::core::concat!($($doc),*);
                const STATUS_CODE: StatusCode = ::axum::http::StatusCode::$status;
            }

            impl ::axum::response::IntoResponse for $ident {
                fn into_response(self) -> ::axum::response::Response {
                    ::axum::response::IntoResponse::into_response((
                        <Self as $crate::errors::ApiErrorCode>::STATUS_CODE,
                        ::axum::Json($crate::errors::ApiError {
                            code: self,
                        }),
                    ))
                }
            }
        )*
    };
}
