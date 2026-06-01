// src/radio/modes/PacketRadioPanel.test.tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { PacketRadioPanel } from './PacketRadioPanel';

// Tauri IPC mocks. `invoke` returns command-specific defaults; `listen`
// resolves to a no-op unlisten so useSessionLog cleanup runs cleanly
// (matches TelnetRadioPanel.test idiom; we don't dispatch synthetic log
// events in this suite, so no handler-capture is needed).
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}));

const DEFAULT_CONFIG = {
  ssid: 7,
  listenDefault: true,
  linkKind: 'Tcp',
  tcpHost: '127.0.0.1',
  tcpPort: 8001,
  serialDevice: null,
  serialBaud: null,
  txdelay: 30,
  persistence: 63,
  slotTime: 10,
  paclen: 128,
  maxframe: 4,
  t1Ms: 3000,
  n2Retries: 10,
};

// Default invoke implementation — applied per-test in beforeEach so a test
// that overrides via mockImplementation cannot leak into the next test.
const defaultInvokeImpl = async (cmd: string) => {
  if (cmd === 'packet_config_get') return DEFAULT_CONFIG;
  if (cmd === 'session_log_snapshot') return [];
  return undefined;
};

describe('<PacketRadioPanel>', () => {
  beforeEach(async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockReset();
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(defaultInvokeImpl);
  });

  it('renders the Packet Winlink panel title for intent=cms', () => {
    render(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Packet Winlink');
  });

  it('renders the Packet P2P panel title for intent=p2p', () => {
    render(<PacketRadioPanel intent="p2p" baseCall="N7CPZ" onClose={() => {}} />);
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Packet P2P');
  });

  it('renders the ModemLinkSection', async () => {
    render(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('modem-link-section')).toBeInTheDocument();
    });
  });

  it('renders the SessionLog section', () => {
    render(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    expect(screen.getByTestId('session-log-section')).toBeInTheDocument();
  });

  it('shows Listen action for intent=p2p', async () => {
    render(<PacketRadioPanel intent="p2p" baseCall="N7CPZ" onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('packet-listen-btn')).toBeInTheDocument();
    });
  });

  it('hides Listen action for intent=cms (cms-gateway is connect-only)', async () => {
    render(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('modem-link-section')).toBeInTheDocument();
    });
    expect(screen.queryByTestId('packet-listen-btn')).not.toBeInTheDocument();
  });

  it('shows effective callsign (base-SSID) from config_get', async () => {
    render(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('packet-effective-call')).toHaveTextContent('N7CPZ-7');
    });
  });

  it('clicking Connect fires packet_connect with the typed call sign and empty path', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    render(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    await waitFor(() => expect(screen.getByTestId('packet-target-input')).toBeInTheDocument());
    fireEvent.change(screen.getByTestId('packet-target-input'), { target: { value: 'W7RPT' } });
    fireEvent.click(screen.getByTestId('packet-start-btn'));
    expect(invoke).toHaveBeenCalledWith('packet_connect', { call: 'W7RPT', path: [] });
  });

  it('clicking Connect with a relay path fires packet_connect with that path', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    render(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    await waitFor(() => expect(screen.getByTestId('packet-target-input')).toBeInTheDocument());
    fireEvent.change(screen.getByTestId('packet-target-input'), { target: { value: 'W7RPT' } });
    fireEvent.click(screen.getByTestId('packet-add-relay'));
    fireEvent.change(screen.getByTestId('packet-relay-0'), { target: { value: 'W7XYZ-1' } });
    fireEvent.click(screen.getByTestId('packet-start-btn'));
    expect(invoke).toHaveBeenCalledWith('packet_connect', {
      call: 'W7RPT',
      path: ['W7XYZ-1'],
    });
  });

  it('clicking Connect with empty target does NOT fire packet_connect', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    render(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    await waitFor(() => expect(screen.getByTestId('packet-target-input')).toBeInTheDocument());
    (invoke as ReturnType<typeof vi.fn>).mockClear();
    fireEvent.click(screen.getByTestId('packet-start-btn'));
    // Sift: no call to packet_connect among any invocations.
    const calls = (invoke as ReturnType<typeof vi.fn>).mock.calls.filter(
      (c: unknown[]) => c[0] === 'packet_connect',
    );
    expect(calls).toHaveLength(0);
  });

  it('clicking Listen (intent=p2p) fires packet_listen', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    render(<PacketRadioPanel intent="p2p" baseCall="N7CPZ" onClose={() => {}} />);
    await waitFor(() => expect(screen.getByTestId('packet-listen-btn')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('packet-listen-btn'));
    expect(invoke).toHaveBeenCalledWith('packet_listen');
  });

  it('changing SSID persists the new config via packet_config_set', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    render(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    await waitFor(() => expect(screen.getByTestId('packet-ssid-select')).toBeInTheDocument());
    fireEvent.change(screen.getByTestId('packet-ssid-select'), { target: { value: '10' } });
    expect(invoke).toHaveBeenCalledWith(
      'packet_config_set',
      expect.objectContaining({ dto: expect.objectContaining({ ssid: 10 }) }),
    );
  });

  it('switching modem segment (TCP → USB) persists via packet_config_set', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    render(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    await waitFor(() => expect(screen.getByTestId('modem-seg-usb')).toBeInTheDocument());
    (invoke as ReturnType<typeof vi.fn>).mockClear();
    fireEvent.click(screen.getByTestId('modem-seg-usb'));
    expect(invoke).toHaveBeenCalledWith(
      'packet_config_set',
      expect.objectContaining({
        dto: expect.objectContaining({ linkKind: 'Serial' }),
      }),
    );
  });

  it('falls back to defaults when packet_config_get rejects', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'packet_config_get') throw new Error('NotConfigured');
      if (cmd === 'session_log_snapshot') return [];
      return undefined;
    });
    render(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    // Panel still renders the modem section using fallback defaults.
    await waitFor(() => {
      expect(screen.getByTestId('modem-link-section')).toBeInTheDocument();
    });
  });
});
