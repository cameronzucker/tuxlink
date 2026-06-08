// src/connections/useInboundSelection.test.tsx
//
// (b) Production-mount test: the panel is event-driven and mounted in the REAL
//     App provider stack. A mock `inbound_proposals_offered` b2f-event drives
//     the panel into view; clicking the footer invokes
//     `cms_resolve_inbound_selection` with the EXACT wire-contract args.
// (c) Hook stale-filter test: an event with a lower attempt_id than one already
//     seen does not replace the active prompt (and logs a console.warn).

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { renderHook } from '@testing-library/react';
import type { PendingProposalDto } from './sessionTypes';

// --- Tauri IPC mocks (modeled on App.test.tsx) ------------------------------
// invoke is routed by command so AppShell's ribbon/mailbox/status queries get
// shape-correct values; the listen mock captures the b2f-event handler so the
// test can dispatch a synthetic event into the production tree.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

let b2fHandler: ((e: { payload: unknown }) => void) | null = null;
const listenMock = vi.fn(
  async (event: string, cb: (e: { payload: unknown }) => void): Promise<() => void> => {
    if (event === 'b2f-event') b2fHandler = cb;
    return () => {
      if (event === 'b2f-event') b2fHandler = null;
    };
  },
);
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) =>
    (listenMock as (...a: unknown[]) => Promise<() => void>)(...args),
}));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    label: 'main',
    onCloseRequested: vi.fn(async () => () => {}),
    close: vi.fn(async () => {}),
    setTitle: vi.fn(async () => {}),
  }),
}));
// react-virtuoso renders nothing under jsdom; stub so MessageList mounts.
vi.mock('react-virtuoso', () => ({ Virtuoso: () => <div data-testid="virtuoso-mock" /> }));
// Keep the wizard/compose lazy chunks synchronous + cheap.
vi.mock('../wizard/Wizard', () => ({
  Wizard: () => <div data-testid="wizard-root" />,
}));
vi.mock('../compose/Compose', () => ({
  Compose: () => <div data-testid="compose-root" />,
}));

import { invoke } from '@tauri-apps/api/core';
const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

function setPath(pathname: string) {
  Object.defineProperty(window, 'location', {
    configurable: true,
    value: { pathname },
  });
}

const PROPOSALS: PendingProposalDto[] = [
  { mid: 'MID-AAA', uncompressed_size: 2048, compressed_size: 1024 },
  { mid: 'MID-BBB', uncompressed_size: 4096, compressed_size: 2048 },
];

import App from '../App';

// ---------------------------------------------------------------------------
// (b) Production-mount
// ---------------------------------------------------------------------------

describe('InboundSelectionPanel — production mount (tuxlink-bsiy)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    b2fHandler = null;
    setPath('/');
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'get_wizard_completed') return Promise.resolve(true);
      if (cmd === 'mailbox_list') return Promise.resolve([]);
      if (cmd === 'config_read') return Promise.resolve(null);
      if (cmd === 'backend_status') return Promise.resolve(null);
      if (cmd === 'session_log_snapshot') return Promise.resolve([]);
      // Shape-correct values for the shell's background queries so react-query
      // doesn't warn about undefined query data while the panel test runs.
      if (cmd === 'saved_searches_list') return Promise.resolve([]);
      if (cmd === 'recent_searches_list') return Promise.resolve([]);
      if (cmd === 'position_status') return Promise.resolve(null);
      if (cmd === 'user_folders_list') return Promise.resolve([]);
      return Promise.resolve(null);
    });
  });
  afterEach(() => setPath('/'));

  it('renders the panel on an inbound_proposals_offered event and resolves with the exact wire args', async () => {
    render(<App />);
    // Wait for the production shell to mount (post-wizard) and the hook to
    // register its b2f-event listener.
    await screen.findByTestId('app-shell-root');
    await waitFor(() => expect(b2fHandler).not.toBeNull());

    // Fire a synthetic backend event through the SAME channel the hook listens
    // on. attempt_id + request_id are the correlation keys the resolve command
    // echoes back.
    act(() => {
      b2fHandler!({
        payload: {
          kind: 'inbound_proposals_offered',
          request_id: 1,
          attempt_id: 1,
          proposals: PROPOSALS,
        },
      });
    });

    // The lazy panel mounts; wait for the footer button.
    const submit = await screen.findByRole('button', { name: /download 2 checked/i });
    fireEvent.click(submit);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('cms_resolve_inbound_selection', {
        attemptId: 1,
        requestId: 1,
        selection: {
          selected_mids: expect.arrayContaining(['MID-AAA', 'MID-BBB']),
          disposition: 'hold',
        },
      });
    });
    // Exactly the two mids, nothing extra.
    const call = invokeMock.mock.calls.find(
      (c) => c[0] === 'cms_resolve_inbound_selection',
    );
    expect(call?.[1].selection.selected_mids).toHaveLength(2);
  });
});

// ---------------------------------------------------------------------------
// (c) Hook stale-filter
// ---------------------------------------------------------------------------

// Import the hook directly for the unit-level stale-filter test. The Tauri
// modules are already mocked at the top of this file, so the hook's
// listen()/invoke() resolve against the captured handler.
import { useInboundSelection } from './useInboundSelection';

describe('useInboundSelection — AttemptId stale filter (tuxlink-bsiy)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    b2fHandler = null;
    invokeMock.mockResolvedValue(undefined);
  });

  it('drops an event with a lower attempt_id than one already seen and warns', async () => {
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
    try {
      const { result } = renderHook(() => useInboundSelection());
      await waitFor(() => expect(b2fHandler).not.toBeNull());

      // First (newer) prompt.
      act(() => {
        b2fHandler!({
          payload: {
            kind: 'inbound_proposals_offered',
            request_id: 9,
            attempt_id: 5,
            proposals: PROPOSALS,
          },
        });
      });
      expect(result.current.prompt).not.toBeNull();
      expect(result.current.prompt?.attemptId).toBe(5);

      // Stale prompt — lower attempt_id. Must be dropped, not replace.
      act(() => {
        b2fHandler!({
          payload: {
            kind: 'inbound_proposals_offered',
            request_id: 2,
            attempt_id: 2,
            proposals: [{ mid: 'STALE', uncompressed_size: 1, compressed_size: 1 }],
          },
        });
      });

      // The active prompt is unchanged (still attempt 5 / request 9).
      expect(result.current.prompt?.attemptId).toBe(5);
      expect(result.current.prompt?.requestId).toBe(9);
      expect(warnSpy).toHaveBeenCalled();
    } finally {
      warnSpy.mockRestore();
    }
  });

  it('close() clears the active prompt locally', async () => {
    const { result } = renderHook(() => useInboundSelection());
    await waitFor(() => expect(b2fHandler).not.toBeNull());

    act(() => {
      b2fHandler!({
        payload: {
          kind: 'inbound_proposals_offered',
          request_id: 1,
          attempt_id: 1,
          proposals: PROPOSALS,
        },
      });
    });
    expect(result.current.prompt).not.toBeNull();

    act(() => {
      result.current.close();
    });
    expect(result.current.prompt).toBeNull();
  });

  it('submit() invokes cms_resolve_inbound_selection then clears the prompt', async () => {
    const { result } = renderHook(() => useInboundSelection());
    await waitFor(() => expect(b2fHandler).not.toBeNull());

    act(() => {
      b2fHandler!({
        payload: {
          kind: 'inbound_proposals_offered',
          request_id: 7,
          attempt_id: 3,
          proposals: PROPOSALS,
        },
      });
    });

    await act(async () => {
      await result.current.submit({ selected_mids: ['MID-AAA'], disposition: 'delete' });
    });

    expect(invokeMock).toHaveBeenCalledWith('cms_resolve_inbound_selection', {
      attemptId: 3,
      requestId: 7,
      selection: { selected_mids: ['MID-AAA'], disposition: 'delete' },
    });
    expect(result.current.prompt).toBeNull();
  });
});
