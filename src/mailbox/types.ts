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
  /// populated only by the dev fixture. Backend `snippet` is a v0.1 follow-up
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
/// Rust command boundary; declared here as the shared contract). v0.0.1
/// lists attachment names only; form rendering + attachment open are v0.1.
export interface ParsedMessage {
  id: string;
  subject: string;
  from: string;
  to: string[];
  cc: string[];
  date: string; // RFC 3339 UTC
  body: string; // decoded text/plain
  attachments: AttachmentMeta[]; // names + sizes; bytes fetched lazily (v0.1)
  isForm: boolean; // body is a Winlink form payload → v0.1 placeholder
  routing: string | null; // e.g. "via CMS-SSL"; null if unknown
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

/// Sidebar folder identifiers. `drafts` is a local (localStorage) store, not
/// a backend folder; `deleted` is a disabled placeholder in v0.0.1 (spec
/// §2.2). The Rust `parse_folder` rejects both for backend commands.
export type MailboxFolder = 'inbox' | 'outbox' | 'sent' | 'drafts' | 'deleted';

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
