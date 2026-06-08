// src/shell/AppShell.authDiagnostic.test.tsx
//
// AppShell-level integration test for the AuthDiagnosticBanner production mount
// path (tuxlink-7do4 Task 23, R1 #14 of R5 adrev).
//
// WHY THIS FILE EXISTS:
// Task 21's 63 unit tests verify the banner renders correctly given a mocked
// useAuthDiagnostic state. They don't confirm that the full chain —
//   AppShell → TelnetRadioPanel → AuthDiagnosticBanner → useAuthDiagnostic →
//   Tauri `b2f-event` channel
// — integrates correctly in the production mount path.
//
// This test mounts AppShell with the Telnet CMS connection selected, captures
// the `b2f-event` handler that useAuthDiagnostic registers via `listen`, then
// synthesizes an auth_classified event to verify the banner renders end-to-end
// with the correct Mode 3 (password_rejected) copy.
//
// MOCKED vs PRODUCTION:
//   - Mocked: invoke, listen (captured), window controls, react-virtuoso,
//     useModemStatus, useMailbox, useUserFolders, plugin-shell.
//   - Production (not mocked): useAuthDiagnostic, AuthDiagnosticBanner,
//     TelnetRadioPanel, AppShell, authDiagnosticCopy.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { MessageMeta } from '../mailbox/types';
import type { ModemStatus } from '../modem/types';
import type { B2fEvent } from '../connections/sessionTypes';

// ---------------------------------------------------------------------------
// listen() handler capture infrastructure
//
// useAuthDiagnostic registers two listeners:
//   - 'b2f-event'            → dispatches auth state changes
//   - 'auth-diagnostic-clear' → resets state to initial
// useSessionLog registers:
//   - 'session_log:line'     → log entries
//
// The mock captures all three so tests can dispatch synthetic events.
// ---------------------------------------------------------------------------

type EventHandler<T> = (event: { payload: T }) => void;

let capturedB2fHandler: EventHandler<B2fEvent> | null = null;
let capturedClearHandler: EventHandler<void> | null = null;

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (event: string, handler: EventHandler<unknown>) => {
    if (event === 'b2f-event') {
      capturedB2fHandler = handler as EventHandler<B2fEvent>;
    }
    if (event === 'auth-diagnostic-clear') {
      capturedClearHandler = handler as EventHandler<void>;
    }
    // Return a no-op unlisten function.
    return () => {};
  }),
}));

// ---------------------------------------------------------------------------
// useModemStatus — mock so we control whether the radio panel mounts.
// ---------------------------------------------------------------------------
const mockUseModemStatus = vi.fn();
vi.mock('../modem/useModemStatus', () => ({
  useModemStatus: () => mockUseModemStatus(),
  useModemIsActive: () => mockUseModemStatus().status.state !== 'stopped',
  MODEM_STATUS_EVENT: 'modem:status',
}));

// ---------------------------------------------------------------------------
// Tauri IPC mocks (mirrors AppShell.radioPanel.test.tsx).
// ---------------------------------------------------------------------------
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'config_read') return { host: 'cms.winlink.org', transport: 'CmsSsl' };
    if (cmd === 'backend_status') return null;
    if (cmd === 'session_log_snapshot') return [];
    if (cmd === 'message_read') return null;
    if (cmd === 'config_get_ardop') return { cmd_port: 8515 };
    if (cmd === 'config_get_vara') {
      return { host: '127.0.0.1', cmd_port: 8300, data_port: 8301, bandwidth_hz: null };
    }
    if (cmd === 'vara_status') {
      return { state: 'closed', lastError: null, boundHost: null, boundCmdPort: null };
    }
    if (cmd === 'platform_info') {
      return { arch: 'x86_64', os: 'linux', varaSupported: true };
    }
    if (cmd === 'packet_config_get') {
      return {
        ssid: 7, listenDefault: true, linkKind: 'Tcp', tcpHost: '127.0.0.1',
        tcpPort: 8001, serialDevice: null, serialBaud: null, txdelay: 30,
        persistence: 63, slotTime: 10, paclen: 128, maxframe: 4, t1Ms: 3000, n2Retries: 10,
      };
    }
    if (cmd === 'tauri_search_list_saved') return [];
    if (cmd === 'tauri_search_list_recent') return [];
    // auth_diagnostic_clear: invoked by dismiss(); no-op is fine.
    if (cmd === 'auth_diagnostic_clear') return null;
    return undefined;
  }),
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
  ResizeDirection: {
    North: 'North', South: 'South', East: 'East', West: 'West',
    NorthEast: 'NorthEast', NorthWest: 'NorthWest',
    SouthEast: 'SouthEast', SouthWest: 'SouthWest',
  },
}));

vi.mock('@tauri-apps/plugin-shell', () => ({
  open: vi.fn(async () => {}),
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
  useMailboxChangeEvents: () => {},
  useMailbox: (_folder: string) => ({
    messages: [],
    isLoading: false,
    isError: false,
    error: null,
  }),
  isBackendFolder: (f: string) => f === 'inbox' || f === 'outbox' || f === 'sent',
  isUserFolderSlug: (s: string) => /^[a-z0-9-]+$/.test(s) && !s.startsWith('-') && !s.endsWith('-'),
}));

vi.mock('../mailbox/useUserFolders', () => ({
  useUserFolders: () => ({ folders: [], isLoading: false, isError: false, error: null }),
  useCreateUserFolder: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useDeleteUserFolder: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useRenameUserFolder: () => ({ mutateAsync: vi.fn(), isPending: false }),
  USER_FOLDERS_QUERY_KEY: ['userFolders'],
}));

// ---------------------------------------------------------------------------
// Subject under test — imported AFTER mocks are set up.
// ---------------------------------------------------------------------------
import { AppShell } from './AppShell';

// ---------------------------------------------------------------------------
// Render helper
// ---------------------------------------------------------------------------
function renderShell() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <AppShell />
    </QueryClientProvider>,
  );
}

// tuxlink-813d P1 fix: the shell passes `compact={isCompact}` to FolderSidebar.
// jsdom has no `matchMedia` (no global stub in test-setup), so `useViewport`
// returns `isCompact=false` and the shell renders the DESKTOP labeled sidebar —
// the Connections accordion (`sess-*` / `proto-*`) is inline, with no `☰`
// rail-expand button. Click the session header + protocol directly.
function selectConnection(sessTestId: string, protoTestId: string) {
  fireEvent.click(screen.getByTestId(sessTestId));
  fireEvent.click(screen.getByTestId(protoTestId));
}

const STOPPED_STATUS: ModemStatus = {
  state: 'stopped',
  peer: null,
  mode: null,
  widthHz: null,
  pttBackend: null,
  snDb: null,
  vuDbfs: null,
  throughputBps: null,
  bytesRx: 0,
  bytesTx: 0,
  uptimeSec: 0,
  arqFlags: { busy: false, rx: false, tx: false },
  lastError: null,
  quality: null,
};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
describe('<AppShell> AuthDiagnosticBanner integration (production mount path)', () => {
  beforeEach(() => {
    capturedB2fHandler = null;
    capturedClearHandler = null;
    mockUseModemStatus.mockReset();
    mockUseModemStatus.mockReturnValue({ status: STOPPED_STATUS, loading: false, error: null });
    globalThis.localStorage?.clear?.();
  });

  it('renders the AuthDiagnosticBanner with Mode 3 copy when a b2f-event arrives via the Tauri channel', async () => {
    // Step 1: Mount AppShell and navigate to the Telnet CMS panel.
    renderShell();

    // Select CMS → Telnet to mount TelnetRadioPanel (same pattern as AppShell.test.tsx).
    selectConnection('sess-cms', 'proto-cms-telnet');

    // Wait for the lazy TelnetRadioPanel to resolve and render.
    await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 });
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Telnet Winlink');

    // Step 2: Wait for useAuthDiagnostic's listen() call to register the b2f-event handler.
    // The handler is registered inside a useEffect on mount; give the microtask queue
    // a tick to resolve the listen() Promise.
    await waitFor(() => {
      expect(capturedB2fHandler).not.toBeNull();
    }, { timeout: 5000 });

    // Precondition: banner is absent before any auth event.
    expect(screen.queryByTestId('diag-banner')).toBeNull();

    // Step 3: Synthesize a b2f-event with kind='auth_classified', mode='password_rejected'.
    act(() => {
      capturedB2fHandler!({
        payload: {
          kind: 'auth_classified',
          mode: 'password_rejected',
          raw: '*** Secure login failed',
          attempt_id: 1,
        },
      });
    });

    // Step 4: Assert the banner renders with Mode 3 (password_rejected) copy.
    // authDiagnosticCopy returns:
    //   headline: "Your password wasn't accepted by the Winlink server."
    //   body: 'Reset it on winlink.org or re-enter it here.'
    const banner = await screen.findByTestId('diag-banner', undefined, { timeout: 5000 });
    expect(banner).toBeInTheDocument();
    expect(screen.getByTestId('diag-title')).toHaveTextContent(/password/i);

    // Mode 3 affordances render: re-enter password button must be present.
    expect(screen.getByTestId('diag-reenter-password-btn')).toBeInTheDocument();
  });

  it('dismisses the banner when an auth-diagnostic-clear event arrives', async () => {
    // Mount shell + select Telnet to get TelnetRadioPanel.
    renderShell();
    selectConnection('sess-cms', 'proto-cms-telnet');
    await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 });

    // Wait for both handlers to register.
    await waitFor(() => {
      expect(capturedB2fHandler).not.toBeNull();
      expect(capturedClearHandler).not.toBeNull();
    }, { timeout: 5000 });

    // Fire auth_classified to bring the banner up.
    act(() => {
      capturedB2fHandler!({
        payload: {
          kind: 'auth_classified',
          mode: 'password_rejected',
          raw: '*** Secure login failed',
          attempt_id: 1,
        },
      });
    });
    await screen.findByTestId('diag-banner', undefined, { timeout: 5000 });

    // Fire auth-diagnostic-clear to dismiss.
    act(() => {
      capturedClearHandler!({ payload: undefined as unknown as void });
    });

    // Banner must unmount.
    await waitFor(() => {
      expect(screen.queryByTestId('diag-banner')).toBeNull();
    }, { timeout: 5000 });
  });
});
