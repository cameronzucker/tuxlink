/**
 * SessionLog.tsx unit tests — Mock B human-shaped session log.
 *
 * Tauri IPC (listen + invoke) is mocked. DEV_FIXTURE is false under vitest
 * (MODE=test), so the component renders the projected IPC lines (not the dev
 * fixture steps). Verifies the strip + Human/Raw toggle + snapshot seeding +
 * live tail + the human projection suppressing raw wire noise.
 *
 * E1 tests (tuxlink-22l adrev #3/#4):
 *   - Two lines with the SAME timestampIso but DIFFERENT seq are BOTH rendered
 *     (the old timestamp-only dedupe wrongly dropped one).
 *   - A live event whose seq was already in the snapshot is NOT duplicated
 *     (dedupe on seq, not timestamp).
 *   - Subscribe-then-snapshot: an event arriving after listen() attach but
 *     before/during snapshot resolve is captured and deduped by seq — the
 *     final rendered set is the union ordered by seq with no duplicates.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act, waitFor } from '@testing-library/react';
import type { LogLineDto } from './logProjection';

type ListenCallback = (event: { payload: LogLineDto }) => void;
let capturedListeners: ListenCallback[] = [];
let mockSnapshotLines: LogLineDto[] = [];

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((_eventName: string, cb: ListenCallback) => {
    capturedListeners.push(cb);
    return Promise.resolve(() => {
      capturedListeners = capturedListeners.filter((l) => l !== cb);
    });
  }),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((_cmd: string) => Promise.resolve(mockSnapshotLines)),
}));

import { SessionLog } from './SessionLog';

const backendLine: LogLineDto = {
  seq: 1,
  timestampIso: '2026-05-19T12:00:00Z',
  level: 'info',
  source: 'backend',
  message: 'Pat process started',
};
const wireLine: LogLineDto = {
  seq: 2,
  timestampIso: '2026-05-19T12:00:01Z',
  level: 'debug',
  source: 'wire',
  message: ';PQ: WL2K AUTH REQUIRED',
};

beforeEach(() => {
  capturedListeners = [];
  mockSnapshotLines = [];
});
afterEach(() => {
  vi.clearAllMocks();
});

describe('SessionLog (Mock B human-shaped)', () => {
  it('renders the strip with a Human/Raw toggle (Human active by default)', async () => {
    await act(async () => {
      render(<SessionLog />);
    });
    expect(screen.getByTestId('session-log-root')).toBeInTheDocument();
    expect(screen.getByTestId('session-log-root').className).toContain('human');
    expect(screen.getByTestId('toggle-human')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByTestId('toggle-raw')).toHaveAttribute('aria-pressed', 'false');
  });

  it('seeds from the snapshot on mount', async () => {
    mockSnapshotLines = [backendLine];
    await act(async () => {
      render(<SessionLog />);
    });
    await waitFor(() =>
      expect(screen.getByTestId('session-log-lines')).toHaveTextContent('Pat process started'),
    );
  });

  it('Human suppresses raw wire lines; Raw shows them', async () => {
    mockSnapshotLines = [backendLine, wireLine];
    await act(async () => {
      render(<SessionLog />);
    });
    await waitFor(() =>
      expect(screen.getByTestId('session-log-lines')).toHaveTextContent('Pat process started'),
    );
    // Human projection drops non-annotated wire noise.
    expect(screen.getByTestId('session-log-lines')).not.toHaveTextContent('WL2K AUTH REQUIRED');

    await act(async () => {
      fireEvent.click(screen.getByTestId('toggle-raw'));
    });
    expect(screen.getByTestId('toggle-raw')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByTestId('session-log-lines')).toHaveTextContent('WL2K AUTH REQUIRED');
  });

  it('appends a line arriving via listen()', async () => {
    await act(async () => {
      render(<SessionLog />);
    });
    await act(async () => {
      capturedListeners.forEach((cb) => cb({ payload: backendLine }));
    });
    expect(screen.getByTestId('session-log-lines')).toHaveTextContent('Pat process started');
  });

  // -------------------------------------------------------------------------
  // E1 — adrev #4: dedupe on seq (not timestampIso)
  // -------------------------------------------------------------------------

  it('renders two lines sharing the same timestampIso when they have different seq', async () => {
    // Two distinct log lines that happened at exactly the same timestamp.
    // The old dedupe (by timestampIso) would drop one. The new dedupe (by seq)
    // must render both.
    const lineA: LogLineDto = {
      seq: 10,
      timestampIso: '2026-05-19T12:00:00Z',
      level: 'info',
      source: 'backend',
      message: 'First simultaneous line',
    };
    const lineB: LogLineDto = {
      seq: 11,
      timestampIso: '2026-05-19T12:00:00Z', // SAME timestamp, different seq
      level: 'info',
      source: 'backend',
      message: 'Second simultaneous line',
    };
    mockSnapshotLines = [lineA, lineB];
    await act(async () => {
      render(<SessionLog />);
    });
    // Switch to Raw so the raw projection shows both backend lines.
    await act(async () => {
      fireEvent.click(screen.getByTestId('toggle-raw'));
    });
    await waitFor(() => {
      expect(screen.getByTestId('session-log-lines')).toHaveTextContent('First simultaneous line');
      expect(screen.getByTestId('session-log-lines')).toHaveTextContent('Second simultaneous line');
    });
  });

  it('does not duplicate a live event whose seq was already in the snapshot', async () => {
    // Snapshot contains seq:1. A live event with seq:1 arrives (overlap window).
    // Dedupe-on-seq must suppress the duplicate; only one occurrence rendered.
    const line: LogLineDto = {
      seq: 1,
      timestampIso: '2026-05-19T12:00:00Z',
      level: 'info',
      source: 'backend',
      message: 'Overlap line',
    };
    mockSnapshotLines = [line];
    await act(async () => {
      render(<SessionLog />);
    });
    // Wait for snapshot to seed
    await waitFor(() =>
      expect(screen.getByTestId('session-log-lines')).toHaveTextContent('Overlap line'),
    );
    // Switch to Raw so we see backend lines
    await act(async () => {
      fireEvent.click(screen.getByTestId('toggle-raw'));
    });
    // Now fire the same line via listen (simulating the overlap window)
    await act(async () => {
      capturedListeners.forEach((cb) => cb({ payload: line }));
    });
    // Should still render exactly one occurrence
    const logLines = screen.getByTestId('session-log-lines');
    const occurrences = (logLines.textContent ?? '').split('Overlap line').length - 1;
    expect(occurrences).toBe(1);
  });

  // -------------------------------------------------------------------------
  // FIX 8 — session-log Map must be capped at SESSION_LOG_CAP (500) entries
  // -------------------------------------------------------------------------

  it('caps the rendered line set at 500 when fed > 500 lines, retaining the newest seqs', async () => {
    // Feed 600 lines (seq 1..600). The Map must evict the 100 oldest (seq 1..100)
    // and retain only the newest 500 (seq 101..600).
    const lines: LogLineDto[] = Array.from({ length: 600 }, (_, i) => ({
      seq: i + 1,
      timestampIso: `2026-05-20T12:00:00Z`,
      level: 'info' as const,
      source: 'backend' as const,
      message: `Log line seq ${i + 1}`,
    }));
    mockSnapshotLines = lines;
    await act(async () => {
      render(<SessionLog />);
    });
    // Switch to Raw so all backend lines are visible
    await act(async () => {
      fireEvent.click(screen.getByTestId('toggle-raw'));
    });
    await waitFor(() =>
      expect(screen.getByTestId('session-log-lines')).toHaveTextContent('Log line seq 600'),
    );
    const logContent = screen.getByTestId('session-log-lines').textContent ?? '';
    // Newest 500 entries (seq 101..600) must be present
    expect(logContent).toContain('Log line seq 600');
    expect(logContent).toContain('Log line seq 101');
    // Oldest 100 (seq 1..100) must be evicted
    expect(logContent).not.toContain('Log line seq 1\n');
    expect(logContent).not.toContain('Log line seq 100');
    // Verify the total rendered line count is capped at 500
    const lineCount = (logContent.match(/Log line seq \d+/g) ?? []).length;
    expect(lineCount).toBe(500);
  });

  it('subscribe-then-snapshot: event arriving before snapshot resolves is captured and merged deduped by seq', async () => {
    // This test simulates the subscribe-then-snapshot window:
    // 1. listen() is attached (captures the callback in capturedListeners).
    // 2. An event with seq:3 arrives via the listener BEFORE snapshot resolves.
    // 3. The snapshot returns [seq:1, seq:2, seq:3] (seq:3 overlaps).
    // 4. Final rendered set must be [seq:1, seq:2, seq:3] — no duplicate for seq:3.
    //
    // Implementation: render first (which triggers listen+invoke), fire the event
    // synchronously before the promises flush, then wait for settle.
    //
    // Because both listen() and invoke() return Promise.resolve (microtask), the
    // component's useEffect runs in order: listen attaches → invoke is called →
    // both settle. We fire the event after listen attaches (capturedListeners has
    // the callback) but the snapshot promise hasn't been consumed yet.

    const snapLine1: LogLineDto = { seq: 1, timestampIso: '2026-05-19T12:00:01Z', level: 'info', source: 'backend', message: 'Snap line one' };
    const snapLine2: LogLineDto = { seq: 2, timestampIso: '2026-05-19T12:00:02Z', level: 'info', source: 'backend', message: 'Snap line two' };
    const snapLine3: LogLineDto = { seq: 3, timestampIso: '2026-05-19T12:00:03Z', level: 'info', source: 'backend', message: 'Window line' };
    // Snapshot includes all three (seq:3 was already received via listen)
    mockSnapshotLines = [snapLine1, snapLine2, snapLine3];

    // Render the component — useEffect runs, listen attaches synchronously
    // (the callback is captured before any await)
    render(<SessionLog />);

    // At this point listen has been called but promises haven't flushed.
    // Fire the event for seq:3 to simulate it arriving in the window.
    capturedListeners.forEach((cb) => cb({ payload: snapLine3 }));

    // Now let all pending promises flush (listen resolve + invoke + snapshot merge)
    await act(async () => {
      // flush microtasks
    });

    // Wait for the final state to settle
    await waitFor(() =>
      expect(screen.getByTestId('session-log-lines')).toHaveTextContent('Snap line one'),
    );

    // Switch to Raw so all backend lines are visible
    await act(async () => {
      fireEvent.click(screen.getByTestId('toggle-raw'));
    });

    const logLines = screen.getByTestId('session-log-lines');
    // All three unique lines must appear
    expect(logLines).toHaveTextContent('Snap line one');
    expect(logLines).toHaveTextContent('Snap line two');
    expect(logLines).toHaveTextContent('Window line');
    // 'Window line' (seq:3) must appear exactly once — not duplicated
    const duplicateCount = (logLines.textContent ?? '').split('Window line').length - 1;
    expect(duplicateCount).toBe(1);
  });
});
