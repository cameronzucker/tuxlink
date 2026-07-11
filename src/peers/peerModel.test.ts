import { describe, it, expect } from 'vitest';
import { aggregatePeers, peerTacChatTier, type AggregatedPeer } from './peerModel';
import type { Channel, Contact, Endpoint } from '../contacts/types';

const contact = (over: Partial<Contact>): Contact => ({
  id: 'c1', name: '', callsign: 'W6ABC-7', tier: 'unconfirmed', origin: 'outgoing',
  channels: [], endpoints: [], created_at: '2026-07-10T12:00:00-07:00',
  updated_at: '2026-07-10T12:00:00-07:00', ...over,
});

const endpoint = (over: Partial<Endpoint> = {}): Endpoint => ({
  id: 'e1', host: 'x.example', port: 8774, provenance: 'operator', last_seen: '', last_ok: null,
  ...over,
});

describe('aggregatePeers', () => {
  it('drops a contact with no channels and no endpoints (address-book-only, not a peer)', () => {
    const out = aggregatePeers([contact({})]);
    expect(out).toHaveLength(0);
  });

  it('keeps gridless contacts with reachability instead of dropping them', () => {
    const out = aggregatePeers([
      contact({ grid: undefined, endpoints: [endpoint()] }),
    ]);
    expect(out).toHaveLength(1);
    expect(out[0].grid).toBeUndefined();
    expect(out[0].mapPlaceable).toBe(false);
  });

  it('keys on the exact callsign — two distinct records both surface (no base merge)', () => {
    const out = aggregatePeers([
      contact({ id: 'a', callsign: 'W6ABC-7', endpoints: [endpoint()] }),
      contact({ id: 'b', callsign: 'W6ABC-9', endpoints: [endpoint({ id: 'e2' })] }),
    ]);
    expect(out.map((p) => p.id).sort()).toEqual(['a', 'b']);
    expect(out.map((p) => p.callsign).sort()).toEqual(['W6ABC-7', 'W6ABC-9']);
  });

  it('marks a gridded contact map-placeable', () => {
    const out = aggregatePeers([
      contact({ grid: { value: 'CN87', source: 'manual' }, endpoints: [endpoint()] }),
    ]);
    expect(out[0].mapPlaceable).toBe(true);
    expect(out[0].grid).toBe('CN87');
  });

  it('carries tier and origin through', () => {
    const out = aggregatePeers([
      contact({ tier: 'confirmed', origin: 'manual', endpoints: [endpoint()] }),
    ]);
    expect(out[0].tier).toBe('confirmed');
    expect(out[0].origin).toBe('manual');
  });

  it('derives lastSeen as the most recent last_seen across channels + endpoints', () => {
    const out = aggregatePeers([
      contact({
        channels: [
          {
            transport: 'vara-hf', target_callsign: 'W6ABC-7', via: [], freq_hz: null,
            bandwidth: null, direction: 'outgoing', counts: { ok: 1, fail: 0 },
            last_seen: '2026-07-01T00:00:00Z', last_ok: null,
          },
        ],
        endpoints: [endpoint({ last_seen: '2026-07-05T00:00:00Z' })],
      }),
    ]);
    expect(out[0].lastSeen).toBe('2026-07-05T00:00:00Z');
  });

  it('lastSeen is null when no channel/endpoint carries a parseable timestamp', () => {
    const out = aggregatePeers([contact({ endpoints: [endpoint({ last_seen: '' })] })]);
    expect(out[0].lastSeen).toBeNull();
  });

  it('derives lastOk (success-only) as the most recent last_ok across channels + endpoints', () => {
    const ch: Channel = {
      transport: 'vara-hf', target_callsign: 'W6ABC-7', via: [], freq_hz: null,
      bandwidth: null, direction: 'outgoing', counts: { ok: 1, fail: 3 },
      last_seen: '2026-07-09T00:00:00Z', last_ok: '2026-07-05T00:00:00Z',
    };
    const out = aggregatePeers([
      contact({ channels: [ch], endpoints: [endpoint({ last_ok: '2026-07-07T00:00:00Z' })] }),
    ]);
    expect(out[0].lastOk).toBe('2026-07-07T00:00:00Z');
    // lastSeen (fail-bumped) is NEWER than lastOk — they are distinct quantities.
    expect(out[0].lastSeen).toBe('2026-07-09T00:00:00Z');
  });

  it('lastOk is null when nothing has ever succeeded (only failed attempts)', () => {
    const ch: Channel = {
      transport: 'ardop', target_callsign: 'W6ABC-7', via: [], freq_hz: null,
      bandwidth: null, direction: 'outgoing', counts: { ok: 0, fail: 4 },
      last_seen: '2026-07-09T00:00:00Z', last_ok: null,
    };
    const out = aggregatePeers([contact({ channels: [ch] })]);
    expect(out[0].lastOk).toBeNull();
    expect(out[0].lastSeen).toBe('2026-07-09T00:00:00Z');
  });
});

// Tac-chat map tier chain — the T-F Part 0 truth-fix, extracted from
// AprsPositionsMap so the reachable-failed-tier + no-failed-timestamp
// invariants are unit-pinned.
describe('peerTacChatTier — success-only tier chain', () => {
  const NOW_MS = Date.parse('2026-07-11T12:00:00-07:00');
  const failChannel: Channel = {
    transport: 'vara-hf', target_callsign: 'W6ABC', via: [], freq_hz: null,
    bandwidth: null, direction: 'outgoing', counts: { ok: 0, fail: 3 },
    last_seen: '2026-07-11T11:59:00-07:00', last_ok: null,
  };
  function agg(over: Partial<AggregatedPeer> = {}): AggregatedPeer {
    return {
      id: 'p1', callsign: 'W6ABC', origin: 'outgoing', tier: 'unconfirmed',
      grid: 'CN87', mapPlaceable: true, lastSeen: null, lastOk: null,
      channels: [], endpoints: [], ...over,
    };
  }

  it('live wins when the peer base matches the connected modem peer', () => {
    const v = peerTacChatTier(agg({ lastOk: '2026-07-11T11:59:00-07:00' }), {
      livePeerBase: 'W6ABC', nowMs: NOW_MS,
    });
    expect(v.tierClass).toBe('peer-pin--live');
  });

  it('reached when lastOk is within the hour', () => {
    const v = peerTacChatTier(agg({ lastOk: '2026-07-11T11:30:00-07:00' }), {
      livePeerBase: null, nowMs: NOW_MS,
    });
    expect(v.tierClass).toBe('peer-pin--reached');
  });

  it('stale when lastOk is older than the hour', () => {
    const v = peerTacChatTier(agg({ lastOk: '2026-07-11T09:00:00-07:00' }), {
      livePeerBase: null, nowMs: NOW_MS,
    });
    expect(v.tierClass).toBe('peer-pin--stale');
  });

  it('FAILED tier is reachable: attempts with fails but NO success → peer-pin--failed', () => {
    // The T-F Part 0 bug fix. Before: a fail-bumped lastSeen made the reached
    // branch win first, so peer-pin--failed was unreachable. Now the chain keys
    // on lastOk, so a never-succeeded-but-attempted peer lands on failed.
    const v = peerTacChatTier(agg({ lastOk: null, lastSeen: '2026-07-11T11:59:00-07:00', channels: [failChannel] }), {
      livePeerBase: null, nowMs: NOW_MS,
    });
    expect(v.tierClass).toBe('peer-pin--failed');
  });

  it('a fail-bumped lastSeen never masquerades as reached (no-failed-timestamp)', () => {
    // Even though lastSeen is seconds old, the peer has no success — it must NOT
    // be reached/stale. It is failed (has a fail) — proving reached derives from
    // lastOk, not lastSeen.
    const v = peerTacChatTier(agg({ lastOk: null, lastSeen: '2026-07-11T11:59:59-07:00', channels: [failChannel] }), {
      livePeerBase: null, nowMs: NOW_MS,
    });
    expect(v.tierClass).not.toBe('peer-pin--reached');
    expect(v.tierClass).toBe('peer-pin--failed');
  });

  it('unknown when there is neither a success nor a recorded failure', () => {
    const v = peerTacChatTier(agg({ lastOk: null, channels: [] }), { livePeerBase: null, nowMs: NOW_MS });
    expect(v.tierClass).toBe('peer-pin--unknown');
  });

  it('a never-connected manual peer is dashed', () => {
    const v = peerTacChatTier(agg({ origin: 'manual', lastOk: null }), { livePeerBase: null, nowMs: NOW_MS });
    expect(v.dashed).toBe(true);
  });
});
