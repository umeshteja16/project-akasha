use std::io;

use futures_util::stream;

use super::*;

fn backends() -> Vec<(&'static str, Storage, Option<tempfile::TempDir>)> {
    let dir = tempfile::tempdir().expect("tempdir");
    let local = Storage::local(dir.path()).expect("local store");
    vec![
        ("memory", Storage::in_memory(), None),
        ("local", local, Some(dir)),
    ]
}

async fn read_all(storage: &Storage, hash: &ContentHash) -> Vec<u8> {
    let blob = storage.get(hash).await.expect("get");
    let chunks: Vec<Bytes> = blob.stream.try_collect().await.expect("stream");
    let out = chunks.concat();
    assert_eq!(out.len() as u64, blob.size);
    out
}

#[tokio::test]
async fn put_bytes_round_trips_and_dedupes() {
    for (name, storage, _dir) in backends() {
        let first = storage.put_bytes(&b"hello world"[..]).await.expect(name);
        assert_eq!(first.size, 11);
        assert!(!first.deduplicated, "{name}");
        assert_eq!(first.hash, ContentHash::of(b"hello world"));

        let again = storage.put_bytes(&b"hello world"[..]).await.expect(name);
        assert!(again.deduplicated, "{name}");
        assert_eq!(again.hash, first.hash);

        assert_eq!(read_all(&storage, &first.hash).await, b"hello world");
        assert_eq!(
            storage.get_bytes(&first.hash).await.expect(name),
            &b"hello world"[..]
        );
    }
}

#[tokio::test]
async fn streaming_upload_matches_buffered_hash() {
    // Larger than one 5 MiB multipart chunk, so several parts are uploaded.
    let data: Vec<u8> = (0..6 * 1024 * 1024 + 123)
        .map(|i| (i % 251) as u8)
        .collect();
    for (name, storage, _dir) in backends() {
        let chunks: Vec<Result<Bytes, io::Error>> = data
            .chunks(64 * 1024)
            .map(|c| Ok(Bytes::copy_from_slice(c)))
            .collect();
        let info = storage.put_stream(stream::iter(chunks)).await.expect(name);
        assert_eq!(info.hash, ContentHash::of(&data), "{name}");
        assert_eq!(info.size, data.len() as u64);
        assert!(!info.deduplicated);
        assert_eq!(read_all(&storage, &info.hash).await, data, "{name}");

        let dup = storage
            .put_stream(stream::iter([Ok::<_, io::Error>(Bytes::from(
                data.clone(),
            ))]))
            .await
            .expect(name);
        assert!(dup.deduplicated, "{name}");
        assert_eq!(storage.prune_staging(Duration::ZERO).await.expect(name), 0);
    }
}

#[tokio::test]
async fn empty_blob_is_storable() {
    for (name, storage, _dir) in backends() {
        let info = storage
            .put_stream(stream::empty::<Result<Bytes, io::Error>>())
            .await
            .expect(name);
        assert_eq!(info.size, 0);
        assert_eq!(info.hash, ContentHash::of(b""));
        assert!(storage.exists(&info.hash).await.expect(name));
    }
}

#[tokio::test]
async fn failed_stream_leaves_nothing_behind() {
    for (name, storage, _dir) in backends() {
        let chunks = vec![
            Ok(Bytes::from_static(b"partial")),
            Err(io::Error::other("client went away")),
        ];
        let err = storage.put_stream(stream::iter(chunks)).await;
        assert!(matches!(err, Err(StorageError::Source(_))), "{name}");
        assert!(
            !storage
                .exists(&ContentHash::of(b"partial"))
                .await
                .expect(name)
        );
        assert_eq!(storage.prune_staging(Duration::ZERO).await.expect(name), 0);
    }
}

#[tokio::test]
async fn staged_blob_reports_size_and_abort_cleans_up() {
    for (name, storage, _dir) in backends() {
        let mut staged = storage.stage().await.expect(name);
        staged.write(Bytes::from_static(b"abc")).await.expect(name);
        staged.write(Bytes::from_static(b"def")).await.expect(name);
        assert_eq!(staged.size(), 6);
        staged.abort().await.expect(name);
        assert!(
            !storage
                .exists(&ContentHash::of(b"abcdef"))
                .await
                .expect(name)
        );
    }
}

#[tokio::test]
async fn abandoned_staging_is_pruned() {
    for (name, storage, _dir) in backends() {
        let mut staged = storage.stage().await.expect(name);
        staged
            .write(Bytes::from_static(b"left behind"))
            .await
            .expect(name);
        drop(staged);
        // Write a complete stray object too (as if a commit crashed before the move).
        storage
            .store
            .put(&Path::from("staging/stray"), PutPayload::from_static(b"x"))
            .await
            .expect(name);
        assert_eq!(
            storage
                .prune_staging(Duration::from_secs(3600))
                .await
                .expect(name),
            0
        );
        assert_eq!(storage.prune_staging(Duration::ZERO).await.expect(name), 1);
    }
}

#[tokio::test]
async fn delete_is_idempotent_and_missing_blobs_are_not_found() {
    for (name, storage, _dir) in backends() {
        let info = storage.put_bytes(&b"bye"[..]).await.expect(name);
        storage.delete(&info.hash).await.expect(name);
        storage.delete(&info.hash).await.expect(name);
        assert!(!storage.exists(&info.hash).await.expect(name));
        assert!(matches!(
            storage.get(&info.hash).await,
            Err(StorageError::NotFound)
        ));
    }
}

#[test]
fn local_backend_uses_sharded_paths_on_disk() {
    let dir = tempfile::tempdir().expect("tempdir");
    let storage = Storage::local(dir.path()).expect("local");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let info = rt.block_on(storage.put_bytes(&b"hello"[..])).expect("put");
    let hex = info.hash.to_hex();
    let on_disk = dir
        .path()
        .join("blobs")
        .join(&hex[0..2])
        .join(&hex[2..4])
        .join(&hex);
    assert_eq!(std::fs::read(on_disk).expect("blob file"), b"hello");
}

#[test]
fn config_selects_backend() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut config = akasha_core::Config {
        storage_dir: dir.path().join("nested").display().to_string(),
        ..Default::default()
    };
    Storage::from_config(&config).expect("local from config");
    assert!(dir.path().join("nested").is_dir());

    config.storage_backend = akasha_core::StorageBackend::S3;
    assert!(matches!(
        Storage::from_config(&config),
        Err(StorageError::Config(_))
    ));

    config.storage_s3_bucket = Some("akasha".into());
    config.storage_s3_region = Some("us-east-1".into());
    config.storage_s3_endpoint = Some("http://localhost:9000".into());
    config.storage_s3_allow_http = true;
    config.storage_s3_access_key_id = Some("key".into());
    config.storage_s3_secret_access_key = Some(akasha_core::Secret::new("secret"));
    Storage::from_config(&config).expect("s3 from config (no network needed)");
}
