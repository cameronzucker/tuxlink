import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock('@tauri-apps/api/event', () => ({
  // Resolve to a no-op unlisten; the component-under-test drives state from the
  // command returns, so the event subscription is inert in these tests.
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import { UvproControlStrip } from './UvproControlStrip';
import type { UvproStatus } from './uvproTypes';

const base: UvproStatus = {
  state: 'disconnected',
  isTx: false,
  isRx: false,
  squelchOpen: false,
  powerOn: false,
  gpsLocked: false,
};

const connected: UvproStatus = {
  ...base,
  state: 'connected',
  deviceModel: 'UV-Pro',
  currentChannelId: 1,
  rxMhz: 146.52,
  txMhz: 146.52,
  mode: 'fm',
  batteryPercent: 80,
  rssi: 7,
  powerOn: true,
};

describe('UvproControlStrip', () => {
  beforeEach(() => invokeMock.mockReset());

  it('offers Connect when disconnected', async () => {
    invokeMock.mockResolvedValue(base);
    render(<UvproControlStrip />);
    expect(await screen.findByTestId('uvpro-connect')).toBeInTheDocument();
    expect(screen.getByTestId('uvpro-state')).toHaveAttribute('data-state', 'disconnected');
  });

  it('shows device status + a channel selector once connected', async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'uvpro_get_status') return Promise.resolve(connected);
      if (cmd === 'uvpro_get_channels')
        return Promise.resolve([
          { channelId: 1, name: 'Simplex', rxMhz: 146.52, txMhz: 146.52, mode: 'fm', bandwidth: 'wide', txDisable: false },
          { channelId: 2, name: 'Repeater', rxMhz: 146.94, txMhz: 146.34, mode: 'fm', bandwidth: 'wide', txDisable: false },
        ]);
      return Promise.resolve(undefined);
    });
    render(<UvproControlStrip />);
    expect(await screen.findByTestId('uvpro-connected')).toBeInTheDocument();
    expect(screen.getByTestId('uvpro-battery')).toHaveTextContent('80%');
    const select = await screen.findByTestId('uvpro-channel-select');
    await waitFor(() =>
      expect(select.querySelectorAll('option').length).toBeGreaterThanOrEqual(2),
    );
  });

  it('invokes uvpro_connect when Connect is clicked', async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'uvpro_connect') return Promise.resolve(connected);
      return Promise.resolve(base); // initial get_status
    });
    render(<UvproControlStrip />);
    fireEvent.click(await screen.findByTestId('uvpro-connect'));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('uvpro_connect', {}));
    expect(await screen.findByTestId('uvpro-connected')).toBeInTheDocument();
  });

  it('switches channel via uvpro_set_channel', async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'uvpro_get_status') return Promise.resolve(connected);
      if (cmd === 'uvpro_get_channels')
        return Promise.resolve([
          { channelId: 1, name: 'Simplex', rxMhz: 146.52, txMhz: 146.52, mode: 'fm', bandwidth: 'wide', txDisable: false },
          { channelId: 2, name: 'Repeater', rxMhz: 146.94, txMhz: 146.34, mode: 'fm', bandwidth: 'wide', txDisable: false },
        ]);
      if (cmd === 'uvpro_set_channel') return Promise.resolve({ ...connected, currentChannelId: 2 });
      return Promise.resolve(undefined);
    });
    render(<UvproControlStrip />);
    const select = await screen.findByTestId('uvpro-channel-select');
    await waitFor(() =>
      expect(select.querySelectorAll('option').length).toBeGreaterThanOrEqual(2),
    );
    fireEvent.change(select, { target: { value: '2' } });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('uvpro_set_channel', { channelId: 2 }),
    );
  });

  it('surfaces a command error (e.g. external LinkBusy holder)', async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'uvpro_connect')
        return Promise.reject({ kind: 'LinkBusy', message: 'radio in use by phone' });
      return Promise.resolve(base);
    });
    render(<UvproControlStrip />);
    fireEvent.click(await screen.findByTestId('uvpro-connect'));
    expect(await screen.findByTestId('uvpro-error')).toHaveTextContent('radio in use by phone');
  });
});
