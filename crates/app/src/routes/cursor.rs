//! Opaque keyset cursor on `(timestamp, id)`: base64url of `<µs>:<id>`.
//! Used by lists that only page newest first (conversations, messages).

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use akasha_core::Error;

pub fn encode_cursor(at: DateTime<Utc>, id: Uuid) -> String {
    URL_SAFE_NO_PAD.encode(format!("{}:{id}", at.timestamp_micros()))
}

pub fn decode_cursor(cursor: &str) -> Result<(DateTime<Utc>, Uuid), Error> {
    let invalid = || Error::bad_request("invalid cursor");
    let raw = URL_SAFE_NO_PAD.decode(cursor).map_err(|_| invalid())?;
    let raw = String::from_utf8(raw).map_err(|_| invalid())?;
    let (micros, id) = raw.split_once(':').ok_or_else(invalid)?;
    let micros: i64 = micros.parse().map_err(|_| invalid())?;
    let at = DateTime::from_timestamp_micros(micros).ok_or_else(invalid)?;
    let id = id.parse().map_err(|_| invalid())?;
    Ok((at, id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_round_trips_and_rejects_garbage() {
        let now = DateTime::from_timestamp_micros(1_760_000_000_123_456).expect("ts");
        let id = Uuid::new_v4();
        assert_eq!(decode_cursor(&encode_cursor(now, id)).ok(), Some((now, id)));
        for bad in ["", "!!", "bm9wZQ", &URL_SAFE_NO_PAD.encode("1:not-a-uuid")] {
            assert!(decode_cursor(bad).is_err(), "{bad}");
        }
    }
}
