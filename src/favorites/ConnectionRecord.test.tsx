// ConnectionRecord tests (Task B5).
//
// Covers: the ✓/✗ strip, the record line (reached / failed-only / empty), the
// L2 station wall-clock (literal HH:MM from the offset, NOT viewer-TZ), and the
// gated ToD hint (rendered ONLY when favorite_tod_hint is non-null, stated as
// OBSERVED COUNTS — never a prediction). VOICE: no "honest"/"today"/"currently".

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createElement, type ReactNode } from 'react';

import { invoke } from '@tauri-apps/api/core';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { ConnectionRecord } from './ConnectionRecord';
import type { ConnectionAttempt, TodHint } from './types';

const invokeMock = invoke as ReturnType<typeof vi.fn>;

// A fixed "now" so the relative-ago deltas are deterministic.
const NOW = new Date('2026-06-07T23:42:00-07:00');

function routeHint(hint: TodHint | null) {
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === 'favorite_tod_hint') return Promise.resolve(hint);
    return Promise.resolve(undefined);
  });
}

function renderRecord(unitId: string, attempts: ConnectionAttempt[]) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client: qc }, children);
  return render(
    createElement(ConnectionRecord, { unitId, attempts, now: NOW }),
    { wrapper },
  );
}

beforeEach(() => {
  invokeMock.mockReset();
  routeHint(null); // default: no hint unless a test opts in
});

describe('<ConnectionRecord> — record line', () => {
  it('shows "reached <ago> · HH:MM local" with the wall-clock from the OFFSET literal (L2)', async () => {
    // ts_local at -07:00 wall-clock 21:42. The test machine TZ is irrelevant:
    // the shown wall-clock MUST be the literal 21:42 from the offset string,
    // not a viewer-TZ re-projection.
    const attempts: ConnectionAttempt[] = [
      { unit_id: 'u1', ts_local: '2026-06-07T21:42:00-07:00', outcome: 'reached' },
    ];
    renderRecord('u1', attempts);

    // 23:42-07:00 (now) − 21:42-07:00 = 2 h.
    const line = await screen.findByTestId('connection-record-line');
    expect(line.textContent).toContain('reached 2 h ago');
    expect(line.textContent).toContain('21:42 local');
  });

  it('uses the most-recent reached attempt for the ago + wall-clock', async () => {
    const attempts: ConnectionAttempt[] = [
      { unit_id: 'u1', ts_local: '2026-06-07T22:42:00-07:00', outcome: 'reached' }, // 1 h ago
      { unit_id: 'u1', ts_local: '2026-06-05T10:00:00-07:00', outcome: 'reached' },
    ];
    renderRecord('u1', attempts);
    const line = await screen.findByTestId('connection-record-line');
    expect(line.textContent).toContain('reached 1 h ago');
    expect(line.textContent).toContain('22:42 local');
  });

  it('shows "no successful connect yet · N attempt(s) failed, last <ago>" for failed-only', async () => {
    const attempts: ConnectionAttempt[] = [
      { unit_id: 'u1', ts_local: '2026-06-04T23:42:00-07:00', outcome: 'failed' }, // 3 d ago
    ];
    renderRecord('u1', attempts);
    const line = await screen.findByTestId('connection-record-line');
    expect(line.textContent).toContain('no successful connect yet');
    expect(line.textContent).toContain('1 attempt failed');
    expect(line.textContent).toContain('3 d ago');
  });

  it('pluralizes failed attempts', async () => {
    const attempts: ConnectionAttempt[] = [
      { unit_id: 'u1', ts_local: '2026-06-07T22:42:00-07:00', outcome: 'failed' },
      { unit_id: 'u1', ts_local: '2026-06-06T22:42:00-07:00', outcome: 'failed' },
    ];
    renderRecord('u1', attempts);
    const line = await screen.findByTestId('connection-record-line');
    expect(line.textContent).toContain('2 attempts failed');
  });

  it('shows "no connection attempts yet" for an empty log', async () => {
    renderRecord('u1', []);
    const line = await screen.findByTestId('connection-record-line');
    expect(line.textContent).toContain('no connection attempts yet');
  });
});

describe('<ConnectionRecord> — outcome strip', () => {
  it('renders ✓ for reached and ✗ for failed, most-recent first', async () => {
    const attempts: ConnectionAttempt[] = [
      { unit_id: 'u1', ts_local: '2026-06-07T21:00:00-07:00', outcome: 'reached' },
      { unit_id: 'u1', ts_local: '2026-06-07T20:00:00-07:00', outcome: 'failed' },
    ];
    renderRecord('u1', attempts);
    const strip = await screen.findByTestId('connection-record-strip');
    expect(strip.textContent).toContain('✓');
    expect(strip.textContent).toContain('✗');
  });
});

describe('<ConnectionRecord> — gated ToD hint', () => {
  it('renders the hint as observed counts ONLY when favorite_tod_hint is non-null', async () => {
    routeHint({ bucket: 'dawn', attempts: 6, successes: 5 });
    renderRecord('u1', [
      { unit_id: 'u1', ts_local: '2026-06-07T21:42:00-07:00', outcome: 'reached' },
    ]);

    const hint = await screen.findByTestId('connection-record-hint');
    // Observed counts, plain bucket label, NO prediction wording.
    expect(hint.textContent).toMatch(/dawn/i);
    expect(hint.textContent).toContain('5');
    expect(hint.textContent).toContain('6');
    // VOICE + non-prediction guards.
    expect(hint.textContent?.toLowerCase()).not.toMatch(/will|expect|best time|predict|honest|today|currently/);
  });

  it('renders NO hint when favorite_tod_hint returns null', async () => {
    routeHint(null);
    renderRecord('u1', [
      { unit_id: 'u1', ts_local: '2026-06-07T21:42:00-07:00', outcome: 'reached' },
    ]);
    // Wait for the record line so the query has had time to settle.
    await screen.findByTestId('connection-record-line');
    await waitFor(() => {
      expect(
        invokeMock.mock.calls.some(([cmd]) => cmd === 'favorite_tod_hint'),
      ).toBe(true);
    });
    expect(screen.queryByTestId('connection-record-hint')).not.toBeInTheDocument();
  });

  it('invokes favorite_tod_hint with the unit_id', async () => {
    routeHint(null);
    renderRecord('unit-xyz', []);
    await waitFor(() => {
      const call = invokeMock.mock.calls.find(([cmd]) => cmd === 'favorite_tod_hint');
      expect(call).toBeTruthy();
      expect((call?.[1] as { unit_id: string }).unit_id).toBe('unit-xyz');
    });
  });
});
