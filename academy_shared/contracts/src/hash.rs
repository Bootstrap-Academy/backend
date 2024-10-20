use std::fmt::Debug;

use academy_models::Sha256Hash;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait HashService: Send + Sync + 'static {
    /// Compute the SHA-256 hash of the given data.
    fn sha256<T: AsRef<[u8]> + Debug + 'static>(&self, data: &T) -> Sha256Hash;
}

#[cfg(feature = "mock")]
impl MockHashService {
    pub fn with_sha256<T: AsRef<[u8]> + PartialEq + Debug + Send + 'static>(
        mut self,
        data: T,
        result: Sha256Hash,
    ) -> Self {
        self.expect_sha256()
            .once()
            .with(mockall::predicate::eq(data))
            .return_once(move |_| result);
        self
    }
}
