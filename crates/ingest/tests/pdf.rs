//! PDF extraction: per-page text, page spans, scans, damaged and encrypted files.

mod support;

use akasha_ingest::{IngestError, Options, PageSource, extract};
use support::{PageSpec, encrypted_pdf, pdf, scan_jpeg};

const PDF: &str = "application/pdf";

fn span(text: &str, start: usize, end: usize) -> String {
    text.chars().skip(start).take(end - start).collect()
}

#[test]
fn text_pages_have_numbers_and_spans() {
    let bytes = pdf(&[
        PageSpec::Text(&["Hello from page one", "second line"]),
        PageSpec::Blank,
        PageSpec::Text(&["Page three talks about Rust"]),
    ]);
    let e = extract(&bytes, PDF, &Options::default()).expect("pdf");
    assert_eq!(e.extractor, "pdf");
    assert_eq!(e.pages.len(), 3);
    assert_eq!(
        e.pages.iter().map(|p| p.number).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    let first = &e.pages[0];
    assert_eq!(first.source, PageSource::Text);
    assert_eq!(
        span(&e.text, first.char_start, first.char_end),
        "Hello from page one\nsecond line"
    );
    let blank = &e.pages[1];
    assert_eq!(
        (blank.source, blank.char_start == blank.char_end),
        (PageSource::Text, true),
        "a blank page has an empty span and is not sent to OCR"
    );
    let third = &e.pages[2];
    assert_eq!(
        span(&e.text, third.char_start, third.char_end),
        "Page three talks about Rust"
    );
    assert_eq!(e.text.chars().count(), third.char_end);
    assert!(e.notes.is_empty(), "{:?}", e.notes);
}

#[test]
fn scanned_pages_are_marked_for_ocr_when_ocr_is_off() {
    let (jpeg, width, height) = scan_jpeg();
    let bytes = pdf(&[
        PageSpec::Text(&["Typed page"]),
        PageSpec::Scan {
            jpeg: &jpeg,
            width,
            height,
        },
    ]);
    let e = extract(&bytes, PDF, &Options::default()).expect("pdf");
    assert_eq!(e.pages[1].source, PageSource::NeedsOcr);
    assert_eq!(e.pages_needing_ocr(), 1);
    assert_eq!(e.text, "Typed page");
    assert!(e.notes.iter().any(|n| n.contains("OCR is disabled")));
}

#[test]
fn damaged_pdfs_fail_cleanly() {
    let good = pdf(&[PageSpec::Text(&["Some text"])]);
    let cases: Vec<(&str, Vec<u8>)> = vec![
        (
            "header only",
            b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog >>\nendobj\n".to_vec(),
        ),
        ("truncated", good[..good.len() / 2].to_vec()),
        (
            "garbage",
            b"%PDF-1.7\n\x00\x01\x02 not really a pdf".to_vec(),
        ),
        ("empty", Vec::new()),
    ];
    for (name, bytes) in cases {
        match extract(&bytes, PDF, &Options::default()) {
            Err(err @ IngestError::Corrupt(_)) => {
                assert!(err.is_permanent(), "{name}");
                assert!(
                    err.to_string().contains("could not be read"),
                    "{name}: {err}"
                );
            }
            other => panic!("{name}: expected Corrupt, got {other:?}"),
        }
    }
}

#[test]
fn password_protected_pdfs_are_reported_as_such() {
    let bytes = encrypted_pdf(&[PageSpec::Text(&["Top secret"])], "hunter2");
    let err = extract(&bytes, PDF, &Options::default()).expect_err("needs password");
    assert!(matches!(err, IngestError::Encrypted), "{err:?}");
    assert!(err.is_permanent());
    assert!(err.to_string().contains("password-protected"));
}

#[test]
fn encrypted_pdfs_with_an_empty_password_are_read() {
    let bytes = encrypted_pdf(&[PageSpec::Text(&["Owner-locked but readable"])], "");
    let e = extract(&bytes, PDF, &Options::default()).expect("pdf");
    assert_eq!(e.text, "Owner-locked but readable");
}

#[test]
fn long_documents_are_truncated_at_a_page() {
    let bytes = pdf(&[
        PageSpec::Text(&["0123456789"]),
        PageSpec::Text(&["abcdefghij"]),
        PageSpec::Text(&["never read"]),
    ]);
    let options = Options {
        max_chars: 15,
        ..Options::default()
    };
    let e = extract(&bytes, PDF, &options).expect("pdf");
    assert_eq!(e.text, "0123456789\n\nabc");
    assert_eq!(e.pages.len(), 2, "stops after the page that hit the limit");
    assert!(e.notes.iter().any(|n| n.contains("truncated")));
}

#[test]
fn committed_fixture_extracts_per_page() {
    let bytes = std::fs::read(support::fixture("three-pages.pdf")).expect("fixture");
    let e = extract(&bytes, PDF, &Options::default()).expect("pdf");
    assert_eq!(
        e.text,
        "Akasha fixture, page one.\nIt mentions the word aardvark.\n\nPage three is about zebras."
    );
    let spans: Vec<_> = e
        .pages
        .iter()
        .map(|p| (p.number, p.char_start, p.char_end))
        .collect();
    assert_eq!(spans, vec![(1, 0, 56), (2, 56, 56), (3, 58, 85)]);
}
