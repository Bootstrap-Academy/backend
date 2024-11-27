use std::{io::ErrorKind, path::Path};

use academy_di::Build;
use academy_shared_contracts::fs::FsService;

#[derive(Debug, Clone, Build)]
pub struct FsServiceImpl;

impl FsService for FsServiceImpl {
    async fn store_file(&self, path: &Path, content: &[u8]) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(path, content).await.map_err(Into::into)
    }

    async fn read_file(&self, path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
        match tokio::fs::read(path).await {
            Ok(content) => Ok(Some(content)),
            Err(err) => match err.kind() {
                ErrorKind::NotFound => Ok(None),
                _ => Err(err.into()),
            },
        }
    }
}
