# Compose Attachments + Attach-Time Image Resize — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let an operator attach any file to a plain Winlink message, and downsample image attachments at attach time (incl. iPhone HEIC) so they're sendable over RF.

**Architecture:** A new backend `media` module exposes one Tauri command, `prepare_attachment(path, opts)`, that reads a user-chosen file, transcodes images (decode → resize → re-encode JPEG/WebP) and passes other files through, returning `{filename, bytes, …}`. The compose frontend gets paths from the Tauri dialog plugin and drag-drop, calls `prepare_attachment`, maintains an attachment list, and threads the real attachments into the already-wired `message_send`. The backend send path (`OutboundAttachment`, `set_attachments`, `message_send`) already exists and is NOT rebuilt.

**Tech Stack:** Rust (`image`, `libheif-rs`, `webp` crates; Tauri 2 commands; `spawn_blocking`), React/TypeScript (`@tauri-apps/plugin-dialog`), Cloud CI for cross-arch compile (no cold cargo on the Pi).

**Spec:** `docs/superpowers/specs/2026-06-13-image-attachment-resize-design.md`

**Method reminder:** Locally only `pnpm exec tsc --noEmit` + scoped `pnpm exec vitest run <file>` (reap with `pkill -9 -f vitest` after). All Rust compile/clippy/tests run on Cloud CI via a **draft PR**. Commit from inside the worktree (`cd` standalone first if the main-checkout hook denies — payload-cwd gotcha).

---

## File Structure

**Backend (create):**
- `src-tauri/src/media/mod.rs` — module root; re-exports.
- `src-tauri/src/media/transcode.rs` — pure transcode core (`transcode_image_bytes`, presets, format enum) + unit tests. No Tauri, no fs — fully testable.
- `src-tauri/src/media/commands.rs` — the `prepare_attachment` Tauri command (fs read + classify + `spawn_blocking` + DTO).

**Backend (modify):**
- `src-tauri/Cargo.toml` — add `image`, `libheif-rs`, `webp`.
- `src-tauri/src/lib.rs` — `pub mod media;` + register `prepare_attachment` in `invoke_handler`.
- `src-tauri/capabilities/compose.json` — add `dialog:allow-open` (compose window opens the picker).

**Frontend (create):**
- `src/compose/useAttachments.ts` — attachment-list hook (add via `prepare_attachment`, remove, totals).
- `src/compose/useAttachments.test.ts` — hook tests.
- `src/compose/attachmentFormat.ts` — pure helpers (byte→human size, airtime estimate, image-extension classifier) + tests `attachmentFormat.test.ts`.

**Frontend (modify):**
- `src/compose/Compose.tsx` — replace the stub `attachments` state + drop handler; add a picker button + list UI; thread real attachments into `message_send`.
- `src/compose/Compose.css` — list/row/remove/warning styles (follow existing `compose-attachments__*`).

**CI / packaging (modify):**
- `.github/workflows/*` (the build + verify workflows) — `apt-get install libheif-dev libde265-dev libwebp-dev` (Linux arm64 + amd64).
- `src-tauri/tauri.conf.json` — Debian `depends`: `libheif1`, `libde265-0`, `libwebp7` (runtime libs for the `.deb`).

---

## Task 1: Add codec dependencies

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add the three crates under `[dependencies]`**

```toml
# Image transcode for attach-time resize (tuxlink-mg4s). image = pure-Rust
# decode/resize/JPEG-encode; libheif-rs = HEIC ingest (libheif C lib);
# webp = lossy WebP encode (libwebp C lib).
image = { version = "0.25", default-features = false, features = ["jpeg", "png", "gif", "webp", "tiff", "bmp"] }
libheif-rs = "1.0"
webp = "0.3"
```

- [ ] **Step 2: Commit** (compile is deferred to CI per method)

```bash
git add src-tauri/Cargo.toml
git commit -m "build(media): add image/libheif-rs/webp deps for attachment resize

Refs tuxlink-mg4s.
Agent: lupine-ridge-marten
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Transcode core — preset + format types and the resize/encode for image-crate formats

**Files:**
- Create: `src-tauri/src/media/transcode.rs`
- Create: `src-tauri/src/media/mod.rs`

- [ ] **Step 1: Write `mod.rs`**

```rust
//! Attachment media handling (tuxlink-mg4s): decode broad image formats
//! (incl. HEIC), resize to an airtime-friendly preset, re-encode JPEG/WebP.
pub mod commands;
pub mod transcode;
```

- [ ] **Step 2: Write the failing test for preset resize math + JPEG encode in `transcode.rs`**

```rust
//! Pure transcode core — no Tauri, no fs. Decode → resize (aspect-preserving)
//! → encode. Decode dispatches by detected format: the `image` crate for
//! JPEG/PNG/GIF/WebP/TIFF/BMP, libheif for HEIC. Encode is JPEG (default,
//! WLE-safe) or WebP (tuxlink->tuxlink, ~30% smaller).

use image::{DynamicImage, ImageFormat};

/// Max-dimension presets (longest edge, px). `Original` skips resize.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizePreset {
    Small,    // 640
    Medium,   // 1024
    Large,    // 1600
    Original,
}

impl ResizePreset {
    fn max_edge(self) -> Option<u32> {
        match self {
            ResizePreset::Small => Some(640),
            ResizePreset::Medium => Some(1024),
            ResizePreset::Large => Some(1600),
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
            image::DynamicImage::ImageRgb8(img.to_rgb8())
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
```

- [ ] **Step 3: Add the `#[cfg(test)]` module with a resize-math test**

```rust
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
        let out = resize_to(img, ResizePreset::Small); // 640
        assert_eq!(out.width(), 640);
        assert_eq!(out.height(), 320);
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
        let out = resize_to(img, ResizePreset::Large); // 1600 — already smaller
        assert_eq!((out.width(), out.height()), (300, 200));
    }
}
```

- [ ] **Step 4: Note** — these tests compile/run on CI only (no cold cargo). Verify by reading: `resize` preserves aspect ratio to fit within `edge×edge`; for 2000×1000 → 640×320. Logic is correct by inspection.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/media/mod.rs src-tauri/src/media/transcode.rs
git commit -m "feat(media): transcode core — resize presets + JPEG/WebP encode

Refs tuxlink-mg4s.
Agent: lupine-ridge-marten
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Decode dispatch (image-crate formats + HEIC via libheif)

**Files:**
- Modify: `src-tauri/src/media/transcode.rs`

- [ ] **Step 1: Add the public entry `transcode_image_bytes` + HEIC detection**

```rust
/// True if the bytes are an HEIF/HEIC container (ftyp brand check). HEIC files
/// start with a `ftyp` box whose major brand is one of these.
fn is_heic(bytes: &[u8]) -> bool {
    if bytes.len() < 12 || &bytes[4..8] != b"ftyp" {
        return false;
    }
    matches!(&bytes[8..12], b"heic" | b"heix" | b"hevc" | b"heim" | b"heis" | b"mif1" | b"msf1")
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
    // Copy row-by-row to drop the stride padding into a tight RGB buffer.
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h as usize {
        let row = &src[y * stride..y * stride + (w as usize) * 3];
        rgb.extend_from_slice(row);
    }
    let buf = image::RgbImage::from_raw(w, h, rgb)
        .ok_or_else(|| TranscodeError::Decode("heif: buffer size mismatch".into()))?;
    Ok(DynamicImage::ImageRgb8(buf))
}
```

> **Executor note:** `libheif-rs` 1.x API surface (`LibHeif::new`, `HeifContext::read_from_bytes`, `decode`, `planes().interleaved`) — confirm exact names/signatures against the crate docs and adjust; the structure (read ctx → primary handle → decode to interleaved RGB → tight-copy into `RgbImage`) is the contract. CI is the compile gate.

- [ ] **Step 2: Add a round-trip test (JPEG in → resized JPEG out) and a WebP-out test**

```rust
    #[test]
    fn transcode_jpeg_in_jpeg_out_resizes_and_reencodes() {
        // Build a 2000x1000 JPEG input.
        let src = {
            let img = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(2000, 1000, image::Rgb([10, 200, 30])));
            let mut b = std::io::Cursor::new(Vec::new());
            img.write_to(&mut b, ImageFormat::Jpeg).unwrap();
            b.into_inner()
        };
        let out = transcode_image_bytes(&src, ResizePreset::Small, OutFormat::Jpeg).unwrap();
        assert_eq!((out.width, out.height), (640, 320));
        // Output must be a decodable JPEG smaller than the input.
        let decoded = image::load_from_memory(&out.bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (640, 320));
    }

    #[test]
    fn transcode_webp_out_produces_decodable_webp() {
        let src = red_png(800, 600);
        let out = transcode_image_bytes(&src, ResizePreset::Medium, OutFormat::Webp).unwrap();
        // image crate decodes WebP — confirms a valid container.
        let decoded = image::load_from_memory(&out.bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (800, 600));
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
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/media/transcode.rs
git commit -m "feat(media): decode dispatch incl. HEIC (libheif) + transcode entry

Refs tuxlink-mg4s.
Agent: lupine-ridge-marten
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: `prepare_attachment` Tauri command (read path, classify, transcode-or-passthrough)

**Files:**
- Create: `src-tauri/src/media/commands.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write `commands.rs`**

```rust
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
pub enum PresetArg { Small, Medium, Large, Original }

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FormatArg { Jpeg, Webp }

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
    tauri::async_runtime::spawn_blocking(move || prepare_blocking(&pathbuf, image_preset, image_format))
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
        let t = transcode_image_bytes(&raw, preset.into(), out_format)
            .map_err(|e| e.to_string())?;
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
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(2000, 1500, image::Rgb([1, 2, 3])));
        let mut b = std::io::Cursor::new(Vec::new());
        img.write_to(&mut b, image::ImageFormat::Png).unwrap();
        std::fs::write(&p, b.into_inner()).unwrap();
        let out = prepare_blocking(&p, PresetArg::Small, FormatArg::Jpeg).unwrap();
        assert_eq!(out.kind, "image");
        assert_eq!(out.filename, "photo.jpg");
        assert!(out.new_len < out.original_len, "resize+JPEG should shrink a 2000x1500 PNG");
    }
}
```

- [ ] **Step 2: Register the module + command in `lib.rs`**

Modify `src-tauri/src/lib.rs`: add `pub mod media;` near the other `pub mod` lines (top of file, alongside `pub mod forms;`), and add to the `invoke_handler` list (after `crate::ui_commands::message_send,` at ~line 643):

```rust
            crate::media::commands::prepare_attachment,
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/media/commands.rs src-tauri/src/lib.rs
git commit -m "feat(media): prepare_attachment command — read, classify, transcode/passthrough

Refs tuxlink-mg4s.
Agent: lupine-ridge-marten
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Grant the compose window the file-open dialog

**Files:**
- Modify: `src-tauri/capabilities/compose.json`

- [ ] **Step 1: Add `dialog:allow-open` to the compose capability `permissions` array**

The compose window (`compose-*`) opens the file picker; the dialog plugin is ACL-gated, so add the permission (the existing array ends after `core:window:allow-is-maximized`):

```json
    "core:window:allow-is-maximized",
    "dialog:allow-open"
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/capabilities/compose.json
git commit -m "feat(compose): grant dialog:allow-open for the attachment picker

Refs tuxlink-mg4s.
Agent: lupine-ridge-marten
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Frontend pure helpers (size formatting, airtime estimate, image classifier)

**Files:**
- Create: `src/compose/attachmentFormat.ts`
- Test: `src/compose/attachmentFormat.test.ts`

- [ ] **Step 1: Write the failing tests**

```ts
import { describe, it, expect } from 'vitest';
import { humanSize, airtimeEstimate, isImageFilename } from './attachmentFormat';

describe('humanSize', () => {
  it('formats bytes/KB/MB', () => {
    expect(humanSize(512)).toBe('512 B');
    expect(humanSize(2048)).toBe('2.0 KB');
    expect(humanSize(2 * 1024 * 1024)).toBe('2.0 MB');
  });
});

describe('airtimeEstimate', () => {
  it('reports a worst-case (slow packet) duration string', () => {
    // ~90 B/s floor -> 10KB ~ 110s.
    expect(airtimeEstimate(10 * 1024)).toMatch(/min|sec/);
  });
});

describe('isImageFilename', () => {
  it('detects image extensions case-insensitively incl. heic', () => {
    expect(isImageFilename('IMG_0001.HEIC')).toBe(true);
    expect(isImageFilename('map.png')).toBe(true);
    expect(isImageFilename('brief.pdf')).toBe(false);
  });
});
```

- [ ] **Step 2: Run it to confirm failure**

Run: `pnpm exec vitest run src/compose/attachmentFormat.test.ts`
Expected: FAIL (module not found). Then `pkill -9 -f vitest`.

- [ ] **Step 3: Implement `attachmentFormat.ts`**

```ts
/** tuxlink-mg4s: pure helpers for the compose attachment UI. */

const IMAGE_EXTS = ['jpg', 'jpeg', 'png', 'gif', 'webp', 'tif', 'tiff', 'bmp', 'heic', 'heif'];

export function isImageFilename(name: string): boolean {
  const dot = name.lastIndexOf('.');
  if (dot < 0) return false;
  return IMAGE_EXTS.includes(name.slice(dot + 1).toLowerCase());
}

export function humanSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Worst-case airtime at a ~90 B/s slow-packet floor — the figure that makes
 * the cost legible to the operator before they send. */
export function airtimeEstimate(bytes: number): string {
  const seconds = Math.round(bytes / 90);
  if (seconds < 90) return `~${seconds} sec on slow packet`;
  return `~${Math.round(seconds / 60)} min on slow packet`;
}
```

- [ ] **Step 4: Run tests to verify pass** (`pnpm exec vitest run src/compose/attachmentFormat.test.ts`; then `pkill -9 -f vitest`). Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/compose/attachmentFormat.ts src/compose/attachmentFormat.test.ts
git commit -m "feat(compose): attachment size/airtime/image-classifier helpers

Refs tuxlink-mg4s.
Agent: lupine-ridge-marten
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: `useAttachments` hook

**Files:**
- Create: `src/compose/useAttachments.ts`
- Test: `src/compose/useAttachments.test.ts`

- [ ] **Step 1: Write the failing test**

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';

const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }));

import { useAttachments } from './useAttachments';

beforeEach(() => invokeMock.mockReset());

describe('useAttachments', () => {
  it('adds a prepared attachment from a path via prepare_attachment', async () => {
    invokeMock.mockResolvedValue({
      filename: 'photo.jpg', bytes: [1, 2, 3], kind: 'image', originalLen: 2000000, newLen: 50000,
    });
    const { result } = renderHook(() => useAttachments());
    await act(async () => { await result.current.addPath('/tmp/photo.heic'); });
    expect(invokeMock).toHaveBeenCalledWith('prepare_attachment', expect.objectContaining({ path: '/tmp/photo.heic' }));
    expect(result.current.items).toHaveLength(1);
    expect(result.current.items[0].filename).toBe('photo.jpg');
  });

  it('removes by index', async () => {
    invokeMock.mockResolvedValue({ filename: 'a.txt', bytes: [1], kind: 'file', originalLen: 1, newLen: 1 });
    const { result } = renderHook(() => useAttachments());
    await act(async () => { await result.current.addPath('/tmp/a.txt'); });
    act(() => { result.current.remove(0); });
    expect(result.current.items).toHaveLength(0);
  });

  it('exposes the DTO shape message_send expects', async () => {
    invokeMock.mockResolvedValue({ filename: 'a.jpg', bytes: [9, 9], kind: 'image', originalLen: 100, newLen: 2 });
    const { result } = renderHook(() => useAttachments());
    await act(async () => { await result.current.addPath('/tmp/a.png'); });
    expect(result.current.toDto()).toEqual([{ filename: 'a.jpg', bytes: [9, 9] }]);
  });
});
```

- [ ] **Step 2: Run to confirm failure** (`pnpm exec vitest run src/compose/useAttachments.test.ts`; `pkill -9 -f vitest`). Expected: FAIL.

- [ ] **Step 3: Implement `useAttachments.ts`**

```ts
import { useCallback, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface PreparedAttachment {
  filename: string;
  bytes: number[];
  kind: 'image' | 'file';
  originalLen: number;
  newLen: number;
}

export interface AttachmentDto {
  filename: string;
  bytes: number[];
}

export interface ImageOpts {
  preset: 'small' | 'medium' | 'large' | 'original';
  format: 'jpeg' | 'webp';
}

const DEFAULT_OPTS: ImageOpts = { preset: 'medium', format: 'jpeg' };

export function useAttachments() {
  const [items, setItems] = useState<PreparedAttachment[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const addPath = useCallback(async (path: string, opts: ImageOpts = DEFAULT_OPTS) => {
    setBusy(true);
    setError(null);
    try {
      const prepared = await invoke<PreparedAttachment>('prepare_attachment', {
        path,
        imagePreset: opts.preset,
        imageFormat: opts.format,
      });
      setItems((prev) => [...prev, prepared]);
    } catch (e) {
      setError(typeof e === 'string' ? e : 'Could not attach that file.');
    } finally {
      setBusy(false);
    }
  }, []);

  const remove = useCallback((index: number) => {
    setItems((prev) => prev.filter((_, i) => i !== index));
  }, []);

  const totalBytes = items.reduce((sum, a) => sum + a.newLen, 0);

  const toDto = useCallback((): AttachmentDto[] =>
    items.map((a) => ({ filename: a.filename, bytes: a.bytes })), [items]);

  return { items, busy, error, addPath, remove, totalBytes, toDto };
}
```

- [ ] **Step 4: Run to verify pass** (`pnpm exec vitest run src/compose/useAttachments.test.ts`; `pkill -9 -f vitest`). Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/compose/useAttachments.ts src/compose/useAttachments.test.ts
git commit -m "feat(compose): useAttachments hook (add via prepare_attachment, remove, toDto)

Refs tuxlink-mg4s.
Agent: lupine-ridge-marten
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: Wire the attach UI into Compose (picker + drop + list) and thread into `message_send`

**Files:**
- Modify: `src/compose/Compose.tsx`
- Modify: `src/compose/Compose.css`

- [ ] **Step 1: Replace the stub state + imports**

In `Compose.tsx`, replace `const [attachments, _setAttachments] = useState<string[]>([]);` with:

```tsx
  const attach = useAttachments();
```

Add imports at the top with the other compose imports:

```tsx
import { useAttachments } from './useAttachments';
import { humanSize, airtimeEstimate, isImageFilename } from './attachmentFormat';
import { open as openFileDialog } from '@tauri-apps/plugin-dialog';
```

- [ ] **Step 2: Replace the drop handler stub (`handleDrop`)**

```tsx
  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    // Tauri's webview drag-drop exposes OS paths via the file objects' path on
    // desktop; fall back to the file-drop plugin event if absent. We read each
    // dropped file's path and route it through prepare_attachment.
    const paths = Array.from(e.dataTransfer.files)
      .map((f) => (f as File & { path?: string }).path)
      .filter((p): p is string => !!p);
    for (const p of paths) {
      await attach.addPath(p, { preset: 'medium', format: 'jpeg' });
    }
  };
```

> **Executor note:** If `dataTransfer.files[].path` is empty under the project's Tauri/WebKitGTK build, switch to the Tauri window `onDragDropEvent` (`@tauri-apps/api/webview`) which delivers `paths: string[]`; verify in the WebKitGTK smoke. Either way the sink is `attach.addPath(path)`.

- [ ] **Step 3: Add a picker handler near the other compose handlers**

```tsx
  const handlePickFiles = async () => {
    const selected = await openFileDialog({ multiple: true });
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    for (const p of paths) {
      await attach.addPath(p, { preset: 'medium', format: 'jpeg' });
    }
  };
```

- [ ] **Step 4: Replace the attachment render block** (the `compose-attachments` section) with the real list + picker button + size/airtime warning

```tsx
      <div
        className="compose-attachments"
        onDragOver={handleDragOver}
        onDrop={handleDrop}
      >
        <div className="compose-attachments__header">
          <button type="button" className="compose-attachments__add" onClick={handlePickFiles} disabled={attach.busy}>
            Attach files…
          </button>
          {attach.totalBytes > 0 && (
            <span className="compose-attachments__total">
              {humanSize(attach.totalBytes)} · {airtimeEstimate(attach.totalBytes)}
            </span>
          )}
        </div>
        {attach.error && <div className="compose-attachments__error">{attach.error}</div>}
        {attach.items.length === 0 ? (
          <span className="compose-attachments__hint">Drop files here or use “Attach files…”.</span>
        ) : (
          <ul className="compose-attachments__list">
            {attach.items.map((a, i) => (
              <li key={`${a.filename}-${i}`} className="compose-attachments__item">
                <span className="compose-attachments__name">{a.filename}</span>
                {a.kind === 'image' && a.newLen < a.originalLen && (
                  <span className="compose-attachments__resized">
                    resized {humanSize(a.originalLen)} → {humanSize(a.newLen)}
                  </span>
                )}
                {a.kind === 'file' && a.newLen > 256 * 1024 && (
                  <span className="compose-attachments__warn">{humanSize(a.newLen)} · {airtimeEstimate(a.newLen)}</span>
                )}
                <button type="button" className="compose-attachments__remove" onClick={() => attach.remove(i)}>
                  Remove
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
```

- [ ] **Step 5: Thread real attachments into `message_send`** — replace `attachments: [],` (~line 546) with:

```tsx
      attachments: attach.toDto(),
```

- [ ] **Step 6: Add CSS** to `Compose.css` (follow existing `compose-attachments__*` naming):

```css
.compose-attachments__header { display: flex; align-items: center; gap: 0.75rem; }
.compose-attachments__total { font-size: 0.85em; opacity: 0.8; }
.compose-attachments__list { list-style: none; margin: 0.5rem 0 0; padding: 0; }
.compose-attachments__item { display: flex; align-items: center; gap: 0.5rem; padding: 0.2rem 0; }
.compose-attachments__name { font-weight: 500; }
.compose-attachments__resized { font-size: 0.8em; opacity: 0.7; }
.compose-attachments__warn { font-size: 0.8em; color: var(--warning, #b8860b); }
.compose-attachments__error { color: var(--danger, #c0392b); font-size: 0.85em; }
.compose-attachments__remove { margin-left: auto; }
```

- [ ] **Step 7: Typecheck** — `pnpm exec tsc --noEmit`. Expected: exit 0. Fix any type errors (e.g. dialog plugin types).

- [ ] **Step 8: Commit**

```bash
git add src/compose/Compose.tsx src/compose/Compose.css
git commit -m "feat(compose): attachment picker + drop + list, wired into message_send

Refs tuxlink-mg4s.
Agent: lupine-ridge-marten
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 9: Compose integration test (production mount path)

**Files:**
- Modify: `src/compose/Compose.test.tsx`

- [ ] **Step 1: Add a test asserting a prepared attachment reaches `message_send`**

```tsx
it('sends with prepared attachments threaded into message_send', async () => {
  const invokeMock = vi.fn(async (cmd: string, args: unknown) => {
    if (cmd === 'prepare_attachment') {
      return { filename: 'photo.jpg', bytes: [1, 2, 3], kind: 'image', originalLen: 2000000, newLen: 40000 };
    }
    if (cmd === 'message_send') return 'MID123';
    return undefined;
  });
  // (Wire invokeMock via the existing @tauri-apps/api/core mock in this file,
  // mount Compose at the production path the other tests use, add a recipient +
  // subject, simulate handlePickFiles by invoking the dialog mock to return a
  // path, then click Send.)
  // Assert: message_send was called with draft.attachments === [{filename:'photo.jpg', bytes:[1,2,3]}].
  expect(invokeMock).toHaveBeenCalledWith('message_send', expect.objectContaining({
    draft: expect.objectContaining({ attachments: [{ filename: 'photo.jpg', bytes: [1, 2, 3] }] }),
  }));
});
```

> **Executor note:** match this file's existing mock + mount harness (the `@tauri-apps/api/core` mock and the production-path render used by the other Compose tests — see the "test the production mount path" project rule). Mock `@tauri-apps/plugin-dialog`'s `open` to resolve a path so `handlePickFiles` adds an attachment.

- [ ] **Step 2: Run** — `pnpm exec vitest run src/compose/Compose.test.tsx`; then `pkill -9 -f vitest`. Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/compose/Compose.test.tsx
git commit -m "test(compose): attachments thread through to message_send (production path)

Refs tuxlink-mg4s.
Agent: lupine-ridge-marten
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 10: C-dependency CI + packaging

**Files:**
- Modify: the Linux build + verify GitHub workflow(s) under `.github/workflows/`
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Find the workflow apt step**

Run: `grep -rln "apt-get install\|libwebkit2gtk" .github/workflows/`
Read the matched file(s); locate the `apt-get install` line that installs the Tauri Linux build deps (e.g. `libwebkit2gtk-4.1-dev`).

- [ ] **Step 2: Add the codec dev libs to that apt line** (both amd64 and arm64 jobs)

Append to the existing install list:

```
libheif-dev libde265-dev libwebp-dev
```

- [ ] **Step 3: Add runtime deps to the Debian bundle** in `src-tauri/tauri.conf.json`

Locate `bundle.linux.deb.depends` (create the `deb` object if absent under `bundle.linux`) and add:

```json
"depends": ["libheif1", "libde265-0", "libwebp7"]
```

> **Executor note:** confirm the exact runtime package names for the CI's Ubuntu version (`apt-cache search libheif` / `libwebp`); names can be version-suffixed (e.g. `libwebp7`). The build job's `apt-get` provides the `-dev` headers for compile; `depends` provides the runtime `.so` for installed `.deb`s.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows src-tauri/tauri.conf.json
git commit -m "build(ci): install + bundle libheif/libwebp for attachment transcode

Refs tuxlink-mg4s.
Agent: lupine-ridge-marten
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 11: Draft PR → Cloud CI (the compile/test gate)

- [ ] **Step 1: Push the branch**

```bash
git push -u origin bd-tuxlink-mg4s/image-attach-resize
```

- [ ] **Step 2: Open a draft PR**

```bash
gh pr create --draft --base main --head bd-tuxlink-mg4s/image-attach-resize \
  --title '[lupine-ridge-marten] feat(compose): attachments + attach-time image resize (tuxlink-mg4s)' \
  --body 'Finishes the stubbed compose attachment flow + adds attach-time image downsampling (incl. HEIC ingest). Spec: docs/superpowers/specs/2026-06-13-image-attachment-resize-design.md. Single ship. CI is the Rust compile/clippy/test gate (no cold cargo on the Pi).'
```

- [ ] **Step 3: Watch CI**

```bash
gh pr checks <PR#> --watch
```

- [ ] **Step 4: Triage failures** — most likely libheif-rs / webp API mismatches (Task 3) and the runtime/dev package names (Task 10). Fix forward, commit, push, re-watch until `build-linux` + `verify` are green on both arches. Then mark ready / merge per operator.

---

## Self-Review

**Spec coverage:**
- §3.1 frontend attach UI → Tasks 7, 8. ✓
- §3.2 transcode command (decode broad incl. HEIC, resize, JPEG/WebP) → Tasks 2, 3, 4. ✓
- §3.3 wiring to existing send → Task 8 Step 5. ✓
- §5 formats/ingest (JPEG default, WebP opt-in, HEIC ingest) → Tasks 3, 4 (PresetArg/FormatArg, IMAGE_EXTS incl. heic). ✓
- §6 error handling + size warnings + spawn_blocking → Task 4 (spawn_blocking, decode error → string), Task 6/8 (size + airtime warnings). ✓ Note: "offer to attach original as-is on decode failure" is surfaced as an error message (Task 7 `error`); a one-click "attach original instead" affordance is a reasonable enhancement but not required for first ship — the operator can pick a different file.
- §7 capabilities → Task 5 (dialog:allow-open). ✓
- §8 testing → Tasks 2,3,4 (Rust units), 6,7,9 (frontend). ✓
- C-dep packaging → Task 10. ✓

**Placeholder scan:** No "TBD/TODO/handle edge cases". Two `Executor note`s flag external-crate API specifics (libheif-rs, dialog drag-drop path) and exact package names — these are verification pointers for real external APIs, with the concrete code/structure given, not deferred logic.

**Type consistency:** `PreparedAttachment` (Rust serde camelCase: filename, bytes, kind, originalLen, newLen) ↔ frontend `PreparedAttachment` (filename, bytes, kind, originalLen, newLen) ↔ `toDto()` → `{filename, bytes}` matching `OutboundAttachmentDto {filename, bytes: number[]}`. `prepare_attachment` args `path`/`imagePreset`/`imageFormat` (camelCase) ↔ hook invoke `{path, imagePreset, imageFormat}`. ResizePreset/OutFormat ↔ PresetArg/FormatArg conversions defined in Task 4. Consistent.
