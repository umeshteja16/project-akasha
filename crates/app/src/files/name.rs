//! Filename sanitising and safe `Content-Disposition` headers (RFC 6266 / RFC 8187).

use std::fmt::Write as _;

/// Longest stored name, in bytes.
pub const MAX_NAME_BYTES: usize = 255;
const FALLBACK: &str = "unnamed";

/// Characters that reorder or hide text (bidi overrides, zero-width marks): a classic
/// trick to make `evil\u{202e}fdp.exe` render as `evilexe.pdf`.
fn is_invisible(c: char) -> bool {
    matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}')
}

/// Turn a client-supplied filename into a safe display name: no directories, no
/// control or invisible characters, no `..`, at most 255 bytes (keeping the
/// extension). Never returns an empty string.
pub fn sanitize(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or_default();
    let mut name: String = base
        .chars()
        .filter(|c| !c.is_control() && !is_invisible(*c))
        .collect();
    while name.contains("..") {
        name = name.replace("..", ".");
    }
    let name = name.trim().trim_matches('.').trim();
    if name.is_empty() {
        return FALLBACK.to_owned();
    }
    truncate(name)
}

fn truncate(name: &str) -> String {
    if name.len() <= MAX_NAME_BYTES {
        return name.to_owned();
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && ext.len() <= 16 => (stem, Some(ext)),
        _ => (name, None),
    };
    let budget = MAX_NAME_BYTES - ext.map_or(0, |e| e.len() + 1);
    let mut cut = budget.min(stem.len());
    while !stem.is_char_boundary(cut) {
        cut -= 1;
    }
    match ext {
        Some(ext) => format!("{}.{ext}", &stem[..cut]),
        None => stem[..cut].to_owned(),
    }
}

/// Validate a new name supplied by the user on rename.
pub fn validate_rename(raw: &str) -> Result<String, akasha_core::Error> {
    if raw.trim().is_empty() {
        return Err(akasha_core::Error::bad_request("name must not be empty"));
    }
    Ok(sanitize(raw))
}

/// `attachment; filename="<ascii fallback>"; filename*=UTF-8''<percent-encoded>`.
pub fn content_disposition(name: &str) -> String {
    let fallback: String = name
        .chars()
        .map(|c| {
            if c.is_ascii() && !c.is_ascii_control() && c != '"' && c != '\\' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut encoded = String::with_capacity(name.len() * 3);
    for byte in name.bytes() {
        // RFC 8187 attr-char: ALPHA / DIGIT / "!#$&+-.^_`|~"
        if byte.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    format!("attachment; filename=\"{fallback}\"; filename*=UTF-8''{encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_paths_traversal_and_control_characters() {
        assert_eq!(sanitize("../../etc/passwd"), "passwd");
        assert_eq!(sanitize("C:\\Users\\me\\report.pdf"), "report.pdf");
        assert_eq!(sanitize(".."), "unnamed");
        assert_eq!(sanitize("..."), "unnamed");
        assert_eq!(sanitize(""), "unnamed");
        assert_eq!(sanitize("  /  "), "unnamed");
        assert_eq!(sanitize("a\0b\r\nc.txt"), "abc.txt");
        assert_eq!(sanitize("notes..txt"), "notes.txt");
        assert_eq!(sanitize("evil\u{202e}fdp.exe"), "evilfdp.exe");
        assert_eq!(sanitize(".hidden.md"), "hidden.md");
    }

    /// Ported from the old implementation's filename upload script: special characters are kept.
    #[test]
    fn keeps_ordinary_special_characters() {
        let name = "Digital Product Design & Development Agency - Significa.md";
        assert_eq!(sanitize(name), name);
        assert_eq!(sanitize("résumé (final) #2.pdf"), "résumé (final) #2.pdf");
    }

    #[test]
    fn caps_length_keeping_the_extension() {
        let long = format!("{}.pdf", "é".repeat(300));
        let out = sanitize(&long);
        assert!(out.len() <= MAX_NAME_BYTES);
        assert!(out.ends_with("é.pdf"));
        assert!(sanitize(&"x".repeat(1000)).len() == MAX_NAME_BYTES);
    }

    #[test]
    fn disposition_is_header_safe() {
        assert_eq!(
            content_disposition("a b.pdf"),
            "attachment; filename=\"a b.pdf\"; filename*=UTF-8''a%20b.pdf"
        );
        let tricky = content_disposition("x\";evil=1 \u{e9}.txt");
        assert!(tricky.starts_with("attachment; filename=\"x_;evil=1 _.txt\";"));
        assert!(tricky.ends_with("x%22%3Bevil%3D1%20%C3%A9.txt"));
        assert!(axum::http::HeaderValue::from_str(&tricky).is_ok());
    }
}
