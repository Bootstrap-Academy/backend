#[macro_export]
macro_rules! bin_name {
    () => {
        ::core::env!("CARGO_BIN_NAME")
    };
}

#[macro_export]
macro_rules! assert_matches {
    ($expr:expr, $pat:pat) => {
        match ($expr) {
            $pat => (),
            val => ::core::panic!(
                "Assertion failed: Value {val:?} did not match pattern {}",
                ::core::stringify!($pat)
            ),
        }
    };
    ($expr:expr, $pat:pat if $pred:expr) => {{
        let val = $expr;
        match (&val) {
            $pat if $pred => (),
            #[allow(unused_variables)]
            $pat => ::core::panic!(
                "Assertion failed: Value {val:?} does not match predicate {}",
                ::core::stringify!($pred)
            ),
            _ => ::core::panic!(
                "Assertion failed: Value {val:?} did not match pattern {}",
                ::core::stringify!($pat)
            ),
        }
    }};
}
