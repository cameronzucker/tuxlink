//! Pure transcode core — no Tauri, no fs. Decode → resize (aspect-preserving)
//! → encode. Decode dispatches by detected format: the `image` crate for
//! JPEG/PNG/GIF/WebP/TIFF/BMP, libheif for HEIC. Encode is JPEG (default,
//! WLE-safe) or WebP (tuxlink->tuxlink, ~30% smaller). tuxlink-mg4s.

use image::{DynamicImage, ImageFormat};

/// Max-dimension presets (longest edge, px). `Original` skips resize.
///
/// Calibrated to the Winlink CMS ~120 KB message-size ceiling (tuxlink-rbhg):
/// a JPEG photo's bytes scale ~with pixel count, and 1024px+ routinely blows
/// 120 KB. Small/Medium usually fit; Large is borderline; Original is for
/// fast/local links only. The compose UI shows the resulting byte size live so
/// the operator picks a preset that actually fits — content-dependent size
/// means no fixed dimension can guarantee the fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizePreset {
    Small,    // 480 — safely under the CMS limit for most photos
    Medium,   // 640 — usually fits
    Large,    // 800 — borderline; pair with WebP or check the live size
    Original,
}

impl ResizePreset {
    fn max_edge(self) -> Option<u32> {
        match self {
            ResizePreset::Small => Some(480),
            ResizePreset::Medium => Some(640),
            ResizePreset::Large => Some(800),
            ResizePreset::Original => None,
        }
    }
}

/// Output wire format. JPEG is the safe default (any recipient incl. Winlink
/// Express); WebP is the tuxlink->tuxlink efficiency opt-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutFormat {
    Jpeg,
    Webp,
}

impl OutFormat {
    pub fn ext(self) -> &'static str {
        match self {
            OutFormat::Jpeg => "jpg",
            OutFormat::Webp => "webp",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcoded {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug)]
pub enum TranscodeError {
    Decode(String),
    Encode(String),
}

impl std::fmt::Display for TranscodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranscodeError::Decode(s) => write!(f, "decode failed: {s}"),
            TranscodeError::Encode(s) => write!(f, "encode failed: {s}"),
        }
    }
}

impl std::error::Error for TranscodeError {}

/// True if the bytes are an HEIF/HEIC container (ftyp brand check). HEIC files
/// start with a `ftyp` box whose major brand is one of these.
fn is_heic(bytes: &[u8]) -> bool {
    if bytes.len() < 12 || &bytes[4..8] != b"ftyp" {
        return false;
    }
    let brand = &bytes[8..12];
    const HEIF_BRANDS: [&[u8; 4]; 7] =
        [b"heic", b"heix", b"hevc", b"heim", b"heis", b"mif1", b"msf1"];
    HEIF_BRANDS.iter().any(|b| brand == b.as_slice())
}

/// Decode `bytes` (any supported input incl. HEIC) → resize to `preset` →
/// encode to `format`. The single entry point the command calls.
pub fn transcode_image_bytes(
    bytes: &[u8],
    preset: ResizePreset,
    format: OutFormat,
) -> Result<Transcoded, TranscodeError> {
    let img = if is_heic(bytes) {
        decode_heic(bytes)?
    } else {
        image::load_from_memory(bytes).map_err(|e| TranscodeError::Decode(e.to_string()))?
    };
    let resized = resize_to(img, preset);
    let (width, height) = (resized.width(), resized.height());
    let out = encode(&resized, format)?;
    Ok(Transcoded { bytes: out, width, height })
}

/// Resize a decoded image to the preset (no-op for `Original` or when already
/// within bounds) preserving aspect ratio with a high-quality filter.
fn resize_to(img: DynamicImage, preset: ResizePreset) -> DynamicImage {
    match preset.max_edge() {
        Some(edge) if img.width() > edge || img.height() > edge => {
            img.resize(edge, edge, image::imageops::FilterType::Lanczos3)
        }
        _ => img,
    }
}

/// Encode a decoded image to the requested wire format.
fn encode(img: &DynamicImage, format: OutFormat) -> Result<Vec<u8>, TranscodeError> {
    match format {
        OutFormat::Jpeg => {
            let mut buf = std::io::Cursor::new(Vec::new());
            // Drop alpha (JPEG has none) by writing RGB8.
            DynamicImage::ImageRgb8(img.to_rgb8())
                .write_to(&mut buf, ImageFormat::Jpeg)
                .map_err(|e| TranscodeError::Encode(e.to_string()))?;
            Ok(buf.into_inner())
        }
        OutFormat::Webp => {
            // `webp` crate: lossy encode at quality 80.
            let rgba = img.to_rgba8();
            let encoder = webp::Encoder::from_rgba(&rgba, rgba.width(), rgba.height());
            let mem = encoder.encode(80.0);
            Ok(mem.to_vec())
        }
    }
}

/// Decode an HEIC/HEIF image to a `DynamicImage` via libheif. Reads the
/// primary image, converts to interleaved RGB, copies into an `RgbImage`.
fn decode_heic(bytes: &[u8]) -> Result<DynamicImage, TranscodeError> {
    use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};
    let lib = LibHeif::new();
    let ctx = HeifContext::read_from_bytes(bytes)
        .map_err(|e| TranscodeError::Decode(format!("heif: {e}")))?;
    let handle = ctx
        .primary_image_handle()
        .map_err(|e| TranscodeError::Decode(format!("heif handle: {e}")))?;
    let image = lib
        .decode(&handle, ColorSpace::Rgb(RgbChroma::Rgb), None)
        .map_err(|e| TranscodeError::Decode(format!("heif decode: {e}")))?;
    let planes = image.planes();
    let interleaved = planes
        .interleaved
        .ok_or_else(|| TranscodeError::Decode("heif: no interleaved plane".into()))?;
    let w = interleaved.width;
    let h = interleaved.height;
    let stride = interleaved.stride;
    let src = interleaved.data;
    let row_bytes = (w as usize) * 3;
    // Copy row-by-row to drop the stride padding into a tight RGB buffer.
    let mut rgb = Vec::with_capacity((w as usize) * (h as usize) * 3);
    for row in src.chunks(stride).take(h as usize) {
        rgb.extend_from_slice(&row[..row_bytes]);
    }
    let buf = image::RgbImage::from_raw(w, h, rgb)
        .ok_or_else(|| TranscodeError::Decode("heif: buffer size mismatch".into()))?;
    Ok(DynamicImage::ImageRgb8(buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red_png(w: u32, h: u32) -> Vec<u8> {
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(w, h, image::Rgb([255, 0, 0])));
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    #[test]
    fn resize_caps_longest_edge_preserving_aspect() {
        let img = image::load_from_memory(&red_png(2000, 1000)).unwrap();
        let out = resize_to(img, ResizePreset::Small); // 480
        assert_eq!(out.width(), 480);
        assert_eq!(out.height(), 240);
    }

    #[test]
    fn resize_original_is_noop() {
        let img = image::load_from_memory(&red_png(800, 600)).unwrap();
        let out = resize_to(img, ResizePreset::Original);
        assert_eq!((out.width(), out.height()), (800, 600));
    }

    #[test]
    fn resize_skips_upscale_when_within_bounds() {
        let img = image::load_from_memory(&red_png(300, 200)).unwrap();
        let out = resize_to(img, ResizePreset::Large); // 800 — already smaller
        assert_eq!((out.width(), out.height()), (300, 200));
    }

    #[test]
    fn transcode_jpeg_in_jpeg_out_resizes_and_reencodes() {
        let src = {
            let img =
                DynamicImage::ImageRgb8(image::RgbImage::from_pixel(2000, 1000, image::Rgb([10, 200, 30])));
            let mut b = std::io::Cursor::new(Vec::new());
            img.write_to(&mut b, ImageFormat::Jpeg).unwrap();
            b.into_inner()
        };
        let out = transcode_image_bytes(&src, ResizePreset::Small, OutFormat::Jpeg).unwrap();
        assert_eq!((out.width, out.height), (480, 240));
        let decoded = image::load_from_memory(&out.bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (480, 240));
    }

    #[test]
    fn transcode_webp_out_produces_decodable_webp() {
        // 800x600 → Medium (640) caps the longest edge → 640x480.
        let src = red_png(800, 600);
        let out = transcode_image_bytes(&src, ResizePreset::Medium, OutFormat::Webp).unwrap();
        let decoded = image::load_from_memory(&out.bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (640, 480));
    }

    #[test]
    fn transcode_rejects_garbage_bytes() {
        let err = transcode_image_bytes(b"not an image", ResizePreset::Small, OutFormat::Jpeg);
        assert!(matches!(err, Err(TranscodeError::Decode(_))));
    }

    #[test]
    fn is_heic_detects_ftyp_heic_brand() {
        let mut b = vec![0, 0, 0, 0x18];
        b.extend_from_slice(b"ftypheic");
        b.extend_from_slice(&[0u8; 8]);
        assert!(is_heic(&b));
        assert!(!is_heic(b"\x89PNG\r\n\x1a\n........"));
    }
}
