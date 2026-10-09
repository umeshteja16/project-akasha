//! PDF text per page, via `pdf-extract` on `lopdf` (pure Rust, ADR 0008).
//!
//! Pages are parsed one at a time inside a panic guard, so one malformed page
//! costs only that page. Pages without a text layer that contain images are marked
//! for OCR; with an engine available, their largest embedded image is recognised
//! when it is a JPEG or an uncompressed/Flate-compressed 8-bit gray or RGB bitmap
//! (what scanners produce). Other encodings (JBIG2, CCITT, JPEG 2000) stay
//! [`PageSource::NeedsOcr`].

use image::{GrayImage, ImageFormat, RgbImage};
use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::{
    Extraction, IngestError, Options, Page, PageSource, TextBuilder, error::guard,
    normalize::normalize, ocr::Ocr,
};

const UNREADABLE: &str = "this PDF could not be read; it may be damaged";

pub(crate) fn extract(bytes: &[u8], options: &Options<'_>) -> Result<Extraction, IngestError> {
    let doc = load(bytes)?;
    let page_ids = guard(
        || IngestError::Corrupt(UNREADABLE.into()),
        || Ok(doc.get_pages()),
    )?;
    if page_ids.is_empty() {
        return Err(IngestError::Corrupt("this PDF has no pages".into()));
    }

    let mut builder = TextBuilder::new(options.max_chars);
    let mut pages = Vec::with_capacity(page_ids.len());
    for (&number, &id) in &page_ids {
        if builder.is_full() {
            break;
        }
        let (text, source) = page(&doc, number, id, options.ocr);
        let (char_start, char_end) = builder.push(&text);
        pages.push(Page {
            number,
            char_start,
            char_end,
            source,
        });
    }

    let unreadable = count(&pages, PageSource::Unreadable);
    if unreadable == pages.len() {
        return Err(IngestError::Corrupt(UNREADABLE.into()));
    }
    let mut notes = Vec::new();
    if unreadable > 0 {
        notes.push(format!("{unreadable} page(s) could not be read"));
    }
    let needs_ocr = count(&pages, PageSource::NeedsOcr);
    if needs_ocr > 0 {
        notes.push(match options.ocr {
            None => format!("{needs_ocr} page(s) have no text layer and OCR is disabled"),
            Some(_) => {
                format!("{needs_ocr} page(s) have no text layer and OCR could not read them")
            }
        });
    }
    let text = builder.finish(&mut notes);
    Ok(Extraction {
        extractor: "pdf",
        text,
        pages,
        notes,
    })
}

fn count(pages: &[Page], source: PageSource) -> usize {
    pages.iter().filter(|p| p.source == source).count()
}

/// Parse the document, decrypting it if it only has an empty user password.
fn load(bytes: &[u8]) -> Result<Document, IngestError> {
    guard(
        || IngestError::Corrupt(UNREADABLE.into()),
        || {
            let mut doc = Document::load_mem(bytes).map_err(|err| match err {
                lopdf::Error::Decryption(_) => IngestError::Encrypted,
                other => IngestError::corrupt(UNREADABLE, other),
            })?;
            if doc.is_encrypted() {
                doc.decrypt("").map_err(|_| IngestError::Encrypted)?;
            }
            Ok(doc)
        },
    )
}

/// One page's normalised text and where it came from.
fn page(doc: &Document, number: u32, id: ObjectId, ocr: Option<&Ocr>) -> (String, PageSource) {
    let text = guard(
        || IngestError::Corrupt(UNREADABLE.into()),
        || page_text(doc, number),
    );
    let text = match text {
        Ok(text) => normalize(&text),
        Err(_) => return (String::new(), PageSource::Unreadable),
    };
    if !text.is_empty() {
        return (text, PageSource::Text);
    }
    let images = guard(
        || IngestError::Corrupt(UNREADABLE.into()),
        || Ok(page_images(doc, id)),
    )
    .unwrap_or_default();
    if images.is_empty() {
        // A blank page: nothing to recognise.
        return (String::new(), PageSource::Text);
    }
    let Some(ocr) = ocr else {
        return (String::new(), PageSource::NeedsOcr);
    };
    let recognised = images
        .iter()
        .max_by_key(|img| img.width.saturating_mul(img.height))
        .and_then(|img| {
            guard(
                || IngestError::Corrupt(UNREADABLE.into()),
                || Ok(decode_image(doc, img)),
            )
            .ok()
            .flatten()
        })
        .map(|bitmap| ocr.recognize(&bitmap));
    match recognised {
        Some(Ok(text)) if !normalize(&text).is_empty() => (normalize(&text), PageSource::Ocr),
        Some(Err(err)) => {
            tracing::warn!(page = number, %err, "OCR of a PDF page failed");
            (String::new(), PageSource::NeedsOcr)
        }
        _ => (String::new(), PageSource::NeedsOcr),
    }
}

fn page_text(doc: &Document, number: u32) -> Result<String, IngestError> {
    let mut text = String::new();
    {
        let mut out = pdf_extract::PlainTextOutput::new(&mut text);
        pdf_extract::output_doc_page(doc, &mut out, number)
            .map_err(|err| IngestError::corrupt(UNREADABLE, format!("{err:?}")))?;
    }
    Ok(text)
}

/// An image XObject drawn on a page.
struct PageImage {
    id: ObjectId,
    width: u32,
    height: u32,
}

/// Image XObjects in the page's resources (inherited from parent nodes if the
/// page has none of its own).
fn page_images(doc: &Document, page_id: ObjectId) -> Vec<PageImage> {
    let Some(resources) = resources(doc, page_id) else {
        return Vec::new();
    };
    let Some(xobjects) = resources
        .get(b"XObject")
        .ok()
        .and_then(|o| deref_dict(doc, o))
    else {
        return Vec::new();
    };
    xobjects
        .iter()
        .filter_map(|(_, value)| {
            let id = value.as_reference().ok()?;
            let dict = &doc.get_object(id).ok()?.as_stream().ok()?.dict;
            if dict.get(b"Subtype").ok()?.as_name().ok()? != b"Image" {
                return None;
            }
            let width = u32::try_from(dict.get(b"Width").ok()?.as_i64().ok()?).ok()?;
            let height = u32::try_from(dict.get(b"Height").ok()?.as_i64().ok()?).ok()?;
            Some(PageImage { id, width, height })
        })
        .collect()
}

fn resources(doc: &Document, page_id: ObjectId) -> Option<&Dictionary> {
    let mut node = doc.get_dictionary(page_id).ok()?;
    // Bounded walk up the page tree (guards against reference cycles).
    for _ in 0..32 {
        if let Some(found) = node.get(b"Resources").ok().and_then(|o| deref_dict(doc, o)) {
            return Some(found);
        }
        let parent = node.get(b"Parent").ok()?.as_reference().ok()?;
        node = doc.get_dictionary(parent).ok()?;
    }
    None
}

fn deref_dict<'a>(doc: &'a Document, object: &'a Object) -> Option<&'a Dictionary> {
    doc.dereference(object).ok()?.1.as_dict().ok()
}

/// Decode the encodings scanners use; `None` for anything else.
fn decode_image(doc: &Document, img: &PageImage) -> Option<RgbImage> {
    let stream = doc.get_object(img.id).ok()?.as_stream().ok()?;
    let dict = &stream.dict;
    let filters: Vec<Vec<u8>> = match dict.get(b"Filter") {
        Ok(Object::Name(name)) => vec![name.clone()],
        Ok(Object::Array(items)) => items
            .iter()
            .filter_map(|o| o.as_name().ok().map(<[u8]>::to_vec))
            .collect(),
        _ => Vec::new(),
    };
    match filters
        .iter()
        .map(Vec::as_slice)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [b"DCTDecode"] => image::load_from_memory_with_format(&stream.content, ImageFormat::Jpeg)
            .ok()
            .map(|i| i.into_rgb8()),
        [] | [b"FlateDecode"] => {
            if dict.get(b"BitsPerComponent").ok()?.as_i64().ok()? != 8 {
                return None;
            }
            let data = if filters.is_empty() {
                stream.content.clone()
            } else {
                stream.decompressed_content().ok()?
            };
            let space = dict.get(b"ColorSpace").ok()?.as_name().ok()?;
            match space {
                b"DeviceGray" => GrayImage::from_raw(img.width, img.height, data)
                    .map(|g| image::DynamicImage::ImageLuma8(g).into_rgb8()),
                b"DeviceRGB" => RgbImage::from_raw(img.width, img.height, data),
                _ => None,
            }
        }
        _ => None,
    }
}
