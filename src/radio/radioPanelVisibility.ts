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
    reason.activeModem !== null ||
    reason.togglePinned
  );
}

export function computePanelMode(
  reason: RadioPanelMountReason,
): RadioPanelMode | null {
  if (!computePanelVisibility(reason)) {
    return null;
  }

  // Sidebar selection wins when present (operator's explicit context).
  if (reason.sidebarSelected !== null) {
    const { sessionType, protocol } = reason.sidebarSelected;
    const intent: 'cms' | 'p2p' = sessionType === 'p2p' ? 'p2p' : 'cms';
    switch (protocol) {
      case 'telnet':   return { kind: 'telnet',   intent };
      case 'packet':   return { kind: 'packet',   intent };
      case 'ardop-hf': return { kind: 'ardop-hf', intent: 'cms' };
      case 'vara-hf':  return { kind: 'vara-hf',  intent };
      case 'vara-fm':  return { kind: 'vara-fm',  intent };
    }
  }

  // No sidebar: an active modem's mode reflects the operator's actual
  // current context. Spec §3.3 — the panel stays mounted on a running
  // modem and the mode shown should match the running modem, not a
  // default.
  if (reason.activeModem !== null) {
    return reason.activeModem;
  }

  // togglePinned with no sidebar and no modem: default to Telnet Winlink
  // as a reasonable empty state; operator clicks a sidebar entry to set
  // the actual mode.
  return { kind: 'telnet', intent: 'cms' };
}
