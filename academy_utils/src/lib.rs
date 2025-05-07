mod macros;
pub mod patch;
pub mod serde;

use std::sync::LazyLock;

pub use academy_utils_derive::trace_instrument;

static ACADEMY_VERSION: LazyLock<&str> = LazyLock::new(|| {
    std::env::var("ACADEMY_VERSION")
        .map(|x| &*x.leak())
        .unwrap_or("0.0.0-dev")
});

pub fn academy_version() -> &'static str {
    &ACADEMY_VERSION
}

pub trait Apply: Sized {
    /// Apply the function `f` with a mutable reference to `self`.
    ///
    /// #### Example
    /// ```rust
    /// # use academy_utils::Apply;
    /// let x = 1.with(|x: &mut i32| *x += 2);
    /// assert_eq!(x, 3);
    /// ```
    fn with<X>(mut self, f: impl FnOnce(&mut Self) -> X) -> Self {
        f(&mut self);
        self
    }

    /// Apply the function `f`.
    ///
    /// #### Example
    /// ```rust
    /// # use academy_utils::Apply;
    /// fn inc(x: i32) -> i32 {
    ///     x + 1
    /// }
    /// assert_eq!(1.apply(inc), 2);
    /// ```
    fn apply<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }

    /// Apply the function `f` only if `value` is `Some(...)` and provides the
    /// contained value to `f`.
    ///
    /// #### Example
    /// ```rust
    /// # use academy_utils::Apply;
    /// fn add_option(a: i32, b: Option<i32>) -> i32 {
    ///     a.apply_map(b, |slf, arg| slf + arg)
    /// }
    /// assert_eq!(add_option(1, None), 1);
    /// assert_eq!(add_option(1, Some(2)), 3);
    /// ```
    fn apply_map<U>(self, value: Option<U>, f: impl FnOnce(Self, U) -> Self) -> Self {
        if let Some(value) = value {
            f(self, value)
        } else {
            self
        }
    }

    /// Apply the function `f` only if `apply` is `true`.
    ///
    /// #### Example
    /// ```rust
    /// # use academy_utils::Apply;
    /// fn maybe_add_two(a: i32, add: bool) -> i32 {
    ///     a.apply_if(add, |slf| slf + 2)
    /// }
    /// assert_eq!(maybe_add_two(1, false), 1);
    /// assert_eq!(maybe_add_two(1, true), 3);
    /// ```
    fn apply_if(self, apply: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if apply { f(self) } else { self }
    }
}

impl<T> Apply for T {}
