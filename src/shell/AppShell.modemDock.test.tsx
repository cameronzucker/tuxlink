// Task 4.3 — AppShell mounts the ArdopDock as a 4th panes-grid column when
// the modem is anything other than stopped, and applies the `panes--with-dock`
// class swap so the CSS widens to four tracks (200 / 340 / 1fr / 290). When the
// modem is stopped, the dock is absent and the existing 3-col grid stays.
//
// This file lives separately from AppShell.test.tsx so the dock-mount story is
// readable in isolation. The provider wrapping + Tauri IPC mocks mirror the
// existing AppShell test so the shell mounts cleanly under jsdom.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { MailboxFolder, MessageMeta } from '../mailbox/types';
import { STOPPED, type ModemStatus } from '../modem/types';

// Mock the useModemStatus hook directly so the test controls the modem state
// without touching `invoke('modem_get_status')` or the `listen` event channel.
const mockUseModemStatus = vi.fn();
vi.mock('../modem/useModemStatus', () => ({
  useModemStatus: () => mockUseModemStatus(),
  MODEM_STATUS_EVENT: 'modem:status',
}));

// Tauri IPC mocks — match the existing AppShell.test.tsx setup so the shell
// mounts (DashboardRibbon's useStatusData, SessionLog snapshot, etc.).
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'config_read') return null;
    if (cmd === 'backend_status') return null;
    if (cmd === 'session_log_snapshot') return [];
    if (cmd === 'message_read') return null;
    if (cmd === 'packet_config_get') {
      return {
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
    }
    return undefined;
  }),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    label: 'main',
    setTitle: vi.fn(async () => {}),
    minimize: vi.fn(async () => {}),
    toggleMaximize: vi.fn(async () => {}),
    close: vi.fn(async () => {}),
    startResizeDragging: vi.fn(async () => {}),
  }),
  ResizeDirection: { North:'North',South:'South',East:'East',West:'West',NorthEast:'NorthEast',NorthWest:'NorthWest',SouthEast:'SouthEast',SouthWest:'SouthWest' },
}));

vi.mock('react-virtuoso', () => ({
  Virtuoso: ({
    data,
    itemContent,
  }: {
    data: MessageMeta[];
    itemContent: (i: number, m: MessageMeta) => unknown;
  }) => (
    <div data-testid="virtuoso-mock">
      {data.map((m, i) => (
        <div key={m.id}>{itemContent(i, m) as ReactNode}</div>
      ))}
    </div>
  ),
}));

vi.mock('../mailbox/useMailbox', () => ({
  useMailbox: (_folder: MailboxFolder) => ({
    messages: [],
    isLoading: false,
    isError: false,
    error: null,
  }),
  isBackendFolder: (f: MailboxFolder) => f === 'inbox' || f === 'outbox' || f === 'sent',
}));

import { AppShell } from './AppShell';

function renderShell() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <AppShell />
    </QueryClientProvider>,
  );
}

const RUNNING: ModemStatus = {
  state: 'connected-irs',
  peer: 'W7RMS-10',
  mode: '4FSK 500',
  widthHz: 500,
  pttBackend: 'rts',
  snDb: 8.4,
  vuDbfs: -18.0,
  throughputBps: 540,
  bytesRx: 4128,
  bytesTx: 982,
  uptimeSec: 222,
  arqFlags: { busy: false, rx: false, tx: false },
  lastError: null,
};

describe('<AppShell> modem dock', () => {
  beforeEach(() => {
    mockUseModemStatus.mockReset();
  });

  it('does NOT render the dock when modem is stopped', () => {
    mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false, error: null });
    renderShell();
    expect(screen.queryByTestId('ardop-dock-root')).not.toBeInTheDocument();
    // The 3-col grid class swap is absent.
    expect(screen.getByTestId('shell-panes')).not.toHaveClass('panes--with-dock');
  });

  it('renders the dock + applies the 4-col grid class when modem is running', () => {
    mockUseModemStatus.mockReturnValue({ status: RUNNING, loading: false, error: null });
    renderShell();
    expect(screen.getByTestId('ardop-dock-root')).toBeInTheDocument();
    expect(screen.getByTestId('shell-panes')).toHaveClass('panes--with-dock');
  });

  it('renders the dock for transient (non-stopped) states like connecting', () => {
    mockUseModemStatus.mockReturnValue({
      status: { ...STOPPED, state: 'connecting' },
      loading: false,
      error: null,
    });
    renderShell();
    expect(screen.getByTestId('ardop-dock-root')).toBeInTheDocument();
    expect(screen.getByTestId('shell-panes')).toHaveClass('panes--with-dock');
  });
});
