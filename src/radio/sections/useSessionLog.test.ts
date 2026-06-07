import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import {
  toSessionLogEntry,
  toSessionLogEntries,
  mergeLogLine,
  mergeLogLines,
  useSessionLog,
} from './useSessionLog';
import type { LogLineDto } from '../../session/logProjection';

// Tauri IPC mocks — only loaded for the renderHook tests below (the pure
// projection/merge tests above don't need them).
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}));

const at = (ts: string, level: LogLineDto['level'], source: LogLineDto['source'], message: string, seq = 1): LogLineDto => ({
  seq,
  timestampIso: ts,
  level,
  source,
  message,
});

describe('toSessionLogEntry', () => {
  it('extracts HH:MM:SS from an ISO timestamp', () => {
    const entry = toSessionLogEntry(at('2026-05-31T19:35:58.123Z', 'info', 'backend', 'hi'));
    expect(entry.ts).toBe('19:35:58');
  });

  it('maps level=error to "alert" (⊘ glyph in the section)', () => {
    expect(toSessionLogEntry(at('2026-05-31T00:00:00Z', 'error', 'backend', 'crash')).level).toBe('alert');
  });

  it('maps level=warn to "warn" (⚠ glyph)', () => {
    expect(toSessionLogEntry(at('2026-05-31T00:00:00Z', 'warn', 'backend', 'odd')).level).toBe('warn');
  });

  it('maps source=wire to "raw" (B2F protocol noise; hidden until Show raw)', () => {
    expect(toSessionLogEntry(at('2026-05-31T00:00:00Z', 'info', 'wire', ';PQ')).level).toBe('raw');
  });

  it('maps level=trace and level=debug to "raw"', () => {
    expect(toSessionLogEntry(at('2026-05-31T00:00:00Z', 'trace', 'backend', 't')).level).toBe('raw');
    expect(toSessionLogEntry(at('2026-05-31T00:00:00Z', 'debug', 'backend', 'd')).level).toBe('raw');
  });

  it('promotes *** annotated wire lines to "info" (session boundaries pass the Show raw filter)', () => {
    expect(toSessionLogEntry(at('2026-05-31T00:00:00Z', 'info', 'wire', '*** Session started')).level).toBe('info');
  });

  it('maps level=info on backend/transport to "info"', () => {
    expect(toSessionLogEntry(at('2026-05-31T00:00:00Z', 'info', 'backend', 'hello')).level).toBe('info');
    expect(toSessionLogEntry(at('2026-05-31T00:00:00Z', 'info', 'transport', 'connected')).level).toBe('info');
  });

  it('preserves the message verbatim', () => {
    expect(toSessionLogEntry(at('2026-05-31T00:00:00Z', 'info', 'backend', 'verbatim text')).message).toBe('verbatim text');
  });
});

describe('toSessionLogEntries', () => {
  it('projects an array of LogLineDto onto SessionLogEntry[]', () => {
    const out = toSessionLogEntries([
      at('2026-05-31T19:00:00Z', 'info', 'backend', 'a'),
      at('2026-05-31T19:00:01Z', 'warn', 'transport', 'b'),
      at('2026-05-31T19:00:02Z', 'error', 'backend', 'c'),
    ]);
    expect(out.map((e) => e.level)).toEqual(['info', 'warn', 'alert']);
    expect(out.map((e) => e.ts)).toEqual(['19:00:00', '19:00:01', '19:00:02']);
  });
});

describe('mergeLogLine (Codex R2 — snapshot/listen race)', () => {
  it('appends a new line at the seq-correct position', () => {
    const merged = mergeLogLine(
      [at('t', 'info', 'backend', 'a', 1), at('t', 'info', 'backend', 'c', 3)],
      at('t', 'info', 'backend', 'b', 2),
    );
    expect(merged.map((l) => l.message)).toEqual(['a', 'b', 'c']);
  });

  it('deduplicates by seq (idempotent when the same line arrives twice)', () => {
    const start = [at('t', 'info', 'backend', 'a', 1)];
    const merged = mergeLogLine(start, at('t', 'info', 'backend', 'a-dup', 1));
    expect(merged).toEqual(start); // no change; existing line wins
  });

  it('appends seq=0 synthetic lines unconditionally (no dedup, tail position)', () => {
    const merged = mergeLogLine(
      [at('t', 'info', 'backend', 'a', 1)],
      at('t', 'info', 'backend', '*** Session summary', 0),
    );
    expect(merged.map((l) => l.message)).toEqual(['a', '*** Session summary']);
  });

  it('synthetic seq=0 anchors in its insertion-order position (subsequent real lines slot after it)', () => {
    // Synthetic seq=0 has no canonical time ordering; it's a frontend-only
    // marker (e.g., session-boundary annotation). It stays where it was
    // inserted. A real line arriving later doesn't reorder around it —
    // the synthetic stays anchored at its insertion point, and subsequent
    // lines append after.
    const merged = mergeLogLine(
      [at('t', 'info', 'backend', 'a', 1), at('t', 'info', 'backend', '*** Synthetic', 0)],
      at('t', 'info', 'backend', 'c', 3),
    );
    expect(merged.map((l) => l.message)).toEqual(['a', '*** Synthetic', 'c']);
  });
});

describe('mergeLogLines (snapshot ingestion)', () => {
  it('idempotently merges a snapshot that contains a line already streamed live', () => {
    // Race scenario: a live event with seq=5 arrived first; later the
    // snapshot resolves and includes the same seq=5 line.
    const live = [at('t', 'info', 'backend', 'live-5', 5)];
    const snapshot = [
      at('t', 'info', 'backend', 'snap-3', 3),
      at('t', 'info', 'backend', 'snap-5', 5), // dup of live; should be ignored
      at('t', 'info', 'backend', 'snap-7', 7),
    ];
    const merged = mergeLogLines(live, snapshot);
    expect(merged.map((l) => l.seq)).toEqual([3, 5, 7]);
    // The live-5 message wins (first writer); snap-5 was deduplicated.
    expect(merged.find((l) => l.seq === 5)?.message).toBe('live-5');
  });

  it('produces the same result regardless of arrival order (commutative for real seqs)', () => {
    const a: LogLineDto[] = [
      at('t', 'info', 'backend', 'm3', 3),
      at('t', 'info', 'backend', 'm1', 1),
    ];
    const b: LogLineDto[] = [
      at('t', 'info', 'backend', 'm2', 2),
    ];
    const ab = mergeLogLines(mergeLogLines([], a), b);
    const ba = mergeLogLines(mergeLogLines([], b), a);
    expect(ab.map((l) => l.seq)).toEqual([1, 2, 3]);
    expect(ba.map((l) => l.seq)).toEqual([1, 2, 3]);
  });
});

describe('useSessionLog() retained history', () => {
  beforeEach(async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockReset();
  });

  it('returns the full snapshot so only the rendered panel owns the visible cap', async () => {
    const core = await import('@tauri-apps/api/core');
    const snapshot = Array.from({ length: 501 }, (_, idx) =>
      at('2026-05-31T00:00:00Z', 'info', 'transport', `line ${idx}`, idx + 1),
    );
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'session_log_snapshot') return snapshot;
      return undefined;
    });

    const { result } = renderHook(() => useSessionLog());

    await waitFor(() => {
      expect(result.current.entries).toHaveLength(501);
    });
    expect(result.current.entries[0].message).toBe('line 0');
    expect(result.current.entries[500].message).toBe('line 500');
  });
});

// ---------------------------------------------------------------------------
// useSessionLog.clear() — backend drain + local-state reset (round-2 fix)
// Operator smoke 2026-05-31: prior `clear()` only reset React state, so a
// panel re-mount (mode switch) refetched the snapshot and the "cleared"
// entries reappeared. The fix invokes `session_log_clear` first.
// ---------------------------------------------------------------------------

describe('useSessionLog().clear (operator smoke round-2: backend drain)', () => {
  beforeEach(async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockReset();
    // Default invoke: empty snapshot + a no-op session_log_clear.
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'session_log_snapshot') return [];
      if (cmd === 'session_log_clear') return undefined;
      return undefined;
    });
  });

  it('invokes session_log_clear on the backend when clear() is called', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;

    const { result } = renderHook(() => useSessionLog());
    // Wait for the listen/snapshot effect to settle before triggering clear,
    // so the assertion below isn't polluted by the mount-time invokes.
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('session_log_snapshot');
    });

    act(() => {
      result.current.clear();
    });

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('session_log_clear');
    });
  });

  it('still clears local state when the backend invoke rejects (offline degrade)', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'session_log_snapshot') return [];
      if (cmd === 'session_log_clear') throw new Error('backend offline');
      return undefined;
    });

    const { result } = renderHook(() => useSessionLog());

    // The clear path should not throw even though the backend invoke rejects.
    expect(() => {
      act(() => {
        result.current.clear();
      });
    }).not.toThrow();

    // Local entries stay empty (started empty + clear is best-effort).
    expect(result.current.entries).toEqual([]);
  });
});
