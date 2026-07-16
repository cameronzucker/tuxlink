// src/dock/surfaceRegistry.tsx — the three-entry surface registry
// PoppedSurfaceHost mounts from (spec §4). bd tuxlink-dmwte task 7.
//
// Deliberate deviation from the spec §4 registry sketch: NO `defaultSize`
// field here — first-spawn sizes live Rust-side in `pop_window_spec`
// (backend Task 3). Do not "restore" it.
import { useCallback, useEffect, useRef, useState, type ComponentType } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
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
import { isRoutinesView, type RoutinesTokenState } from '../routines/routinesToken';
import type { RoutineDef } from '../routines/routinesApi';
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
   *  host's ref stays null.
   *
   *  Re-registration contract (review-loop-3 F3): register a FRESH closure
   *  whenever the state it reports changes — call `registerGetContext`
   *  inside a `useEffect` whose deps include that state, not once with `[]`.
   *  A `[]`-deps registration captures the value live at mount time in its
   *  closure forever, so every later dock-back would ship that stale
   *  mount-time snapshot as `state` instead of the surface's current place.
   *  See `RoutinesPopped` below for the canonical pattern — it already does
   *  this correctly (`useEffect(() => registerGetContext(() => view), [registerGetContext, view])`). */
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
// Renders RoutinesSurface seeded from the continuity token's `state` half
// (spec §7). Token-shape contract (tuxlink-dmwte task 8): the registry stores
// the FULL envelope `{ foreground, state }` per surface; `PoppedSurfaceHost`
// UNWRAPS `.state` and passes it here as `context`, so `context` is the bare
// token state `{ view, draft }` (or null) — matching this file's
// `SurfaceComponentProps.context` doc ("the token's `state` half"). The
// getContext callback reports that same bare `{ view, draft }` shape back; the
// host re-wraps it in the envelope on every dock-back.
//
// The live draft is held in a ref (not state): getContext reads it at
// dock-back time, so a keystroke-frequency re-registration is unnecessary —
// re-registering only on `view` change keeps the reported closure fresh for
// the value that DOES need capture (the re-registration contract above), while
// the ref supplies the always-current draft.
function RoutinesPopped({ context, registerGetContext }: SurfaceComponentProps) {
  const seed = (context ?? null) as RoutinesTokenState | null;
  const [view, setView] = useState<RoutinesView>(
    seed && isRoutinesView(seed.view) ? seed.view : { view: 'dashboard' },
  );
  // The seed draft is consumed by the FIRST (token) designer render only; any
  // navigation clears it so re-opening a routine fetches fresh rather than
  // re-seeding from the stale popped-in draft.
  const [seedDraft, setSeedDraft] = useState<RoutineDef | undefined>(seed?.draft);
  const draftRef = useRef<RoutineDef | undefined>(seed?.draft);

  const onNavigate = useCallback((next: RoutinesView) => {
    setSeedDraft(undefined);
    setView(next);
  }, []);

  useEffect(() => {
    registerGetContext(() => ({ view, draft: draftRef.current }) satisfies RoutinesTokenState);
  }, [registerGetContext, view]);

  // Cross-window menu-verb forwarding (spec §5, adrev R4-F6): a "New Routine…"
  // menu click on MAIN while Routines is popped focuses this window and emits
  // `dock:intent`; the popped surface forwards it to the dashboard's existing
  // new-routine entry point (a fresh, unsaved designer draft).
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    listen<{ surface: SurfaceId; intent: string }>('dock:intent', (event) => {
      if (event.payload.surface === 'routines' && event.payload.intent === 'new_routine') {
        onNavigate({ view: 'designer', routine: '', tab: 'design' });
      }
    })
      .then((u) => {
        if (disposed) u();
        else unlisten = u;
      })
      .catch(() => {
        // No Tauri runtime (test/dev harness).
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [onNavigate]);

  return (
    <RoutinesSurface
      view={view}
      onNavigate={onNavigate}
      initialDraft={seedDraft}
      onDraftChange={(d) => {
        draftRef.current = d;
      }}
    />
  );
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
