//! Sort orders of the file list and their opaque keyset cursors.

use akasha_db::files::{File, ListKey, ListOrder};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::DateTime;
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use akasha_core::Error;

/// Sort order of the file list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum FileSort {
    /// Most recently uploaded first.
    #[default]
    Newest,
    Oldest,
    /// By name, A to Z, ignoring case.
    Name,
    /// Largest first.
    Size,
    /// Most recently opened first; only files you have opened.
    Opened,
}

impl FileSort {
    pub fn order(self) -> ListOrder {
        match self {
            Self::Newest => ListOrder::Newest,
            Self::Oldest => ListOrder::Oldest,
            Self::Name => ListOrder::Name,
            Self::Size => ListOrder::Largest,
            Self::Opened => ListOrder::Opened,
        }
    }

    fn tag(self) -> char {
        match self {
            Self::Newest => 'n',
            Self::Oldest => 'o',
            Self::Name => 'a',
            Self::Size => 's',
            Self::Opened => 'r',
        }
    }
}

/// Opaque keyset cursor: base64url of `<sort tag>:<id>:<key>`, where the key is
/// the creation (or last-opened) time in µs, the size in bytes or the name (last, so it may hold `:`).
pub fn encode_cursor(sort: FileSort, file: &File) -> String {
    let key = match ListKey::of(file, sort.order()) {
        ListKey::Created(ts) | ListKey::Opened(ts) => ts.timestamp_micros().to_string(),
        ListKey::Size(size) => size.to_string(),
        ListKey::Name(name) => name,
    };
    URL_SAFE_NO_PAD.encode(format!("{}:{}:{key}", sort.tag(), file.id))
}

/// Decode a cursor made by [`encode_cursor`] for the same `sort`.
pub fn decode_cursor(sort: FileSort, cursor: &str) -> Result<(ListKey, Uuid), Error> {
    let invalid = || Error::bad_request("invalid cursor");
    let raw = URL_SAFE_NO_PAD.decode(cursor).map_err(|_| invalid())?;
    let raw = String::from_utf8(raw).map_err(|_| invalid())?;
    let mut parts = raw.splitn(3, ':');
    let (Some(tag), Some(id), Some(key)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(invalid());
    };
    if tag.chars().ne(std::iter::once(sort.tag())) {
        return Err(Error::bad_request(
            "cursor belongs to a different sort order",
        ));
    }
    let id: Uuid = id.parse().map_err(|_| invalid())?;
    let key = match sort {
        FileSort::Newest | FileSort::Oldest => {
            let micros: i64 = key.parse().map_err(|_| invalid())?;
            ListKey::Created(DateTime::from_timestamp_micros(micros).ok_or_else(invalid)?)
        }
        FileSort::Opened => {
            let micros: i64 = key.parse().map_err(|_| invalid())?;
            ListKey::Opened(DateTime::from_timestamp_micros(micros).ok_or_else(invalid)?)
        }
        FileSort::Size => ListKey::Size(key.parse().map_err(|_| invalid())?),
        FileSort::Name => ListKey::Name(key.to_owned()),
    };
    Ok((key, id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, size: i64) -> File {
        let now = DateTime::from_timestamp_micros(1_760_000_000_123_456).expect("ts");
        File {
            id: Uuid::new_v4(),
            owner_id: Uuid::new_v4(),
            original_name: name.to_owned(),
            content_hash: String::new(),
            mime_type: "text/plain".into(),
            size_bytes: size,
            status: "ready".into(),
            error: None,
            is_pinned: false,
            tags: vec![],
            summary: None,
            auto_tags: vec![],
            enrichment_status: None,
            enrichment_model: None,
            enriched_from: None,
            enriched_at: None,
            created_at: now,
            updated_at: now,
            last_opened_at: Some(now),
            open_count: 1,
        }
    }

    #[test]
    fn cursor_round_trips_per_sort_and_rejects_garbage() {
        let f = file("a:b.txt", 42);
        let cases = [
            (FileSort::Newest, ListKey::Created(f.created_at)),
            (FileSort::Oldest, ListKey::Created(f.created_at)),
            (FileSort::Name, ListKey::Name("a:b.txt".into())),
            (FileSort::Size, ListKey::Size(42)),
            (FileSort::Opened, ListKey::Opened(f.created_at)),
        ];
        for (sort, key) in cases {
            let cursor = encode_cursor(sort, &f);
            assert_eq!(decode_cursor(sort, &cursor).ok(), Some((key, f.id)));
        }
        let newest = encode_cursor(FileSort::Newest, &f);
        assert!(decode_cursor(FileSort::Size, &newest).is_err());
        let bad_id = URL_SAFE_NO_PAD.encode("n:not-a-uuid:1");
        for bad in ["", "!!", "bm9wZQ", &bad_id] {
            assert!(decode_cursor(FileSort::Newest, bad).is_err(), "{bad}");
        }
    }
}
