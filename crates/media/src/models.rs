//! Whisper model files (whisper.cpp GGML format): a small catalog with pinned
//! SHA-256 digests, found in a models directory or downloaded on first use.
//!
//! Downloads stream to a temporary file while hashing (the larger models are
//! over a gigabyte), and only a verified file is renamed into place. Blocking
//! (file and network I/O): call from a blocking thread.

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use sha2::{Digest, Sha256};

use crate::{MediaError, fake::FAKE_MODEL};

/// A downloadable Whisper model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhisperModel {
    /// Name used in `AKASHA_WHISPER_MODEL`.
    pub name: &'static str,
    /// File name in the `ggerganov/whisper.cpp` repository.
    pub file: &'static str,
    /// SHA-256, lowercase hex.
    pub sha256: &'static str,
    /// Approximate download size, for messages.
    pub size_mb: u32,
}

/// Multilingual models (they also detect the language). `base` is the default:
/// fast on a CPU and good enough for clear speech; `small` is noticeably better
/// and ~3x slower; `large-v3-turbo` is best and needs a fast machine.
pub const CATALOG: &[WhisperModel] = &[
    WhisperModel {
        name: "tiny",
        file: "ggml-tiny.bin",
        sha256: "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21",
        size_mb: 75,
    },
    WhisperModel {
        name: "base",
        file: "ggml-base.bin",
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
        size_mb: 142,
    },
    WhisperModel {
        name: "small",
        file: "ggml-small.bin",
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        size_mb: 466,
    },
    WhisperModel {
        name: "medium",
        file: "ggml-medium.bin",
        sha256: "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208",
        size_mb: 1500,
    },
    WhisperModel {
        name: "large-v3-turbo",
        file: "ggml-large-v3-turbo.bin",
        sha256: "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
        size_mb: 1600,
    },
];

/// Hugging Face repository the files come from.
pub const REPOSITORY: &str = "ggerganov/whisper.cpp";

/// What `AKASHA_WHISPER_MODEL` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelChoice {
    /// The deterministic test transcriber ([`crate::ToneTranscriber`]).
    Fake,
    Whisper(&'static WhisperModel),
}

/// Look up a model by name (`base`, `small`, ..., or `fake`).
pub fn find(name: &str) -> Result<ModelChoice, MediaError> {
    let name = name.trim();
    if name == FAKE_MODEL {
        return Ok(ModelChoice::Fake);
    }
    CATALOG
        .iter()
        .find(|m| m.name.eq_ignore_ascii_case(name))
        .map(ModelChoice::Whisper)
        .ok_or_else(|| {
            let known: Vec<_> = CATALOG.iter().map(|m| m.name).collect();
            MediaError::Models(format!(
                "unknown Whisper model {name:?}; choose one of {}",
                known.join(", ")
            ))
        })
}

/// Where `model` lives under the models directory.
pub fn path(dir: &Path, model: &WhisperModel) -> PathBuf {
    dir.join("whisper").join(model.file)
}

/// The path of a verified copy of `model` under `dir`, downloading it from the
/// Hugging Face compatible `endpoint` first if it is missing or corrupt (an empty
/// endpoint means never download).
pub fn ensure(dir: &Path, endpoint: &str, model: &WhisperModel) -> Result<PathBuf, MediaError> {
    let target = path(dir, model);
    match digest_file(&target) {
        Ok(digest) if digest == model.sha256 => return Ok(target),
        Ok(_) => {
            tracing::warn!(path = %target.display(), "model checksum mismatch; fetching again")
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(MediaError::Models(format!(
                "reading {}: {err}",
                target.display()
            )));
        }
    }
    if endpoint.is_empty() {
        return Err(MediaError::Models(format!(
            "{} is missing or invalid and downloads are disabled",
            target.display()
        )));
    }
    let url = format!(
        "{}/{REPOSITORY}/resolve/main/{}",
        endpoint.trim_end_matches('/'),
        model.file
    );
    download(&url, &target, model)?;
    Ok(target)
}

fn digest_file(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

fn download(url: &str, target: &Path, model: &WhisperModel) -> Result<(), MediaError> {
    tracing::info!(%url, size_mb = model.size_mb, "downloading Whisper model");
    let err = |e: &dyn std::fmt::Display| MediaError::Models(format!("downloading {url}: {e}"));
    let dir = target.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir).map_err(|e| err(&e))?;
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(4 * 60 * 60))
        .build()
        .map_err(|e| err(&e))?;
    let mut response = client
        .get(url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|e| err(&e))?;
    let tmp = dir.join(format!(".download-{}", uuid::Uuid::new_v4().simple()));
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 1 << 20];
        // Refuse anything far larger than the catalog says.
        let limit = u64::from(model.size_mb) * 1024 * 1024 * 2;
        let mut total = 0u64;
        loop {
            let n = response.read(&mut buf)?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > limit {
                return Err(std::io::Error::other("larger than expected"));
            }
            hasher.update(&buf[..n]);
            file.write_all(&buf[..n])?;
        }
        file.sync_all()?;
        Ok(hex::encode(hasher.finalize()))
    })();
    let digest = match result {
        Ok(digest) => digest,
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            return Err(err(&e));
        }
    };
    if digest != model.sha256 {
        let _ = fs::remove_file(&tmp);
        return Err(MediaError::Models(format!(
            "downloaded {} has SHA-256 {digest}, expected {}",
            model.file, model.sha256
        )));
    }
    fs::rename(&tmp, target).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        err(&e)
    })?;
    tracing::info!(path = %target.display(), "downloaded Whisper model");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_models_by_name() {
        assert_eq!(find("fake").expect("fake"), ModelChoice::Fake);
        let ModelChoice::Whisper(m) = find("Base").expect("base") else {
            panic!("not whisper");
        };
        assert_eq!(m.file, "ggml-base.bin");
        assert!(find("huge").is_err());
        for m in CATALOG {
            assert_eq!(m.sha256.len(), 64);
            assert!(m.sha256.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn verified_files_are_used_and_missing_ones_fail_offline() {
        let dir = tempfile::tempdir().expect("tempdir");
        let model = WhisperModel {
            name: "test",
            file: "ggml-test.bin",
            sha256: Box::leak(hex::encode(Sha256::digest(b"weights")).into_boxed_str()),
            size_mb: 1,
        };
        let err = ensure(dir.path(), "", &model).expect_err("missing");
        assert!(!err.is_permanent());
        fs::create_dir_all(dir.path().join("whisper")).expect("mkdir");
        fs::write(path(dir.path(), &model), b"tampered").expect("write");
        assert!(ensure(dir.path(), "", &model).is_err(), "bad checksum");
        fs::write(path(dir.path(), &model), b"weights").expect("write");
        assert_eq!(
            ensure(dir.path(), "", &model).expect("ok"),
            path(dir.path(), &model)
        );
    }
}
