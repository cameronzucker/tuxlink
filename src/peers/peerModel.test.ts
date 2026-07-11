import { describe, it, expect } from 'vitest';
import { aggregatePeers } from './peerModel';
import type { Contact, Endpoint } from '../contacts/types';

const contact = (over: Partial<Contact>): Contact => ({
  id: 'c1', name: '', callsign: 'W6ABC-7', tier: 'unconfirmed', origin: 'outgoing',
  channels: [], endpoints: [], created_at: '2026-07-10T12:00:00-07:00',
  updated_at: '2026-07-10T12:00:00-07:00', ...over,
});

const endpoint = (over: Partial<Endpoint> = {}): Endpoint => ({
  id: 'e1', host: 'x.example', port: 8774, provenance: 'operator', last_seen: '',
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
            last_seen: '2026-07-01T00:00:00Z',
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
});
