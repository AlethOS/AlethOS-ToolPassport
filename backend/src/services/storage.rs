use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::{
    fs::{self, OpenOptions},
    io::AsyncWriteExt,
};
use uuid::Uuid;

pub const DEFAULT_MAX_STORED_BYTES: usize = 1024 * 1024;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("stored content exceeds the {max_bytes} byte limit")]
    TooLarge { max_bytes: usize },
    #[error("invalid storage key")]
    InvalidStorageKey,
    #[error("storage path escaped the configured artifact root")]
    PathTraversal,
}

#[derive(Debug)]
pub struct StoredFile {
    pub storage_key: String,
    pub size_bytes: i64,
    pub sha256_hash: String,
}

#[derive(Clone)]
pub struct StorageService {
    base_path: PathBuf,
    max_bytes: usize,
}

impl StorageService {
    pub fn new(base_path: impl Into<PathBuf>, max_bytes: usize) -> Self {
        Self {
            base_path: base_path.into(),
            max_bytes,
        }
    }

    pub const fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    pub async fn save_artifact(
        &self,
        run_id: Uuid,
        artifact_id: Uuid,
        content: &[u8],
    ) -> Result<StoredFile, StorageError> {
        self.save_file(format!("{run_id}/artifacts/{artifact_id}"), content)
            .await
    }

    pub async fn save_evidence(
        &self,
        run_id: Uuid,
        evidence_id: Uuid,
        content: &[u8],
    ) -> Result<StoredFile, StorageError> {
        self.save_file(format!("{run_id}/evidence/{evidence_id}.json"), content)
            .await
    }

    pub async fn remove_file(&self, storage_key: &str) -> Result<(), StorageError> {
        let target = self.resolve_storage_key(storage_key).await?;
        match fs::remove_file(target).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    async fn save_file(
        &self,
        storage_key: String,
        content: &[u8],
    ) -> Result<StoredFile, StorageError> {
        if content.len() > self.max_bytes {
            return Err(StorageError::TooLarge {
                max_bytes: self.max_bytes,
            });
        }

        let target = self.resolve_storage_key(&storage_key).await?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&target)
            .await?;

        if let Err(error) = file.write_all(content).await {
            let _ = fs::remove_file(&target).await;
            return Err(error.into());
        }
        if let Err(error) = file.flush().await {
            let _ = fs::remove_file(&target).await;
            return Err(error.into());
        }

        let mut hasher = Sha256::new();
        hasher.update(content);

        Ok(StoredFile {
            storage_key,
            size_bytes: content.len() as i64,
            sha256_hash: format!("0x{}", hex::encode(hasher.finalize())),
        })
    }

    async fn resolve_storage_key(&self, storage_key: &str) -> Result<PathBuf, StorageError> {
        let relative = Path::new(storage_key);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(StorageError::InvalidStorageKey);
        }

        fs::create_dir_all(&self.base_path).await?;
        let absolute_root = fs::canonicalize(&self.base_path).await?;
        let target = absolute_root.join(relative);
        let parent = target.parent().ok_or(StorageError::InvalidStorageKey)?;
        fs::create_dir_all(parent).await?;
        let absolute_parent = fs::canonicalize(parent).await?;

        if !absolute_parent.starts_with(&absolute_root) {
            return Err(StorageError::PathTraversal);
        }

        Ok(absolute_parent.join(target.file_name().ok_or(StorageError::InvalidStorageKey)?))
    }
}
