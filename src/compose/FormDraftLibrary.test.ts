/**
 * FormDraftLibrary.ts unit tests.
 *
 * `invoke` is mocked at the module level so these tests run without a Tauri
 * runtime. The tests verify that each wrapper forwards the right command name
 * and argument shape, and that the return value is passed through correctly.
 *
 * bd tuxlink-hnkn P2 Task 4 (backend)
 */

import { describe, expect, it, vi, beforeEach } from 'vitest';

// ---------------------------------------------------------------------------
// Mock @tauri-apps/api/core before importing the module under test.
// ---------------------------------------------------------------------------

const mocks = vi.hoisted(() => {
  const invoke = vi.fn();
  return { invoke };
});

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));

// Import AFTER vi.mock so the module-level `import { invoke }` resolves to
// the mock.
import { listSlots, upsertSlot, deleteSlot } from './FormDraftLibrary';
import type { FormDraftSlot } from './FormDraftLibrary';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeSlot(overrides: Partial<FormDraftSlot> = {}): FormDraftSlot {
  return {
    slot_id: 'test-uuid-1234',
    form_id: 'Winlink_Check-In',
    label: 'Monday Night Net',
    payload: { callsign: 'N7CPZ', status: 'Available' },
    created_at: '2026-06-04T12:00:00Z',
    updated_at: '2026-06-04T12:00:00Z',
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// listSlots
// ---------------------------------------------------------------------------

describe('listSlots', () => {
  beforeEach(() => mocks.invoke.mockClear());

  it('calls form_draft_library_list with the form_id param', async () => {
    mocks.invoke.mockResolvedValueOnce([]);
    await listSlots('Winlink_Check-In');
    expect(mocks.invoke).toHaveBeenCalledWith('form_draft_library_list', {
      form_id: 'Winlink_Check-In',
    });
  });

  it('returns the array returned by invoke', async () => {
    const slots = [makeSlot(), makeSlot({ slot_id: 'uuid-2', label: 'Tuesday Net' })];
    mocks.invoke.mockResolvedValueOnce(slots);
    const result = await listSlots('Winlink_Check-In');
    expect(result).toEqual(slots);
  });

  it('returns an empty array when no slots exist', async () => {
    mocks.invoke.mockResolvedValueOnce([]);
    const result = await listSlots('Unknown_Form');
    expect(result).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// upsertSlot — new (no slot_id)
// ---------------------------------------------------------------------------

describe('upsertSlot — new slot', () => {
  beforeEach(() => mocks.invoke.mockClear());

  it('calls form_draft_library_upsert with slot_id=null when slot_id is omitted', async () => {
    const returned = makeSlot();
    mocks.invoke.mockResolvedValueOnce(returned);
    await upsertSlot({
      form_id: 'Winlink_Check-In',
      label: 'Monday Night Net',
      payload: { callsign: 'N7CPZ' },
    });
    expect(mocks.invoke).toHaveBeenCalledWith('form_draft_library_upsert', {
      slot_id: null,
      form_id: 'Winlink_Check-In',
      label: 'Monday Night Net',
      payload: { callsign: 'N7CPZ' },
    });
  });

  it('returns the slot echoed back from the backend (includes minted slot_id)', async () => {
    const returned = makeSlot({ slot_id: 'backend-minted-uuid' });
    mocks.invoke.mockResolvedValueOnce(returned);
    const result = await upsertSlot({
      form_id: 'Winlink_Check-In',
      label: 'Monday Night Net',
      payload: {},
    });
    expect(result.slot_id).toBe('backend-minted-uuid');
  });
});

// ---------------------------------------------------------------------------
// upsertSlot — update (slot_id provided)
// ---------------------------------------------------------------------------

describe('upsertSlot — update existing slot', () => {
  beforeEach(() => mocks.invoke.mockClear());

  it('forwards the provided slot_id to the backend', async () => {
    const existing = makeSlot({ slot_id: 'existing-uuid', label: 'Updated Label' });
    mocks.invoke.mockResolvedValueOnce(existing);
    await upsertSlot({
      slot_id: 'existing-uuid',
      form_id: 'Winlink_Check-In',
      label: 'Updated Label',
      payload: { callsign: 'N7CPZ' },
    });
    expect(mocks.invoke).toHaveBeenCalledWith('form_draft_library_upsert', {
      slot_id: 'existing-uuid',
      form_id: 'Winlink_Check-In',
      label: 'Updated Label',
      payload: { callsign: 'N7CPZ' },
    });
  });

  it('returns the updated slot with preserved created_at', async () => {
    const updated = makeSlot({
      slot_id: 'existing-uuid',
      label: 'New Label',
      created_at: '2026-06-01T00:00:00Z',
      updated_at: '2026-06-04T15:30:00Z',
    });
    mocks.invoke.mockResolvedValueOnce(updated);
    const result = await upsertSlot({
      slot_id: 'existing-uuid',
      form_id: 'Winlink_Check-In',
      label: 'New Label',
      payload: {},
    });
    expect(result.created_at).toBe('2026-06-01T00:00:00Z');
    expect(result.updated_at).toBe('2026-06-04T15:30:00Z');
    expect(result.label).toBe('New Label');
  });
});

// ---------------------------------------------------------------------------
// deleteSlot
// ---------------------------------------------------------------------------

describe('deleteSlot', () => {
  beforeEach(() => mocks.invoke.mockClear());

  it('calls form_draft_library_delete with the slot_id param', async () => {
    mocks.invoke.mockResolvedValueOnce(undefined);
    await deleteSlot('some-slot-id');
    expect(mocks.invoke).toHaveBeenCalledWith('form_draft_library_delete', {
      slot_id: 'some-slot-id',
    });
  });

  it('resolves void on success', async () => {
    mocks.invoke.mockResolvedValueOnce(undefined);
    await expect(deleteSlot('any-id')).resolves.toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// Payload round-trips through serde_json::Value — unicode + nested objects
// ---------------------------------------------------------------------------

describe('payload unicode and nested object round-trip', () => {
  beforeEach(() => mocks.invoke.mockClear());

  it('round-trips a unicode + nested payload through upsert', async () => {
    const complexPayload = {
      callsign: 'N7CPZ',
      comment: '73 de tuxlink — unicode: 日本語 emoji 🎙️',
      nested: { list: [1, 2, 3], nullable: null },
    };
    const returned = makeSlot({ payload: complexPayload });
    mocks.invoke.mockResolvedValueOnce(returned);

    const result = await upsertSlot({
      form_id: 'Winlink_Check-In',
      label: 'Unicode',
      payload: complexPayload,
    });

    // The payload forwarded to invoke must be identical to what was passed in.
    const call = mocks.invoke.mock.calls[0];
    expect(call[1].payload).toEqual(complexPayload);
    // The returned slot carries the same payload.
    expect(result.payload).toEqual(complexPayload);
  });

  it('round-trips a unicode + nested payload through listSlots response', async () => {
    const complexPayload = {
      net: 'Cascadia EmComm Net',
      notes: 'Δ freq shift ≈ 1.2 kHz',
      fields: { a: true, b: [null, 'x'] },
    };
    const slot = makeSlot({ payload: complexPayload });
    mocks.invoke.mockResolvedValueOnce([slot]);

    const [fetched] = await listSlots('Winlink_Check-In');
    expect(fetched.payload).toEqual(complexPayload);
  });
});
