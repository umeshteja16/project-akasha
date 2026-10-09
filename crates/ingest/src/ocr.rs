//! OCR with `ocrs` (pure Rust, models in `.rten` format; see [`crate::models`]).

use image::{RgbImage, imageops::FilterType};
use ocrs::{ImageSource, OcrEngine, OcrEngineParams};

use crate::{Extraction, IngestError, TextBuilder, error::guard, normalize::normalize};

/// Larger images are scaled down first: recognition time grows with pixel count
/// and text at this size is still legible.
const MAX_SIDE: u32 = 4000;

/// A loaded OCR engine. Cheap to share (`Arc`) across threads; loading the models
/// takes a moment, so build one per process (see [`crate::models::load_ocr`]).
pub struct Ocr {
    engine: OcrEngine,
}

impl std::fmt::Debug for Ocr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Ocr")
    }
}

impl Ocr {
    /// Build an engine from the detection and recognition models' bytes.
    pub fn from_models(detection: Vec<u8>, recognition: Vec<u8>) -> Result<Self, IngestError> {
        let detection =
            rten::Model::load(detection).map_err(|e| IngestError::Models(e.to_string()))?;
        let recognition =
            rten::Model::load(recognition).map_err(|e| IngestError::Models(e.to_string()))?;
        let engine = OcrEngine::new(OcrEngineParams {
            detection_model: Some(detection),
            recognition_model: Some(recognition),
            ..Default::default()
        })
        .map_err(|e| IngestError::Models(e.to_string()))?;
        Ok(Self { engine })
    }

    /// Recognise the text in an image (lines separated by `\n`).
    pub fn recognize(&self, image: &RgbImage) -> Result<String, IngestError> {
        let (w, h) = image.dimensions();
        if w == 0 || h == 0 {
            return Ok(String::new());
        }
        let scaled;
        let image = if w.max(h) > MAX_SIDE {
            let scale = f64::from(MAX_SIDE) / f64::from(w.max(h));
            let (nw, nh) = (scaled_side(w, scale), scaled_side(h, scale));
            scaled = image::imageops::resize(image, nw, nh, FilterType::Triangle);
            &scaled
        } else {
            image
        };
        let source = ImageSource::from_bytes(image.as_raw(), image.dimensions())
            .map_err(|e| IngestError::Ocr(e.to_string()))?;
        guard(
            || IngestError::Ocr("the OCR engine crashed on this image".into()),
            || {
                let input = self
                    .engine
                    .prepare_input(source)
                    .map_err(|e| IngestError::Ocr(e.to_string()))?;
                self.engine
                    .get_text(&input)
                    .map_err(|e| IngestError::Ocr(e.to_string()))
            },
        )
    }
}

fn scaled_side(side: u32, scale: f64) -> u32 {
    // Rounded and at least 1 px; `as` saturates, and the value is <= MAX_SIDE.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let scaled = (f64::from(side) * scale).round() as u32;
    scaled.max(1)
}

/// Text of an image file (the first frame of animated or multi-page images).
pub(crate) fn image(bytes: &[u8], ocr: &Ocr, max_chars: usize) -> Result<Extraction, IngestError> {
    const BAD_IMAGE: &str = "this image could not be decoded; it may be damaged";
    let decoded = guard(
        || IngestError::Corrupt(BAD_IMAGE.into()),
        || image::load_from_memory(bytes).map_err(|e| IngestError::corrupt(BAD_IMAGE, e)),
    )?;
    let text = normalize(&ocr.recognize(&decoded.into_rgb8())?);
    let mut builder = TextBuilder::new(max_chars);
    builder.push(&text);
    let mut notes = Vec::new();
    if text.is_empty() {
        notes.push("no text was found in this image".to_owned());
    }
    let text = builder.finish(&mut notes);
    Ok(Extraction {
        extractor: "ocr",
        text,
        pages: Vec::new(),
        notes,
    })
}
