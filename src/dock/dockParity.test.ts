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
import { isElmerTokenState } from '../elmer/elmerToken';

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
    // The per-surface context is the continuity ENVELOPE `{ foreground, state }`
    // (tuxlink-dmwte task 8, spec §5/§7) — the `foreground` bit drives the main
    // window's ⇤-vs-✕ presentation, and `state` is the opaque Routines token
    // (`{ view, draft }`). The backend stores/forwards this verbatim (opaque
    // `serde_json::Value`); the Rust parity test round-trips it without
    // inspecting the internal shape, so this frontend shape is authoritative.
    expect(snap.context.routines).toEqual({
      foreground: true,
      state: {
        view: { view: 'designer', routine: 'morning-ics-cycle', tab: 'design' },
        draft: {},
      },
    });
    expect(snap.context.tac_map).toBeNull();
    expect(snap.context.aprs_chat).toBeNull();
  });

  it('elmerPopped parses, carries the conversation token, and does NOT move the routines consent host (tuxlink-mfssz)', () => {
    const snap = fixture.elmerPopped as DockSnapshot;
    expect(snap.surfaces.elmer).toBe('popped');
    expect(snap.surfaces.routines).toBe('docked');
    // Elmer's approval UX renders inside the pane itself, so the routines
    // consent host is untouched by an Elmer pop-out.
    expect(consentHostWindow(snap.surfaces)).toBe('main');
    expect(snap.context.elmer).toEqual({
      foreground: true,
      state: { items: [{ kind: 'turn', id: 'elmer-item-0', role: 'user', text: 'hello' }] },
    });
    // Adrev 2026-07-20 (5.5 round, P3): the fixture's token must PASS the
    // runtime seed validator — a fixture the seed guard would discard blesses
    // a wire shape the app can't actually adopt, hiding token-shape drift.
    const envelope = snap.context.elmer as { state?: unknown };
    expect(isElmerTokenState(envelope.state)).toBe(true);
  });
});
