import { describe, it, expect } from 'vitest';
import { toSessionLogEntry, toSessionLogEntries } from './useSessionLog';
import type { LogLineDto } from '../../session/logProjection';

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
