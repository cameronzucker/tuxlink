// src/radio/modes/VaraRadioPanel.test.tsx
//
// Behavioral tests for the Phase 2 VARA panel. Mocks `@tauri-apps/api/core`
// so the panel can render + transition without a Tauri runtime. The mock
// returns command-specific defaults; individual tests override via
// `mockImplementation` for failure-path coverage.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { VaraRadioPanel } from './VaraRadioPanel';
import type { RadioPanelMode } from '../types';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}));

const HF_MODE: RadioPanelMode = { kind: 'vara-hf', intent: 'cms' };
const FM_MODE: RadioPanelMode = { kind: 'vara-fm', intent: 'cms' };

const defaultConfig = {
  host: '127.0.0.1',
  cmd_port: 8300,
  data_port: 8301,
  bandwidth_hz: null as number | null,
};

const closedStatus = {
  state: 'closed',
  lastError: null,
  boundHost: null,
  boundCmdPort: null,
};

const openStatus = {
  state: 'open',
  lastError: null,
  boundHost: '127.0.0.1',
  boundCmdPort: 8300,
};

const x86Platform = { arch: 'x86_64', os: 'linux', varaSupported: true };
const armPlatform = { arch: 'aarch64', os: 'linux', varaSupported: false };

function makeInvoke(overrides: Record<string, unknown> = {}) {
  return async (cmd: string, _args?: unknown) => {
    if (cmd in overrides) {
      const v = overrides[cmd];
      if (v instanceof Error) throw v;
      return v;
    }
    if (cmd === 'config_get_vara') return defaultConfig;
    if (cmd === 'vara_status') return closedStatus;
    if (cmd === 'platform_info') return x86Platform;
    if (cmd === 'session_log_snapshot') return [];
    return undefined;
  };
}

describe('<VaraRadioPanel>', () => {
  beforeEach(async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockReset();
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(makeInvoke());
  });

  it('renders the VARA HF panel title for vara-hf mode', async () => {
    render(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Vara HF');
  });

  it('renders the VARA FM panel title for vara-fm mode', async () => {
    render(<VaraRadioPanel mode={FM_MODE} onClose={() => {}} />);
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Vara FM');
  });

  it('hydrates host + ports from config_get_vara', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
      makeInvoke({
        config_get_vara: { host: '10.0.0.5', cmd_port: 8400, data_port: 8401, bandwidth_hz: 2300 },
      }),
    );
    render(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await waitFor(() => {
      expect((screen.getByTestId('vara-host-input') as HTMLInputElement).value).toBe('10.0.0.5');
    });
    expect((screen.getByTestId('vara-cmd-port-input') as HTMLInputElement).value).toBe('8400');
    expect((screen.getByTestId('vara-data-port-input') as HTMLInputElement).value).toBe('8401');
  });

  it('uses defaults when config_get_vara rejects (pre-wizard)', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
      makeInvoke({ config_get_vara: new Error('NotConfigured') }),
    );
    render(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await waitFor(() => {
      expect((screen.getByTestId('vara-host-input') as HTMLInputElement).value).toBe('127.0.0.1');
    });
    expect((screen.getByTestId('vara-cmd-port-input') as HTMLInputElement).value).toBe('8300');
  });

  it('hydrates status from vara_status', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
      makeInvoke({ vara_status: openStatus }),
    );
    render(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('vara-state-display')).toHaveTextContent('State: open');
    });
  });

  it('disables Start when transport is already open', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
      makeInvoke({ vara_status: openStatus }),
    );
    render(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('vara-start-btn')).toBeDisabled();
    });
  });

  it('disables Stop when transport is closed', async () => {
    render(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('vara-stop-btn')).toBeDisabled();
    });
  });

  it('invokes vara_start_session on Start click and updates status', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
    invokeSpy.mockImplementation(
      makeInvoke({ vara_start_session: openStatus }),
    );
    render(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('vara-start-btn')).not.toBeDisabled();
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('vara-start-btn'));
    });

    await waitFor(() => {
      expect(screen.getByTestId('vara-state-display')).toHaveTextContent('State: open');
    });
    expect(invokeSpy).toHaveBeenCalledWith('vara_start_session');
  });

  it('surfaces start-failure error inline', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
      makeInvoke({ vara_start_session: new Error('TCP connect failed: Connection refused (os error 111)') }),
    );
    render(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('vara-start-btn')).not.toBeDisabled();
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('vara-start-btn'));
    });

    await waitFor(() => {
      expect(screen.getByTestId('vara-action-error')).toHaveTextContent('Start failed');
    });
  });

  it('invokes vara_stop_session on Stop click', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
    invokeSpy.mockImplementation(
      makeInvoke({
        vara_status: openStatus,
        vara_stop_session: closedStatus,
      }),
    );
    render(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('vara-stop-btn')).not.toBeDisabled();
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('vara-stop-btn'));
    });

    await waitFor(() => {
      expect(screen.getByTestId('vara-state-display')).toHaveTextContent('State: closed');
    });
    expect(invokeSpy).toHaveBeenCalledWith('vara_stop_session');
  });

  it('renders the Pi-availability banner on ARM and disables Start', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
      makeInvoke({ platform_info: armPlatform }),
    );
    render(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('vara-platform-banner')).toBeInTheDocument();
    });
    expect(screen.getByTestId('vara-start-btn')).toBeDisabled();
    // Host input is also disabled on blocked platforms.
    expect(screen.getByTestId('vara-host-input')).toBeDisabled();
  });

  it('does not render the banner on x86_64', async () => {
    render(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await waitFor(() => {
      // Wait for at least one hydration so platform_info has been awaited.
      expect((screen.getByTestId('vara-host-input') as HTMLInputElement).value).toBe('127.0.0.1');
    });
    expect(screen.queryByTestId('vara-platform-banner')).toBeNull();
  });

  it('rejects an out-of-range cmd_port and reverts the input', async () => {
    render(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    const input = screen.getByTestId('vara-cmd-port-input') as HTMLInputElement;
    await waitFor(() => expect(input.value).toBe('8300'));

    await act(async () => {
      fireEvent.change(input, { target: { value: '99999' } });
      fireEvent.blur(input);
    });

    await waitFor(() => {
      expect(input.value).toBe('8300'); // reverted
    });
    expect(screen.getByTestId('vara-action-error')).toHaveTextContent('Invalid cmd port');
  });

  it('renders the bandwidth options and reflects null as Auto', async () => {
    render(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    const select = await waitFor(() => screen.getByTestId('vara-bandwidth-select') as HTMLSelectElement);
    expect(select.value).toBe(''); // null bandwidth = "" (Auto)
    expect(screen.getByText(/2300 Hz \(HF Standard\)/)).toBeInTheDocument();
    expect(screen.getByText(/2750 Hz \(HF Tactical\)/)).toBeInTheDocument();
  });

  it('persists bandwidth change via setConfig → config_set_vara', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
    render(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    const select = await waitFor(() => screen.getByTestId('vara-bandwidth-select') as HTMLSelectElement);

    await act(async () => {
      fireEvent.change(select, { target: { value: '2750' } });
    });

    await waitFor(() => {
      const setCalls = invokeSpy.mock.calls.filter((c) => c[0] === 'config_set_vara');
      expect(setCalls.length).toBeGreaterThanOrEqual(1);
      expect(setCalls[setCalls.length - 1][1]).toEqual({
        value: { host: '127.0.0.1', cmd_port: 8300, data_port: 8301, bandwidth_hz: 2750 },
      });
    });
  });
});
