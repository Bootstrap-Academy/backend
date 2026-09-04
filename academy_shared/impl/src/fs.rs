use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};

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

    async fn delete_file(&self, path: &Path) -> anyhow::Result<bool> {
        match tokio::fs::remove_file(path).await {
            Ok(()) => Ok(true),
            Err(err) => match err.kind() {
                ErrorKind::NotFound => Ok(false),
                _ => Err(err.into()),
            },
        }
    }

    async fn list_files(&self, path: &Path) -> anyhow::Result<Vec<PathBuf>> {
        let mut dir = match tokio::fs::read_dir(path).await {
            Ok(dir) => dir,
            Err(err) => {
                return match err.kind() {
                    ErrorKind::NotFound => Ok(Vec::new()),
                    _ => Err(err.into()),
                };
            }
        };

        let mut files = Vec::new();
        while let Some(entry) = dir.next_entry().await? {
            if entry.file_type().await?.is_file() {
                files.push(entry.path());
            }
        }
        files.sort();

        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn store_read_list_delete() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("file.txt");
        let sut = FsServiceImpl;

        assert_eq!(
            sut.list_files(&dir.path().join("nested")).await.unwrap(),
            Vec::<PathBuf>::new()
        );
        assert_eq!(sut.read_file(&path).await.unwrap(), None);
        assert!(!sut.delete_file(&path).await.unwrap());

        sut.store_file(&path, b"hello").await.unwrap();

        assert_eq!(sut.read_file(&path).await.unwrap(), Some(b"hello".to_vec()));
        assert_eq!(
            sut.list_files(&dir.path().join("nested")).await.unwrap(),
            vec![path.clone()]
        );

        assert!(sut.delete_file(&path).await.unwrap());
        assert_eq!(sut.read_file(&path).await.unwrap(), None);
        assert!(!sut.delete_file(&path).await.unwrap());
    }
}
