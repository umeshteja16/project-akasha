//! Backend construction from configuration.

use std::sync::Arc;

use akasha_core::{Config, StorageBackend};
use object_store::{ObjectStore, aws::AmazonS3Builder, local::LocalFileSystem};

use crate::{Result, StorageError};

pub(crate) fn build(config: &Config) -> Result<Arc<dyn ObjectStore>> {
    match config.storage_backend {
        StorageBackend::Local => local(std::path::Path::new(&config.storage_dir)),
        StorageBackend::S3 => s3(config),
    }
}

pub(crate) fn local(dir: &std::path::Path) -> Result<Arc<dyn ObjectStore>> {
    std::fs::create_dir_all(dir).map_err(|err| {
        StorageError::Config(format!(
            "cannot create storage dir {}: {err}",
            dir.display()
        ))
    })?;
    let store = LocalFileSystem::new_with_prefix(dir)
        .map_err(|err| StorageError::Config(format!("storage dir {}: {err}", dir.display())))?
        // Remove empty shard directories after deletes.
        .with_automatic_cleanup(true);
    Ok(Arc::new(store))
}

fn s3(config: &Config) -> Result<Arc<dyn ObjectStore>> {
    let bucket = config.storage_s3_bucket.as_deref().ok_or_else(|| {
        StorageError::Config("AKASHA_STORAGE_S3_BUCKET is required for the s3 backend".into())
    })?;
    // Start from the standard AWS_* variables, then apply explicit Akasha settings.
    let mut builder = AmazonS3Builder::from_env()
        .with_bucket_name(bucket)
        .with_allow_http(config.storage_s3_allow_http);
    if let Some(region) = &config.storage_s3_region {
        builder = builder.with_region(region);
    }
    if let Some(endpoint) = &config.storage_s3_endpoint {
        builder = builder.with_endpoint(endpoint);
    }
    if let Some(key) = &config.storage_s3_access_key_id {
        builder = builder.with_access_key_id(key);
    }
    if let Some(secret) = &config.storage_s3_secret_access_key {
        builder = builder.with_secret_access_key(secret.expose());
    }
    let store = builder
        .build()
        .map_err(|err| StorageError::Config(format!("s3: {err}")))?;
    Ok(Arc::new(store))
}
