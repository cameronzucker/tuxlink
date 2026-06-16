import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';

// Command-aware invoke: AprsSettings now ALSO mounts the radio-link picker
// (usePacketConfig + ModemLinkSection), so packet_config_get / device-list /
// packet_config_set calls must resolve sensibly (tuxlink-rypw #3).
const PACKET_CFG = {
  ssid: 7,
  listenDefault: true,
  linkKind: 'UvproNative',
  tcpHost: null,
  tcpPort: null,
  serialDevice: null,
  serialBaud: null,
  btMac: 'AA:BB:CC:DD:EE:FF',
  txdelay: 30,
  persistence: 63,
  slotTime: 10,
  paclen: 128,
  maxframe: 4,
  t1Ms: 3000,
  n2Retries: 10,
};
const invoke = vi.fn(async (...args: unknown[]) => {
  const cmd = args[0] as string;
  if (cmd === 'aprs_config_get') return { sourceSsid: 0, tocall: 'APZTUX', path: 'WIDE1-1,WIDE2-1' };
  if (cmd === 'packet_config_get') return PACKET_CFG;
  if (cmd === 'packet_list_serial_devices') return [];
  if (cmd === 'packet_list_bluetooth_devices') return [];
  return undefined;
});
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
import { AprsSettings } from './AprsSettings';

describe('AprsSettings', () => {
  it('loads and displays the current APRS config', async () => {
    render(<AprsSettings />);
    await waitFor(() => expect(screen.getByDisplayValue('WIDE1-1,WIDE2-1')).toBeInTheDocument());
    expect(screen.getByText('APZTUX')).toBeInTheDocument();
  });

  it('persists a changed path via aprs_config_set', async () => {
    render(<AprsSettings />);
    await waitFor(() => screen.getByDisplayValue('WIDE1-1,WIDE2-1'));
    fireEvent.change(screen.getByLabelText(/path/i), { target: { value: 'WIDE2-1' } });
    fireEvent.click(screen.getByRole('button', { name: /save/i }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        'aprs_config_set',
        expect.objectContaining({ dto: expect.objectContaining({ path: 'WIDE2-1' }) }),
      ),
    );
  });

  it('renders the radio-link picker so the link is configurable from Settings (tuxlink-rypw #3)', async () => {
    render(<AprsSettings />);
    // The picker decouples link configuration from the connect attempt — fixes the
    // catch-22 where the only place to set the link was the dock connect strip.
    expect(await screen.findByTestId('modem-link-section')).toBeInTheDocument();
  });

  it('persists a link change from Settings via packet_config_set', async () => {
    render(<AprsSettings />);
    await screen.findByTestId('modem-link-section');
    // Wait for the persisted config to load and re-seed the picker before
    // clicking: `usePacketConfig.setLink` is a NO-OP until `config` is loaded
    // (`if (!config) return`), and the section renders before the async
    // `packet_config_get` resolves. Clicking too early drops the change
    // silently — a race that flakes on slow CI (arm64) but passes locally.
    // The loaded config is UvproNative, so the UV-Pro segment becoming active
    // is the deterministic "config loaded" signal.
    await waitFor(() =>
      expect(screen.getByTestId('modem-seg-uvpro')).toHaveAttribute('aria-pressed', 'true'),
    );
    // Tap the TCP segment → ModemLinkSection emits a Tcp link → usePacketConfig.setLink.
    fireEvent.click(screen.getByTestId('modem-seg-tcp'));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        'packet_config_set',
        expect.objectContaining({ dto: expect.objectContaining({ linkKind: 'Tcp' }) }),
      ),
    );
  });
});
