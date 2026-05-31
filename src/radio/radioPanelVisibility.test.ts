// src/radio/radioPanelVisibility.test.ts
import { describe, it, expect } from 'vitest';
import { computePanelMode, computePanelVisibility } from './radioPanelVisibility';

describe('computePanelVisibility', () => {
  it('hides the panel when nothing is active', () => {
    expect(computePanelVisibility({
      sidebarSelected: null,
      modemActive: false,
      togglePinned: false,
    })).toBe(false);
  });

  it('shows the panel when a connection is selected in the sidebar', () => {
    expect(computePanelVisibility({
      sidebarSelected: { sessionType: 'cms', protocol: 'ardop-hf' },
      modemActive: false,
      togglePinned: false,
    })).toBe(true);
  });

  it('shows the panel when any modem is non-stopped', () => {
    expect(computePanelVisibility({
      sidebarSelected: null,
      modemActive: true,
      togglePinned: false,
    })).toBe(true);
  });

  it('shows the panel when View → Toggle Radio Panel is pinned on', () => {
    expect(computePanelVisibility({
      sidebarSelected: null,
      modemActive: false,
      togglePinned: true,
    })).toBe(true);
  });
});

describe('computePanelMode', () => {
  it('returns null when nothing is active', () => {
    expect(computePanelMode(
      { sidebarSelected: null, modemActive: false, togglePinned: false },
    )).toBeNull();
  });

  it('prefers the sidebar selection when the operator has one', () => {
    // operator on Packet view but ARDOP is connecting — sidebar wins because
    // it's the operator's explicit active context (spec §8: one modem at a
    // time, sidebar is the active context).
    const mode = computePanelMode(
      { sidebarSelected: { sessionType: 'cms', protocol: 'packet' },
        modemActive: true, togglePinned: false },
    );
    expect(mode).toEqual({ kind: 'packet', intent: 'cms' });
  });

  it('returns sidebar selection when modem is stopped and pin is off', () => {
    const mode = computePanelMode(
      { sidebarSelected: { sessionType: 'p2p', protocol: 'packet' },
        modemActive: false, togglePinned: false },
    );
    expect(mode).toEqual({ kind: 'packet', intent: 'p2p' });
  });

  it('returns telnet/cms as the default empty state when only togglePinned is on', () => {
    const mode = computePanelMode(
      { sidebarSelected: null, modemActive: false, togglePinned: true },
    );
    expect(mode).toEqual({ kind: 'telnet', intent: 'cms' });
  });
});
