// App-level Routines menu + inline full-pane surface mount (production mount
// path). routines plan-5 Task 7: Routines → Routines / Routines → New
// Routine… open the inline RoutinesSurface in the main pane — the chrome rows
// (titlebar, menubar, ribbon, statusbar) stay visible; the mailbox
// master-detail (FolderSidebar + message list + reading pane) is replaced,
// not layered under an overlay. Mock scaffold copied from
// AppShell.aprs.test.tsx per the task-7 brief (that file knows which backends
// must be mocked for a real AppShell mount).
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, within } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import type { MessageMeta } from '../mailbox/types';
import type { RunListEntry, JournalEntry } from '../routines/routinesApi';

// routines plan-5 Task 14 (spec §12): mutable launch-recovery fixture for
// <ConsentGate>'s `routines_runs_list` read. Referenced inside the
// `vi.mock('@tauri-apps/api/core', …)` factory below — safe because the
// factory's inner `invoke` body only runs at actual call time (deep inside a
// test's `render()`), long after every top-level declaration in this module
// (including this `let`, declared AFTER the vi.mock call) has initialized.
// Defaults to `[]` so every PRE-EXISTING test in this file (which never
// touches this variable) sees no live parked run, unchanged from before.
let consentRunsListResult: RunListEntry[] = [];
const CONSENT_TEST_RUN_ID = 'run-consent-1';
const CONSENT_TEST_ROUTINE = 'Net-opening checklist (attended)';
const CONSENT_TEST_STEP_ID = 's4';
const CONSENT_TEST_JOURNAL: JournalEntry[] = [
  {
    ts_unix: 1000,
    run_id: CONSENT_TEST_RUN_ID,
    seq: 1,
    event: {
      type: 'step_intent',
      step: CONSENT_TEST_STEP_ID,
      action: 'radio.connect',
      resolved_params: { gateway: 'W7BO-10' },
    },
  },
];

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd?: string, args?: Record<string, unknown>) => {
    // Teardown pitfall: invoke mocks are called with NO args at teardown —
    // always resolve rather than falling through to `undefined` command
    // branches that might throw.
    if (cmd === undefined) return undefined;
    // routines plan-5 Task 14: <ConsentGate>'s launch-recovery reads, mounted
    // unconditionally at AppShell level (see below).
    if (cmd === 'routines_runs_list') return consentRunsListResult;
    if (cmd === 'routines_run_status') {
      return args?.runId === CONSENT_TEST_RUN_ID
        ? { runId: CONSENT_TEST_RUN_ID, routine: CONSENT_TEST_ROUTINE, dryRun: false, state: 'awaiting_consent' }
        : null;
    }
    if (cmd === 'routines_journal') {
      return args?.runId === CONSENT_TEST_RUN_ID ? CONSENT_TEST_JOURNAL : [];
    }
    if (cmd === 'config_read') return null;
    if (cmd === 'backend_status') return null;
    if (cmd === 'session_log_snapshot') return [];
    if (cmd === 'modem_get_status') {
      return {
        state: 'stopped',
        peer: null, mode: null, widthHz: null, pttBackend: null,
        snDb: null, vuDbfs: null, throughputBps: null,
        bytesRx: 0, bytesTx: 0, uptimeSec: 0,
        arqFlags: { busy: false, rx: false, tx: false },
        lastError: null,
      };
    }
    if (cmd === 'packet_config_get') {
      return {
        ssid: 7, listenDefault: true, linkKind: null, btMac: null, tcpHost: null,
        tcpPort: null, serialDevice: null, serialBaud: null, txdelay: 30,
        persistence: 63, slotTime: 10, paclen: 128, maxframe: 4,
        t1Ms: 3000, n2Retries: 10,
      };
    }
    if (cmd === 'position_status') return { gps_ready: false, broadcast_grid: '', ui_grid: '' };
    if (cmd === 'tauri_search_list_saved') return [];
    if (cmd === 'tauri_search_list_recent') return [];
    if (cmd === 'contacts_read') return [];
    if (cmd === 'contacts_suggestions') return [];
    if (cmd === 'aprs_config_get') return { listenDefault: false };
    if (cmd === 'mailbox_list') return [ROUTINES_TEST_INBOX_MSG];
    // RoutineDesigner's always-on validation debounce (Task 9) can fire
    // while this suite's tests are still running real timers — resolve with
    // an empty finding list rather than falling through to `undefined`
    // (`ValBar`'s `findings.filter(...)` would throw on that).
    if (cmd === 'routines_validate_draft') return [];
    // CanvasTab's (Task 10) `layoutCanvas` calls `.map` on the action
    // registry unconditionally — resolve with an empty registry rather than
    // `undefined` (every action then renders as "unknown", which is a valid,
    // non-crashing render, not a test concern here).
    if (cmd === 'routines_actions_list') return [];
    return undefined;
  }),
}));

// A single inbox message so the mailbox master-detail has a real row to
// assert against when routinesView is null.
const ROUTINES_TEST_INBOX_MSG: MessageMeta = {
  id: 'INBOX1',
  subject: 'Inbox subject',
  from: 'KK4XYZ@winlink.org',
  to: [],
  date: '2026-05-19T14:00:00Z',
  unread: true,
  bodySize: 100,
  hasAttachments: false,
};

// Real react-virtuoso renders into a zero-height scroller under jsdom (no
// layout engine), so rows never paint. Flat-render stub per AppShell.test.tsx.
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

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ setTitle: vi.fn(async () => {}) }),
}));

vi.mock('../map/basemapLeaflet', async () => {
  const L = (await import('leaflet')).default;
  return {
    buildBaseLayers: () => [L.layerGroup()],
    OSM_ATTRIBUTION: '© OpenStreetMap contributors',
    flavorBackground: () => '#34373d',
  };
});

import { AppShell } from './AppShell';

function renderShell() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <AppShell />
    </QueryClientProvider>,
  );
}

// Drive a menu action through the rendered <MenuBar> (mirrors AppShell.test.tsx's
// clickMenu helper) — scoped to the menubar so item labels don't collide with
// other on-screen buttons. The Routines top-level label and its "Routines"
// leaf item share the same accessible name by design (menuModel.ts), so both
// match `item` once the dropdown is open — the leaf is always the LAST match
// (MenuBar.tsx renders the top button, then the dropdown, in that DOM order).
function clickMenu(top: string, item: RegExp) {
  const menubar = screen.getByRole('menubar');
  fireEvent.click(within(menubar).getByRole('button', { name: top }));
  const matches = within(menubar).getAllByRole('button', { name: item });
  fireEvent.click(matches[matches.length - 1]);
}

describe('Routines menu + inline surface mount', () => {
  beforeEach(() => {
    globalThis.localStorage?.clear?.();
  });

  it('offers a top-level Routines menu with Routines and New Routine… items', () => {
    renderShell();
    const menubar = screen.getByRole('menubar');
    fireEvent.click(within(menubar).getByRole('button', { name: 'Routines' }));
    // Both the top-level menu button AND the "Routines" leaf item match this
    // name (menuModel.ts, by design) — the top button + the dropdown's leaf.
    expect(within(menubar).getAllByRole('button', { name: 'Routines' })).toHaveLength(2);
    expect(within(menubar).getByRole('button', { name: 'New Routine…' })).toBeInTheDocument();
  });

  it('mounts the mailbox master-detail (FolderSidebar) before Routines is opened', () => {
    renderShell();
    expect(screen.getByTestId('folder-sidebar')).toBeInTheDocument();
    expect(screen.queryByTestId('routines-dashboard')).not.toBeInTheDocument();
  });

  it('Routines → Routines opens the inline dashboard surface, replacing the mailbox master-detail', async () => {
    renderShell();
    clickMenu('Routines', /^Routines$/);
    expect(await screen.findByTestId('routines-dashboard')).toBeInTheDocument();
    // Full-pane, no folder sidebar (Global Constraint 2/brief binding constraint 2).
    expect(screen.queryByTestId('folder-sidebar')).not.toBeInTheDocument();
    // The chrome rows stay mounted — the menubar and titlebar controls
    // (min/max/close) remain visible, proving this is an inline in-pane
    // mount, not a new OS window.
    expect(screen.getByRole('menubar')).toBeInTheDocument();
    expect(screen.getByTestId('status-bar')).toBeInTheDocument();
  });

  it('Routines → New Routine… opens the surface on a fresh, unsaved designer draft', async () => {
    renderShell();
    clickMenu('Routines', /New Routine…/);
    // RoutineDesigner (Task 9) mounts for real now — a fresh/new draft
    // (empty routine name) renders an editable name field and never fetches
    // a def from the backend (task-9 brief binding constraint 6).
    await screen.findByTestId('routine-designer');
    expect(screen.getByTestId('designer-name-input')).toBeInTheDocument();
    expect(screen.queryByTestId('routines-dashboard')).not.toBeInTheDocument();
    expect(screen.queryByTestId('folder-sidebar')).not.toBeInTheDocument();
  });

  // Post-review narrowing (task-7 reviewer finding): only mail-domain actions
  // that navigate the main pane back to the mailbox close the surface
  // (ROUTINES_CLOSING_MENU_ACTIONS in AppShell.tsx). Overlays layer over it
  // and view/chrome toggles restyle it in place — mirroring the existing
  // overlay interplay, where SettingsPanel / StationFinderPanel /
  // RequestCenter are position:fixed overlays that never close each other or
  // the pane beneath them.

  it('a mailbox-navigating action (Message → New Message) closes the surface', async () => {
    renderShell();
    clickMenu('Routines', /^Routines$/);
    expect(await screen.findByTestId('routines-dashboard')).toBeInTheDocument();
    clickMenu('Message', /New Message/);
    expect(screen.queryByTestId('routines-dashboard')).not.toBeInTheDocument();
    // The mailbox master-detail is restored.
    expect(screen.getByTestId('folder-sidebar')).toBeInTheDocument();
  });

  it('a color-scheme change re-themes the surface without closing it', async () => {
    renderShell();
    clickMenu('Routines', /^Routines$/);
    expect(await screen.findByTestId('routines-dashboard')).toBeInTheDocument();
    // Scheme items live in the View → Color scheme submenu; its items are in
    // the DOM once the View dropdown is open (MenuBar renders submenus
    // unconditionally inside the open dropdown; CSS handles the reveal).
    clickMenu('View', /Night \/ tactical \(red\)/);
    expect(screen.getByTestId('routines-dashboard')).toBeInTheDocument();
    expect(screen.queryByTestId('folder-sidebar')).not.toBeInTheDocument();
  });

  it('the radio-panel toggle (Ctrl+Shift+M muscle memory) does not close the surface', async () => {
    renderShell();
    clickMenu('Routines', /^Routines$/);
    expect(await screen.findByTestId('routines-dashboard')).toBeInTheDocument();
    clickMenu('View', /Toggle Radio Panel/);
    expect(screen.getByTestId('routines-dashboard')).toBeInTheDocument();
  });

  it('opening the Settings overlay layers it OVER the surface without closing it', async () => {
    renderShell();
    clickMenu('Routines', /^Routines$/);
    expect(await screen.findByTestId('routines-dashboard')).toBeInTheDocument();
    clickMenu('Tools', /Settings…/);
    // SettingsPanel is a position:fixed inset-0 overlay — it renders over the
    // routines surface exactly as it renders over the mailbox, and closing it
    // returns the operator to where they were. Routines stays mounted.
    expect(await screen.findByTestId('settings-panel', {}, { timeout: 5000 })).toBeInTheDocument();
    expect(screen.getByTestId('routines-dashboard')).toBeInTheDocument();
  });
});

// routines plan-5 Task 14 (spec §12, flow 4): the Part 97 transmit-consent
// moment, mounted ALWAYS at AppShell level ("consent cannot hide" — visible
// regardless of which surface is open, not just from inside Routines).
describe('Task 14: Part 97 consent moment — chrome wiring', () => {
  beforeEach(() => {
    consentRunsListResult = [];
  });

  it('a run parked awaiting consent at launch badges the Routines menu, names it on the status bar, and shows the modal — with no menu ever opened', async () => {
    consentRunsListResult = [
      {
        runId: CONSENT_TEST_RUN_ID,
        routine: CONSENT_TEST_ROUTINE,
        dryRun: false,
        startedUnix: 1000,
        state: 'awaiting_consent',
        finishedUnix: null,
      },
    ];
    renderShell();

    expect(await screen.findByTestId('menu-badge-routines')).toHaveTextContent('1');
    expect(await screen.findByTestId('status-bar-consent')).toHaveTextContent(CONSENT_TEST_ROUTINE);
    expect(await screen.findByTestId('consent-gate-modal')).toBeInTheDocument();
    // The mailbox master-detail is still the main pane underneath — the
    // consent modal overlays it, it does not replace it (unlike the
    // Routines surface, which is a menu-driven pane swap).
    expect(screen.getByTestId('folder-sidebar')).toBeInTheDocument();
  });

  it('with nothing parked, the badge and status-bar consent item are absent', async () => {
    renderShell();
    await screen.findByTestId('folder-sidebar');
    expect(screen.queryByTestId('menu-badge-routines')).not.toBeInTheDocument();
    expect(screen.queryByTestId('status-bar-consent')).not.toBeInTheDocument();
    expect(screen.queryByTestId('consent-gate-modal')).not.toBeInTheDocument();
  });
});
