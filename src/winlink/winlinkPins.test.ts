import { describe, it, expect } from 'vitest';
import { toWinlinkPins } from './winlinkPins';

const NOW = Date.parse('2026-06-22T12:00:00-07:00');

// Helper to build a RecentGatewayPin with defaults
const row = (o: object) => ({
  gateway: 'W6DRZ',
  grid: 'CM97',
  last_attempt_at: '2026-06-22T11:30:00-07:00',
  outcome: 'reached' as const,
  ...o,
});

// --- Brief-specified tests ---

it('drops grid-less gateways', () => {
  const pins = toWinlinkPins([row({ grid: undefined })], { livePeer: null, nowMs: NOW });
  expect(pins).toEqual([]);
});

it('marks the live peer regardless of outcome/age', () => {
  const pins = toWinlinkPins([row({ outcome: 'failed' })], { livePeer: 'w6drz-1', nowMs: NOW });
  expect(pins[0].tierClass).toBe('winlink-pin--live');
});

it('reached within 1h is --reached; failed is --failed', () => {
  expect(toWinlinkPins([row({})], { livePeer: null, nowMs: NOW })[0].tierClass).toBe('winlink-pin--reached');
  expect(toWinlinkPins([row({ outcome: 'failed' })], { livePeer: null, nowMs: NOW })[0].tierClass).toBe('winlink-pin--failed');
});

// --- Added tests (all 4 tier branches + both drop reasons) ---

describe('stale case — reached but older than 1h', () => {
  it('emits --stale for reached rows older than 1 hour', () => {
    // 90 minutes ago
    const oldTs = new Date(NOW - 90 * 60 * 1000).toISOString();
    const pins = toWinlinkPins([row({ last_attempt_at: oldTs })], { livePeer: null, nowMs: NOW });
    expect(pins).toHaveLength(1);
    expect(pins[0].tierClass).toBe('winlink-pin--stale');
  });

  it('treats a row exactly at the 1h boundary as --reached (inclusive)', () => {
    const boundaryTs = new Date(NOW - 3_600_000).toISOString();
    const pins = toWinlinkPins([row({ last_attempt_at: boundaryTs })], { livePeer: null, nowMs: NOW });
    expect(pins[0].tierClass).toBe('winlink-pin--reached');
  });
});

describe('null-grid-from-gridToLatLon — malformed grid string drop', () => {
  it('drops rows whose grid string is too short to parse', () => {
    // "CM" is only 2 chars — gridToLatLon returns null
    const pins = toWinlinkPins([row({ grid: 'CM' })], { livePeer: null, nowMs: NOW });
    expect(pins).toEqual([]);
  });

  it('drops rows whose grid string has invalid field characters', () => {
    // "ZZ99" — field chars Z are out of A-R range → gridToLatLon returns null
    const pins = toWinlinkPins([row({ grid: 'ZZ99' })], { livePeer: null, nowMs: NOW });
    expect(pins).toEqual([]);
  });
});

describe('isLive flag and SSID stripping', () => {
  it('sets isLive=true for the live peer', () => {
    const pins = toWinlinkPins([row({})], { livePeer: 'W6DRZ-10', nowMs: NOW });
    expect(pins[0].isLive).toBe(true);
  });

  it('sets isLive=false for other gateways', () => {
    const pins = toWinlinkPins([row({ gateway: 'K6ABC' })], { livePeer: 'W6DRZ-1', nowMs: NOW });
    expect(pins[0].isLive).toBe(false);
  });

  it('matches live peer case-insensitively', () => {
    const pins = toWinlinkPins([row({ gateway: 'W6DRZ' })], { livePeer: 'w6drz', nowMs: NOW });
    expect(pins[0].isLive).toBe(true);
    expect(pins[0].tierClass).toBe('winlink-pin--live');
  });

  it('returns isLive=false for all gateways when livePeer is null', () => {
    const pins = toWinlinkPins([row({})], { livePeer: null, nowMs: NOW });
    expect(pins[0].isLive).toBe(false);
  });
});

describe('lat/lon mapping', () => {
  it('maps grid to lat/lon via gridToLatLon', () => {
    // CM97 grid center: lon = ('C'-'A'=2)*20 - 180 + 9*2 + 1 = 40-180+18+1 = -121
    //                   lat = ('M'-'A'=12)*10 - 90 + 7*1 + 0.5 = 120-90+7+0.5 = 37.5
    const pins = toWinlinkPins([row({ grid: 'CM97' })], { livePeer: null, nowMs: NOW });
    expect(pins[0].lat).toBe(37.5);
    expect(pins[0].lon).toBe(-121);
  });
});
