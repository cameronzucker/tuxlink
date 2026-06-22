import { it, expect } from 'vitest';
import { linkDrawState } from './winlinkLinkAnim';

const base = {
  state: 'idle' as const,
  arqFlags: { busy: false, rx: false, tx: false },
  throughputBps: null,
  quality: null,
  snDb: null,
};

// --- Brief-prescribed tests ---

it('idle → inactive', () => expect(linkDrawState(base).active).toBe(false));

it('connecting → connecting+active', () => {
  const d = linkDrawState({ ...base, state: 'connecting' });
  expect(d.phase).toBe('connecting');
  expect(d.active).toBe(true);
});

it('connected-iss with throughput → data-out with flow', () => {
  const d = linkDrawState({ ...base, state: 'connected-iss', throughputBps: 2000 });
  expect(d.phase).toBe('data-out');
  expect(d.flow).toBeCloseTo(0.5, 1);
});

it('busy overrides data direction', () => {
  const d = linkDrawState({
    ...base,
    state: 'connected-iss',
    arqFlags: { busy: true, rx: false, tx: false },
  });
  expect(d.phase).toBe('busy');
});

it('error → error phase, still active (for the flash)', () => {
  const d = linkDrawState({ ...base, state: 'error' });
  expect(d.phase).toBe('error');
  expect(d.active).toBe(true);
});

// --- Additional tests ---

it('connected-irs → data-in', () => {
  const d = linkDrawState({ ...base, state: 'connected-irs', throughputBps: 1000 });
  expect(d.phase).toBe('data-in');
  expect(d.active).toBe(true);
  expect(d.flow).toBeCloseTo(0.25, 2);
});

it('disconnecting → closing', () => {
  const d = linkDrawState({ ...base, state: 'disconnecting' });
  expect(d.phase).toBe('closing');
  expect(d.active).toBe(true);
});

it('quality passthrough: quality=80 → 0.8', () => {
  const d = linkDrawState({ ...base, state: 'idle', quality: 80 });
  expect(d.quality).toBeCloseTo(0.8, 5);
});

it('snDb fallback when quality null: snDb=20 → clamp01((20+10)/30)=1.0', () => {
  const d = linkDrawState({ ...base, snDb: 20, quality: null });
  expect(d.quality).toBeCloseTo(1.0, 5);
});

it('snDb fallback mid-range: snDb=5 → clamp01((5+10)/30)=0.5', () => {
  const d = linkDrawState({ ...base, snDb: 5, quality: null });
  expect(d.quality).toBeCloseTo(0.5, 2);
});

it('default 0.6 when both quality and snDb are null', () => {
  const d = linkDrawState({ ...base, quality: null, snDb: null });
  expect(d.quality).toBeCloseTo(0.6, 5);
});

it('stopped → idle (inactive)', () => {
  const d = linkDrawState({ ...base, state: 'stopped' });
  expect(d.phase).toBe('idle');
  expect(d.active).toBe(false);
});

it('spawning → idle (inactive)', () => {
  const d = linkDrawState({ ...base, state: 'spawning' });
  expect(d.phase).toBe('idle');
  expect(d.active).toBe(false);
});

it('initializing → idle (inactive)', () => {
  const d = linkDrawState({ ...base, state: 'initializing' });
  expect(d.phase).toBe('idle');
  expect(d.active).toBe(false);
});

it('busy on connected-irs also resolves to busy', () => {
  const d = linkDrawState({
    ...base,
    state: 'connected-irs',
    arqFlags: { busy: true, rx: false, tx: false },
  });
  expect(d.phase).toBe('busy');
});

it('flow is 0 in non-data phases (connecting)', () => {
  const d = linkDrawState({ ...base, state: 'connecting', throughputBps: 4000 });
  expect(d.flow).toBe(0);
});

it('flow clamped to 1 for very high throughput', () => {
  const d = linkDrawState({ ...base, state: 'connected-iss', throughputBps: 99999 });
  expect(d.flow).toBe(1);
});
