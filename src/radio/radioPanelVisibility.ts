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
    // The RF kinds (ardop-hf/vara-hf/vara-fm) accept cms|p2p|radio-only but not the
    // telnet-only post-office/network-po intents; coerce those to cms once for all
    // three RF arms (a single telnet-only intent added later updates only this line).
    const rfSafeIntent = intent === 'post-office' || intent === 'network-po' ? 'cms' : intent;
    switch (protocol) {
      case 'telnet':
        // telnet supports all intents; radio-only degrades to cms (no RF transport)
        return { kind: 'telnet', intent: intent === 'radio-only' ? 'cms' : intent };
      case 'packet':
        // packet (AX.25) supports only cms|p2p; radio-only and the telnet-only
        // post-office/network-po intents all coerce to cms (no Post Office over packet)
        return { kind: 'packet', intent: intent === 'p2p' ? 'p2p' : 'cms' };
      case 'ardop-hf': return { kind: 'ardop-hf', intent: rfSafeIntent };
      case 'vara-hf':  return { kind: 'vara-hf',  intent: rfSafeIntent };
      case 'vara-fm':  return { kind: 'vara-fm',  intent: rfSafeIntent };
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
