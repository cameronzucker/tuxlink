// TS mirror of the identity wire DTOs + a UiError message extractor.
//
// Phase 7 (tuxlink-noa0). These mirror the serde-`Serialize` DTOs in
// `src-tauri/src/identity/commands.rs`. The DTOs NEVER carry secret material
// (the activation secret lives only in the OS keyring); these types intentionally
// have no credential/handle field to match.
//
// `cms_badge` is a STRING on the wire (Rust `&'static str`), NOT a tagged enum.
// The identity list is FLAT — `full` and `tactical` are sibling arrays; the UI
// derives nesting by matching `tactical.parent === full.callsign`.

import { asUiError } from '../mailbox/types';

/// Tactical CMS-registration badge. Mirrors the Rust `&'static str` the backend
/// emits: `TacticalCmsState::{Unknown, Registered, NotRegistered}`.
export type CmsBadge = 'unknown' | 'registered' | 'not_registered';

/// A FULL (Part-97 licensed) identity projected for the frontend. No secrets.
/// `needs_auth` is `true` for every FULL except the one authenticated this
/// launch (re-auth-on-launch; see `list_inner` in commands.rs).
export interface FullIdentityDto {
  callsign: string;
  label: string | null;
  has_cms_account: boolean;
  cms_registered: boolean;
  needs_auth: boolean;
}

/// A tactical identity projected for the frontend. `parent` is the FULL
/// callsign it belongs to; the UI matches it against `FullIdentityDto.callsign`.
export interface TacticalIdentityDto {
  label: string;
  parent: string;
  cms_badge: CmsBadge;
}

/// The full identity list as the dashboard reads it. Flat (FULLs + tacticals as
/// sibling arrays with `parent` pointers). `last_selected` is a display-only
/// hint (the UI pre-highlights that row); it is NOT authority over the active
/// session (which is in-memory on the backend).
export interface IdentityListDto {
  full: FullIdentityDto[];
  tactical: TacticalIdentityDto[];
  last_selected: string | null;
}

/// The active session projected for the closed-state chip + header. `mycall` is
/// ALWAYS the Part-97 FULL callsign (the RF station ID); `address_as` is the
/// presented FULL callsign OR tactical label.
export interface ActiveIdentityDto {
  mycall: string;
  address_as: string;
  is_tactical: boolean;
}

/// Extract a human-readable message from a thrown Tauri `UiError` (or anything).
/// Mirrors `catalogErrorMessage` in `src/catalog/stationTypes.ts` and matches the
/// `#[serde(tag="kind", content="detail")]` wire shape from
/// `src-tauri/src/ui_commands.rs`. Consumed by the inline-unlock error surface
/// (Task 9).
export function parseIdentityError(e: unknown): string {
  const ui = asUiError(e);
  if (!ui) return e instanceof Error ? e.message : String(e);
  switch (ui.kind) {
    case 'NotConfigured':
    case 'NotFound':
    case 'Rejected':
      return ui.detail;
    case 'AuthFailed':
    case 'Transport':
    case 'Unavailable':
      return ui.detail.reason;
    case 'Internal':
      return ui.detail.detail;
    case 'Cancelled':
      return 'cancelled';
  }
}
