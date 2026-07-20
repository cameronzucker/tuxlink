// src/dock/strips.tsx — mini status strips for a popped surface's own OS
// window (spec §4 "chrome option B"). Each strip shows that surface's own
// vitals, never a vital the surface component already renders in the same
// window (adrev R4-F8): the Tac Map's plotted-station total is already shown
// by AprsLayersPanel's "All stations" row (src/aprs/AprsLayersPanel.tsx:73),
// so TacMapStrip omits it and shows only the live-ticking last-packet age.
//
// Each strip mounts its OWN hook instance, independent of the surface
// component's — every hook here (useAprsPositions, useAprsChat, useRoutines,
// useParkedRuns) is a plain listen()-based subscription, not a singleton, so
// two live instances in one window is the same pattern the app already uses
// (e.g. useEnvStations({snapshotRole}) hosts + clients).
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useAprsPositions } from '../aprs/useAprsPositions';
import { useAprsChat } from '../aprs/useAprsChat';
import { countUnread } from '../aprs/aprsUnread';
import { useRoutines } from '../routines/useRoutines';
import { useParkedRuns } from '../routines/ConsentGate';
import { listRuns, type RunState } from '../routines/routinesApi';
import { listenRoutinesEvents } from '../routines/routinesEvents';
import { formatUtc } from '../routines/format';
import { useEgressArm } from '../security/useEgressArm';
import { EV_OUTCOME } from '../elmer/elmerEvents';
import type { ConfigReadDto } from '../elmer/elmerModelConfig';
import './PoppedSurfaceHost.css';

/** Non-terminal `RunState`s — mirrors RoutinesDashboard.tsx's LIVE_STATES. */
const LIVE_RUN_STATES = new Set<RunState>([
  'pending', 'running', 'waiting', 'awaiting_consent', 'awaiting_radio',
]);
/** Routines events that can move the live-run count — mirrors the subset
 *  RoutinesDashboard.tsx re-fetches `listRuns()` on. */
const RUN_COUNT_EVENTS = new Set([
  'runStarted', 'stateChanged', 'awaitingConsent', 'runFinished', 'scheduledFire',
]);

/** Count of currently-live (non-terminal) runs, refreshed on mount and on
 *  every run-lifecycle event. A standalone instance (no debounce) — this is
 *  a peripheral badge, not the full runs table, so a burst of re-fetches
 *  during a busy run is an acceptable trade for staying under the strip's
 *  line budget. */
function useRunningCount(): number {
  const [count, setCount] = useState(0);
  useEffect(() => {
    let mounted = true;
    const refresh = () => {
      listRuns()
        .then((runs) => {
          if (mounted) setCount(runs.filter((r) => LIVE_RUN_STATES.has(r.state)).length);
        })
        .catch(() => {});
    };
    refresh();
    let unlisten: (() => void) | null = null;
    listenRoutinesEvents((e) => {
      if (RUN_COUNT_EVENTS.has(e.kind)) refresh();
    })
      .then((u) => { if (mounted) unlisten = u; else u(); })
      .catch(() => {});
    return () => { mounted = false; unlisten?.(); };
  }, []);
  return count;
}

export function RoutinesStrip() {
  const { parked } = useParkedRuns();
  const running = useRunningCount();
  const { nextFires } = useRoutines();
  const fires = Object.values(nextFires);
  const soonest = fires.length > 0 ? Math.min(...fires) : null;
  return (
    <div className="pop-strip" data-testid="pop-strip-routines">
      <span className="pop-strip-item">{parked.length} parked</span>
      <span className="pop-strip-divider" aria-hidden="true">·</span>
      <span className="pop-strip-item">{running} running</span>
      <span className="pop-strip-divider" aria-hidden="true">·</span>
      <span className="pop-strip-item">
        {soonest !== null ? `next ${formatUtc(soonest)}` : 'no scheduled fire'}
      </span>
    </div>
  );
}

/** `now` ticks every second so a frozen "2 min ago" never misleads about
 *  channel liveness (spec §4). */
function useNowTick(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);
  return now;
}

function formatAge(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  if (s < 60) return `${s}s ago`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  return `${Math.floor(m / 60)}h ago`;
}

export function TacMapStrip() {
  // Rider B (Task 9 review): seed from the host snapshot (spec §7) — a bare
  // useAprsPositions() would show "no packets heard" beside a live-seeded map,
  // a false-liveness signal (spec §4 violation class).
  const { positions } = useAprsPositions({ snapshotRole: 'client' });
  const now = useNowTick();
  const newest = positions.reduce<number | null>(
    (max, p) => (max === null || p.at > max ? p.at : max),
    null,
  );
  return (
    <div className="pop-strip" data-testid="pop-strip-tac-map">
      <span className="pop-strip-item">
        {newest !== null ? `last packet ${formatAge(now - newest)}` : 'no packets heard'}
      </span>
    </div>
  );
}

export function ChatStrip() {
  // Rider B: same seeding as TacMapStrip — the last-heard vital seeds from the
  // host snapshot (spec §7) so a fresh pop-out window doesn't read "no stations
  // heard" beside a seeded chat feed.
  const { messages, heardStations } = useAprsChat({ snapshotRole: 'client' });
  const lastHeard = heardStations[0];

  // Unread reuses AppShell's REAL mechanism — countUnread (src/aprs/aprsUnread.ts:
  // inbound messages past a seen-watermark), the same helper AppShell drives for
  // the in-dock unread badge.
  //
  // Watermark semantics in THIS window: the popped window's AprsChatPanel renders
  // this same live feed directly above the strip, so the operator is always
  // "viewing" the chat here — the honest analogue of AppShell's
  // `viewingAprsChat === true` branch, which likewise reports 0. AppShell's own
  // seen-watermark is dock-open-keyed local state that never travels to a popped
  // window (spec §7 continuity is feed-only), and a cross-window "messages since
  // this window lost focus" signal is not plumbed. So, exactly as AppShell does
  // while the chat tab is the active view, we advance the watermark on every
  // arriving message; countUnread then honestly reports 0 while the popped feed
  // is on screen. A true-but-boring 0 beats a fabricated count (no-stubs rule),
  // and advancing (rather than a fixed mount watermark) avoids the very
  // climbs-forever bug AppShell fixed for the always-visible strip count.
  const [seenAt, setSeenAt] = useState(() => Date.now());
  useEffect(() => {
    setSeenAt(Date.now());
  }, [messages.length]);
  const unread = countUnread(messages, seenAt);

  return (
    <div className="pop-strip" data-testid="pop-strip-chat">
      <span className="pop-strip-item">
        {lastHeard ? `last heard ${lastHeard.call}` : 'no stations heard'}
      </span>
      <span className="pop-strip-divider" aria-hidden="true">·</span>
      <span className="pop-strip-item" data-testid="pop-strip-chat-unread">{unread} unread</span>
    </div>
  );
}

/** bd tuxlink-mfssz: the popped Elmer window's vitals — the configured
 *  model @ endpoint origin, and the send-authority (egress-arm) state. Both
 *  are vitals the pane does NOT render as standing chrome in the same
 *  window (adrev R4-F8 rule): the model shows in-pane only transiently (an
 *  attribution chip on change), and the arm chip's countdown detail lives
 *  in a popover. Config re-reads on every EV_OUTCOME so a mid-conversation
 *  model save (which lands before the next turn) is reflected by the time
 *  that turn completes. */
export function ElmerStrip() {
  const { status } = useEgressArm();
  const [config, setConfig] = useState<ConfigReadDto | null>(null);

  useEffect(() => {
    let mounted = true;
    const read = () => {
      invoke<ConfigReadDto>('elmer_config_read')
        .then((dto) => {
          if (mounted) setConfig(dto);
        })
        .catch(() => {});
    };
    read();
    let unlisten: (() => void) | null = null;
    listen(EV_OUTCOME, read)
      .then((u) => { if (mounted) unlisten = u; else u(); })
      .catch(() => {});
    // Adrev 2026-07-20 (both rounds, P2): a model saved from the popped pane
    // while IDLE emits no event this strip can hear, so the standing chrome
    // would contradict the active config until the next turn. The 10s
    // re-read matches the sidebar badge cadence (cheap config-file read).
    const interval = setInterval(read, 10_000);
    return () => { mounted = false; clearInterval(interval); unlisten?.(); };
  }, []);

  const origin = (() => {
    if (!config?.agentEndpoint) return null;
    try { return new URL(config.agentEndpoint).host; } catch { return config.agentEndpoint; }
  })();

  return (
    <div className="pop-strip" data-testid="pop-strip-elmer">
      <span className="pop-strip-item">
        {config?.agentModel ? `${config.agentModel}${origin ? ` @ ${origin}` : ''}` : 'no model configured'}
      </span>
      <span className="pop-strip-divider" aria-hidden="true">·</span>
      <span className="pop-strip-item" data-testid="pop-strip-elmer-arm">
        {status?.tainted
          ? 'send authority tainted'
          : status?.armed
            ? `send authority armed ${status.armedRemainingSecs}s`
            : 'send authority off'}
      </span>
    </div>
  );
}
