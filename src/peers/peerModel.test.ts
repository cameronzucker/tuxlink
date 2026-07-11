import { describe, it, expect } from 'vitest';
import { aggregatePeers } from './peerModel';
import type { Peer } from './types';

const peer = (over: Partial<Peer>): Peer => ({
  id: 'p1', canonical_base: 'W6ABC', presented_callsigns: ['W6ABC-7'],
  identity_kind: 'unknown', do_not_merge: false, conflict: false,
  source: 'auto', origin: 'outgoing', contact_id: null, grid: null,
  note: '', created_at: '2026-07-10T12:00:00-07:00', last_connected_at: null,
  channels: [], endpoints: [], ...over,
});

describe('aggregatePeers', () => {
  it('keeps gridless peers instead of dropping them', () => {
    const out = aggregatePeers([peer({ grid: null, endpoints: [
      { id: 'e1', host: 'x.example', port: 8774, provenance: 'operator', last_seen: '' },
    ] })]);
    expect(out).toHaveLength(1);
    expect(out[0].grid).toBeUndefined();
    expect(out[0].mapPlaceable).toBe(false);
  });

  it('keys on canonical_base, merging presented forms', () => {
    const out = aggregatePeers([
      peer({ id: 'a', presented_callsigns: ['W6ABC-7'] }),
      peer({ id: 'b', presented_callsigns: ['W6ABC-9'] }),
    ]);
    // Two distinct records (distinct ids) both surface; aggregation is by
    // base for map placement but never collapses distinct stored records.
    expect(out.map((p) => p.id).sort()).toEqual(['a', 'b']);
  });

  it('marks a gridded peer map-placeable', () => {
    const out = aggregatePeers([peer({ grid: { value: 'CN87', source: 'manual' } })]);
    expect(out[0].mapPlaceable).toBe(true);
    expect(out[0].grid).toBe('CN87');
  });
});
