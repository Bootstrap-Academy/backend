macro_rules! id {
    ($ident:ident) => {
        #[::nutype::nutype(derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Deref,
            From,
            Serialize,
            Deserialize,
        ))]
        pub struct $ident(::uuid::Uuid);

        impl ::schemars::JsonSchema for $ident {
            fn schema_name() -> ::std::string::String {
                ::core::stringify!($ident).into()
            }

            fn json_schema(
                generator: &mut ::schemars::r#gen::SchemaGenerator,
            ) -> ::schemars::schema::Schema {
                <::uuid::Uuid as ::schemars::JsonSchema>::json_schema(generator)
            }
        }
    };
}

macro_rules! sha256hash {
    ($ident:ident) => {
        #[::nutype::nutype(derive(
            Debug,
            Display,
            Clone,
            Copy,
            PartialEq,
            Eq,
            Deref,
            From,
            Serialize,
            Deserialize
        ))]
        pub struct $ident($crate::Sha256Hash);
    };
}

macro_rules! nutype_string {
    ($ident:ident) => {
        $crate::macros::nutype_string!($ident());
    };
    ($ident:ident(
        $(sanitize( $($sanitize:tt)* ) $(,)? )?
        validate( $($validate:tt)* ) $(,)?
        $(derive( $($derive:tt)* ) $(,)? )?
        $(default = $default:expr $(,)? )?
    )) => {
        #[::nutype::nutype(
            $(sanitize($($sanitize)*),)?
            validate($($validate)*),
            derive(
                Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, AsRef, Deref, TryFrom, Serialize, Deserialize, JsonSchema,
                $($($derive)*)?
            ),
            $(default = $default,)?
        )]
        pub struct $ident(String);
        $crate::macros::nutype_string!(@ as_ref_u8 $ident);
    };
    ($ident:ident(
        $(sanitize( $($sanitize:tt)* ) $(,)? )?
        $(derive( $($derive:tt)* ) $(,)? )?
        $(default = $default:expr $(,)? )?
    )) => {
        #[::nutype::nutype(
            $(sanitize($($sanitize)*),)?
            derive(
                Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, AsRef, Deref, From, Serialize, Deserialize, JsonSchema,
                $($($derive)*)?
            ),
            $(default = $default,)?
        )]
        pub struct $ident(String);
        $crate::macros::nutype_string!(@ as_ref_u8 $ident);
    };
    ($ident:ident(
        sensitive $(,)?
        $(sanitize( $($sanitize:tt)* ) $(,)? )?
        validate( $($validate:tt)* ) $(,)?
        $(derive( $($derive:tt)* ) $(,)? )?
        $(default = $default:expr $(,)? )?
    )) => {
        #[::nutype::nutype(
            $(sanitize($($sanitize)*),)?
            validate($($validate)*),
            derive(
                Clone, PartialEq, Eq, PartialOrd, Ord, Hash, AsRef, Deref, TryFrom, Serialize, Deserialize, JsonSchema,
                $($($derive)*)?
            ),
            $(default = $default,)?
        )]
        pub struct $ident(String);
        $crate::macros::nutype_string!(@ as_ref_u8 $ident);
        $crate::macros::sensitive_debug!($ident);
    };
    ($ident:ident(
        sensitive $(,)?
        $(sanitize( $($sanitize:tt)* ) $(,)? )?
        $(derive( $($derive:tt)* ) $(,)? )?
        $(default = $default:expr $(,)? )?
    )) => {
        #[::nutype::nutype(
            $(sanitize($($sanitize)*),)?
            derive(
                Clone, PartialEq, Eq, PartialOrd, Ord, Hash, AsRef, Deref, From, Serialize, Deserialize, JsonSchema,
                $($($derive)*)?
            ),
            $(default = $default,)?
        )]
        pub struct $ident(String);
        $crate::macros::nutype_string!(@ as_ref_u8 $ident);
        $crate::macros::sensitive_debug!($ident);
    };
    (@ as_ref_u8 $ident:ident) => {
        impl ::std::convert::AsRef<[u8]> for $ident {
            fn as_ref(&self) -> &[u8] { self.as_bytes() }
        }
    };
}

macro_rules! sensitive_debug {
    ($ident:ident $(<$($generics:ident),*>)?) => {
        impl $(<$($generics : ::std::fmt::Debug),*>)? ::std::fmt::Debug for $ident $(<$($generics),*>)? {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                // debug: use default Debug implementation
                #[cfg(debug_assertions)]
                { f.debug_tuple(::core::stringify!($ident)).field(&**self).finish() }

                // release: hide secrets
                #[cfg(not(debug_assertions))]
                { f.write_str(::core::concat!(::core::stringify!($ident), "(<redacted>)")) }
            }
        }
    };
}

pub(crate) use id;
pub(crate) use nutype_string;
pub(crate) use sensitive_debug;
pub(crate) use sha256hash;
