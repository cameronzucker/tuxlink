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
 * Validate a CMS password being CREATED (tuxlink-vfb3 sub-project 1).
 * Returns null if valid; a human-readable error string if invalid.
 *
 * Unlike {@link validatePassword} (sign-in: ≥6, no upper bound), account *creation*
 * enforces the live API rule of 6–12 characters, verified from the ServiceStack
 * metadata (`AccountAdd`: "no less than 6 and no more than 12 characters long"). The
 * server rejects out-of-range passwords, so bounding here gives immediate feedback
 * instead of a round-trip rejection.
 */
export function validateAccountPassword(input: string): string | null {
  if (!input) return 'Password is required.';
  if (input.length < 6 || input.length > 12) {
    return 'Password must be 6 to 12 characters.';
  }
  return null;
}

/**
 * Validate a callsign being used to CREATE a CMS account (tuxlink-vfb3 sub-project 1).
 * Returns null if valid; a human-readable error string if invalid.
 *
 * STRICTER than {@link validateCallsign} (which is loose and accepts tactical
 * addresses): account creation requires a real amateur callsign, mirroring the backend
 * `looks_like_amateur_callsign` grammar so the user gets early feedback rather than a
 * backend `InvalidInput`. A too-loose check is the dangerous direction — the string is
 * sent verbatim as `Callsign` to a full-account mutation. Grammar: a 1–2 char prefix
 * (`[A-Z]{1,2}` or digit-led `[0-9][A-Z]` for `2E0AAA`/`9A1AA`), the single call-area
 * digit, then a 1–4 letter suffix. The SSID/qualifier is stripped first (matching the
 * backend's base-callsign normalization).
 */
export function validateAmateurCallsign(input: string): string | null {
  if (!input.trim()) return 'Callsign is required.';
  // Mirror the backend: strip the SSID/qualifier, then uppercase, then grammar-check.
  const base = input.trim().split(/[-.]/)[0].toUpperCase();
  if (!/^([A-Z]{1,2}|[0-9][A-Z])[0-9][A-Z]{1,4}$/.test(base)) {
    return 'Enter your licensed amateur callsign (e.g. KK7ABC). A tactical address cannot hold a CMS account.';
  }
  return null;
}

/**
 * Validate a MANDATORY recovery email (tuxlink-vfb3 sub-project 1).
 * Returns null if valid; a human-readable error string if invalid.
 *
 * Required (empty/whitespace rejected) plus a light shape check — `local@domain.tld`
 * with no whitespace. The CMS is authoritative for exact acceptance; this catches the
 * obvious typos before the round trip. Recovery email is mandatory at creation per the
 * locked support-burden decision (a missing recovery address is a large fraction of
 * Winlink support requests).
 */
export function validateRecoveryEmail(input: string): string | null {
  const v = input.trim();
  if (!v) return 'A recovery email is required.';
  if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(v)) {
    return 'Enter a valid email address (e.g. you@example.com).';
  }
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
