/// Define unit structs that serialize into constant values
#[macro_export]
macro_rules! const_schema {
    ($($vis:vis $ident:ident($expr:expr));* $(;)*) => { $(
        $vis struct $ident;
        impl $ident {
            pub fn value() -> ::serde_json::Value { ($expr).into() }
        }
        impl ::schemars::JsonSchema for $ident {
            fn schema_name() -> ::std::borrow::Cow<'static, str> {
                ::std::borrow::Cow::Borrowed(::core::stringify!($ident))
            }
            fn json_schema(_gen: &mut ::schemars::SchemaGenerator) -> ::schemars::Schema {
                ::schemars::json_schema!({"const": Self::value()})
            }
            fn inline_schema() -> bool { true }
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
