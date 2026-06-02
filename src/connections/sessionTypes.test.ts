// src/connections/sessionTypes.test.ts
import { describe, it, expect } from 'vitest';
import { SESSION_TYPES, protocolsFor, isBuilt, type ConnectionKey } from './sessionTypes';

describe('session-type catalog', () => {
  it('lists the five routing intents in order', () => {
    expect(SESSION_TYPES.map((s) => s.id)).toEqual([
      'cms', 'radio-only', 'post-office', 'p2p', 'network-po',
    ]);
  });
  it('CMS offers Telnet, Packet, ARDOP HF, and VARA HF/FM as built protocols', () => {
    // tuxlink-dfmf Phase 2: cms.vara-hf + cms.vara-fm flipped to built:true
    // (UI ships TCP-transport + config + Pi-availability gating; RF CONNECT
    // is Phase 3 territory). The "shown but not built" wording belonged
    // to the pre-Phase-2 era when the protocols had sidebar entries but
    // no panel. P2P VARA stays unbuilt — see the next test.
    const protos = protocolsFor('cms');
    expect(protos.find((p) => p.id === 'telnet')?.built).toBe(true);
    expect(protos.find((p) => p.id === 'packet')?.built).toBe(true);
    expect(protos.find((p) => p.id === 'vara-hf')?.built).toBe(true);
    expect(protos.find((p) => p.id === 'vara-fm')?.built).toBe(true);
  });
  it('P2P VARA HF/FM stay unbuilt for Phase 2 (Phase 3 enables peer-connect)', () => {
    // The P2P intent's VARA entries remain unbuilt because the peer-dial
    // flow needs the session state machine + RADIO-1 consent (Phase 3 —
    // tuxlink-fzl7). Only flipping CMS in Phase 2 keeps the operator-
    // facing scope bounded to what the implementation actually covers.
    const protos = protocolsFor('p2p');
    expect(protos.find((p) => p.id === 'vara-hf')?.built).toBe(false);
    expect(protos.find((p) => p.id === 'vara-fm')?.built).toBe(false);
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
  it('isBuilt is true for p2p+telnet (tuxlink-0pnb shipped client-dial)', () => {
    expect(isBuilt({ sessionType: 'p2p', protocol: 'telnet' })).toBe(true);
  });
  it('protocolsFor returns correct list for known id and [] for unknown id', () => {
    expect(protocolsFor('cms' as any).map((p) => p.id)).toContain('telnet');
    expect(protocolsFor('bogus' as any)).toEqual([]);
  });
  it('isBuilt is false when protocol is not listed under the intent (network-po + vara-hf)', () => {
    expect(isBuilt({ sessionType: 'network-po', protocol: 'vara-hf' })).toBe(false);
  });
});

describe('ARDOP HF catalog entry', () => {
  it('exposes ardop-hf as a built protocol under cms intent', () => {
    const protos = protocolsFor('cms');
    const ardop = protos.find((p) => p.id === 'ardop-hf');
    expect(ardop).toBeDefined();
    expect(ardop?.label).toBe('ARDOP HF');
    expect(ardop?.built).toBe(true);
  });

  it('isBuilt returns true for cms × ardop-hf', () => {
    expect(isBuilt({ sessionType: 'cms', protocol: 'ardop-hf' })).toBe(true);
  });
});
