// Display-side sanitization for untrusted attachment filenames.
//
// Per spec §10. Server-side outbound rejection of \r\n\0 in attachment
// filenames is already enforced at src-tauri/src/winlink/compose.rs
// (9phd Codex P2.2). This module is the inbound display-side defense:
// incoming messages may carry malicious attachment names; we sanitize
// before render so display-layer code never sees raw control chars,
// path separators, or unbounded-length names.

const MAX_FILENAME_DISPLAY_LEN = 255;

/// Sanitize a filename for display. Strips:
///   - ASCII control chars (0x00-0x1f) and DEL (0x7f)
///   - Path separators (/ \) → underscore (prevents perceived directory)
/// Truncates to 255 chars (filesystem norm).
///
/// Does NOT decode percent-encoding or re-encode for HTML — React's
/// default escaping handles HTML safety. This function is concerned
/// only with control chars + path injection + length.
export function sanitizeAttachmentName(name: string): string {
  return name
    .replace(/[\x00-\x1f\x7f]/g, '')
    .replace(/[/\\]/g, '_')
    .slice(0, MAX_FILENAME_DISPLAY_LEN);
}
