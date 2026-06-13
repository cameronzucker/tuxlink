# Design — Compose attachments (all types) + attach-time image resize

**Date:** 2026-06-13 · **Agent:** lupine-ridge-marten · **bd:** tuxlink-mg4s · **Epic:** tuxlink-zkuk (forms push)

## 1. Goal & macro context

Let an operator attach files to a plain Winlink message, and — for image files —
downsample them at attach time so they're sendable over RF. Airtime is the driver:
at marginal-HF throughput (~110 B/s) a 2 MB phone photo is **~5 hours** of airtime;
resized to ~50 KB it is **~8 minutes**. The resize (pixel-dimension/quality
reduction) is the dominant lever — B2F's LZHUF compression (already shipped) does
nothing for an already-compressed JPEG/HEIC.

This is WLE parity (Winlink Express has a built-in attach-time image resizer),
compose-side (not RADIO-1 gated), and entirely sender-side (no interop or
wire-format change). It is **not** a tuxlink-added safeguard — it mirrors a WLE
feature and is operator-driven, never an enforced cap.

## 2. What already exists (scope guard)

- **Backend send path: complete.** `OutboundAttachment { filename, bytes }`,
  `OutboundMessage.attachments`, `winlink::message::set_attachments` (synthesizes
  `File:` headers, builds the B2F MIME message), LZHUF compression via
  `to_proposal()`, and `message_send` — which **already maps
  `draft.attachments` (DTO: filename + bytes) into the outbound message and sends
  it.** The IPC contract accepts attachments today.
- **Frontend compose attach UI: an inert stub.** The drop zone only `console.warn`s;
  `Compose.tsx` hard-passes `attachments: []`. So a user currently cannot attach
  any file to a plain message.

**Therefore the gap is: (a) a real frontend attach UI wired to the existing
`message_send`, and (b) a new backend image-transcode command the frontend calls
for image files.** No backend send work, no wire change.

## 3. Architecture

Three units, each independently testable:

### 3.1 Frontend — attach UI (`Compose.tsx` + a small `useAttachments` hook)
- Add files via the Tauri **dialog** plugin (`open`, multi-select) and via the
  existing **drag-drop** zone (replace the `console.warn` stub).
- For each selected file: read bytes (Tauri **fs** plugin `readFile` for picker
  paths; the drop event supplies bytes/paths per the Tauri file-drop API).
- Classify by extension/MIME. **Image** files are routed through the transcode
  command (3.2) before being added; **non-image** files are added as-is, with a
  size warning when large (see §6). Maintain an `attachments: {filename, bytes,
  originalName, originalBytes, kind}[]` list with add/remove; render the list
  (replacing today's placeholder).
- On send, pass the real `attachments` (filename + base64/bytes) to `message_send`
  instead of `[]`.

### 3.2 Backend — image transcode command (`media` module, new)
- `#[tauri::command] transcode_image(bytes, preset, format) -> TranscodeResult`
  where `TranscodeResult { bytes, filename_ext, width, height, original_len,
  new_len }`.
- **Decode (broad ingest):** the `image` crate (JPEG/PNG/GIF/WebP/TIFF/BMP)
  **plus HEIC via libheif** (`libheif-rs`) so iPhone photos ingest directly.
- **Resize:** to the chosen preset's max dimension, preserving aspect ratio
  (Lanczos3). Presets (proposed): Small 640px / Medium 1024px / Large 1600px /
  Original. (WLE-style choices; final numbers in the plan.)
- **Encode (narrow wire):** JPEG (quality ~80) **default**; **WebP opt-in**
  (libwebp via the `webp` crate) for tuxlink→tuxlink. Format is the caller's
  choice; default JPEG keeps Winlink-Express recipients safe.

### 3.3 Wiring — already done
`message_send` consumes `draft.attachments`; the frontend just has to populate it.

## 4. Data flow

operator picks/drops file → frontend reads bytes → if image: `transcode_image`
(backend decode→resize→encode) → frontend shows original vs new size → attachment
list → `message_send(draft{..., attachments})` → existing `set_attachments` →
B2F MIME + LZHUF → transport.

## 5. Format & ingest decisions (locked, with phasing)

Locked direction (from the 2026-06-13 design session):
- **Ingest = breadth** (accept whatever device shows up in a contingency);
  **wire = narrow & safe.**
- **Wire format:** JPEG default (decodes everywhere incl. Winlink Express — safe
  when the recipient is unknown). **WebP opt-in** for tuxlink→tuxlink (~30%
  smaller, displays in our WebKitGTK 2.52). **AVIF** noted as a later "max
  compression" opt-in, blocked on Pi encode-time + decode verification. **HEIC is
  never a wire format** (HEVC patents; WebKitGTK can't display it).
- **HEIC ingest** (iPhone photos — the contingency "people show up with whatever
  devices" case): decode via **libheif** (`libheif-rs`; C dep). Decode-only is the
  lower-risk patent case. macOS/Windows decode HEIC at OS level, so Linux is the
  primary bundling target — but the build vendors/links libheif uniformly so
  behavior is identical across platforms rather than OS-conditional.

**Single ship (no phasing).** Everything above — any-file attach, broad decode
incl. HEIC, resize, JPEG-default/WebP-opt-in encode — lands in one feature.
Phasing was rejected: it has caused feature-edge gaps before, and the codec
landscape is fully in context now. The C-dependency integration (libheif,
libwebp) is part of this build and is verified through Cloud CI on both arches
(no cold cargo on the Pi); the cross-platform `.deb`/bundle packaging of those
libs is an explicit plan task, not a follow-on.

## 6. Error handling & limits
- Corrupt/undecodable image → surface a clear error; offer to attach the original
  as-is (non-image path) rather than silently failing.
- Large non-image attachment → warn with the airtime estimate ("~N MB ≈ ~T on
  HF") but allow (operator's call; no hard cap — not a tuxlink-added safeguard).
- Total message-size advisory shown in compose (sum of attachment bytes).
- `transcode_image` is pure/synchronous CPU work → run under `spawn_blocking` so
  it never stalls the async runtime (matches `forms_import_preview`).

## 7. Capabilities
- Tauri `dialog:allow-open` + `fs` read scoped to user-chosen paths (the dialog
  returns the path; reading it is the minimal grant). Confirm exact capability
  shape in the plan; follow the existing `forms-webview.json`/import precedent.

## 8. Testing
- Backend: `transcode_image` unit tests — resize math (aspect-preserving),
  format/quality, decode of each supported input, oversize input, corrupt input.
  Pure-Rust so they run in CI cheaply.
- Frontend: `useAttachments` add/remove/classify; Compose passes real attachments
  to `message_send` (vitest, mocked invoke); the production mount path (per the
  "test the production mount path" memory).
- Method: no cold cargo on the Pi — tsc + scoped vitest locally; draft PR → Cloud
  CI compiles + tests both arches.

## 9. Out of scope
- **AVIF encode** — deferred pending a viability spike, NOT a convenience phase:
  rav1e encode time on a Pi 5 and AVIF decode in our WebKitGTK 2.52 / mac+Win
  bundles both need runtime confirmation before committing. JPEG+WebP already give
  a safe default + an efficient tuxlink→tuxlink option; AVIF is a marginal further
  gain with real unknowns. Revisit once the spike confirms encode-time + decode.
- Wire-format changes. Receive-side attachment rendering changes. Self-contained
  form payloads (separate, parked — tuxlink-z0gx).
