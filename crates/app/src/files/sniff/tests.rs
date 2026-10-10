use super::*;

const PDF: &[u8] = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
/// ELF headers are 64 bytes; `infer` needs more than 52 to recognise one.
const ELF: &[u8] = &[
    0x7f, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0x3e, 0, 1, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0,
];
const PE: &[u8] = b"MZ\x90\0\x03\0\0\0\x04\0\0\0\xff\xff\0\0";
const ZIP: &[u8] = b"PK\x03\x04\x14\0\0\0\x08\0";

fn mime(head: &[u8], name: &str) -> Result<&'static str, String> {
    detect(head, name).map(|d| d.mime).map_err(|e| {
        assert_eq!(e.code, akasha_core::ErrorCode::UnsupportedMediaType);
        e.message
    })
}

#[test]
fn accepts_allowed_types() {
    assert_eq!(mime(PDF, "report.pdf"), Ok("application/pdf"));
    assert_eq!(mime(PNG, "pic.png"), Ok("image/png"));
    // A JPEG saved as .png is still an image: allowed, stored with the real type.
    assert_eq!(
        mime(b"\xff\xd8\xff\xe0\0\x10JFIF", "pic.png"),
        Ok("image/jpeg")
    );
    assert_eq!(mime(b"ID3\x03\0\0\0\0\0", "song.mp3"), Ok("audio/mpeg"));
    assert_eq!(mime(b"RIFF\0\0\0\0WAVEfmt ", "a.wav"), Ok("audio/wav"));
    assert_eq!(mime(b"fLaC\0\0\0\x22", "a.flac"), Ok("audio/flac"));
    assert_eq!(
        mime(b"\0\0\0\x20ftypisom\0\0\x02\0", "v.mp4"),
        Ok("video/mp4")
    );
    assert_eq!(
        mime(b"\x1a\x45\xdf\xa3\x01\0\0\0", "v.webm"),
        Ok("video/webm")
    );
    assert_eq!(mime(b"# Title\n", "notes.md"), Ok("text/markdown"));
    assert_eq!(mime(b"a,b\n1,2\n", "data.CSV"), Ok("text/csv"));
    assert_eq!(mime(b"{\"a\": 1}", "x.json"), Ok("application/json"));
    assert_eq!(mime(b"plain", "README"), Ok("text/plain"));
    assert!(detect(b"hi", "a.txt").expect("text").is_text);
    assert!(!detect(PDF, "a.pdf").expect("pdf").is_text);
}

/// Ported from the old implementation's magic-bytes script, case A: a script uploaded as
/// `fake_malicious.pdf` (declared `application/pdf`) must be refused with 415.
#[test]
fn rejects_text_masquerading_as_pdf() {
    let err = mime(
        b"echo 'Malicious binary masquerader!'\n",
        "fake_malicious.pdf",
    );
    assert!(err.is_err_and(|m| m.contains(".pdf")));
}

#[test]
fn rejects_executables_and_archives_whatever_the_name() {
    for name in ["notes.txt", "notes.md", "report.pdf", "setup.exe", "noext"] {
        for bytes in [ELF, PE, ZIP] {
            assert!(mime(bytes, name).is_err(), "{name}");
        }
    }
    assert!(mime(b"#!/bin/sh\nrm -rf /\n", "install.txt").is_err());
    assert!(mime(b"<html><body>x</body></html>", "page.txt").is_err());
}

#[test]
fn rejects_mismatched_and_unknown_extensions() {
    assert!(
        mime(PNG, "notes.txt").is_err(),
        "binary under a text extension"
    );
    assert!(
        mime(PDF, "pic.png").is_err(),
        "pdf under an image extension"
    );
    assert!(mime(b"echo hi", "run.sh").is_err(), "unknown extension");
    assert!(mime(b"MZ but text", "x.exe").is_err());
}

#[test]
fn text_validator_handles_split_characters() {
    let snowman = "a\u{2603}b".as_bytes();
    let mut v = TextValidator::default();
    assert!(v.feed(&snowman[..2]));
    assert!(v.feed(&snowman[2..3]));
    assert!(v.feed(&snowman[3..]));
    assert!(v.finish());

    let mut cut = TextValidator::default();
    assert!(cut.feed(&snowman[..2]));
    assert!(!cut.finish(), "stream ended mid-character");

    assert!(!TextValidator::default().feed(b"\xff\xfe"));
    assert!(!TextValidator::default().feed(b"nul\0byte"));
}
