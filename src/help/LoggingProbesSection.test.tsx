/**
 * Tests for LoggingProbesSection (and useEnvProbes).
 *
 * Covers:
 *   - Renders heading
 *   - Empty state before snapshots arrive
 *   - Renders snapshot list after initial fetch
 *   - "Re-run probes" button invokes logging_env_probes_rerun and updates list
 *   - Push event (logging://probes/snapshot-updated) updates the rendered list
 *
 * tuxlink-qjgx alpha-logging plan Task 7.6.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import React from 'react';
import { LoggingProbesSection } from './LoggingProbesSection';

// --- Mocks ----------------------------------------------------------------

// We need the listen fn to be capturable so we can emit synthetic events.
type ListenFn = (event: string, handler: (e: { payload: unknown }) => void) => Promise<() => void>;
const listenerRegistry = new Map<string, ((e: { payload: unknown }) => void)[]>();

const { mockInvoke, mockListen } = vi.hoisted(() => ({
  mockInvoke: vi.fn(),
  mockListen: vi.fn<Parameters<ListenFn>, ReturnType<ListenFn>>(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: mockListen,
}));

// Helper: emit a synthetic Tauri event to all registered handlers.
function emitEvent(event: string, payload: unknown) {
  const handlers = listenerRegistry.get(event) ?? [];
  for (const h of handlers) h({ payload });
}

// --- Sample data ----------------------------------------------------------

const SNAPSHOT_V1 = [
  { probe: 'audio', timestamp: '2026-06-05T00:00:00Z', trigger: 'first_paint', result: { backend: 'pipewire', sinks_count: 2, digirig_detected: true } },
  { probe: 'serial', timestamp: '2026-06-05T00:00:00Z', trigger: 'first_paint', result: { by_id_devices: 3, in_dialout_group: true } },
];

const SNAPSHOT_V2 = [
  { probe: 'audio', timestamp: '2026-06-05T00:01:00Z', trigger: 'manual', result: { backend: 'pipewire', sinks_count: 1, digirig_detected: false } },
];

// --- Setup ----------------------------------------------------------------

beforeEach(() => {
  vi.resetAllMocks();
  listenerRegistry.clear();

  mockListen.mockImplementation((event: string, handler: (e: { payload: unknown }) => void) => {
    const existing = listenerRegistry.get(event) ?? [];
    listenerRegistry.set(event, [...existing, handler]);
    return Promise.resolve(() => {
      const current = listenerRegistry.get(event) ?? [];
      listenerRegistry.set(event, current.filter((h) => h !== handler));
    });
  });

  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === 'logging_env_probes_snapshot') return Promise.resolve(SNAPSHOT_V1);
    if (cmd === 'logging_env_probes_rerun') return Promise.resolve(SNAPSHOT_V2);
    return Promise.resolve(null);
  });
});

// --- Render helper --------------------------------------------------------

function renderProbes() {
  return render(React.createElement(LoggingProbesSection));
}

// --- Tests ----------------------------------------------------------------

describe('LoggingProbesSection — rendering', () => {
  it('renders the Environment probes heading', () => {
    renderProbes();
    expect(screen.getByRole('heading', { name: /environment probes/i })).toBeInTheDocument();
  });

  it('renders Re-run probes button', () => {
    renderProbes();
    expect(screen.getByRole('button', { name: /re-run probes/i })).toBeInTheDocument();
  });
});

describe('LoggingProbesSection — initial snapshot', () => {
  it('renders empty state before probes arrive', () => {
    mockInvoke.mockImplementation(() => new Promise(() => { /* never resolves */ }));
    renderProbes();
    expect(screen.getByText(/no probe results yet/i)).toBeInTheDocument();
  });

  it('renders probe list after initial fetch resolves', async () => {
    renderProbes();
    await waitFor(() => {
      expect(screen.getByText(/audio:/i)).toBeInTheDocument();
    });
    expect(screen.getByText(/serial:/i)).toBeInTheDocument();
  });

  it('renders summarized probe result', async () => {
    renderProbes();
    await waitFor(() => {
      // Should show key=value pairs from the audio probe
      const codeEl = screen.getAllByText(/backend=/)[0];
      expect(codeEl).toBeInTheDocument();
    });
  });
});

describe('LoggingProbesSection — Re-run button', () => {
  it('clicking Re-run invokes logging_env_probes_rerun and updates list', async () => {
    renderProbes();
    // Wait for initial load
    await waitFor(() => expect(screen.getByText(/audio:/i)).toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: /re-run probes/i }));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('logging_env_probes_rerun');
    });
    // SNAPSHOT_V2 has only audio; after rerun the list should still show audio
    await waitFor(() => {
      expect(screen.getByText(/audio:/i)).toBeInTheDocument();
    });
    // serial should no longer be in the list (SNAPSHOT_V2 doesn't have it)
    expect(screen.queryByText(/serial:/i)).not.toBeInTheDocument();
  });
});

describe('LoggingProbesSection — push subscription', () => {
  it('push event updates the rendered list', async () => {
    renderProbes();
    // Wait for initial render
    await waitFor(() => expect(screen.getByText(/audio:/i)).toBeInTheDocument());

    // Emit a push event with different data
    emitEvent('logging://probes/snapshot-updated', SNAPSHOT_V2);

    // After push, serial should be gone
    await waitFor(() => {
      expect(screen.queryByText(/serial:/i)).not.toBeInTheDocument();
    });
    // audio still present
    expect(screen.getByText(/audio:/i)).toBeInTheDocument();
  });
});
