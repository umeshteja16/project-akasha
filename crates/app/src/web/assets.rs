//! Where the built web UI comes from: embedded at compile time (feature `embed-ui`),
//! an in-memory set (tests), or nowhere (API-only builds and development).

use std::{borrow::Cow, collections::HashMap};

use sha2::{Digest, Sha256};

/// One file of the built UI.
pub struct Asset {
    pub bytes: Cow<'static, [u8]>,
    /// Strong validator: hex SHA-256 of the content, quoted.
    pub etag: String,
}

/// The built UI (`web/dist`), looked up by path relative to its root.
pub struct WebAssets {
    source: Source,
}

enum Source {
    None,
    #[cfg(feature = "embed-ui")]
    Embedded,
    Memory(HashMap<String, Vec<u8>>),
}

#[cfg(feature = "embed-ui")]
#[derive(rust_embed::RustEmbed)]
#[folder = "../../web/dist"]
struct Dist;

impl WebAssets {
    /// The UI compiled into this binary, if it was built with `embed-ui`.
    pub fn embedded() -> Self {
        #[cfg(feature = "embed-ui")]
        {
            let source = if Dist::get("index.html").is_some() {
                Source::Embedded
            } else {
                tracing::warn!("built without web/dist/index.html: serving the API only");
                Source::None
            };
            Self { source }
        }
        #[cfg(not(feature = "embed-ui"))]
        {
            Self::none()
        }
    }

    /// No UI: every non-API path is a JSON 404.
    pub fn none() -> Self {
        Self {
            source: Source::None,
        }
    }

    /// A UI made of the given `(path, bytes)` pairs, e.g. for tests.
    pub fn from_files<P: Into<String>>(files: impl IntoIterator<Item = (P, Vec<u8>)>) -> Self {
        Self {
            source: Source::Memory(files.into_iter().map(|(p, b)| (p.into(), b)).collect()),
        }
    }

    /// Whether there is a UI to serve (it always has an `index.html`).
    pub fn is_available(&self) -> bool {
        self.get("index.html").is_some()
    }

    pub fn get(&self, path: &str) -> Option<Asset> {
        match &self.source {
            Source::None => None,
            #[cfg(feature = "embed-ui")]
            Source::Embedded => Dist::get(path).map(|file| Asset {
                etag: quoted_hex(&file.metadata.sha256_hash()),
                bytes: file.data,
            }),
            Source::Memory(files) => files.get(path).map(|bytes| Asset {
                etag: quoted_hex(&Sha256::digest(bytes)),
                bytes: Cow::Owned(bytes.clone()),
            }),
        }
    }
}

fn quoted_hex(digest: &[u8]) -> String {
    let mut out = String::with_capacity(digest.len() * 2 + 2);
    out.push('"');
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out.push('"');
    out
}
