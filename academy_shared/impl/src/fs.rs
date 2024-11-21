use std::path::Path;

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
}
