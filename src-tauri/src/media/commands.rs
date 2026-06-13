//! Tauri command surface for attachment preparation (tuxlink-mg4s).
//!
//! `prepare_attachment` reads a user-chosen file (path comes from the dialog
//! plugin or a drag-drop event — both yield paths), and either transcodes it
//! (image files: decode → resize → re-encode) or passes it through unchanged
//! (any other file). All fs + CPU work runs under `spawn_blocking`. App
//! commands are not ACL-gated in Tauri 2; the compose capability's IPC bridge
//! is sufficient.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::transcode::{transcode_image_bytes, OutFormat, ResizePreset};

/// Image input extensions we route through the transcoder. Everything else is
/// passed through as-is.
const IMAGE_EXTS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "tif", "tiff", "bmp", "heic", "heif",
];

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PresetArg {
    Small,
    Medium,
    Large,
    Original,
}

/// Re-encode target chosen by the operator, independent of resize (tuxlink-rbhg).
/// `Original` means "keep the source format" — combined with no resize it's a
/// byte-for-byte passthrough; combined with a resize it re-encodes to the
/// source's format (resize forces a re-encode).
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FormatArg {
    Original,
    Jpeg,
    Webp,
}

impl From<PresetArg> for ResizePreset {
    fn from(p: PresetArg) -> Self {
        match p {
            PresetArg::Small => ResizePreset::Small,
            PresetArg::Medium => ResizePreset::Medium,
            PresetArg::Large => ResizePreset::Large,
            PresetArg::Original => ResizePreset::Original,
        }
    }
}

/// Resolve the encode format when a transcode IS required (resize and/or an
/// explicit format change). For `Original`, keep the source format where we can
/// encode it (jpeg/png/webp); other sources (HEIC, gif, bmp, tiff) fall back to
/// JPEG — HEIC notably can't be re-saved as HEIC (no encoder), and JPEG is the
/// airtime-sane, universally-viewable photo format.
fn resolve_out_format(format: FormatArg, src_ext_lower: &str) -> OutFormat {
    match format {
        FormatArg::Jpeg => OutFormat::Jpeg,
        FormatArg::Webp => OutFormat::Webp,
        FormatArg::Original => match src_ext_lower {
            "png" => OutFormat::Png,
            "webp" => OutFormat::Webp,
            "jpg" | "jpeg" => OutFormat::Jpeg,
            _ => OutFormat::Jpeg,
        },
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedAttachment {
    /// Final filename (image transcodes get a new extension).
    pub filename: String,
    /// File bytes to attach. serde serializes Vec<u8> as a JSON number array,
    /// matching the frontend OutboundAttachmentDto `bytes: number[]`.
    pub bytes: Vec<u8>,
    /// "image" if it was transcoded, else "file".
    pub kind: String,
    pub original_len: usize,
    pub new_len: usize,
}

/// Read + prepare a single attachment. `image_preset`/`image_format` apply only
/// to image files; ignored for pass-through files.
#[tauri::command]
pub async fn prepare_attachment(
    path: String,
    image_preset: PresetArg,
    image_format: FormatArg,
) -> Result<PreparedAttachment, String> {
    let pathbuf = PathBuf::from(&path);
    tauri::async_runtime::spawn_blocking(move || {
        prepare_blocking(&pathbuf, image_preset, image_format)
    })
    .await
    .map_err(|e| format!("attachment task join failed: {e}"))?
}

fn prepare_blocking(
    path: &std::path::Path,
    preset: PresetArg,
    format: FormatArg,
) -> Result<PreparedAttachment, String> {
    let raw = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let original_len = raw.len();
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("attachment")
        .to_string();
    let ext_lower = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let original_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("attachment")
        .to_string();

    let is_image = IMAGE_EXTS.contains(&ext_lower.as_str());
    // Decouple resize from re-encode (tuxlink-rbhg): a transcode happens only if
    // the operator asked to resize OR change the format. Otherwise the original
    // file is passed through byte-for-byte — true "Original", no recompress.
    let needs_transcode =
        is_image && (preset != PresetArg::Original || format != FormatArg::Original);

    if needs_transcode {
        let out_format = resolve_out_format(format, &ext_lower);
        let t = transcode_image_bytes(&raw, preset.into(), out_format).map_err(|e| e.to_string())?;
        Ok(PreparedAttachment {
            filename: format!("{stem}.{}", out_format.ext()),
            new_len: t.bytes.len(),
            bytes: t.bytes,
            kind: "image".into(),
            original_len,
        })
    } else {
        // Passthrough: non-image files, AND images at Original size + Original
        // format (the untouched original file the operator asked for).
        Ok(PreparedAttachment {
            filename: original_name,
            new_len: original_len,
            bytes: raw,
            kind: if is_image { "image".into() } else { "file".into() },
            original_len,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn passthrough_non_image_returns_bytes_unchanged() {
        let td = TempDir::new().unwrap();
        let p = td.path().join("brief.txt");
        std::fs::write(&p, b"hello brief").unwrap();
        let out = prepare_blocking(&p, PresetArg::Small, FormatArg::Jpeg).unwrap();
        assert_eq!(out.kind, "file");
        assert_eq!(out.filename, "brief.txt");
        assert_eq!(out.bytes, b"hello brief");
        assert_eq!(out.original_len, out.new_len);
    }

    #[test]
    fn image_is_transcoded_and_renamed() {
        let td = TempDir::new().unwrap();
        let p = td.path().join("photo.png");
        let img =
            image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(2000, 1500, image::Rgb([1, 2, 3])));
        let mut b = std::io::Cursor::new(Vec::new());
        img.write_to(&mut b, image::ImageFormat::Png).unwrap();
        std::fs::write(&p, b.into_inner()).unwrap();
        let out = prepare_blocking(&p, PresetArg::Small, FormatArg::Jpeg).unwrap();
        assert_eq!(out.kind, "image");
        assert_eq!(out.filename, "photo.jpg");
        assert!(
            out.new_len < out.original_len,
            "resize+JPEG should shrink a 2000x1500 PNG"
        );
    }

    fn write_png(p: &std::path::Path, w: u32, h: u32) {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(w, h, image::Rgb([5, 6, 7])));
        let mut b = std::io::Cursor::new(Vec::new());
        img.write_to(&mut b, image::ImageFormat::Png).unwrap();
        std::fs::write(p, b.into_inner()).unwrap();
    }

    #[test]
    fn image_original_size_and_format_passes_through_untouched() {
        // tuxlink-rbhg: Original size + Original format = byte-for-byte the
        // source file (no decode/re-encode), keeping the original extension.
        let td = TempDir::new().unwrap();
        let p = td.path().join("shot.png");
        write_png(&p, 100, 80);
        let raw = std::fs::read(&p).unwrap();
        let out = prepare_blocking(&p, PresetArg::Original, FormatArg::Original).unwrap();
        assert_eq!(out.kind, "image");
        assert_eq!(out.filename, "shot.png", "original filename/extension kept");
        assert_eq!(out.bytes, raw, "bytes must be the untouched original");
        assert_eq!(out.original_len, out.new_len);
    }

    #[test]
    fn image_resize_with_keep_format_keeps_png() {
        // Resizing forces a re-encode; "keep original format" on a PNG re-encodes
        // to PNG (not JPEG) — the operator's format choice is honored.
        let td = TempDir::new().unwrap();
        let p = td.path().join("shot.png");
        write_png(&p, 2000, 1500);
        let out = prepare_blocking(&p, PresetArg::Small, FormatArg::Original).unwrap();
        assert_eq!(out.filename, "shot.png", "keep-format on a resized PNG stays .png");
    }

    #[test]
    fn image_original_size_with_format_change_re_encodes_full_res() {
        // Format change alone (no resize) still re-encodes — e.g. recompress a
        // full-res JPEG to WebP to save bytes without changing dimensions.
        let td = TempDir::new().unwrap();
        let p = td.path().join("shot.png");
        write_png(&p, 300, 200);
        let out = prepare_blocking(&p, PresetArg::Original, FormatArg::Webp).unwrap();
        assert_eq!(out.kind, "image");
        assert_eq!(out.filename, "shot.webp");
    }

    #[test]
    fn resolve_out_format_keeps_known_source_formats_else_jpeg() {
        assert_eq!(resolve_out_format(FormatArg::Original, "png"), OutFormat::Png);
        assert_eq!(resolve_out_format(FormatArg::Original, "webp"), OutFormat::Webp);
        assert_eq!(resolve_out_format(FormatArg::Original, "jpeg"), OutFormat::Jpeg);
        // HEIC can't be re-saved as HEIC → JPEG fallback.
        assert_eq!(resolve_out_format(FormatArg::Original, "heic"), OutFormat::Jpeg);
        assert_eq!(resolve_out_format(FormatArg::Jpeg, "png"), OutFormat::Jpeg);
        assert_eq!(resolve_out_format(FormatArg::Webp, "png"), OutFormat::Webp);
    }
}
