/**
 * FormDraftLibrary — thin async wrappers around the three Tauri IPC commands
 * that expose the per-form-id draft-slot store.
 *
 * bd tuxlink-hnkn P2 Task 4 (backend)
 *
 * These are plain async functions, NOT React hooks. Form components import
 * them directly and call them in event handlers / useEffect bodies.
 *
 * The shapes here mirror the Rust `FormDraftSlot` struct in
 * `src-tauri/src/forms/draft_library.rs`. The `payload` field is typed as
 * `Record<string, unknown>` — intentionally broad, because each form owns its
 * own field-value schema and the store is opaque to that schema.
 */

import { invoke } from '@tauri-apps/api/core';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/**
 * A single named draft slot for one form. Mirrors `FormDraftSlot` in Rust.
 */
export interface FormDraftSlot {
  /** UUID v4 assigned by the backend on creation. */
  slot_id: string;
  /** e.g. `'Winlink_Check-In'` */
  form_id: string;
  /** Operator-assigned label, e.g. `'Monday Night Net'`. */
  label: string;
  /** Saved field-values map. Shape is determined by the form, not the store. */
  payload: Record<string, unknown>;
  /** RFC 3339 UTC timestamp of creation. */
  created_at: string;
  /** RFC 3339 UTC timestamp of last update. */
  updated_at: string;
}

/** Arguments for {@link upsertSlot}. */
export interface UpsertSlotArgs {
  /**
   * If omitted (or `undefined`), a new slot is created with a backend-minted
   * UUID. If provided, the matching slot is updated in place (or inserted with
   * the given id if it does not yet exist).
   */
  slot_id?: string;
  form_id: string;
  label: string;
  payload: Record<string, unknown>;
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/**
 * List all saved draft slots for `formId`.
 *
 * Returns an empty array when no slots exist (not an error). Slots are
 * ordered by `created_at` ascending (oldest / first-created first).
 */
export async function listSlots(formId: string): Promise<FormDraftSlot[]> {
  return invoke<FormDraftSlot[]>('form_draft_library_list', { form_id: formId });
}

/**
 * Insert or update a draft slot.
 *
 * - `args.slot_id` omitted → new slot, backend mints a UUID.
 * - `args.slot_id` provided → update the matching row in place, or insert if
 *   it does not yet exist.
 *
 * Returns the final `FormDraftSlot`, including the assigned `slot_id` on
 * creates and the preserved `created_at` on updates.
 */
export async function upsertSlot(args: UpsertSlotArgs): Promise<FormDraftSlot> {
  return invoke<FormDraftSlot>('form_draft_library_upsert', {
    slot_id: args.slot_id ?? null,
    form_id: args.form_id,
    label: args.label,
    payload: args.payload,
  });
}

/**
 * Delete a draft slot by its `slotId`. No-op-safe if the slot does not exist.
 */
export async function deleteSlot(slotId: string): Promise<void> {
  return invoke<void>('form_draft_library_delete', { slot_id: slotId });
}
