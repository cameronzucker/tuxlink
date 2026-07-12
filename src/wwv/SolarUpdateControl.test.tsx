import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

// vi.hoisted + a hoisted mock reference (rather than `vi.mock(factory)` +
// `vi.mocked(invoke)`) — the pattern VerifyCmsDialog.test.tsx uses for its
// invoke-rejection cases.
const invokeMock = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

import { SolarUpdateControl } from './SolarUpdateControl';

// Block-bodied (not `() => invokeMock.mockReset()`): an expression-bodied
// beforeEach here returns `mockReset()`'s own return value (the mock, for
// chaining) as the hook's result, which reproducibly turned the second test's
// caught rejection into a spurious cross-test "unhandled rejection" failure
// under this file's exact mock-reset + reject-then-render + findByTestId
// shape — confirmed by a bisection that isolated this one line as the sole
// variable. A void-returning block body avoids it.
beforeEach(() => {
  invokeMock.mockReset();
});

describe('SolarUpdateControl', () => {
  it('invokes propagation_update_solar and shows updating→idle across the request', async () => {
    let resolveInvoke: (v: unknown) => void = () => {};
    invokeMock.mockReturnValue(
      new Promise((resolve) => {
        resolveInvoke = resolve;
      }),
    );

    render(<SolarUpdateControl />);
    const button = screen.getByTestId('solar-update-button');
    expect(button.textContent).toBe('Update propagation data');

    fireEvent.click(button);
    expect(invokeMock).toHaveBeenCalledWith('propagation_update_solar');
    expect(button.textContent).toBe('Updating…');
    expect((button as HTMLButtonElement).disabled).toBe(true);

    resolveInvoke({ forecast_updated: true, indices: { sfi: 117, a_index: 6, k_index: 1.33 }, source: 'swpc' });

    await waitFor(() => expect(button.textContent).toBe('Update propagation data'));
    expect((button as HTMLButtonElement).disabled).toBe(false);
    expect(screen.queryByTestId('solar-update-error')).toBeNull();
  });

  it('shows an error note and re-enables the button when the fetch fails', async () => {
    invokeMock.mockRejectedValue(new Error('network down'));

    render(<SolarUpdateControl />);
    fireEvent.click(screen.getByTestId('solar-update-button'));

    const errorNote = await screen.findByTestId('solar-update-error');
    expect(errorNote.textContent).toMatch(/propagation update failed/i);
    expect((screen.getByTestId('solar-update-button') as HTMLButtonElement).disabled).toBe(false);
  });
});
