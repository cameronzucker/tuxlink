// src/wwv/WwvOffairControl.test.tsx
//
// Tests for WwvOffairControl (Task 15, wwv offair spec). The hook module is
// mocked entirely so the component renders deterministically without a real
// Tauri context — see useWwvOffair.ts for the real (invoke-backed) shape.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import type { UseWwvOffairResult, WwvOffairStatus } from './useWwvOffair';
import type { SolarSnapshot } from './wwvApi';

// ---------------------------------------------------------------------------
// Module-level mock (hoisted above imports per vi.mock hoisting rules)
// ---------------------------------------------------------------------------

const mockArm = vi.fn((_nowMs: number) => Promise.resolve());
const mockRefreshSnapshot = vi.fn(() => Promise.resolve());

let mockStatus: WwvOffairStatus = 'idle';
let mockSnapshot: SolarSnapshot | null = null;

vi.mock('./useWwvOffair', () => ({
  useWwvOffair: (): UseWwvOffairResult => ({
    status: mockStatus,
    result: null,
    snapshot: mockSnapshot,
    arm: mockArm,
    refreshSnapshot: mockRefreshSnapshot,
  }),
}));

// ---------------------------------------------------------------------------
// Subject under test (imported AFTER the mock is set up)
// ---------------------------------------------------------------------------

import { WwvOffairControl } from './WwvOffairControl';

beforeEach(() => {
  vi.clearAllMocks();
  mockStatus = 'idle';
  mockSnapshot = null;
});

describe('WwvOffairControl', () => {
  it('renders a "Refresh off-air" button that arms a capture on click', async () => {
    render(<WwvOffairControl />);
    const button = screen.getByRole('button', { name: 'Refresh off-air' });
    fireEvent.click(button);
    await waitFor(() => {
      expect(mockArm).toHaveBeenCalledOnce();
    });
    // arm(Date.now()) — just check it was called with a number.
    expect(typeof mockArm.mock.calls[0][0]).toBe('number');
  });

  it('mounts without crashing and kicks a background snapshot refresh', () => {
    render(<WwvOffairControl />);
    expect(mockRefreshSnapshot).toHaveBeenCalledOnce();
  });

  it('shows "Capturing…" and disables the button while status is capturing', () => {
    mockStatus = 'capturing';
    render(<WwvOffairControl />);
    const button = screen.getByRole('button', { name: 'Capturing…' });
    expect(button).toBeDisabled();
  });

  it('shows the off-air provenance stamp when the snapshot is rf-wwv-voice sourced', () => {
    mockSnapshot = {
      indices: { sfi: 150, k_index: 2 },
      updated_at_ms: 1_700_000_000_000,
      source: 'rf-wwv-voice',
      forecast_updated: true,
    };
    render(<WwvOffairControl />);
    const stamp = screen.getByTestId('wwv-offair-provenance');
    expect(stamp.textContent).toContain('SFI');
    expect(stamp.textContent).toContain('150');
    expect(stamp.textContent).toContain('K');
    expect(stamp.textContent).toContain('2');
  });

  it('omits the K index from the provenance stamp when absent', () => {
    mockSnapshot = {
      indices: { sfi: 140 },
      updated_at_ms: 1_700_000_000_000,
      source: 'rf-wwv-voice',
      forecast_updated: true,
    };
    render(<WwvOffairControl />);
    const stamp = screen.getByTestId('wwv-offair-provenance');
    expect(stamp.textContent).toContain('SFI');
    expect(stamp.textContent).not.toContain('K ');
  });

  it('does not show the provenance stamp for a non-off-air-sourced snapshot', () => {
    mockSnapshot = {
      indices: { sfi: 150, k_index: 2 },
      updated_at_ms: 1_700_000_000_000,
      source: 'swpc',
      forecast_updated: true,
    };
    render(<WwvOffairControl />);
    expect(screen.queryByTestId('wwv-offair-provenance')).toBeNull();
  });

  it('shows a no-copy note when status is nocopy', () => {
    mockStatus = 'nocopy';
    render(<WwvOffairControl />);
    expect(screen.getByTestId('wwv-offair-nocopy')).toBeInTheDocument();
  });

  it('shows an error note when status is error', () => {
    mockStatus = 'error';
    render(<WwvOffairControl />);
    expect(screen.getByTestId('wwv-offair-error')).toBeInTheDocument();
  });
});
