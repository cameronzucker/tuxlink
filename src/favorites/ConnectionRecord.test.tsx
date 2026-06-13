// ConnectionRecord tests (Task B5; props-lifted for tuxlink-je5d).
//
// Covers: the ✓/✗ strip, the record line (reached / failed-only / empty), the
// L2 station wall-clock (literal HH:MM from the offset, NOT viewer-TZ), and the
// gated ToD hint (rendered ONLY when a non-null `hint` PROP is passed, stated as
// OBSERVED COUNTS — never a prediction). VOICE: no "honest"/"today"/"currently".
//
// Post-lift, ConnectionRecord no longer fetches `favorite_tod_hint` itself — its
// caller passes `hint` as a prop. So these tests render with explicit props and
// need no QueryClient or invoke mock. The query-out-of-the-component contract is
// covered at the caller level (FavoritesTabs.test.tsx routes favorite_tod_hint).

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { createElement } from 'react';

import { ConnectionRecord } from './ConnectionRecord';
import type { ConnectionAttempt, TodHint } from './types';

// A fixed "now" so the relative-ago deltas are deterministic.
const NOW = new Date('2026-06-07T23:42:00-07:00');

function renderRecord(attempts: ConnectionAttempt[], hint: TodHint | null = null) {
  return render(createElement(ConnectionRecord, { attempts, hint, now: NOW }));
}

describe('<ConnectionRecord> — record line', () => {
  it('shows "reached <ago> · HH:MM local" with the wall-clock from the OFFSET literal (L2)', () => {
    // ts_local at -07:00 wall-clock 21:42. The test machine TZ is irrelevant:
    // the shown wall-clock MUST be the literal 21:42 from the offset string,
    // not a viewer-TZ re-projection.
    const attempts: ConnectionAttempt[] = [
      { unit_id: 'u1', ts_local: '2026-06-07T21:42:00-07:00', outcome: 'reached' },
    ];
    renderRecord(attempts);

    // 23:42-07:00 (now) − 21:42-07:00 = 2 h.
    const line = screen.getByTestId('connection-record-line');
    expect(line.textContent).toContain('reached 2 h ago');
    expect(line.textContent).toContain('21:42 local');
  });

  it('uses the most-recent reached attempt for the ago + wall-clock', () => {
    const attempts: ConnectionAttempt[] = [
      { unit_id: 'u1', ts_local: '2026-06-07T22:42:00-07:00', outcome: 'reached' }, // 1 h ago
      { unit_id: 'u1', ts_local: '2026-06-05T10:00:00-07:00', outcome: 'reached' },
    ];
    renderRecord(attempts);
    const line = screen.getByTestId('connection-record-line');
    expect(line.textContent).toContain('reached 1 h ago');
    expect(line.textContent).toContain('22:42 local');
  });

  it('shows "no successful connect yet · N attempt(s) failed, last <ago>" for failed-only', () => {
    const attempts: ConnectionAttempt[] = [
      { unit_id: 'u1', ts_local: '2026-06-04T23:42:00-07:00', outcome: 'failed' }, // 3 d ago
    ];
    renderRecord(attempts);
    const line = screen.getByTestId('connection-record-line');
    expect(line.textContent).toContain('no successful connect yet');
    expect(line.textContent).toContain('1 attempt failed');
    expect(line.textContent).toContain('3 d ago');
  });

  it('pluralizes failed attempts', () => {
    const attempts: ConnectionAttempt[] = [
      { unit_id: 'u1', ts_local: '2026-06-07T22:42:00-07:00', outcome: 'failed' },
      { unit_id: 'u1', ts_local: '2026-06-06T22:42:00-07:00', outcome: 'failed' },
    ];
    renderRecord(attempts);
    const line = screen.getByTestId('connection-record-line');
    expect(line.textContent).toContain('2 attempts failed');
  });

  it('shows "no connection attempts yet" for an empty log', () => {
    renderRecord([]);
    const line = screen.getByTestId('connection-record-line');
    expect(line.textContent).toContain('no connection attempts yet');
  });
});

describe('<ConnectionRecord> — outcome strip', () => {
  it('renders ✓ for reached and ✗ for failed, most-recent first', () => {
    const attempts: ConnectionAttempt[] = [
      { unit_id: 'u1', ts_local: '2026-06-07T21:00:00-07:00', outcome: 'reached' },
      { unit_id: 'u1', ts_local: '2026-06-07T20:00:00-07:00', outcome: 'failed' },
    ];
    renderRecord(attempts);
    const strip = screen.getByTestId('connection-record-strip');
    expect(strip.textContent).toContain('✓');
    expect(strip.textContent).toContain('✗');
  });

  it('renders NO strip when there are no attempts', () => {
    renderRecord([]);
    expect(screen.queryByTestId('connection-record-strip')).not.toBeInTheDocument();
  });
});

describe('<ConnectionRecord> — gated ToD hint', () => {
  it('renders the hint as observed counts ONLY when a non-null hint prop is passed', () => {
    renderRecord(
      [{ unit_id: 'u1', ts_local: '2026-06-07T21:42:00-07:00', outcome: 'reached' }],
      { bucket: 'dawn', attempts: 6, successes: 5 },
    );

    const hint = screen.getByTestId('connection-record-hint');
    // Observed counts, plain bucket label, NO prediction wording.
    expect(hint.textContent).toMatch(/dawn/i);
    expect(hint.textContent).toContain('5');
    expect(hint.textContent).toContain('6');
    // VOICE + non-prediction guards.
    expect(hint.textContent?.toLowerCase()).not.toMatch(/will|expect|best time|predict|honest|today|currently/);
  });

  it('renders NO hint when the hint prop is null', () => {
    renderRecord(
      [{ unit_id: 'u1', ts_local: '2026-06-07T21:42:00-07:00', outcome: 'reached' }],
      null,
    );
    expect(screen.queryByTestId('connection-record-hint')).not.toBeInTheDocument();
  });
});
