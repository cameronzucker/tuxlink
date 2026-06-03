import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';

const invokeMock = vi.fn();
const listenMock = vi.fn();
const unlistenMock = vi.fn();
const applyMock = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => listenMock(...args),
}));
vi.mock('../shell/colorScheme', () => ({
  applyColorScheme: (...args: unknown[]) => applyMock(...args),
}));

import { useHelpTheme } from './useHelpTheme';

beforeEach(() => {
  invokeMock.mockReset();
  listenMock.mockReset();
  unlistenMock.mockReset();
  applyMock.mockReset();
  listenMock.mockResolvedValue(unlistenMock);
});

describe('useHelpTheme', () => {
  it('queries theme_get_scheme on mount and applies the result without rebroadcasting (tuxlink-och6)', async () => {
    invokeMock.mockResolvedValue('night-red');
    renderHook(() => useHelpTheme());
    // The hook MUST pass { broadcast: false } so applyColorScheme's own
    // broadcast doesn't loop back through the help window's own listener.
    await waitFor(() => expect(applyMock).toHaveBeenCalledWith('night-red', { broadcast: false }));
    expect(invokeMock).toHaveBeenCalledWith('theme_get_scheme');
  });

  it('subscribes to color_scheme_changed and re-applies on event without rebroadcasting (tuxlink-och6)', async () => {
    invokeMock.mockResolvedValue(null);
    renderHook(() => useHelpTheme());
    await waitFor(() => expect(listenMock).toHaveBeenCalled());
    expect(listenMock.mock.calls[0][0]).toBe('color_scheme_changed');
    // Invoke the handler the hook registered.
    const handler = listenMock.mock.calls[0][1];
    handler({ payload: 'daylight' });
    expect(applyMock).toHaveBeenCalledWith('daylight', { broadcast: false });
  });

  it('does not apply on mount when theme_get_scheme returns null', async () => {
    invokeMock.mockResolvedValue(null);
    renderHook(() => useHelpTheme());
    // Wait for the subscription before asserting on apply.
    await waitFor(() => expect(listenMock).toHaveBeenCalled());
    expect(applyMock).not.toHaveBeenCalled();
  });
});
