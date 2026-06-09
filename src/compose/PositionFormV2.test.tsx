import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { PositionFormV2 } from './PositionFormV2';

// Default mock: fresh GPS fix with a valid grid.
// "Gps" is PascalCase — matches format!("{:?}", PositionSource::Gps) from the
// Debug derive; the component's .toUpperCase() normalizes it for display.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'position_current_fix') {
      return { grid: 'CN87us', source: 'Gps', fresh: true };
    }
    if (cmd === 'send_form') return 'MID-MOCK-123';
    if (cmd === 'form_draft_library_list') return [];
    if (cmd === 'form_draft_library_upsert') {
      return {
        slot_id: 'mock-slot-id',
        form_id: 'Position_Report',
        label: 'Test Slot',
        payload: { message: 'Test remark' },
        created_at: '2026-06-04T12:00:00Z',
        updated_at: '2026-06-04T12:00:00Z',
      };
    }
    if (cmd === 'form_draft_library_delete') return undefined;
    return null;
  }),
}));

// Reset mock to defaults before each test so per-test overrides don't bleed.
beforeEach(async () => {
  const { invoke } = await import('@tauri-apps/api/core');
  const mockInvoke = invoke as ReturnType<typeof vi.fn>;
  mockInvoke.mockImplementation(async (cmd: string) => {
    if (cmd === 'position_current_fix') {
      return { grid: 'CN87us', source: 'Gps', fresh: true };
    }
    if (cmd === 'send_form') return 'MID-MOCK-123';
    if (cmd === 'form_draft_library_list') return [];
    if (cmd === 'form_draft_library_upsert') {
      return {
        slot_id: 'mock-slot-id',
        form_id: 'Position_Report',
        label: 'Test Slot',
        payload: { message: 'Test remark' },
        created_at: '2026-06-04T12:00:00Z',
        updated_at: '2026-06-04T12:00:00Z',
      };
    }
    if (cmd === 'form_draft_library_delete') return undefined;
    return null;
  });
});

describe('<PositionFormV2>', () => {
  it('renders the current GPS grid with a fresh-fix indicator', async () => {
    render(<PositionFormV2 onSubmit={vi.fn()} onCancel={vi.fn()} />);
    // GPS-returned grids are normalised to uppercase (Minor 1 fix — be consistent
    // with the user-typed uppercase normalization in the input handler).
    expect(await screen.findByDisplayValue('CN87US')).toBeInTheDocument();
    expect(screen.getByText(/fresh.*GPS/i)).toBeInTheDocument();
  });

  it('allows manual grid override', async () => {
    render(<PositionFormV2 onSubmit={vi.fn()} onCancel={vi.fn()} />);
    const input = await screen.findByLabelText(/Maidenhead grid/i);
    fireEvent.change(input, { target: { value: 'EM26' } });
    expect((input as HTMLInputElement).value).toBe('EM26');
  });

  it('keeps the manual grid input editable while the offline map is mounted (C9)', async () => {
    // The map is an aid, never the only path: the manual Maidenhead input must
    // remain present and editable whenever the (offline) map is mounted.
    render(<PositionFormV2 onSubmit={vi.fn()} onCancel={vi.fn()} />);
    const input = await screen.findByLabelText(/Maidenhead grid/i);

    // Default GPS fix → grid set → the map mounts inside the mount div.
    // Wait for the async GPS fix to populate the grid before asserting the
    // map-mount's grid-derived `--active` class. `findByLabelText` above only
    // awaits the input's presence (it renders on the first pass, before the
    // position_current_fix invoke resolves and setGrid re-renders); asserting
    // `--active` synchronously here races that re-render. Awaiting the grid's
    // display value is how the other tests in this file wait for the fix to
    // land. (This raced and failed the amd64 verify job on the 0.39.1 release
    // CI run 2026-06-09; arm64 passed the same commit — classic timing flake.)
    await screen.findByDisplayValue('CN87US');
    const mount = screen.getByTestId('position-map-mount');
    expect(mount.className).toContain('--active');
    expect(mount.children.length).toBeGreaterThan(0);

    // The manual input is still present and editable.
    expect(input).toBeEnabled();
    fireEvent.change(input, { target: { value: 'EM26' } });
    expect((input as HTMLInputElement).value).toBe('EM26');
  });

  it('Send button calls onSubmit with the wire-format payload', async () => {
    const onSubmit = vi.fn();
    render(<PositionFormV2 onSubmit={onSubmit} onCancel={vi.fn()} />);
    // GPS-returned grid normalised to uppercase (Minor 1 fix)
    await screen.findByDisplayValue('CN87US');
    fireEvent.click(screen.getByRole('button', { name: /send/i }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalled());
    const arg = onSubmit.mock.calls[0][0] as Record<string, string>;
    // Wire-format keys — what POSITION_REPORT template's serialize_form_xml iterates
    expect(arg.thetime).toBeTruthy();
    expect(arg.lat).toBeTruthy();
    expect(arg.lon).toBeTruthy();
    expect('message' in arg).toBe(true);
    // lat + lon must be stringified numbers with ≥4 decimal places
    expect(parseFloat(arg.lat)).not.toBeNaN();
    expect(parseFloat(arg.lon)).not.toBeNaN();
    expect(arg.lat).toMatch(/\.\d{4}/);
    expect(arg.lon).toMatch(/\.\d{4}/);
    // Must NOT include the old UI-shape keys
    expect('formId' in arg).toBe(false);
    expect('grid' in arg).toBe(false);
    expect('remark' in arg).toBe(false);
  });

  it('shows a stale-fix warning when fresh=false', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    // "Gps" PascalCase — matches Debug derive output from the Rust enum
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      grid: 'CN87us',
      source: 'Gps',
      fresh: false,
    });
    render(<PositionFormV2 onSubmit={vi.fn()} onCancel={vi.fn()} />);
    expect(await screen.findByText(/stale/i)).toBeInTheDocument();
  });

  // ── Draft restore (Critical 2) ──────────────────────────────────────────

  it('rehydrates from initialValues when a draft exists (ignores GPS pull)', async () => {
    render(
      <PositionFormV2
        initialValues={{ grid: 'EM26', message: 'Test draft' }}
        onSubmit={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    // Draft grid shows immediately (no async needed — seeded from initialValues)
    const gridInput = screen.getByLabelText(/Maidenhead grid/i) as HTMLInputElement;
    expect(gridInput.value).toBe('EM26');
    const remarkInput = screen.getByLabelText(/Remark/i) as HTMLTextAreaElement;
    expect(remarkInput.value).toBe('Test draft');
    // GPS pulled grid (CN87us) must NOT replace the draft value even after async resolves
    await waitFor(() => {
      const input = screen.getByLabelText(/Maidenhead grid/i) as HTMLInputElement;
      expect(input.value).toBe('EM26');
    });
  });

  it('fires onChange with UI-shape payload when the user types in the grid input', async () => {
    const onChange = vi.fn();
    render(<PositionFormV2 onChange={onChange} onSubmit={vi.fn()} onCancel={vi.fn()} />);
    const input = await screen.findByLabelText(/Maidenhead grid/i);
    fireEvent.change(input, { target: { value: 'FN31' } });
    // onChange fires synchronously in the input event handler — no waitFor needed,
    // but wrap for safety in case the GPS effect settles concurrently.
    await waitFor(() => {
      const lastCall = onChange.mock.calls[onChange.mock.calls.length - 1][0] as Record<string, string>;
      expect(lastCall.grid).toBe('FN31');
      expect('message' in lastCall).toBe(true);
    });
  });

  it('fires onChange with UI-shape payload when the user types in the remark textarea', async () => {
    const onChange = vi.fn();
    render(<PositionFormV2 onChange={onChange} onSubmit={vi.fn()} onCancel={vi.fn()} />);
    const textarea = await screen.findByLabelText(/Remark/i);
    fireEvent.change(textarea, { target: { value: 'Hello net' } });
    await waitFor(() => {
      const lastCall = onChange.mock.calls[onChange.mock.calls.length - 1][0] as Record<string, string>;
      expect(lastCall.message).toBe('Hello net');
      expect('grid' in lastCall).toBe(true);
    });
  });

  // ── Inline grid error (Important 1 fix) ────────────────────────────────────

  it('shows inline grid error and blocks onSubmit for an invalid Maidenhead grid', async () => {
    const onSubmit = vi.fn();
    render(<PositionFormV2 onSubmit={onSubmit} onCancel={vi.fn()} />);
    // Wait for GPS pull to settle, then override with an invalid grid
    const input = await screen.findByLabelText(/Maidenhead grid/i);
    fireEvent.change(input, { target: { value: 'ZZ99' } });
    fireEvent.click(screen.getByRole('button', { name: /send/i }));
    // gridError rendered inline below the input
    expect(screen.getByRole('alert')).toHaveTextContent(/Invalid Maidenhead grid/i);
    // onSubmit must NOT have been called
    expect(onSubmit).not.toHaveBeenCalled();
    // Form is still interactive — grid input still present
    expect(screen.getByLabelText(/Maidenhead grid/i)).toBeInTheDocument();
  });

  it('clears the inline grid error when the operator starts editing the grid', async () => {
    const onSubmit = vi.fn();
    render(<PositionFormV2 onSubmit={onSubmit} onCancel={vi.fn()} />);
    const input = await screen.findByLabelText(/Maidenhead grid/i);
    fireEvent.change(input, { target: { value: 'ZZ99' } });
    fireEvent.click(screen.getByRole('button', { name: /send/i }));
    expect(screen.getByRole('alert')).toHaveTextContent(/Invalid Maidenhead grid/i);
    // Operator edits the grid — error should clear
    fireEvent.change(input, { target: { value: 'CN87' } });
    expect(screen.queryByRole('alert')).toBeNull();
  });

  // ── No-fix UX (Important 3) ─────────────────────────────────────────────

  it('shows "No GPS fix" hint when backend returns grid: null', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      grid: null,
      source: 'Manual',
      fresh: false,
    });
    render(<PositionFormV2 onSubmit={vi.fn()} onCancel={vi.fn()} />);
    expect(await screen.findByText(/No GPS fix/i)).toBeInTheDocument();
    // Grid input is empty
    const input = screen.getByLabelText(/Maidenhead grid/i) as HTMLInputElement;
    expect(input.value).toBe('');
    // Send is disabled until user types
    expect(screen.getByRole('button', { name: /send/i })).toBeDisabled();
  });

  it('enables Send after user types a valid grid when no fix', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      grid: null,
      source: 'Manual',
      fresh: false,
    });
    render(<PositionFormV2 onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByText(/No GPS fix/i);
    const input = screen.getByLabelText(/Maidenhead grid/i);
    fireEvent.change(input, { target: { value: 'EM26' } });
    expect(screen.getByRole('button', { name: /send/i })).not.toBeDisabled();
  });

  // ── FormDraftLibrary slot tests ────────────────────────────────────────────

  it('lists saved slots on mount', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'position_current_fix') return { grid: 'CN87us', source: 'Gps', fresh: true };
      if (cmd === 'form_draft_library_list') {
        return [
          {
            slot_id: 'slot-1',
            form_id: 'Position_Report',
            label: 'Monday Night Net',
            payload: { message: 'Check-in from home QTH' },
            created_at: '2026-06-04T12:00:00Z',
            updated_at: '2026-06-04T12:00:00Z',
          },
        ];
      }
      return null;
    });
    render(<PositionFormV2 onSubmit={vi.fn()} onCancel={vi.fn()} />);
    expect(await screen.findByText('Monday Night Net')).toBeInTheDocument();
  });

  it('applies a slot payload (message field) when selected', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'position_current_fix') return { grid: 'CN87us', source: 'Gps', fresh: true };
      if (cmd === 'form_draft_library_list') {
        return [
          {
            slot_id: 'slot-1',
            form_id: 'Position_Report',
            label: 'Home QTH',
            payload: { message: 'Checking in from home' },
            created_at: '2026-06-04T12:00:00Z',
            updated_at: '2026-06-04T12:00:00Z',
          },
        ];
      }
      return null;
    });
    render(<PositionFormV2 onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByText('Home QTH');
    const select = screen.getByRole('combobox');
    fireEvent.change(select, { target: { value: 'slot-1' } });
    const textarea = screen.getByLabelText(/Remark/i) as HTMLTextAreaElement;
    expect(textarea.value).toBe('Checking in from home');
  });

  it('saves a new slot via the Save as slot… button', async () => {
    vi.spyOn(window, 'prompt').mockReturnValue('Monday Night Net');
    const { invoke } = await import('@tauri-apps/api/core');
    const mockInvoke = invoke as ReturnType<typeof vi.fn>;
    render(<PositionFormV2 onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByDisplayValue('CN87US');
    // Type a remark so the payload is non-empty
    const textarea = screen.getByLabelText(/Remark/i);
    fireEvent.change(textarea, { target: { value: 'Checking in from home' } });
    mockInvoke.mockClear();
    fireEvent.click(screen.getByTestId('slot-save-btn'));
    await waitFor(() => {
      const upsertCall = mockInvoke.mock.calls.find((c) => c[0] === 'form_draft_library_upsert');
      expect(upsertCall).toBeTruthy();
      expect(upsertCall![1]).toMatchObject({
        formId: 'Position_Report',
        label: 'Monday Night Net',
        payload: { message: 'Checking in from home' },
      });
    });
    vi.restoreAllMocks();
  });

  it('deletes the selected slot', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'position_current_fix') return { grid: 'CN87us', source: 'Gps', fresh: true };
      if (cmd === 'form_draft_library_list') {
        return [
          {
            slot_id: 'slot-to-delete',
            form_id: 'Position_Report',
            label: 'Stale slot',
            payload: { message: '' },
            created_at: '2026-06-04T12:00:00Z',
            updated_at: '2026-06-04T12:00:00Z',
          },
        ];
      }
      if (cmd === 'form_draft_library_delete') return undefined;
      return null;
    });
    render(<PositionFormV2 onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByText('Stale slot');
    const select = screen.getByRole('combobox');
    fireEvent.change(select, { target: { value: 'slot-to-delete' } });
    const deleteBtn = await screen.findByTestId('slot-delete-btn');
    fireEvent.click(deleteBtn);
    const { invoke: inv } = await import('@tauri-apps/api/core');
    await waitFor(() => {
      const deleteCalls = (inv as ReturnType<typeof vi.fn>).mock.calls.filter(
        (c) => c[0] === 'form_draft_library_delete',
      );
      expect(deleteCalls.length).toBeGreaterThan(0);
      expect(deleteCalls[0][1]).toEqual({ slotId: 'slot-to-delete' });
    });
    vi.restoreAllMocks();
  });
});
