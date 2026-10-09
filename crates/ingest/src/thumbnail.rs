//! Thumbnails of images (PNG, JPEG, WebP, GIF, TIFF): decoded with the `image`
//! crate, scaled to fit a square, re-encoded as JPEG (or PNG when the image has
//! transparency).
//!
//! Decode-bomb safety: the header is read first and images over
//! [`MAX_PIXELS`] or [`MAX_SIDE`] are refused before any pixel is decoded; the
//! decoder also runs under an allocation limit. PDFs get no thumbnail (rendering a
//! page needs a native PDF renderer; the client shows a type icon instead).

use std::io::Cursor;

use image::{
    DynamicImage, ImageDecoder, ImageReader, Limits,
    codecs::{jpeg::JpegEncoder, png::PngEncoder},
    imageops::FilterType,
};

/// Largest image (width × height) we decode: 50 megapixels (~200 MB as RGBA).
pub const MAX_PIXELS: u64 = 50_000_000;
/// Longest side we decode.
pub const MAX_SIDE: u32 = 20_000;
/// Decoder allocation cap.
const MAX_ALLOC: u64 = 256 * 1024 * 1024;
const JPEG_QUALITY: u8 = 80;

/// An encoded thumbnail.
#[derive(Debug, Clone)]
pub struct Thumbnail {
    pub bytes: Vec<u8>,
    /// `image/jpeg` or `image/png`.
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum ThumbnailError {
    /// Not an image format we decode.
    #[error("unsupported image format")]
    Unsupported,
    /// Over [`MAX_PIXELS`] / [`MAX_SIDE`]: refused without decoding.
    #[error("image is too large to preview ({width}x{height})")]
    TooLarge { width: u32, height: u32 },
    #[error("image could not be decoded: {0}")]
    Corrupt(String),
    #[error("thumbnail could not be encoded: {0}")]
    Encode(String),
}

/// Render a thumbnail that fits in `max_side` × `max_side` (never upscaled),
/// with EXIF orientation applied. Blocking and CPU-bound.
pub fn thumbnail(bytes: &[u8], max_side: u32) -> Result<Thumbnail, ThumbnailError> {
    // Decoders of hostile files may panic; never let that take down a worker.
    std::panic::catch_unwind(|| render(bytes, max_side))
        .unwrap_or_else(|_| Err(ThumbnailError::Corrupt("decoder panicked".into())))
}

fn render(bytes: &[u8], max_side: u32) -> Result<Thumbnail, ThumbnailError> {
    let corrupt = |e: image::ImageError| ThumbnailError::Corrupt(e.to_string());
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| ThumbnailError::Corrupt(e.to_string()))?;
    if reader.format().is_none() {
        return Err(ThumbnailError::Unsupported);
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|e| match e {
        image::ImageError::Unsupported(_) => ThumbnailError::Unsupported,
        image::ImageError::Limits(_) => ThumbnailError::TooLarge {
            width: 0,
            height: 0,
        },
        other => corrupt(other),
    })?;
    let (width, height) = decoder.dimensions();
    if width > MAX_SIDE || height > MAX_SIDE || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(ThumbnailError::TooLarge { width, height });
    }
    let orientation = decoder.orientation().ok();
    let mut image = DynamicImage::from_decoder(decoder).map_err(corrupt)?;
    if let Some(orientation) = orientation {
        image.apply_orientation(orientation);
    }
    if image.width() > max_side || image.height() > max_side {
        image = image.resize(max_side, max_side, FilterType::Triangle);
    }
    encode(&image)
}

fn encode(image: &DynamicImage) -> Result<Thumbnail, ThumbnailError> {
    let encode_err = |e: image::ImageError| ThumbnailError::Encode(e.to_string());
    let mut bytes = Vec::new();
    let mime = if image.color().has_alpha() {
        image
            .to_rgba8()
            .write_with_encoder(PngEncoder::new(&mut bytes))
            .map_err(encode_err)?;
        "image/png"
    } else {
        image
            .to_rgb8()
            .write_with_encoder(JpegEncoder::new_with_quality(&mut bytes, JPEG_QUALITY))
            .map_err(encode_err)?;
        "image/jpeg"
    };
    Ok(Thumbnail {
        bytes,
        mime,
        width: image.width(),
        height: image.height(),
    })
}

#[cfg(test)]
mod tests {
    use image::{ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};

    use super::*;

    fn png(width: u32, height: u32, alpha: bool) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        if alpha {
            RgbaImage::from_pixel(width, height, Rgba([10, 20, 30, 128]))
                .write_to(&mut out, ImageFormat::Png)
                .expect("png");
        } else {
            RgbImage::from_pixel(width, height, Rgb([200, 10, 10]))
                .write_to(&mut out, ImageFormat::Png)
                .expect("png");
        }
        out.into_inner()
    }

    #[test]
    fn scales_down_keeping_the_aspect_ratio() {
        let t = thumbnail(&png(1000, 500, false), 256).expect("thumb");
        assert_eq!((t.width, t.height, t.mime), (256, 128, "image/jpeg"));
        assert_eq!(&t.bytes[..2], &[0xFF, 0xD8]);
        let decoded = image::load_from_memory(&t.bytes).expect("decodes");
        assert_eq!((decoded.width(), decoded.height()), (256, 128));
    }

    #[test]
    fn small_images_are_not_upscaled_and_alpha_stays_png() {
        let t = thumbnail(&png(40, 30, true), 256).expect("thumb");
        assert_eq!((t.width, t.height, t.mime), (40, 30, "image/png"));
        assert_eq!(&t.bytes[..4], b"\x89PNG");
    }

    /// A real 1x1 PNG whose header claims `width` x `height` (CRC fixed up).
    fn lying_png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = png(1, 1, false);
        bytes[16..20].copy_from_slice(&width.to_be_bytes());
        bytes[20..24].copy_from_slice(&height.to_be_bytes());
        let crc = crc32(&bytes[12..29]);
        bytes[29..33].copy_from_slice(&crc.to_be_bytes());
        bytes
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for byte in data {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    #[test]
    fn huge_dimensions_are_refused_before_decoding() {
        // Over the side limit: the decoder's own limit check refuses it.
        assert!(matches!(
            thumbnail(&lying_png(30_000, 30_000), 256),
            Err(ThumbnailError::TooLarge { .. })
        ));
        // Under the side limit but over the pixel budget.
        assert!(matches!(
            thumbnail(&lying_png(15_000, 15_000), 256),
            Err(ThumbnailError::TooLarge {
                width: 15_000,
                height: 15_000
            })
        ));
    }

    #[test]
    fn non_images_and_garbage_are_errors() {
        assert!(matches!(
            thumbnail(b"%PDF-1.4 not an image", 256),
            Err(ThumbnailError::Unsupported)
        ));
        let mut truncated = png(50, 50, false);
        truncated.truncate(60);
        assert!(thumbnail(&truncated, 256).is_err());
    }
}
