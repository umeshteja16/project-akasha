//! Model files: looked up in the models directory, downloaded from a Hugging Face
//! compatible endpoint on first use.
//!
//! Layout: `<models_dir>/hf/<owner>--<repo>/<path in repo>`. Files are streamed to
//! a temporary name and renamed when complete, so a crash never leaves a partial
//! file under the real name. Blocking: call from a blocking thread.
//!
//! Unlike the OCR models, digests are not pinned: upstream repos are mutable and
//! there are many models. Integrity relies on TLS plus a length check (ADR 0009).

use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use crate::{MlError, catalog::ModelFiles};

/// Where models come from by default.
pub const DEFAULT_BASE_URL: &str = "https://huggingface.co";

/// Refuse anything larger (the biggest supported model, bge-m3, is ~2.3 GB).
const MAX_FILE: u64 = 8 * 1024 * 1024 * 1024;

/// The local directory holding `files.repo`.
pub fn repo_dir(models_dir: &Path, files: &ModelFiles) -> PathBuf {
    models_dir.join("hf").join(files.repo.replace('/', "--"))
}

/// Make sure every file of the model is present, downloading missing ones from
/// `base_url` (empty: never download). Returns the repo directory.
pub fn ensure(models_dir: &Path, base_url: &str, files: &ModelFiles) -> Result<PathBuf, MlError> {
    let dir = repo_dir(models_dir, files);
    for name in files.all() {
        let path = dir.join(name);
        match fs::metadata(&path) {
            Ok(meta) if meta.is_file() && meta.len() > 0 => continue,
            Ok(_) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(models(format!("reading {}: {err}", path.display()))),
        }
        if base_url.is_empty() {
            return Err(models(format!(
                "{} is missing and downloads are disabled (run `akasha models download` \
                 on a connected machine and copy the models directory)",
                path.display()
            )));
        }
        let url = format!(
            "{}/{}/resolve/main/{name}",
            base_url.trim_end_matches('/'),
            files.repo
        );
        download(&url, &path)?;
    }
    Ok(dir)
}

fn models(msg: String) -> MlError {
    MlError::Models(msg)
}

fn download(url: &str, path: &Path) -> Result<(), MlError> {
    tracing::info!(%url, "downloading model file");
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        // Whole-request timeout: generous, model files are large.
        .timeout(Duration::from_secs(60 * 60))
        .build()
        .map_err(|e| models(e.to_string()))?;
    let response = client
        .get(url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|e| models(format!("downloading {url}: {e}")))?;
    let expected = response.content_length();
    if expected.is_some_and(|len| len > MAX_FILE) {
        return Err(models(format!("{url} is larger than expected")));
    }
    let written = write_atomically(path, response.take(MAX_FILE + 1))?;
    if written > MAX_FILE || expected.is_some_and(|len| len != written) {
        let _ = fs::remove_file(path);
        return Err(models(format!("{url}: incomplete or oversized download")));
    }
    tracing::info!(path = %path.display(), bytes = written, "downloaded model file");
    Ok(())
}

/// Stream `reader` into `path` via a temporary file; returns the bytes written.
fn write_atomically(path: &Path, mut reader: impl Read) -> Result<u64, MlError> {
    let dir = path
        .parent()
        .ok_or_else(|| models(format!("{} has no parent directory", path.display())))?;
    let io = |e: io::Error| models(format!("saving {}: {e}", path.display()));
    fs::create_dir_all(dir).map_err(io)?;
    let tmp = dir.join(format!(".download-{}", uuid::Uuid::new_v4().simple()));
    let result = (|| {
        let mut file = File::create(&tmp)?;
        let n = io::copy(&mut reader, &mut file)?;
        file.flush()?;
        file.sync_all()?;
        fs::rename(&tmp, path)?;
        Ok(n)
    })();
    result.map_err(|e| {
        let _ = fs::remove_file(&tmp);
        io(e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILES: ModelFiles = ModelFiles {
        repo: "acme/tiny",
        onnx: "onnx/model.onnx",
        external: &[],
    };

    #[test]
    fn present_files_are_used_without_network() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = repo_dir(dir.path(), &FILES);
        for name in FILES.all() {
            write_atomically(&repo.join(name), &b"x"[..]).expect("write");
        }
        assert_eq!(ensure(dir.path(), "", &FILES).expect("present"), repo);
        assert!(repo.ends_with("hf/acme--tiny"));
    }

    #[test]
    fn missing_files_fail_when_downloads_are_disabled() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = ensure(dir.path(), "", &FILES).expect_err("missing");
        assert!(!err.is_permanent());
        assert!(err.to_string().contains("downloads are disabled"));
    }

    #[test]
    fn atomic_write_leaves_no_temp_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("a").join("m.onnx");
        assert_eq!(write_atomically(&target, &b"abc"[..]).expect("write"), 3);
        let names: Vec<_> = fs::read_dir(dir.path().join("a"))
            .expect("list")
            .map(|e| e.expect("entry").file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("m.onnx")]);
    }
}
