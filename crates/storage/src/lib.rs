//! Content-addressed blob storage.
//!
//! Every blob is stored once under the SHA-256 of its contents
//! (`blobs/ab/cd/abcd…`), so identical uploads are deduplicated for free. Backends
//! come from [`object_store`]: a local directory (default) or any S3-compatible
//! service. Reference counting (which files point at which blob) lives in Postgres,
//! not here: this crate only knows about bytes.

mod backend;
mod hash;
mod staging;

use std::{sync::Arc, time::Duration};

use bytes::Bytes;
use futures_util::{Stream, StreamExt, TryStreamExt, stream::BoxStream};
use object_store::{ObjectStore, ObjectStoreExt, PutPayload, memory::InMemory, path::Path};

pub use hash::ContentHash;
pub use staging::{FinishedBlob, StagedBlob};

pub type Result<T, E = StorageError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("blob not found")]
    NotFound,
    #[error("invalid content hash")]
    InvalidHash,
    #[error("storage misconfigured: {0}")]
    Config(String),
    #[error("storage backend error: {0}")]
    Backend(#[source] object_store::Error),
    #[error("upload stream failed: {0}")]
    Source(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl From<object_store::Error> for StorageError {
    fn from(err: object_store::Error) -> Self {
        match err {
            object_store::Error::NotFound { .. } => Self::NotFound,
            other => Self::Backend(other),
        }
    }
}

impl From<StorageError> for akasha_core::Error {
    fn from(err: StorageError) -> Self {
        match err {
            StorageError::NotFound => Self::not_found("file contents not found"),
            StorageError::InvalidHash => Self::bad_request("invalid content hash"),
            other => {
                tracing::error!(err = %other, "storage error");
                Self::internal("storage error")
            }
        }
    }
}

/// The result of storing a blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlobInfo {
    pub hash: ContentHash,
    pub size: u64,
    /// The blob was already stored; nothing new was written.
    pub deduplicated: bool,
}

/// A blob being read: its size plus a stream of its bytes.
pub struct Blob {
    pub size: u64,
    pub stream: BoxStream<'static, Result<Bytes>>,
}

/// Handle to the blob store. Cheap to clone.
#[derive(Clone, Debug)]
pub struct Storage {
    store: Arc<dyn ObjectStore>,
}

impl Storage {
    /// Wrap any `object_store` backend.
    pub fn new(store: Arc<dyn ObjectStore>) -> Self {
        Self { store }
    }

    /// Build the backend selected in the configuration.
    pub fn from_config(config: &akasha_core::Config) -> Result<Self> {
        backend::build(config).map(Self::new)
    }

    /// A local-directory store rooted at `dir` (created if missing).
    pub fn local(dir: impl AsRef<std::path::Path>) -> Result<Self> {
        backend::local(dir.as_ref()).map(Self::new)
    }

    /// A process-local store for tests.
    pub fn in_memory() -> Self {
        Self::new(Arc::new(InMemory::new()))
    }

    /// Store a buffer that is already in memory.
    pub async fn put_bytes(&self, bytes: impl Into<Bytes>) -> Result<BlobInfo> {
        let bytes = bytes.into();
        let hash = ContentHash::of(&bytes);
        let size = bytes.len() as u64;
        if self.exists(&hash).await? {
            return Ok(BlobInfo {
                hash,
                size,
                deduplicated: true,
            });
        }
        self.store
            .put(&hash.key(), PutPayload::from_bytes(bytes))
            .await?;
        Ok(BlobInfo {
            hash,
            size,
            deduplicated: false,
        })
    }

    /// Start a streaming upload; see [`StagedBlob`].
    pub async fn stage(&self) -> Result<StagedBlob> {
        StagedBlob::start(Arc::clone(&self.store)).await
    }

    /// Store a stream of chunks without buffering it in memory.
    pub async fn put_stream<S, E>(&self, stream: S) -> Result<BlobInfo>
    where
        S: Stream<Item = Result<Bytes, E>> + Send,
        E: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let mut staged = self.stage().await?;
        let mut stream = std::pin::pin!(stream);
        while let Some(chunk) = stream.next().await {
            let written = match chunk {
                Ok(chunk) => staged.write(chunk).await,
                Err(err) => Err(StorageError::Source(err.into())),
            };
            if let Err(err) = written {
                if let Err(abort_err) = staged.abort().await {
                    tracing::warn!(err = %abort_err, "failed to abort staged upload");
                }
                return Err(err);
            }
        }
        staged.commit().await
    }

    /// Read a blob as a stream.
    pub async fn get(&self, hash: &ContentHash) -> Result<Blob> {
        let result = self.store.get(&hash.key()).await?;
        let size = result.meta.size;
        let stream = result.into_stream().map_err(StorageError::from).boxed();
        Ok(Blob { size, stream })
    }

    /// Read a whole blob into memory (small files only).
    pub async fn get_bytes(&self, hash: &ContentHash) -> Result<Bytes> {
        Ok(self.store.get(&hash.key()).await?.bytes().await?)
    }

    pub async fn exists(&self, hash: &ContentHash) -> Result<bool> {
        match self.store.head(&hash.key()).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(err) => Err(err.into()),
        }
    }

    /// Delete a blob. Deleting a missing blob is not an error (idempotent).
    /// Callers must make sure no file row still references it.
    pub async fn delete(&self, hash: &ContentHash) -> Result<()> {
        match self.store.delete(&hash.key()).await {
            Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(err) => Err(err.into()),
        }
    }

    /// Store a thumbnail of the blob `hash`, `size` pixels on its longer side, at
    /// `thumbs/ab/cd/<hash>/<size>`. Overwriting is harmless (same input, same output).
    pub async fn put_thumbnail(&self, hash: &ContentHash, size: u32, bytes: Bytes) -> Result<()> {
        self.store
            .put(&thumb_key(hash, size), PutPayload::from_bytes(bytes))
            .await?;
        Ok(())
    }

    /// A stored thumbnail, or `None` when there is none (yet).
    pub async fn get_thumbnail(&self, hash: &ContentHash, size: u32) -> Result<Option<Bytes>> {
        match self.store.get(&thumb_key(hash, size)).await {
            Ok(result) => Ok(Some(result.bytes().await?)),
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    /// Delete every thumbnail of the blob `hash` (idempotent). Called when the blob
    /// itself is deleted.
    pub async fn delete_thumbnails(&self, hash: &ContentHash) -> Result<()> {
        let mut listing = self.store.list(Some(&hash.thumb_dir()));
        while let Some(meta) = listing.next().await {
            match self.store.delete(&meta?.location).await {
                Ok(()) | Err(object_store::Error::NotFound { .. }) => {}
                Err(err) => return Err(err.into()),
            }
        }
        Ok(())
    }

    /// The hash of every stored blob (for the orphan sweep). Objects under `blobs/`
    /// that are not valid blob keys are skipped.
    pub fn list_blobs(&self) -> BoxStream<'static, Result<ContentHash>> {
        self.store
            .list(Some(&Path::from(hash::BLOB_PREFIX)))
            .filter_map(|meta| async move {
                match meta {
                    Err(err) => Some(Err(StorageError::from(err))),
                    Ok(meta) => {
                        let hash: ContentHash = meta.location.filename()?.parse().ok()?;
                        (hash.key() == meta.location).then_some(Ok(hash))
                    }
                }
            })
            .boxed()
    }

    /// Remove staged uploads older than `older_than` (left behind by crashes or
    /// dropped [`StagedBlob`]s). Returns how many were removed.
    pub async fn prune_staging(&self, older_than: Duration) -> Result<usize> {
        let cutoff = std::time::SystemTime::now()
            .checked_sub(older_than)
            .unwrap_or(std::time::UNIX_EPOCH);
        let prefix = Path::from(staging::STAGING_PREFIX);
        let mut listing = self.store.list(Some(&prefix));
        let mut removed = 0;
        while let Some(meta) = listing.next().await {
            let meta = meta?;
            if std::time::SystemTime::from(meta.last_modified) < cutoff {
                match self.store.delete(&meta.location).await {
                    Ok(()) | Err(object_store::Error::NotFound { .. }) => removed += 1,
                    Err(err) => return Err(err.into()),
                }
            }
        }
        Ok(removed)
    }
}

fn thumb_key(hash: &ContentHash, size: u32) -> Path {
    hash.thumb_dir().join(size.to_string())
}

#[cfg(test)]
mod tests;
