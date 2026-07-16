// src/dock/surfaceRegistry.tsx — the three-entry surface registry
// PoppedSurfaceHost mounts from (spec §4). bd tuxlink-dmwte task 7.
//
// Deliberate deviation from the spec §4 registry sketch: NO `defaultSize`
// field here — first-spawn sizes live Rust-side in `pop_window_spec`
// (backend Task 3). Do not "restore" it.
import { useCallback, useEffect, useRef, useState, type ComponentType } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { SurfaceId } from './dockState';
import { useStatusData } from '../shell/useStatus';
import { AprsPositionsMap } from '../aprs/AprsPositionsMap';
import { useAprsPositions } from '../aprs/useAprsPositions';
import { useEnvStations } from '../aprs/useEnvStations';
import { AprsChatPanel } from '../aprs/AprsChatPanel';
import { AprsConnectStrip } from '../aprs/AprsConnectStrip';
import { useAprsChat } from '../aprs/useAprsChat';
import { usePacketConfig } from '../packet/usePacketConfig';
import type { ModemLinkFields } from '../radio/sections/ModemLinkSection';
import type { PacketConfigDto, PacketLinkKind } from '../packet/packetTypes';
import { UvproControlStrip } from '../uvpro/UvproControlStrip';
import { RoutinesSurface, type RoutinesView } from '../routines/RoutinesSurface';
import { RoutinesStrip, TacMapStrip, ChatStrip } from './strips';

/** Task 8–10 extend this shape (task-7 brief's normative interface block —
 *  binding for the whole dockable-surfaces plan). */
export interface SurfaceComponentProps {
  /** The continuity token's `state` half from `dock_state_get`, null when
   *  absent. */
  context: unknown | null;
  /** The surface registers a live state-collector; PoppedSurfaceHost stores
   *  it in a ref and calls it at every dock-back path (⇤, ✕, Ctrl+W,
   *  close-intent) to build the outgoing token's `state`. Surfaces with no
   *  internal state to carry (tac_map, aprs_chat) never call it — the
   *  host's ref stays null. */
  registerGetContext: (fn: () => unknown | null) => void;
}

export interface SurfaceRegistryEntry {
  id: SurfaceId;
  /** From the spec §3 wire table — static, never changes while popped. */
  title: string;
  Component: ComponentType<SurfaceComponentProps>;
  /** Chrome option B (spec §4) — a thin bottom strip of this surface's own
   *  vitals. */
  StatusStrip: ComponentType;
}

// ---- Tac Map -----------------------------------------------------------
//
// Mounts its own hooks exactly as AppShell does for the in-pane map
// (AppShell.tsx ~2137-2146): useAprsPositions() for the live positions,
// useEnvStations({snapshotRole:'client'}) so the pop-out seeds from the main
// shell's snapshot instead of an empty roster (tuxlink-hzwc bug #4 precedent
// — see src/aprs/StationsView.tsx for the identical snapshot-client
// pattern), and useStatusData().grid for the first-run/recenter center.
// `onFocusStation` is omitted — that callback drives AppShell's own Station
// Data dock tab, which has no equivalent surface in this standalone window.
function TacMapSurface(_props: SurfaceComponentProps) {
  const { positions } = useAprsPositions();
  const envStations = useEnvStations({ snapshotRole: 'client' });
  const statusData = useStatusData();
  return (
    <AprsPositionsMap
      positions={positions}
      operatorGrid={statusData.grid ?? undefined}
      envStations={envStations.stations}
    />
  );
}

// ---- APRS Chat ----------------------------------------------------------
//
// Composes AprsConnectStrip above AprsChatPanel, mirroring AppShell's dock
// composition (AppShell.tsx ~2313-2334) — the strip stays a separate
// component owned by the hosting container (adrev Codex-7: folding it into
// the panel would break the existing APRS ownership model). The
// transport-specific connect/disconnect sequence is AppShell's own composed
// logic (AppShell.tsx ~811-899), replicated here rather than factored out —
// it is not a shared hook today; see the task-7 report for a follow-up note.
function radioLabelFor(c: PacketConfigDto | null | undefined): string | null {
  if (!c) return null;
  switch (c.linkKind) {
    case 'Tcp':
      return c.tcpHost && c.tcpPort ? `${c.tcpHost}:${c.tcpPort}` : 'TCP KISS';
    case 'Serial':
      return c.serialDevice ?? 'USB TNC';
    case 'Bluetooth':
      return c.btMac ? `BT ${c.btMac}` : 'Bluetooth';
    case 'UvproNative':
      return c.btMac ? `UV-Pro ${c.btMac}` : 'UV-Pro (native)';
    default:
      return null;
  }
}

function AprsChatSurface(_props: SurfaceComponentProps) {
  const aprs = useAprsChat();
  const packetConfig = usePacketConfig();
  const [connecting, setConnecting] = useState(false);
  const linkKind = packetConfig.config?.linkKind ?? null;
  // The most-recent link-persist promise + the transport the LIVE listener
  // actually came up on — mirrors AppShell's aprsLinkPersist/aprsActiveTransport
  // refs (AppShell.tsx ~831-836) and their rationale (await the persist before
  // arming; teardown keys off the transport that was actually live, not the
  // editable picker).
  const linkPersist = useRef<Promise<void>>(Promise.resolve());
  const activeTransport = useRef<PacketLinkKind | null>(null);

  const onLinkChange = useCallback(
    (fields: ModemLinkFields) => {
      linkPersist.current = packetConfig.setLink(fields);
    },
    [packetConfig],
  );

  const onConnect = useCallback(async () => {
    await linkPersist.current;
    if (linkKind === 'UvproNative') {
      await invoke('uvpro_connect', {});
      try {
        await invoke('aprs_listen_start');
      } catch (err) {
        await invoke('uvpro_disconnect').catch(() => undefined);
        throw err;
      }
    } else {
      await invoke('aprs_listen_start');
    }
    activeTransport.current = linkKind;
  }, [linkKind]);

  const onDisconnect = useCallback(async () => {
    const active = activeTransport.current;
    try {
      await invoke('aprs_listen_stop');
    } finally {
      if (active === 'UvproNative') await invoke('uvpro_disconnect').catch(() => undefined);
      activeTransport.current = null;
    }
  }, []);

  const runConnect = useCallback(async () => {
    setConnecting(true);
    try {
      await onConnect();
    } finally {
      setConnecting(false);
    }
  }, [onConnect]);

  return (
    <div className="pop-aprs-chat">
      <AprsConnectStrip
        listening={aprs.listening}
        externalConnecting={connecting}
        linkKind={linkKind}
        radioLabel={radioLabelFor(packetConfig.config)}
        allowUvproNative
        tcpHost={packetConfig.config?.tcpHost ?? undefined}
        tcpPort={packetConfig.config?.tcpPort ?? undefined}
        serialDevice={packetConfig.config?.serialDevice ?? undefined}
        serialBaud={packetConfig.config?.serialBaud ?? undefined}
        btMac={packetConfig.config?.btMac ?? undefined}
        onConnect={runConnect}
        onDisconnect={onDisconnect}
        onLinkChange={onLinkChange}
      />
      <AprsChatPanel
        messages={aprs.messages}
        send={aprs.send}
        getConfig={aprs.getConfig}
        setConfig={aprs.setConfig}
        controlStrip={linkKind === 'UvproNative' ? <UvproControlStrip /> : undefined}
      />
    </div>
  );
}

// ---- Routines -----------------------------------------------------------
//
// Renders RoutinesSurface with `view` seeded from the continuity token's
// `state` (default dashboard) and registers a getContext callback reporting
// the CURRENT view back to the host, so a dock-back token round-trips the
// operator's place in the designer. Task 8 finishes this surface's wiring
// (consent-gating prop, deeper designer state).
function isRoutinesView(value: unknown): value is RoutinesView {
  if (!value || typeof value !== 'object') return false;
  const v = value as { view?: unknown };
  if (v.view === 'dashboard') return true;
  if (v.view === 'designer') {
    const d = value as { routine?: unknown; tab?: unknown };
    return (
      typeof d.routine === 'string' &&
      (d.tab === 'design' || d.tab === 'runs' || d.tab === 'settings')
    );
  }
  return false;
}

function RoutinesPopped({ context, registerGetContext }: SurfaceComponentProps) {
  const [view, setView] = useState<RoutinesView>(
    isRoutinesView(context) ? context : { view: 'dashboard' },
  );
  useEffect(() => {
    registerGetContext(() => view);
  }, [registerGetContext, view]);
  return <RoutinesSurface view={view} onNavigate={setView} />;
}

export const SURFACE_REGISTRY: Record<SurfaceId, SurfaceRegistryEntry> = {
  routines: {
    id: 'routines',
    title: 'Routines — Tuxlink',
    Component: RoutinesPopped,
    StatusStrip: RoutinesStrip,
  },
  tac_map: {
    id: 'tac_map',
    title: 'Tac Map — Tuxlink',
    Component: TacMapSurface,
    StatusStrip: TacMapStrip,
  },
  aprs_chat: {
    id: 'aprs_chat',
    title: 'APRS Chat — Tuxlink',
    Component: AprsChatSurface,
    StatusStrip: ChatStrip,
  },
};
