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
  it('P2P VARA HF/FM are built — Phase 2 surface ships for both intents (tuxlink-kb3s)', () => {
    // PR #221's scope-bounding ("only flip CMS in Phase 2") was a
    // packaging choice, not a technical constraint — the VaraRadioPanel
    // + useVaraConfig + radioPanelVisibility router are all intent-
    // agnostic. tuxlink-kb3s flips P2P VARA HF/FM to built:true so the
    // operator can configure + open the VARA TCP transport under either
    // intent. RF CONNECT-to-peer (Phase 3, tuxlink-fzl7) lands parallel
    // to CMS's Phase 3 dial.
    const protos = protocolsFor('p2p');
    expect(protos.find((p) => p.id === 'vara-hf')?.built).toBe(true);
    expect(protos.find((p) => p.id === 'vara-fm')?.built).toBe(true);
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

  it('isBuilt is true for post-office+telnet (tuxlink-6c9y)', () => {
    expect(isBuilt({ sessionType: 'post-office', protocol: 'telnet' })).toBe(true);
  });

  it('isBuilt is true for network-po+telnet (tuxlink-6c9y)', () => {
    expect(isBuilt({ sessionType: 'network-po', protocol: 'telnet' })).toBe(true);
  });

  it('isBuilt is still false for post-office+packet (PKT stays unbuilt, tuxlink-6c9y)', () => {
    expect(isBuilt({ sessionType: 'post-office', protocol: 'packet' })).toBe(false);
  });

  // tuxlink-3wwr: Sonde HF/FM are "coming soon" teasers — listed under the RF
  // session types (mirroring VARA) but always built:false (disabled + "soon"
  // badge), and never wired to a backend. Guards against an accidental
  // built:true flip that would render a dead control.
  it('Sonde HF/FM appear under the RF session types as UNBUILT teasers', () => {
    for (const st of ['cms', 'radio-only', 'p2p'] as const) {
      const ids = protocolsFor(st).map((p) => p.id);
      expect(ids).toContain('sonde-hf');
      expect(ids).toContain('sonde-fm');
      // Never selectable/usable — coming-soon only.
      expect(isBuilt({ sessionType: st, protocol: 'sonde-hf' })).toBe(false);
      expect(isBuilt({ sessionType: st, protocol: 'sonde-fm' })).toBe(false);
    }
  });

  it('Sonde is NOT listed under the telnet/packet-only session types', () => {
    for (const st of ['post-office', 'network-po'] as const) {
      const ids = protocolsFor(st).map((p) => p.id);
      expect(ids).not.toContain('sonde-hf');
      expect(ids).not.toContain('sonde-fm');
    }
  });
});

describe('radio-only session type', () => {
  it('radio-only intent is built for ardop-hf, vara-hf, vara-fm', () => {
    expect(isBuilt({ sessionType: 'radio-only', protocol: 'ardop-hf' })).toBe(true);
    expect(isBuilt({ sessionType: 'radio-only', protocol: 'vara-hf'  })).toBe(true);
    expect(isBuilt({ sessionType: 'radio-only', protocol: 'vara-fm'  })).toBe(true);
  });

  it('radio-only intent is NOT built for telnet, packet (not RF-bearing)', () => {
    expect(isBuilt({ sessionType: 'radio-only', protocol: 'telnet' })).toBe(false);
    expect(isBuilt({ sessionType: 'radio-only', protocol: 'packet' })).toBe(false);
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
