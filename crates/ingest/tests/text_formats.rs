//! Plain text, Markdown, CSV and JSON extraction from committed fixtures.

mod support;

use akasha_ingest::{IngestError, Options, extract};
use support::fixture;

fn read(name: &str) -> Vec<u8> {
    std::fs::read(fixture(name)).expect("fixture")
}

#[test]
fn markdown_strips_syntax_and_keeps_headings() {
    let e = extract(&read("notes.md"), "text/markdown", &Options::default()).expect("md");
    assert_eq!(e.extractor, "markdown");
    assert!(e.pages.is_empty());
    assert_eq!(
        e.text,
        "Project Akasha\n\nAkasha is a personal knowledge store. See the docs.\n\nGoals\n\n\
         Find anything you saved\nCite the page it came from\n\njust check\n\n\
         Format | Extractor\nPDF | pdf"
    );
    assert!(
        !e.text.contains("Front matter"),
        "YAML front matter is not content"
    );
    assert!(!e.text.contains("Inline HTML"));
}

#[test]
fn csv_keeps_rows_with_normalised_line_ends() {
    let e = extract(&read("people.csv"), "text/csv", &Options::default()).expect("csv");
    assert_eq!(e.extractor, "csv");
    assert_eq!(
        e.text,
        "name,city,score\nAda,London,99\nGrace,\"New York\",97"
    );
}

#[test]
fn json_is_indexed_as_text() {
    let e = extract(
        &read("config.json"),
        "application/json",
        &Options::default(),
    )
    .expect("json");
    assert_eq!(e.extractor, "json");
    assert!(e.text.starts_with("{\n\"service\": \"akasha\","));
    assert_eq!(e.char_count(), e.text.chars().count());
}

#[test]
fn plain_text_is_normalised_and_capped() {
    let raw = "  Caf\u{65}\u{301}  au   lait \r\n\r\n\r\n second\tparagraph ";
    let e = extract(raw.as_bytes(), "text/plain", &Options::default()).expect("txt");
    assert_eq!(e.text, "Café au lait\n\nsecond paragraph");

    let capped = Options {
        max_chars: 4,
        ..Options::default()
    };
    let e = extract(raw.as_bytes(), "text/plain", &capped).expect("txt");
    assert_eq!(e.text, "Café");
    assert!(e.notes.iter().any(|n| n.contains("truncated")));
}

#[test]
fn invalid_utf8_is_replaced_not_rejected() {
    let e = extract(b"ok \xff\xfe bytes", "text/plain", &Options::default()).expect("txt");
    assert_eq!(e.text, "ok \u{FFFD}\u{FFFD} bytes");
    let _: Option<IngestError> = None;
}
