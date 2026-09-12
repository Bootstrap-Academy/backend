use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

pub const IMAGE_TTL: i64 = 90 * 24 * 3600;
pub const MAX_IMAGE_STORAGE: u64 = 256 * 1024 * 1024;
pub const MAX_RECORDS: usize = 10_000;

/// Minimal durable receipt, never the original report or source screenshot.
/// Pending means the network call MAY have happened. Even an absent search
/// result never grants permission to repeat that non-idempotent POST.
#[derive(Clone, Deserialize, Serialize)]
pub struct Receipt {
    pub request_id: Uuid,
    pub fingerprint: String,
    pub marker: Uuid,
    pub created_at: i64,
    pub image: Option<Uuid>,
    pub issue_url: Option<String>,
}

pub struct Store {
    root: PathBuf,
    // Lifetime ownership prevents two backend processes from using one journal.
    _lock: File,
    pub receipts: HashMap<Uuid, Receipt>,
}

impl Store {
    pub fn open(root: &Path, now: i64) -> anyhow::Result<Self> {
        private_directory(root)?;
        let mut options = OpenOptions::new();
        options.create(true).truncate(false).read(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        let lock = options.open(root.join("writer.lock"))?;
        lock.try_lock()
            .context("feedback storage already has a writer")?;
        for subdir in ["receipts", "images"] {
            private_directory(&root.join(subdir))?;
        }
        let mut receipts = HashMap::new();
        for file in fs::read_dir(root.join("receipts"))? {
            let file = file?;
            if file.path().extension().is_some_and(|ext| ext == "tmp") {
                fs::remove_file(file.path())?;
                continue;
            }
            anyhow::ensure!(
                file.file_type()?.is_file() && file.metadata()?.len() <= 4096,
                "invalid feedback receipt file"
            );
            anyhow::ensure!(
                receipts.len() < MAX_RECORDS,
                "feedback receipt limit exceeded"
            );
            let receipt: Receipt = serde_json::from_slice(&fs::read(file.path())?)?;
            anyhow::ensure!(
                file.file_name().to_string_lossy() == format!("{}.json", receipt.request_id),
                "invalid feedback receipt name"
            );
            receipts.insert(receipt.request_id, receipt);
        }
        let store = Self {
            root: root.into(),
            _lock: lock,
            receipts,
        };
        store.cleanup(now)?;
        Ok(store)
    }

    pub fn save(&mut self, receipt: Receipt) -> anyhow::Result<()> {
        let path = self
            .root
            .join("receipts")
            .join(format!("{}.json", receipt.request_id));
        atomic_write(&path, &serde_json::to_vec(&receipt)?)?;
        self.receipts.insert(receipt.request_id, receipt);
        Ok(())
    }

    pub fn put_image(&self, id: Uuid, bytes: &[u8]) -> anyhow::Result<()> {
        atomic_write(&self.image_path(id), bytes)
    }

    pub fn image_path(&self, id: Uuid) -> PathBuf {
        self.root.join("images").join(format!("{id}.png"))
    }

    pub fn image_is_live(&self, id: Uuid, now: i64) -> bool {
        self.receipts
            .values()
            .any(|r| r.image == Some(id) && now < r.created_at + IMAGE_TTL)
    }

    pub fn image_bytes(&self) -> anyhow::Result<u64> {
        fs::read_dir(self.root.join("images"))?
            .try_fold(0, |total, file| Ok(total + file?.metadata()?.len()))
    }

    pub fn cleanup(&self, now: i64) -> anyhow::Result<()> {
        let live: std::collections::HashSet<_> = self
            .receipts
            .values()
            .filter(|r| now < r.created_at + IMAGE_TTL)
            .filter_map(|r| r.image)
            .collect();
        for file in fs::read_dir(self.root.join("images"))? {
            let file = file?;
            let id = file
                .path()
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.parse::<Uuid>().ok());
            if file.path().extension().is_none_or(|ext| ext != "png")
                || !id.is_some_and(|id| live.contains(&id))
            {
                fs::remove_file(file.path())?;
            }
        }
        File::open(self.root.join("images"))?.sync_all()?;
        Ok(())
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let temp = path.with_extension("tmp");
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&temp, path)?;
    File::open(path.parent().expect("storage path has parent"))?.sync_all()?;
    Ok(())
}

fn private_directory(path: &Path) -> anyhow::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(path)?;
    Ok(())
}
