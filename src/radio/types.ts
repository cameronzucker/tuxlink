// src/radio/types.ts
//
// Types shared by the radio panel and its mode-specific implementations.
// The panel is the right-hand column that owns connection setup, live
// state, modem console, session log, and actions for the currently-
// selected radio mode. Mode-specific panels (Telnet / Packet / ARDOP /
// VARA when built) render their content into the panel's shared
// chrome.
//
// See docs/superpowers/specs/2026-05-31-radio-mode-right-panel-design.md
// for the locked design decisions.

import type { ConnectionKey } from '../mailbox/FolderSidebar';

/**
 * The reason the radio panel is currently mounted. Multiple reasons can
 * be true simultaneously; the panel shows whichever mode is most
 * relevant (active modem > sidebar selection > toggle).
 */
export interface RadioPanelMountReason {
  /** A connection sidebar entry is selected (Telnet / Packet / etc.). */
  sidebarSelected: ConnectionKey | null;
  /** Any modem is in a non-stopped state. */
  modemActive: boolean;
  /** Operator has toggled the View menu item on. */
  togglePinned: boolean;
}

/**
 * The mode the panel is currently displaying. Derived from
 * RadioPanelMountReason; null means the panel is not mounted.
 */
export type RadioPanelMode =
  | { kind: 'telnet'; intent: 'cms' }
  | { kind: 'packet'; intent: 'cms' | 'p2p' }
  | { kind: 'ardop-hf'; intent: 'cms' }
  | { kind: 'vara-hf'; intent: 'cms' | 'p2p' }    // forward-looking
  | { kind: 'vara-fm'; intent: 'cms' | 'p2p' };   // forward-looking

/**
 * Human-readable name for a mode + intent, matching Express vocabulary
 * from docs/scratch/winlink-re/decompiled/. Used in the panel header.
 */
export function panelTitle(mode: RadioPanelMode): string {
  const intentSuffix = mode.intent === 'cms' ? 'Winlink' : 'P2P';
  switch (mode.kind) {
    case 'telnet':   return `Telnet ${intentSuffix}`;
    case 'packet':   return `Packet ${intentSuffix}`;
    case 'ardop-hf': return `Ardop ${intentSuffix}`;
    case 'vara-hf':  return `Vara HF ${intentSuffix}`;
    case 'vara-fm':  return `Vara FM ${intentSuffix}`;
  }
}
