//! OCR with real models. Ignored by default (needs ~12 MB of models, downloaded
//! once into `AKASHA_MODELS_DIR` or `target/ocr-models`). Run with
//! `cargo test -p akasha-ingest --test ocr -- --ignored`.

mod support;

use std::path::PathBuf;

use akasha_ingest::{Ocr, Options, PageSource, extract, models};
use support::{PageSpec, fixture, pdf, scan_jpeg};

fn engine() -> Ocr {
    let dir = std::env::var_os("AKASHA_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ocr-models")
        });
    models::load_ocr(&dir, models::DEFAULT_BASE_URL).expect("OCR models")
}

#[test]
#[ignore = "downloads OCR models"]
fn reads_text_from_an_image() {
    let ocr = engine();
    let png = std::fs::read(fixture("hello-ocr.png")).expect("fixture");
    let options = Options {
        ocr: Some(&ocr),
        ..Options::default()
    };
    let e = extract(&png, "image/png", &options).expect("ocr");
    assert_eq!(e.extractor, "ocr");
    assert!(e.text.contains("Akasha OCR test"), "{:?}", e.text);
    assert!(e.text.contains("Hello world"), "{:?}", e.text);
}

#[test]
#[ignore = "downloads OCR models"]
fn reads_scanned_pdf_pages() {
    let ocr = engine();
    let (jpeg, width, height) = scan_jpeg();
    let bytes = pdf(&[
        PageSpec::Text(&["Typed page"]),
        PageSpec::Scan {
            jpeg: &jpeg,
            width,
            height,
        },
    ]);
    let options = Options {
        ocr: Some(&ocr),
        ..Options::default()
    };
    let e = extract(&bytes, "application/pdf", &options).expect("pdf");
    assert_eq!(e.pages[1].source, PageSource::Ocr);
    assert!(e.text.contains("Hello world"), "{:?}", e.text);
}

#[test]
#[ignore = "downloads OCR models"]
fn undecodable_images_are_corrupt() {
    let ocr = engine();
    let options = Options {
        ocr: Some(&ocr),
        ..Options::default()
    };
    let err = extract(b"\x89PNG\r\n\x1a\nbroken", "image/png", &options).expect_err("bad");
    assert!(err.is_permanent());
}
