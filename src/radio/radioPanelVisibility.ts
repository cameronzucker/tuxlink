// src/radio/radioPanelVisibility.ts
//
// The visibility rule from docs/superpowers/specs/2026-05-31-radio-mode-
// right-panel-design.md §3.3:
//
//   The panel mounts when ANY of:
//     - a connection entry is selected in the sidebar
//     - any modem is in a non-stopped state
//     - View → Toggle Radio Panel is on
//
//   The mode displayed is derived from that same context (§3.3 + §4.1).

import type { RadioPanelMountReason, RadioPanelMode } from './types';

export function computePanelVisibility(reason: RadioPanelMountReason): boolean {
  return (
    reason.sidebarSelected !== null ||
    reason.modemActive ||
    reason.togglePinned
  );
}

export function computePanelMode(
  reason: RadioPanelMountReason,
): RadioPanelMode | null {
  if (!computePanelVisibility(reason)) {
    return null;
  }

  // v1 prefers sidebar selection. Multi-modem coordination (where a
  // running modem differs from the sidebar selection) is out of scope
  // per spec §8 — one active modem at a time, and the sidebar
  // selection is the operator's active context.
  if (reason.sidebarSelected !== null) {
    const { sessionType, protocol } = reason.sidebarSelected;
    const intent: 'cms' | 'p2p' = sessionType === 'p2p' ? 'p2p' : 'cms';
    switch (protocol) {
      case 'telnet':   return { kind: 'telnet',   intent: 'cms' };
      case 'packet':   return { kind: 'packet',   intent };
      case 'ardop-hf': return { kind: 'ardop-hf', intent: 'cms' };
      case 'vara-hf':  return { kind: 'vara-hf',  intent };
      case 'vara-fm':  return { kind: 'vara-fm',  intent };
    }
  }

  // togglePinned + no sidebar selection + no modem: show a "no connection"
  // placeholder. For v1 we default to Telnet Winlink as a reasonable
  // empty state; operators set the actual mode by clicking a sidebar entry.
  return { kind: 'telnet', intent: 'cms' };
}
