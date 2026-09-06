use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};

use academy_di::Build;
use academy_shared_contracts::fs::FsService;

#[derive(Debug, Clone, Build)]
pub struct FsServiceImpl;

impl FsService for FsServiceImpl {
    /// Write the file through a temporary sibling that is renamed into place.
    ///
    /// Writing to the target directly truncates it first, so a crash or a full
    /// disk halfway through would leave a short file behind. Every caller
    /// treats an existing file as the finished document and never renders it
    /// again, so a truncated invoice would be served from then on. A rename
    /// within the same directory is atomic, so the file either is not there at
    /// all or is complete.
    async fn store_file(&self, path: &Path, content: &[u8]) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let mut temp_path = path.as_os_str().to_owned();
        temp_path.push(".tmp");
        let temp_path = PathBuf::from(temp_path);

        if let Err(err) = write_and_sync(&temp_path, content).await {
            // Leaving the temporary file behind would make the next attempt
            // look like a partial write of this one.
            tokio::fs::remove_file(&temp_path).await.ok();
            return Err(err);
        }

        if let Err(err) = tokio::fs::rename(&temp_path, path).await {
            tokio::fs::remove_file(&temp_path).await.ok();
            return Err(err.into());
        }

        Ok(())
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

/// Write the whole content and flush it to disk, so that the rename that
/// follows cannot publish a file the disk does not have yet.
async fn write_and_sync(path: &Path, content: &[u8]) -> anyhow::Result<()> {
    tokio::fs::write(path, content).await?;
    tokio::fs::File::open(path).await?.sync_all().await?;
    Ok(())
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

    /// The file is written through a temporary sibling, which must not be left
    /// behind: the archive directories are listed to find documents whose
    /// record is missing.
    #[tokio::test]
    async fn store_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("R0000042.pdf");
        let sut = FsServiceImpl;

        sut.store_file(&path, b"first").await.unwrap();
        assert_eq!(
            sut.list_files(dir.path()).await.unwrap(),
            vec![path.clone()]
        );

        // Overwriting an existing file replaces it completely.
        sut.store_file(&path, b"second").await.unwrap();
        assert_eq!(
            sut.list_files(dir.path()).await.unwrap(),
            vec![path.clone()]
        );
        assert_eq!(
            sut.read_file(&path).await.unwrap(),
            Some(b"second".to_vec())
        );
    }

    /// A write that cannot be completed must not damage the document that is
    /// already there.
    #[tokio::test]
    async fn a_failed_store_keeps_the_previous_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("R0000042.pdf");
        let sut = FsServiceImpl;

        sut.store_file(&path, b"complete").await.unwrap();

        // The temporary sibling cannot be created, because a directory is
        // already there under its name.
        let temp_path = dir.path().join("R0000042.pdf.tmp");
        tokio::fs::create_dir(&temp_path).await.unwrap();

        assert!(sut.store_file(&path, b"truncated").await.is_err());
        assert_eq!(
            sut.read_file(&path).await.unwrap(),
            Some(b"complete".to_vec())
        );
    }
}
