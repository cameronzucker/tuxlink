import type { RadioPanelState } from '../radio/RadioPanel';

export interface DrawerStateInputs {
  /** True while a CMS connect exchange is in flight (AppShell `connecting`). */
  connecting: boolean;
  /** The live transport status kind (AppShell `statusData.status`), or undefined. */
  status?: { kind?: string } | null;
  /** Fallback: modem in any active state (AppShell `useModemIsActive()`). */
  modemIsActive: boolean;
}

/**
 * Coarse session state for the drawer grip tick. CRITICAL (Claude adrev F2):
 * must NOT show 'connected'/'disconnected' during an RF *connecting* handshake —
 * that is exactly the runaway-connect window (2026-05-22) where the operator
 * needs abort urgency. Switch on the transport status kind so 'Connecting' and
 * 'Listening' surface amber, and an unknown-but-active modem reads amber
 * (cautious), never a safe green. tuxlink-h7q7.
 */
export function deriveDrawerSessionState(i: DrawerStateInputs): RadioPanelState {
  if (i.connecting) return 'connecting';
  switch (i.status?.kind) {
    case 'Connecting':
      return 'connecting';
    case 'Listening':
      return 'connecting'; // armed → amber, not green
    case 'Disconnecting':
      return 'disconnecting';
    case 'Error':
      return 'error';
    case 'Connected':
      return 'connected';
    default:
      // Active but unknown kind → amber (cautious): never read as a safe green.
      return i.modemIsActive ? 'connecting' : 'disconnected';
  }
}
