// In-app form import IPC bindings (Forms-push G5+G6, tuxlink-z0le/fwob).
// TS mirror of the Rust serde shapes in src-tauri/src/forms/import.rs.
// Tauri v2 maps these camelCase invoke keys to the snake_case Rust params.

import { invoke } from '@tauri-apps/api/core';

export type ImportKind =
  | 'added'
  | 'update'
  | 'overridesStandard'
  | 'companion'
  | 'skip'
  | 'reject';

export interface ImportEntry {
  relPath: string;
  id: string;
  folder: string;
  kind: ImportKind;
  reason: string | null;
  hasViewer: boolean;
}

export interface ImportSummary {
  added: number;
  updated: number;
  overridesStandard: number;
  skipped: number;
  rejected: number;
  companions: number;
}

export interface ImportPlan {
  stagingToken: string;
  entries: ImportEntry[];
  summary: ImportSummary;
}

export interface ImportResult {
  installed: string[];
  skippedUpdates: string[];
  entries: ImportEntry[];
}

/** Externally-tagged error from the backend (mirrors UiError's JSON). */
export type ImportError =
  | { kind: 'tokenExpired' }
  | { kind: 'stagingFailed'; reason: string }
  | { kind: 'commitConflict'; reason: string }
  | { kind: 'io'; reason: string };

/** Stage + validate + classify sources WITHOUT writing; returns the plan + token. */
export function importPreview(sources: string[]): Promise<ImportPlan> {
  return invoke<ImportPlan>('forms_import_preview', { sources });
}

/** Commit a previewed import (single-shot token) applying only confirmed overwrites. */
export function importCommit(
  stagingToken: string,
  approvedOverwriteIds: string[],
): Promise<ImportResult> {
  return invoke<ImportResult>('forms_import_commit', { stagingToken, approvedOverwriteIds });
}

/** Cancel an in-flight preview, dropping its staging dir. Never throws on a bad token. */
export function importCancel(stagingToken: string): Promise<void> {
  return invoke<void>('forms_import_cancel', { stagingToken });
}

/** Reveal the custom-forms folder in the OS file manager. */
export function openFormsFolder(): Promise<void> {
  return invoke<void>('open_forms_folder');
}

/** Remove custom forms (+ companions) by id; returns the ids actually removed. */
export function formsCustomDelete(ids: string[]): Promise<string[]> {
  return invoke<string[]>('forms_custom_delete', { ids });
}
