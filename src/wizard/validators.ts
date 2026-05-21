// validators.ts — wizard-cluster plan Phase 3 Task 3.1
// Spec: docs/superpowers/specs/2026-05-18-onboarding-wizard-cluster-design.md §5.9 (non-ASCII)
// AMD-3 (loose validator), AMD-1 (grid).
//
// The frontend validator is the UX first-pass.
// The Rust wizard_persist_cms does a SECOND normalization + validation pass (defense-in-depth).
// Both layers MUST reject non-ASCII callsigns per spec §5.9.

/**
 * Validate a callsign/identifier input.
 * Returns null if valid; a human-readable error string if invalid.
 *
 * "Loose" per AMD-3: non-empty + no whitespace + ≤32 chars + ASCII-printable.
 * Accepts tactical strings (EOC-1, BAOFENG-FM-01).
 * REJECTS any non-ASCII character (Cyrillic homoglyphs, zero-width joiners, etc.) per spec §5.9.
 */
export function validateCallsign(input: string): string | null {
  if (!input) return 'Callsign is required (non-empty).';
  if (/\s/.test(input)) return 'Callsign must contain no internal whitespace.';
  if (input.length > 32) return 'Callsign must be ≤32 characters.';
  // ASCII-printable range: 0x20–0x7E.  Rejects control chars (0x00–0x1F, 0x7F)
  // AND any non-ASCII (≥0x80) — homoglyph + zero-width guard per spec §5.9.
  // eslint-disable-next-line no-control-regex
  if (!/^[\x20-\x7E]+$/.test(input)) {
    return 'Callsign must contain only ASCII letters, digits, and common symbols — no accented or non-Latin characters.';
  }
  return null;
}

/**
 * Validate a CMS password.
 * Returns null if valid; a human-readable error string if invalid.
 *
 * ≥6 chars per the Winlink Express convention (CMS minimum).
 * No upper-bound enforced — the CMS is authoritative for exact acceptance.
 */
export function validatePassword(input: string): string | null {
  if (!input) return 'Password is required.';
  if (input.length < 6) return 'Password must be ≥6 characters (per Winlink Express convention).';
  return null;
}

/**
 * Validate a Maidenhead grid locator.
 * Returns null if valid or empty (field is optional); a human-readable error string if invalid.
 *
 * Accepts 4-char (field + square, case-insensitive) or 6-char (+ sub-square).
 * Field pair: [A-R][A-R] (latitude bands A-R wrap around the globe).
 * Square pair: [0-9][0-9].
 * Sub-square pair (6-char only): [A-X][A-X].
 */
export function validateGrid(input: string): string | null {
  if (!input) return null;  // optional — empty is valid
  const re4 = /^[A-Ra-r]{2}[0-9]{2}$/;
  const re6 = /^[A-Ra-r]{2}[0-9]{2}[A-Xa-x]{2}$/;
  if (!re4.test(input) && !re6.test(input)) {
    return 'Grid must be a 4- or 6-character Maidenhead locator (e.g. EM75 or EM75xx).';
  }
  return null;
}

/**
 * Normalize a Maidenhead grid locator to canonical form:
 * first 2 chars UPPERCASE (field), next 2 chars unchanged (digits), last 2 LOWERCASE (sub-square).
 * Input assumed to already pass validateGrid; this is for display/storage normalization only.
 */
export function normalizeGrid(input: string): string {
  if (input.length === 4) {
    return input.slice(0, 2).toUpperCase() + input.slice(2);
  }
  // 6-char: field uppercase, digits unchanged, sub-square lowercase
  return input.slice(0, 2).toUpperCase() + input.slice(2, 4) + input.slice(4).toLowerCase();
}
