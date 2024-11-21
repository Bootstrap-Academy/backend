use std::{future::Future, path::Path};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait FsService: Send + Sync + 'static {
    /// Write `content` into the file at `path`, creating the file if it does
    /// not exist yet and otherwise overwriting its previous content.
    fn store_file(
        &self,
        path: &Path,
        content: &[u8],
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
}

#[cfg(feature = "mock")]
impl MockFsService {
    pub fn with_store_file(mut self, path: std::path::PathBuf, content: Vec<u8>) -> Self {
        self.expect_store_file()
            .once()
            .with(
                mockall::predicate::eq(path),
                mockall::predicate::eq(content),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(()))));
        self
    }
}
