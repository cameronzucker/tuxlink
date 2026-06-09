// Shared mailbox model — the message-model root for the main-UI cluster.
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §2.3, §3.1
// bd issue: tuxlink-zsm (Task 12 — main-UI cluster ROOT)
//
// Tasks 13 (reading pane) and 14 (compose) IMPORT from this file. It is the
// only hard build-dependency between Task 12 and its dependents (spec §4.3).
// Keep it free of React / Tauri imports so it's a pure type/contract module.

import type { FormPayload } from '../forms/types';

/// List-row metadata. Mirrors `MessageMetaDto` (camelCase) in
/// `src-tauri/src/ui_commands.rs`. `to` and `hasAttachments` degrade to
/// `[]` / `false` when Pat 1.0.0's list DTO omits them (spec §2.1).
export interface MessageMeta {
  id: string; // MID
  subject: string;
  from: string;
  to: string[];
  date: string; // RFC 3339 UTC
  unread: boolean;
  bodySize: number;
  hasAttachments: boolean;
  /// Short preview snippet (Mock D row line 3). OPTIONAL — Pat 1.0.0's list DTO
  /// does not supply a snippet, so this is undefined for live data today and is
  /// populated only by the dev fixture. Backend `snippet` is a follow-up
  /// (tuxlink-yd4 row work); the row renders nothing when absent.
  preview?: string;
  /// Winlink form label for the row's inline form badge (Mock D, e.g.
  /// "ICS-213"). OPTIONAL — the list DTO has no form-type field today
  /// (fixture-only; backend follow-up). Absent → no badge.
  formTag?: string;
  /// Optional folder badge for cross-folder search rendering (spec §7.2).
  /// Absent → no badge.
  folder?: MailboxFolder;
}

/// Reading-pane parsed view (Task 13 produces this from raw RFC5322 at the
/// Rust command boundary; declared here as the shared contract). The current
/// surface lists attachment names/sizes; attachment bytes are fetched lazily
/// by explicit Save As / Preview commands.
export interface ParsedMessage {
  id: string;
  subject: string;
  from: string;
  to: string[];
  cc: string[];
  date: string; // RFC 3339 UTC
  body: string; // decoded text/plain
  attachments: AttachmentMeta[]; // names + sizes; bytes fetched lazily
  isForm: boolean; // body is a Winlink form payload → placeholder for now
  routing: string | null; // e.g. "via CMS-SSL"; null if unknown
  // "post-office" when filed by the local Post Office; null/absent otherwise
  receivedSession?: string | null;
  /// Form ID extracted from RMS_Express_Form_<id>.xml attachment name (T2.2).
  /// Optional + null when not a form. Validated server-side; safe for path use.
  formId?: string | null;
  /// Eagerly-parsed form payload from the attachment XML (T2.2). Optional +
  /// null when not a form OR when server-side parse failed.
  formPayload?: FormPayload | null;
}

export interface AttachmentMeta {
  filename: string;
  size: number;
}

export interface AttachmentPreview {
  filename: string;
  mimeType: string;
  dataBase64: string;
}

/// Sidebar folder identifiers. `drafts` is a local (localStorage) store, not
/// a backend folder; `deleted` is a disabled placeholder for now (spec
/// §2.2). `archive` is a system folder (Phase 1, tuxlink-ca5x); Phase 2
/// (tuxlink-f62f) open-set user-folder slugs are STRINGS that ride alongside
/// this union via `MailboxFolderRef`.
export type MailboxFolder = 'inbox' | 'outbox' | 'sent' | 'drafts' | 'deleted' | 'archive';

/// Any folder reference the Tauri `mailbox_list`/`mailbox_move`/`message_read`
/// commands accept on the wire: a system folder OR a user-folder slug
/// (lowercase ASCII `[a-z0-9-]+`). The Rust backend dispatches on the string
/// at parse time. Frontend code that knows it's working with a system folder
/// should still type as `MailboxFolder` for compile-time safety; the broader
/// `MailboxFolderRef` is used at the boundary where user-folder slugs are
/// also valid.
export type MailboxFolderRef = MailboxFolder | string;

/// One operator-created mailbox folder (tuxlink-f62f). The slug is the
/// stable identifier (URL/path-safe; used by `mailbox_list` / `mailbox_move`
/// over the wire). The display name is what the UI shows; it can be edited
/// without churning messages on disk because the slug never changes.
export interface UserFolder {
  slug: string;
  displayName: string;
  createdAt: string; // RFC 3339 UTC
  /// Parent folder slug (schema v2 / spec D2). Absent/undefined for a top-level
  /// folder — the backend DTO omits the key (not null) for top-level folders.
  parentSlug?: string;
}

/// Serializable backend error. Mirrors `UiError` in
/// `src-tauri/src/ui_commands.rs` via Tauri's
/// `#[serde(tag="kind", content="detail")]` shape. `NotConfigured` is the
/// "not connected" empty-state signal, NOT an error to toast (spec §3.1).
export type UiError =
  | { kind: 'NotConfigured'; detail: string }
  | { kind: 'NotFound'; detail: string }
  | { kind: 'AuthFailed'; detail: { reason: string } }
  | { kind: 'Transport'; detail: { reason: string } }
  | { kind: 'Unavailable'; detail: { reason: string } }
  | { kind: 'Rejected'; detail: string }
  | { kind: 'Cancelled' }
  | { kind: 'Internal'; detail: { detail: string } };

/// Narrow an unknown thrown value (Tauri rejects with the serialized enum)
/// to a `UiError` when it has the discriminated-union shape, else null.
export function asUiError(e: unknown): UiError | null {
  if (e && typeof e === 'object' && 'kind' in e) {
    return e as UiError;
  }
  return null;
}

/// True when the error is the "backend offline" signal that should render as
/// an empty state rather than an error. Spec §1.1 / §3.1.
export function isNotConfigured(e: unknown): boolean {
  return asUiError(e)?.kind === 'NotConfigured';
}
