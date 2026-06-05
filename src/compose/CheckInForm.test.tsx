/**
 * CheckInForm tests — bd tuxlink-hnkn P2 Task 3
 *
 * Mock shape uses the real FormDraftSlot struct (slot_id, form_id, label,
 * payload, created_at, updated_at) — NOT the plan scaffold's abbreviated
 * { id, label, payload } shape, which predates FormDraftLibrary's final API.
 * Reference: FormDraftLibrary.test.ts::makeSlot for the canonical shape.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { CheckInForm } from './CheckInForm';

// ---------------------------------------------------------------------------
// Module-level mock — hoisted so the module-level `import { invoke }` in
// CheckInForm.tsx resolves to this mock before the module is evaluated.
// ---------------------------------------------------------------------------

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'position_current_fix') return { grid: 'CN87', source: 'gps', fresh: true };
    if (cmd === 'config_read') return { callsign: 'W7CPZ' };
    if (cmd === 'send_form') return 'MID';
    if (cmd === 'form_draft_library_list') {
      return [
        {
          slot_id: 'slot-monday-night-net',
          form_id: 'Winlink_Check-In',
          label: 'Monday Night Net',
          payload: { group_net: 'ARES Net', op_name: 'John Smith', comments: '', initials: 'JS' },
          created_at: '2026-06-04T12:00:00Z',
          updated_at: '2026-06-04T12:00:00Z',
        },
      ];
    }
    if (cmd === 'form_draft_library_upsert') {
      return {
        slot_id: 'new-slot-uuid',
        form_id: 'Winlink_Check-In',
        label: 'Test Slot',
        payload: {},
        created_at: '2026-06-04T12:00:00Z',
        updated_at: '2026-06-04T12:00:00Z',
      };
    }
    if (cmd === 'form_draft_library_delete') return undefined;
    return null;
  }),
}));

// Reset to defaults before each test so per-test overrides don't bleed.
beforeEach(async () => {
  const { invoke } = await import('@tauri-apps/api/core');
  const mockInvoke = invoke as ReturnType<typeof vi.fn>;
  mockInvoke.mockImplementation(async (cmd: string) => {
    if (cmd === 'position_current_fix') return { grid: 'CN87', source: 'gps', fresh: true };
    if (cmd === 'config_read') return { callsign: 'W7CPZ' };
    if (cmd === 'send_form') return 'MID';
    if (cmd === 'form_draft_library_list') {
      return [
        {
          slot_id: 'slot-monday-night-net',
          form_id: 'Winlink_Check-In',
          label: 'Monday Night Net',
          payload: { group_net: 'ARES Net', op_name: 'John Smith', comments: '', initials: 'JS' },
          created_at: '2026-06-04T12:00:00Z',
          updated_at: '2026-06-04T12:00:00Z',
        },
      ];
    }
    if (cmd === 'form_draft_library_upsert') {
      return {
        slot_id: 'new-slot-uuid',
        form_id: 'Winlink_Check-In',
        label: 'Test Slot',
        payload: {},
        created_at: '2026-06-04T12:00:00Z',
        updated_at: '2026-06-04T12:00:00Z',
      };
    }
    if (cmd === 'form_draft_library_delete') return undefined;
    return null;
  });
});

// ---------------------------------------------------------------------------
// Plan-spec'd tests (4 required)
// ---------------------------------------------------------------------------

describe('<CheckInForm> — plan-spec tests', () => {
  it('pre-fills tactical call from config', async () => {
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    expect(await screen.findByDisplayValue('W7CPZ')).toBeInTheDocument();
  });

  it('renders the saved-slot dropdown', async () => {
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    expect(await screen.findByText(/Monday Night Net/)).toBeInTheDocument();
  });

  it('clicking a saved slot applies its payload to the form', async () => {
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    // Wait for the slot OPTION to appear before triggering the change event.
    // `findByRole('combobox')` resolves immediately because <select> renders
    // synchronously, but its <option> children come from async setSlots()
    // after listSlots resolves. On slower CI hardware (amd64 GHA runner) the
    // change event would fire before the option existed → applySlot found
    // no matching slot → no-op → assertion failed. Wait for the option text
    // to appear so we know setSlots has fired and React has re-rendered.
    await screen.findByText(/Monday Night Net/);
    fireEvent.change(screen.getByRole('combobox'), {
      target: { value: 'slot-monday-night-net' },
    });
    expect((screen.getByLabelText(/group/i) as HTMLInputElement).value).toBe('ARES Net');
  });

  it('Status defaults to Ready', async () => {
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    // Wait for mount effects to settle, then check radio state.
    await screen.findByDisplayValue('W7CPZ');
    expect((screen.getByLabelText(/ready/i) as HTMLInputElement).checked).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Wire-format alignment tests
// ---------------------------------------------------------------------------

describe('<CheckInForm> — wire-format alignment', () => {
  it('onSubmit payload keys match checkin.rs::FIELDS (wire-format check)', async () => {
    const onSubmit = vi.fn();
    render(<CheckInForm onSubmit={onSubmit} onCancel={vi.fn()} />);
    await screen.findByDisplayValue('W7CPZ');
    fireEvent.click(screen.getByRole('button', { name: /send/i }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalled());
    const payload = onSubmit.mock.calls[0][0] as Record<string, string>;
    // Must contain ALL wire-format keys from checkin.rs::FIELDS
    expect('tactical_call' in payload).toBe(true);
    expect('op_name' in payload).toBe(true);
    expect('group_net' in payload).toBe(true);
    expect('status' in payload).toBe(true);
    expect('comments' in payload).toBe(true);
    expect('grid' in payload).toBe(true);
    expect('initials' in payload).toBe(true);
    // tactical_call pre-filled from config
    expect(payload.tactical_call).toBe('W7CPZ');
    // status defaults to Ready
    expect(payload.status).toBe('Ready');
  });
});

// ---------------------------------------------------------------------------
// Position pre-fill tests
// ---------------------------------------------------------------------------

describe('<CheckInForm> — position pre-fill', () => {
  it('pre-fills grid from position_current_fix', async () => {
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    // Grid is uppercase-normalized (matching PositionFormV2 convention)
    expect(await screen.findByDisplayValue('CN87')).toBeInTheDocument();
  });

  it('leaves grid blank when position_current_fix returns null grid', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { callsign: 'W7CPZ' };
      if (cmd === 'position_current_fix') return { grid: null, source: 'manual', fresh: false };
      if (cmd === 'form_draft_library_list') return [];
      return null;
    });
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByDisplayValue('W7CPZ');
    const gridInput = screen.getByLabelText(/grid/i) as HTMLInputElement;
    expect(gridInput.value).toBe('');
  });

  it('prefers draft grid over GPS fix', async () => {
    render(
      <CheckInForm
        initialValues={{ grid: 'EM26', tactical_call: 'W7CPZ' }}
        onSubmit={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    const gridInput = screen.getByLabelText(/grid/i) as HTMLInputElement;
    expect(gridInput.value).toBe('EM26');
    // GPS effect resolves CN87 but draft must win
    await waitFor(() => {
      expect((screen.getByLabelText(/grid/i) as HTMLInputElement).value).toBe('EM26');
    });
  });
});

// ---------------------------------------------------------------------------
// Status radio tests
// ---------------------------------------------------------------------------

describe('<CheckInForm> — status radios', () => {
  it('status changes to Standby when Standby radio selected', async () => {
    const onSubmit = vi.fn();
    render(<CheckInForm onSubmit={onSubmit} onCancel={vi.fn()} />);
    await screen.findByDisplayValue('W7CPZ');
    const standbyRadio = screen.getByLabelText(/standby/i) as HTMLInputElement;
    fireEvent.click(standbyRadio);
    expect(standbyRadio.checked).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: /send/i }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalled());
    expect(onSubmit.mock.calls[0][0].status).toBe('Standby');
  });

  it('status changes to Out when Out radio selected', async () => {
    const onSubmit = vi.fn();
    render(<CheckInForm onSubmit={onSubmit} onCancel={vi.fn()} />);
    await screen.findByDisplayValue('W7CPZ');
    const outRadio = screen.getByLabelText(/^out/i) as HTMLInputElement;
    fireEvent.click(outRadio);
    expect(outRadio.checked).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// FormDraftLibrary slot integration tests
// ---------------------------------------------------------------------------

describe('<CheckInForm> — slot save/delete', () => {
  it('saves a new slot via Save as slot… button', async () => {
    vi.spyOn(window, 'prompt').mockReturnValue('Monday Night Net');
    const { invoke } = await import('@tauri-apps/api/core');
    const mockInvoke = invoke as ReturnType<typeof vi.fn>;
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByDisplayValue('W7CPZ');
    const groupInput = screen.getByLabelText(/group/i);
    fireEvent.change(groupInput, { target: { value: 'ARES Net' } });
    mockInvoke.mockClear();
    fireEvent.click(screen.getByTestId('slot-save-btn'));
    await waitFor(() => {
      const upsertCall = mockInvoke.mock.calls.find((c) => c[0] === 'form_draft_library_upsert');
      expect(upsertCall).toBeTruthy();
      expect(upsertCall![1]).toMatchObject({
        formId: 'Winlink_Check-In',
        label: 'Monday Night Net',
        payload: expect.objectContaining({ group_net: 'ARES Net' }),
      });
    });
    vi.restoreAllMocks();
  });

  it('deletes the selected slot', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByText('Monday Night Net');
    const combobox = screen.getByRole('combobox');
    fireEvent.change(combobox, { target: { value: 'slot-monday-night-net' } });
    const deleteBtn = await screen.findByTestId('slot-delete-btn');
    fireEvent.click(deleteBtn);
    const { invoke } = await import('@tauri-apps/api/core');
    await waitFor(() => {
      const deleteCalls = (invoke as ReturnType<typeof vi.fn>).mock.calls.filter(
        (c) => c[0] === 'form_draft_library_delete',
      );
      expect(deleteCalls.length).toBeGreaterThan(0);
      expect(deleteCalls[0][1]).toEqual({ slotId: 'slot-monday-night-net' });
    });
    vi.restoreAllMocks();
  });

  it('slot payload includes op_name, group_net, comments, initials but NOT tactical_call or status', async () => {
    vi.spyOn(window, 'prompt').mockReturnValue('Net Config');
    const { invoke } = await import('@tauri-apps/api/core');
    const mockInvoke = invoke as ReturnType<typeof vi.fn>;
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByDisplayValue('W7CPZ');
    mockInvoke.mockClear();
    fireEvent.click(screen.getByTestId('slot-save-btn'));
    await waitFor(() => {
      const upsertCall = mockInvoke.mock.calls.find((c) => c[0] === 'form_draft_library_upsert');
      expect(upsertCall).toBeTruthy();
      const savedPayload = upsertCall![1].payload;
      // Saveable fields present
      expect('op_name' in savedPayload).toBe(true);
      expect('group_net' in savedPayload).toBe(true);
      expect('comments' in savedPayload).toBe(true);
      expect('initials' in savedPayload).toBe(true);
      // Volatile fields NOT in slot payload
      expect('tactical_call' in savedPayload).toBe(false);
      expect('status' in savedPayload).toBe(false);
      expect('grid' in savedPayload).toBe(false);
    });
    vi.restoreAllMocks();
  });
});

// ---------------------------------------------------------------------------
// Draft restore tests
// ---------------------------------------------------------------------------

describe('<CheckInForm> — draft restore', () => {
  it('rehydrates all fields from initialValues', () => {
    render(
      <CheckInForm
        initialValues={{
          tactical_call: 'K7ABC',
          op_name: 'Alice',
          group_net: 'ARES',
          status: 'Standby',
          comments: 'Test comment',
          grid: 'EM26',
          initials: 'A',
        }}
        onSubmit={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect((screen.getByLabelText(/tactical call/i) as HTMLInputElement).value).toBe('K7ABC');
    expect((screen.getByLabelText(/operator name/i) as HTMLInputElement).value).toBe('Alice');
    expect((screen.getByLabelText(/group/i) as HTMLInputElement).value).toBe('ARES');
    expect((screen.getByLabelText(/standby/i) as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText(/comments/i) as HTMLTextAreaElement).value).toBe('Test comment');
    expect((screen.getByLabelText(/grid/i) as HTMLInputElement).value).toBe('EM26');
    expect((screen.getByLabelText(/initials/i) as HTMLInputElement).value).toBe('A');
  });
});

// ---------------------------------------------------------------------------
// Send-disabled guard
// ---------------------------------------------------------------------------

describe('<CheckInForm> — send guard', () => {
  it('Send is disabled when tactical_call is empty', () => {
    render(
      <CheckInForm
        initialValues={{ tactical_call: '' }}
        onSubmit={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    // Config fetch is async; before it resolves, tactical_call is blank
    expect(screen.getByRole('button', { name: /send/i })).toBeDisabled();
  });
});
