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

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PresetArg {
    Small,
    Medium,
    Large,
    Original,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FormatArg {
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
impl From<FormatArg> for OutFormat {
    fn from(f: FormatArg) -> Self {
        match f {
            FormatArg::Jpeg => OutFormat::Jpeg,
            FormatArg::Webp => OutFormat::Webp,
        }
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

    if IMAGE_EXTS.contains(&ext_lower.as_str()) {
        let out_format: OutFormat = format.into();
        let t = transcode_image_bytes(&raw, preset.into(), out_format).map_err(|e| e.to_string())?;
        let filename = format!("{stem}.{}", out_format.ext());
        let new_len = t.bytes.len();
        Ok(PreparedAttachment {
            filename,
            bytes: t.bytes,
            kind: "image".into(),
            original_len,
            new_len,
        })
    } else {
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("attachment")
            .to_string();
        Ok(PreparedAttachment {
            filename,
            new_len: original_len,
            bytes: raw,
            kind: "file".into(),
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
}
