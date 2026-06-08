// Frontend DTOs for the Contacts feature — Task A4.
//
// These MUST mirror the Rust serde shapes EXACTLY (snake_case; the codebase has
// no `rename_all`). Sources of truth:
//   - `src-tauri/src/contacts/store.rs` — Contact / GroupMember / Group / ContactsFile
//   - `src-tauri/src/contacts/suggest.rs` — Suggestion
// When a Rust shape changes, this file MUST be updated in the same PR.

/// One address-book entry. `callsign` is the primary, SSID-bearing identity —
/// never strip the SSID. `created_at` / `updated_at` are RFC3339 UTC strings.
export interface Contact {
  id: string;
  name: string;
  callsign: string;
  email?: string;
  tactical?: string;
  notes?: string;
  created_at: string;
  updated_at: string;
}

/// A distribution-group member. Mirrors the Rust `GroupMember` enum, which is
/// serialized with `#[serde(tag = "type", rename_all = "snake_case")]` — so the
/// wire shape is `{ type: 'contact', contact_id }` or `{ type: 'raw', callsign }`.
/// Added-from-a-contact members store a `contact_id` (so edits propagate); typed
/// members store the raw `callsign` literal.
export type GroupMember =
  | { type: 'contact'; contact_id: string }
  | { type: 'raw'; callsign: string };

/// A distribution group expanded to member callsigns at send time (frontend).
export interface Group {
  id: string;
  name: string;
  members: GroupMember[];
  created_at: string;
  updated_at: string;
}

/// The whole on-disk contacts file (contacts + groups + schema version). Returned
/// verbatim by the `contacts_read` command.
export interface ContactsFile {
  schema_version: number;
  contacts: Contact[];
  groups: Group[];
}

/// A suggest-from-history candidate (Task A8 consumer). `message_count` is how
/// many mailbox messages reference the un-saved correspondent.
export interface Suggestion {
  callsign: string;
  message_count: number;
}
