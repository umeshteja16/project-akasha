//! SHA-256 content hashes and their storage keys.

use std::{fmt, str::FromStr};

use object_store::path::Path;
use sha2::{Digest, Sha256};

use crate::StorageError;

/// The SHA-256 of a blob's contents: its identity in storage.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Hash a complete buffer.
    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    pub fn from_digest(digest: Sha256) -> Self {
        Self(digest.finalize().into())
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Lowercase hex, 64 characters.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// The object key: `blobs/ab/cd/abcd…` (two shard levels keep directories small).
    pub(crate) fn key(&self) -> Path {
        let hex = self.to_hex();
        Path::from_iter(["blobs", &hex[0..2], &hex[2..4], &hex])
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({})", self.to_hex())
    }
}

/// Parses exactly 64 hex characters (either case). Anything else is rejected, so a
/// parsed hash is always safe to turn into a storage key.
impl FromStr for ContentHash {
    type Err = StorageError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut out = [0u8; 32];
        if s.len() != 64 || hex::decode_to_slice(s, &mut out).is_err() {
            return Err(StorageError::InvalidHash);
        }
        Ok(Self(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip_and_sharded_key() {
        let hash = ContentHash::of(b"hello");
        let hex = hash.to_hex();
        assert_eq!(
            hex,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert_eq!(hex.parse::<ContentHash>().ok(), Some(hash));
        assert_eq!(hash.key().as_ref(), format!("blobs/2c/f2/{hex}").as_str());
    }

    #[test]
    fn rejects_malformed_hashes() {
        for bad in [
            "",
            "abc",
            "../../etc/passwd",
            &"g".repeat(64),
            &"a".repeat(65),
        ] {
            assert!(bad.parse::<ContentHash>().is_err(), "{bad}");
        }
        assert!("A".repeat(64).parse::<ContentHash>().is_ok());
    }
}
