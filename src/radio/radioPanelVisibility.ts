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
    // post-office and network-po are telnet-only intents; non-telnet kinds
    // do not support them and coerce them to cms below.
    const intent: 'cms' | 'p2p' | 'radio-only' | 'post-office' | 'network-po' =
      sessionType === 'p2p'         ? 'p2p' :
      sessionType === 'radio-only'  ? 'radio-only' :
      sessionType === 'post-office' ? 'post-office' :
      sessionType === 'network-po'  ? 'network-po' :
      'cms';
    switch (protocol) {
      case 'telnet':
        // telnet supports all intents; radio-only degrades to cms (no RF transport)
        return { kind: 'telnet', intent: intent === 'radio-only' ? 'cms' : intent };
      case 'packet':
        // packet supports only cms|p2p; radio-only, post-office, network-po all
        // coerce to cms (packet is not RF-bearing for Post Office sessions)
        return { kind: 'packet', intent: intent === 'p2p' ? 'p2p' : 'cms' };
      case 'ardop-hf': {
        // ardop-hf supports cms|p2p|radio-only; post-office/network-po are
        // telnet-only and coerce to cms for RF kinds
        const rfIntent = (intent === 'post-office' || intent === 'network-po') ? 'cms' : intent;
        return { kind: 'ardop-hf', intent: rfIntent };
      }
      case 'vara-hf': {
        const rfIntent = (intent === 'post-office' || intent === 'network-po') ? 'cms' : intent;
        return { kind: 'vara-hf', intent: rfIntent };
      }
      case 'vara-fm': {
        const rfIntent = (intent === 'post-office' || intent === 'network-po') ? 'cms' : intent;
        return { kind: 'vara-fm', intent: rfIntent };
      }
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
