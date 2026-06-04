import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { PositionFormV2 } from './PositionFormV2';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'position_current_fix') {
      return { grid: 'CN87us', source: 'gps', fresh: true };
    }
    if (cmd === 'send_form') return 'MID-MOCK-123';
    return null;
  }),
}));

describe('<PositionFormV2>', () => {
  it('renders the current GPS grid with a fresh-fix indicator', async () => {
    render(<PositionFormV2 onSubmit={vi.fn()} onCancel={vi.fn()} />);
    expect(await screen.findByDisplayValue('CN87us')).toBeInTheDocument();
    expect(screen.getByText(/fresh.*GPS/i)).toBeInTheDocument();
  });

  it('allows manual grid override', async () => {
    render(<PositionFormV2 onSubmit={vi.fn()} onCancel={vi.fn()} />);
    const input = await screen.findByLabelText(/grid/i);
    fireEvent.change(input, { target: { value: 'EM26' } });
    expect((input as HTMLInputElement).value).toBe('EM26');
  });

  it('Send button calls onSubmit with the rendered FormPayload shape', async () => {
    const onSubmit = vi.fn();
    render(<PositionFormV2 onSubmit={onSubmit} onCancel={vi.fn()} />);
    await screen.findByDisplayValue('CN87us');
    fireEvent.click(screen.getByRole('button', { name: /send/i }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalled());
    const arg = onSubmit.mock.calls[0][0];
    expect(arg.grid).toBe('CN87us');
    expect(arg.formId).toBe('Position_Report');
  });

  it('shows a stale-fix warning when fresh=false', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      grid: 'CN87us',
      source: 'gps',
      fresh: false,
    });
    render(<PositionFormV2 onSubmit={vi.fn()} onCancel={vi.fn()} />);
    expect(await screen.findByText(/stale/i)).toBeInTheDocument();
  });
});
