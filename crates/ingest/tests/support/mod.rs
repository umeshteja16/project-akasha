//! Builds small PDFs in memory, so no binary PDF fixtures are committed.
#![allow(dead_code)] // each test crate uses a different subset

use lopdf::{
    Document, EncryptionState, EncryptionVersion, Object, Permissions, Stream,
    content::{Content, Operation},
    dictionary,
};

pub enum PageSpec<'a> {
    /// A text layer with these lines (Helvetica, one `Tj` per line).
    Text(&'a [&'a str]),
    /// A full-page JPEG image and no text layer (a scan).
    Scan {
        jpeg: &'a [u8],
        width: u32,
        height: u32,
    },
    Blank,
}

pub fn pdf(pages: &[PageSpec<'_>]) -> Vec<u8> {
    save(&mut document(pages))
}

/// Encrypted with RC4 (V1). An empty `user_password` opens without a password.
pub fn encrypted_pdf(pages: &[PageSpec<'_>], user_password: &str) -> Vec<u8> {
    let mut doc = document(pages);
    doc.trailer.set(
        "ID",
        Object::Array(vec![
            Object::string_literal("0123456789abcdef"),
            Object::string_literal("0123456789abcdef"),
        ]),
    );
    let state = EncryptionState::try_from(EncryptionVersion::V1 {
        document: &doc,
        owner_password: "owner",
        user_password,
        permissions: Permissions::all(),
    })
    .expect("encryption state");
    doc.encrypt(&state).expect("encrypt");
    save(&mut doc)
}

fn save(doc: &mut Document) -> Vec<u8> {
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save pdf");
    out
}

fn document(pages: &[PageSpec<'_>]) -> Document {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let mut kids = Vec::new();
    for spec in pages {
        let (ops, xobjects) = match spec {
            PageSpec::Text(lines) => {
                let mut ops = vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tf", vec!["F1".into(), 14.into()]),
                    Operation::new("TL", vec![18.into()]),
                    Operation::new("Td", vec![72.into(), 760.into()]),
                ];
                for line in *lines {
                    ops.push(Operation::new("Tj", vec![Object::string_literal(*line)]));
                    ops.push(Operation::new("T*", vec![]));
                }
                ops.push(Operation::new("ET", vec![]));
                (ops, dictionary! {})
            }
            PageSpec::Scan {
                jpeg,
                width,
                height,
            } => {
                let image = doc.add_object(Stream::new(
                    dictionary! {
                        "Type" => "XObject", "Subtype" => "Image",
                        "Width" => i64::from(*width), "Height" => i64::from(*height),
                        "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
                        "Filter" => "DCTDecode",
                    },
                    jpeg.to_vec(),
                ));
                let ops = vec![
                    Operation::new("q", vec![]),
                    Operation::new(
                        "cm",
                        vec![
                            i64::from(*width).into(),
                            0.into(),
                            0.into(),
                            i64::from(*height).into(),
                            0.into(),
                            0.into(),
                        ],
                    ),
                    Operation::new("Do", vec!["Im1".into()]),
                    Operation::new("Q", vec![]),
                ];
                (ops, dictionary! { "Im1" => image })
            }
            PageSpec::Blank => (vec![], dictionary! {}),
        };
        let content = Content { operations: ops }
            .encode()
            .expect("encode content");
        let contents = doc.add_object(Stream::new(dictionary! {}, content));
        let mut page = dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => contents,
        };
        if !xobjects.is_empty() {
            // Own resources; text pages inherit the font from the page tree.
            page.set(
                "Resources",
                dictionary! { "Font" => dictionary! { "F1" => font }, "XObject" => xobjects },
            );
        }
        kids.push(Object::Reference(doc.add_object(page)));
    }
    let count = i64::try_from(kids.len()).expect("count");
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => kids, "Count" => count,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    doc
}

/// A small JPEG of the OCR fixture, for scanned-page tests.
pub fn scan_jpeg() -> (Vec<u8>, u32, u32) {
    let png = std::fs::read(fixture("hello-ocr.png")).expect("fixture");
    let img = image::load_from_memory(&png).expect("png").into_rgb8();
    let mut jpeg = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut jpeg),
        image::ImageFormat::Jpeg,
    )
    .expect("jpeg");
    (jpeg, img.width(), img.height())
}

pub fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}
