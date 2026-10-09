//! OCR model files: found in a models directory, downloaded on first use, and
//! always checked against pinned SHA-256 digests before loading.
//!
//! Blocking (file and network I/O): call from a blocking thread. For offline
//! installs, put the files in the directory yourself and pass an empty base URL.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

use sha2::{Digest, Sha256};

use crate::{IngestError, Ocr};

/// One model file and its expected digest.
#[derive(Debug, Clone, Copy)]
pub struct ModelFile {
    pub name: &'static str,
    /// SHA-256, lowercase hex.
    pub sha256: &'static str,
}

/// Where the ocrs project publishes its models.
pub const DEFAULT_BASE_URL: &str = "https://ocrs-models.s3-accelerate.amazonaws.com";

/// Text detection model (ocrs-models, ~2.5 MB).
pub const DETECTION: ModelFile = ModelFile {
    name: "text-detection.rten",
    sha256: "f15cfb56bd02c4bf478a20343986504a1f01e1665c2b3a0ad66340f054b1b5ca",
};

/// Text recognition model (ocrs-models, ~9.7 MB).
pub const RECOGNITION: ModelFile = ModelFile {
    name: "text-recognition.rten",
    sha256: "e484866d4cce403175bd8d00b128feb08ab42e208de30e42cd9889d8f1735a6e",
};

/// Refuse downloads larger than this (both models are far smaller).
const MAX_DOWNLOAD: u64 = 64 * 1024 * 1024;

/// Ensure both OCR models are in `dir` (downloading from `base_url` if missing or
/// corrupt; an empty `base_url` means never download) and load the engine.
pub fn load_ocr(dir: &Path, base_url: &str) -> Result<Ocr, IngestError> {
    let detection = ensure(dir, base_url, &DETECTION)?;
    let recognition = ensure(dir, base_url, &RECOGNITION)?;
    Ocr::from_models(detection, recognition)
}

/// The verified bytes of `model`, fetching it into `dir` first if needed.
pub fn ensure(dir: &Path, base_url: &str, model: &ModelFile) -> Result<Vec<u8>, IngestError> {
    let path = dir.join(model.name);
    match fs::read(&path) {
        Ok(bytes) if digest(&bytes) == model.sha256 => return Ok(bytes),
        Ok(_) => tracing::warn!(path = %path.display(), "model checksum mismatch; fetching again"),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(models_error(format!("reading {}: {err}", path.display()))),
    }
    if base_url.is_empty() {
        return Err(models_error(format!(
            "{} is missing or invalid and downloads are disabled",
            path.display()
        )));
    }
    let bytes = download(&format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        model.name
    ))?;
    let got = digest(&bytes);
    if got != model.sha256 {
        return Err(models_error(format!(
            "downloaded {} has SHA-256 {got}, expected {}",
            model.name, model.sha256
        )));
    }
    write_atomically(dir, &path, &bytes)?;
    tracing::info!(path = %path.display(), "downloaded OCR model");
    Ok(bytes)
}

fn models_error(msg: String) -> IngestError {
    IngestError::Models(msg)
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn download(url: &str) -> Result<Vec<u8>, IngestError> {
    tracing::info!(%url, "downloading OCR model");
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| models_error(e.to_string()))?;
    let response = client
        .get(url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|e| models_error(format!("downloading {url}: {e}")))?;
    let mut bytes = Vec::new();
    response
        .take(MAX_DOWNLOAD + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| models_error(format!("downloading {url}: {e}")))?;
    if bytes.len() as u64 > MAX_DOWNLOAD {
        return Err(models_error(format!("{url} is larger than expected")));
    }
    Ok(bytes)
}

/// Write via a temporary file and rename, so a crash never leaves a half-written
/// model under the real name (and concurrent writers are harmless).
fn write_atomically(dir: &Path, path: &Path, bytes: &[u8]) -> Result<(), IngestError> {
    let io = |e: std::io::Error| models_error(format!("saving to {}: {e}", dir.display()));
    fs::create_dir_all(dir).map_err(io)?;
    let tmp: PathBuf = dir.join(format!(".download-{}", uuid::Uuid::new_v4().simple()));
    fs::write(&tmp, bytes).map_err(io)?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        io(e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY: ModelFile = ModelFile {
        name: "tiny.rten",
        // SHA-256 of b"model".
        sha256: "f3ab5a9e5c7c3bca7d41d5f2ef3e7d8e19fd2f9c1d8a7a0dbd9b9b5b3f2a1c0e",
    };

    #[test]
    fn present_and_valid_files_are_used_without_network() {
        let dir = tempfile::tempdir().expect("tempdir");
        let model = ModelFile {
            sha256: Box::leak(digest(b"model").into_boxed_str()),
            ..TINY
        };
        fs::write(dir.path().join(model.name), b"model").expect("write");
        assert_eq!(ensure(dir.path(), "", &model).expect("ok"), b"model");
    }

    #[test]
    fn missing_or_corrupt_files_fail_when_downloads_are_disabled() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = ensure(dir.path(), "", &TINY).expect_err("missing");
        assert!(!err.is_permanent(), "model problems are retryable");
        fs::write(dir.path().join(TINY.name), b"tampered").expect("write");
        let err = ensure(dir.path(), "", &TINY).expect_err("bad checksum");
        assert!(err.to_string().contains("missing or invalid"));
    }

    #[test]
    fn atomic_write_leaves_no_temp_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("sub").join("m.rten");
        write_atomically(&dir.path().join("sub"), &target, b"x").expect("write");
        let names: Vec<_> = fs::read_dir(dir.path().join("sub"))
            .expect("list")
            .map(|e| e.expect("entry").file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("m.rten")]);
    }
}
