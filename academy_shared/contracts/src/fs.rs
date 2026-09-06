use std::{
    future::Future,
    path::{Path, PathBuf},
};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait FsService: Send + Sync + 'static {
    /// Write `content` into the file at `path`, creating the file if it does
    /// not exist yet and otherwise overwriting its previous content.
    fn store_file(
        &self,
        path: &Path,
        content: &[u8],
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Return the content of the file at `path` if the file exists.
    fn read_file(
        &self,
        path: &Path,
    ) -> impl Future<Output = anyhow::Result<Option<Vec<u8>>>> + Send;

    /// Delete the file at `path` and return whether it existed.
    fn delete_file(&self, path: &Path) -> impl Future<Output = anyhow::Result<bool>> + Send;

    /// Return the paths of all files in the directory at `path`.
    ///
    /// Returns an empty list if the directory does not exist.
    fn list_files(&self, path: &Path) -> impl Future<Output = anyhow::Result<Vec<PathBuf>>> + Send;
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

    pub fn with_read_file(mut self, path: std::path::PathBuf, result: Option<Vec<u8>>) -> Self {
        self.expect_read_file()
            .once()
            .with(mockall::predicate::eq(path))
            .return_once(|_| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_delete_file(mut self, path: std::path::PathBuf, result: bool) -> Self {
        self.expect_delete_file()
            .once()
            .with(mockall::predicate::eq(path))
            .return_once(move |_| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_list_files(mut self, path: std::path::PathBuf, result: Vec<PathBuf>) -> Self {
        self.expect_list_files()
            .once()
            .with(mockall::predicate::eq(path))
            .return_once(|_| Box::pin(std::future::ready(Ok(result))));
        self
    }
}
