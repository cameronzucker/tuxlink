import { describe, it, expect, vi } from 'vitest';
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
    return null;
  }),
}));

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
});
