// Cross-language wire-shape parity (spec §10, the k61j composed-seam class;
// adrev R5-F10/F11). Both this file and the Rust-side test in
// src-tauri/src/dock/mod.rs (`wire_fixture_parity`) assert against the SAME
// committed fixture — src/dock/dock-wire-fixture.json — so a drift between
// the two languages' understanding of the wire shape or the consent-host
// resolution fails on whichever side changed without the other, instead of
// two independently-green per-language unit tests hiding a composed mismatch.

import { describe, it, expect } from 'vitest';
import fixture from './dock-wire-fixture.json';
import { consentHostWindow, type DockSnapshot } from './dockState';

describe('dock wire fixture parity (spec §10)', () => {
  it('routinesDocked parses into a DockSnapshot and resolves the consent host to main', () => {
    const snap = fixture.routinesDocked as DockSnapshot;
    expect(snap.surfaces.routines).toBe('docked');
    expect(snap.surfaces.tac_map).toBe('docked');
    expect(snap.surfaces.aprs_chat).toBe('docked');
    expect(consentHostWindow(snap.surfaces)).toBe('main');
  });

  it('routinesPopped parses into a DockSnapshot, carries its continuity token, and resolves to pop-routines', () => {
    const snap = fixture.routinesPopped as DockSnapshot;
    expect(snap.surfaces.routines).toBe('popped');
    expect(consentHostWindow(snap.surfaces)).toBe('pop-routines');
    expect(snap.context.routines).toEqual({
      view: 'designer',
      routine: 'morning-ics-cycle',
      tab: 'design',
      draft: {},
    });
    expect(snap.context.tac_map).toBeNull();
    expect(snap.context.aprs_chat).toBeNull();
  });
});
