// src/connections/sessionTypes.test.ts
import { describe, it, expect } from 'vitest';
import { SESSION_TYPES, protocolsFor, isBuilt, type ConnectionKey } from './sessionTypes';

describe('session-type catalog', () => {
  it('lists the five routing intents in order', () => {
    expect(SESSION_TYPES.map((s) => s.id)).toEqual([
      'cms', 'radio-only', 'post-office', 'p2p', 'network-po',
    ]);
  });
  it('CMS offers Telnet (built) and Packet (built); VARA shown but not built', () => {
    const protos = protocolsFor('cms');
    expect(protos.find((p) => p.id === 'telnet')?.built).toBe(true);
    expect(protos.find((p) => p.id === 'packet')?.built).toBe(true);
    expect(protos.find((p) => p.id === 'vara-hf')?.built).toBe(false);
  });
  it('isBuilt is false for any protocol under an unbuilt intent (radio-only)', () => {
    const key: ConnectionKey = { sessionType: 'radio-only', protocol: 'packet' };
    expect(isBuilt(key)).toBe(false);
  });
  it('isBuilt is true for cms+telnet, cms+packet, p2p+packet', () => {
    expect(isBuilt({ sessionType: 'cms', protocol: 'telnet' })).toBe(true);
    expect(isBuilt({ sessionType: 'cms', protocol: 'packet' })).toBe(true);
    expect(isBuilt({ sessionType: 'p2p', protocol: 'packet' })).toBe(true);
  });
  it('protocolsFor returns correct list for known id and [] for unknown id', () => {
    expect(protocolsFor('cms' as any).map((p) => p.id)).toContain('telnet');
    expect(protocolsFor('bogus' as any)).toEqual([]);
  });
  it('isBuilt is false when protocol is not listed under the intent (network-po + vara-hf)', () => {
    expect(isBuilt({ sessionType: 'network-po', protocol: 'vara-hf' })).toBe(false);
  });
});
