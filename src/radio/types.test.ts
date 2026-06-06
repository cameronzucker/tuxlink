import { describe, it, expect } from 'vitest';
import type { RadioPanelMode } from './types';
import { panelTitle } from './types';

describe('RadioPanelMode', () => {
  it('accepts radio-only intent for ardop-hf, vara-hf, vara-fm', () => {
    const a: RadioPanelMode = { kind: 'ardop-hf', intent: 'radio-only' };
    const b: RadioPanelMode = { kind: 'vara-hf',  intent: 'radio-only' };
    const c: RadioPanelMode = { kind: 'vara-fm',  intent: 'radio-only' };
    expect([a.intent, b.intent, c.intent]).toEqual(['radio-only','radio-only','radio-only']);
  });

  it('does NOT accept radio-only intent for telnet or packet', () => {
    // @ts-expect-error — telnet/packet are not radio-bearing
    const _t: RadioPanelMode = { kind: 'telnet', intent: 'radio-only' };
    // @ts-expect-error
    const _p: RadioPanelMode = { kind: 'packet', intent: 'radio-only' };
  });
});

describe('panelTitle', () => {
  it('returns Radio-only suffix for radio-only intent', () => {
    expect(panelTitle({ kind: 'ardop-hf', intent: 'radio-only' })).toBe('Ardop Radio-only');
    expect(panelTitle({ kind: 'vara-hf',  intent: 'radio-only' })).toBe('Vara HF Radio-only');
    expect(panelTitle({ kind: 'vara-fm',  intent: 'radio-only' })).toBe('Vara FM Radio-only');
  });

  it('returns Winlink suffix for cms intent', () => {
    expect(panelTitle({ kind: 'telnet', intent: 'cms' })).toBe('Telnet Winlink');
    expect(panelTitle({ kind: 'packet', intent: 'cms' })).toBe('Packet Winlink');
  });

  it('returns P2P suffix for p2p intent', () => {
    expect(panelTitle({ kind: 'telnet', intent: 'p2p' })).toBe('Telnet P2P');
    expect(panelTitle({ kind: 'vara-hf', intent: 'p2p' })).toBe('Vara HF P2P');
  });
});
