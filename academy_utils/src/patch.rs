pub use academy_utils_derive::Patch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchValue<T> {
    Update(T),
    Unchanged,
}

impl<T> Default for PatchValue<T> {
    fn default() -> Self {
        Self::Unchanged
    }
}

impl<T> PatchValue<T> {
    pub fn update_or(self, old_value: T) -> T {
        match self {
            Self::Update(new_value) => new_value,
            Self::Unchanged => old_value,
        }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> PatchValue<U> {
        match self {
            Self::Update(x) => PatchValue::Update(f(x)),
            Self::Unchanged => PatchValue::Unchanged,
        }
    }

    pub fn as_ref(&self) -> PatchValue<&T> {
        match self {
            Self::Update(x) => PatchValue::Update(x),
            Self::Unchanged => PatchValue::Unchanged,
        }
    }

    pub fn is_update(&self) -> bool {
        matches!(self, Self::Update(_))
    }

    pub fn is_update_and(&self, f: impl FnOnce(&T) -> bool) -> bool {
        match self {
            Self::Update(value) => f(value),
            _ => false,
        }
    }

    pub fn update(self) -> Option<T> {
        match self {
            PatchValue::Update(value) => Some(value),
            PatchValue::Unchanged => None,
        }
    }

    pub fn is_unchanged(&self) -> bool {
        matches!(self, Self::Unchanged)
    }

    pub fn minimize(self, old_value: &T) -> Self
    where
        T: PartialEq,
    {
        match self {
            Self::Update(new_value) if &new_value == old_value => Self::Unchanged,
            Self::Update(new_value) => Self::Update(new_value),
            Self::Unchanged => Self::Unchanged,
        }
    }
}

impl<T> From<T> for PatchValue<T> {
    fn from(value: T) -> Self {
        Self::Update(value)
    }
}

impl<T> From<Option<T>> for PatchValue<T> {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(x) => PatchValue::Update(x),
            None => PatchValue::Unchanged,
        }
    }
}

pub trait Patch {
    type Patch;
    type PatchRef<'a>
    where
        Self: 'a;

    fn update(self, patch: Self::Patch) -> Self;

    fn into_patch(self) -> Self::Patch;

    fn as_patch_ref(&self) -> Self::PatchRef<'_>;
}
