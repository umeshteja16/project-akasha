//! Streaming uploads whose hash is only known once the last byte arrives.
//!
//! Bytes are written to `staging/<uuid>` while being hashed, then moved to their
//! content-addressed key on [`StagedBlob::commit`]. If the blob already exists the
//! staged copy is simply deleted (deduplication).

use std::sync::Arc;

use bytes::Bytes;
use object_store::{ObjectStore, ObjectStoreExt, WriteMultipart, path::Path};
use sha2::{Digest, Sha256};

use crate::{BlobInfo, ContentHash, Result};

/// How many multipart chunks may be in flight before `write` waits.
const MAX_IN_FLIGHT: usize = 4;

pub(crate) const STAGING_PREFIX: &str = "staging";

/// An upload in progress. Call [`commit`](Self::commit) or [`abort`](Self::abort);
/// dropping it leaves a staged object behind for [`Storage::prune_staging`](crate::Storage::prune_staging).
pub struct StagedBlob {
    store: Arc<dyn ObjectStore>,
    path: Path,
    writer: WriteMultipart,
    hasher: Sha256,
    size: u64,
}

impl StagedBlob {
    pub(crate) async fn start(store: Arc<dyn ObjectStore>) -> Result<Self> {
        let path = Path::from_iter([STAGING_PREFIX, &uuid::Uuid::new_v4().to_string()]);
        let upload = store.put_multipart(&path).await?;
        Ok(Self {
            store,
            path,
            writer: WriteMultipart::new(upload),
            hasher: Sha256::new(),
            size: 0,
        })
    }

    /// Append a chunk. Applies backpressure when the backend falls behind.
    pub async fn write(&mut self, chunk: Bytes) -> Result<()> {
        if chunk.is_empty() {
            return Ok(());
        }
        self.writer.wait_for_capacity(MAX_IN_FLIGHT).await?;
        self.hasher.update(&chunk);
        self.size += chunk.len() as u64;
        self.writer.put(chunk);
        Ok(())
    }

    /// Bytes written so far (use it to enforce size limits while streaming).
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Finish the upload and move it to its content-addressed key.
    pub async fn commit(self) -> Result<BlobInfo> {
        let Self {
            store,
            path,
            writer,
            hasher,
            size,
        } = self;
        writer.finish().await?;
        let hash = ContentHash::from_digest(hasher);
        let key = hash.key();

        let existed = match store.head(&key).await {
            Ok(_) => true,
            Err(object_store::Error::NotFound { .. }) => false,
            Err(err) => {
                discard(&store, &path).await;
                return Err(err.into());
            }
        };
        if existed {
            discard(&store, &path).await;
        } else if let Err(err) = store.rename(&path, &key).await {
            // Same content is safe to overwrite, so a racing commit of identical bytes is fine.
            discard(&store, &path).await;
            return Err(err.into());
        }
        Ok(BlobInfo {
            hash,
            size,
            deduplicated: existed,
        })
    }

    /// Abandon the upload and remove anything already written.
    pub async fn abort(self) -> Result<()> {
        self.writer.abort().await?;
        discard(&self.store, &self.path).await;
        Ok(())
    }
}

async fn discard(store: &Arc<dyn ObjectStore>, path: &Path) {
    match store.delete(path).await {
        Ok(()) | Err(object_store::Error::NotFound { .. }) => {}
        Err(err) => tracing::warn!(%err, %path, "failed to delete staged upload"),
    }
}
