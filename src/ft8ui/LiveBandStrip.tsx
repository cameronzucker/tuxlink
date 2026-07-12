// LiveBandStrip.tsx — Task C7, plan tuxlink-b026z.4 §Strip + §States
// "flags-overlay layer". The collapsible "Live band · FT-8" strip along the
// panel bottom: header (backend-truth state dot + provenance/health chips +
// stats + `holding <band> ⌄` popover trigger + collapse) composing the
// already-built leaves (Waterfall C8, DecodeFeed C11, BandSubsetPopover C10).
//
// Props-driven, NOT a direct `useFt8Listener()` caller — mirrors the app's
// established "hook once at the shell, props down" shape (BandSubsetPopover's
// own doc: "AppShell calls useFt8Listener(); StationRail / LiveDecodesTab /
// DashboardRibbon all take slices as props ... the eventual strip host
// (LiveBandStrip) threads useFt8Listener().snapshot fields straight
// through"). D1 (StationFinderPanel wiring, not yet built) is the caller that
// threads `useFt8Listener()`'s `{ snapshot, decodesRing, uiState }` in as
// props; this component stays unit-testable without mounting the provider.
//
// **Flags-overlay layer (spec §States, LOAD-BEARING — this component owns
// it):** `uiState.flags` is an INDEPENDENT overlay computed by B2
// (`deriveUiState`) on top of whatever `uiState.state` the body renders —
// never a replacement:
//   - `clockUnsynced` -> an amber banner (naming chrony + the slot-alignment
//     consequence) rendered ABOVE the body, which still renders beneath it.
//   - `jt9Degraded` -> an amber dot + a chip that renders `snapshot.lastFailure`
//     verbatim (L2 stored the diagnostic there for L3 to surface, not invent).
//   - `catFixedBand` -> the OPERATOR-ASSERTED / UNCONFIRMED provenance chip
//     (combined with `snapshot.bandSource` for the full CAT-CONFIRMED /
//     OPERATOR-ASSERTED / dashed-UNCONFIRMED three-way the spec's header
//     enumerates — `catFixedBand` alone only distinguishes the CAT-absent
//     "operator asserted the band" case from a CAT-present session, which
//     itself further splits into confirmed/unconfirmed via `bandSource`).
//
// **Header dot color (CRITICAL cross-task pin):** the strip distinguishes
// severity, unlike the ribbon's coarse "amber for all blocked" reduction —
// `wedged` ALWAYS renders a RED dot (`si-dot--red`, a class distinct from
// `si-dot--amber`) plus a restart banner, taking priority over any flag.
// `needs-setup` / `device-lost` / `yielded` render amber; the three live
// phase rows (`decoding` / `waiting-first-slot` / `band-dead`) render green
// UNLESS a flag (`clockUnsynced` / `jt9Degraded`) overlays amber on top;
// `off` / `transitional` render the neutral/off dot.
//
// **Collapse + force-expand (spec §Strip "Collapse"):** collapse persists
// under the NEW ft8ui-owned localStorage key `tuxlink:ft8:strip` (via the
// shared `usePersistedState` primitive) — default expanded; auto-collapses
// below a plan-time window-height threshold. `needs-setup` / `wedged` /
// `device-lost` FORCE the strip expanded, overriding BOTH the persisted bit
// and auto-collapse (the operator must see the setup/error surface — the
// ~700px first-run window must never hide it behind a chip). The operator
// may still manually re-collapse while force-expanded; per spec that choice
// is explicitly NOT persisted — it is local, transient state scoped to the
// current force-expand episode (reset the moment the state leaves the
// force-expand set).

import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '../controls';
import { Waterfall } from './Waterfall';
import { DecodeFeed } from './DecodeFeed';
import { BandSubsetPopover } from './BandSubsetPopover';
import { stripStats } from './deriveBandActivity';
import { usePersistedState } from '../util/usePersistedState';
import type { Ft8Flags, Ft8Snapshot, Ft8UiState, SlotRecord } from './ft8Types';
import './LiveBandStrip.css';

/** States that force the strip expanded, overriding persisted collapse AND
 *  auto-collapse (spec §Strip "Force-expand override"). */
const FORCE_EXPAND_STATES: ReadonlySet<Ft8UiState> = new Set(['needs-setup', 'wedged', 'device-lost']);

/** States whose body is the LIVE composed surface (Waterfall + DecodeFeed).
 *  Every other state renders its own compact/setup body — mounting a live
 *  waterfall/feed there would either show stale data or needlessly hold a
 *  waterfall subscription token while no capture is running. */
const LIVE_BODY_STATES: ReadonlySet<Ft8UiState> = new Set([
  'decoding',
  'waiting-first-slot',
  'band-dead',
  'yielded',
]);

/** Auto-collapse threshold (spec §Strip "auto-collapse below a window-height
 *  threshold (plan-time constant)"). Deliberately BELOW the project's
 *  canonical ~700px main-window height (docs/design's "Small-height layout
 *  contract") so the default/canonical window size never auto-collapses —
 *  only a genuinely squeezed window does. */
const AUTO_COLLAPSE_MEDIA_QUERY = '(max-height: 640px)';

/** localStorage key suffix — `usePersistedState` prefixes `tuxlink:`, giving
 *  the NEW ft8ui-owned key `tuxlink:ft8:strip` the spec calls for (kept
 *  separate from the catalog-owned `PersistedFinderView` writer). */
const COLLAPSE_STORAGE_KEY = 'ft8:strip';

function isBoolean(v: unknown): v is boolean {
  return typeof v === 'boolean';
}

/** Header dot color. Exported for direct unit testing. */
export type DotTone = 'green' | 'amber' | 'red' | 'off';

/**
 * `wedged` always wins RED regardless of any flag (spec's cross-task pin —
 * the strip must distinguish severity, unlike the ribbon's coarse amber).
 * Otherwise a live/degraded-adjacent flag (`clockUnsynced` / `jt9Degraded`)
 * overlays amber on top of whatever the state's own tone would be; absent
 * those flags, the tone follows the state table directly.
 */
export function dotToneFor(state: Ft8UiState, flags: Ft8Flags): DotTone {
  if (state === 'wedged') return 'red';
  if (flags.clockUnsynced || flags.jt9Degraded) return 'amber';
  switch (state) {
    case 'decoding':
    case 'waiting-first-slot':
    case 'band-dead':
      return 'green';
    case 'needs-setup':
    case 'device-lost':
    case 'yielded':
      return 'amber';
    case 'off':
    case 'transitional':
    default:
      return 'off';
  }
}

/** `14074000` Hz -> `"14.074"` MHz — matches the approved mock's `dial
 *  <b>14.074</b>` stat (3 decimals, no unit suffix — the label carries it). */
export function formatDialMHz(dialHz: number | undefined | null): string {
  if (dialHz === undefined || dialHz === null || !Number.isFinite(dialHz)) return '—';
  return (dialHz / 1_000_000).toFixed(3);
}

// ---------------------------------------------------------------------------
// Auto-collapse-by-height — mirrors `useViewport`'s defensive matchMedia
// pattern (jsdom has no real `matchMedia`; both the initial read and the
// listener registration no-op when it is absent, never throwing).
// ---------------------------------------------------------------------------

function useAutoCollapseByHeight(): boolean {
  const [below, setBelow] = useState<boolean>(() => {
    if (typeof window === 'undefined' || !window.matchMedia) return false;
    return window.matchMedia(AUTO_COLLAPSE_MEDIA_QUERY).matches;
  });

  useEffect(() => {
    if (typeof window === 'undefined' || !window.matchMedia) return;
    const mql = window.matchMedia(AUTO_COLLAPSE_MEDIA_QUERY);
    const onChange = (e: MediaQueryListEvent) => setBelow(e.matches);
    setBelow(mql.matches);
    mql.addEventListener('change', onChange);
    return () => mql.removeEventListener('change', onChange);
  }, []);

  return below;
}

export interface LiveBandStripProps {
  /** `useFt8Listener().snapshot` — `null` before the first hydrate (loading
   *  window); the strip renders a neutral/off body until it resolves. */
  snapshot: Ft8Snapshot | null;
  /** `useFt8Listener().uiState` — the derived 9-member state plus the
   *  independent 3-flag overlay this component is responsible for rendering. */
  uiState: { state: Ft8UiState; flags: Ft8Flags };
  /** `useFt8Listener().decodesRing` — feeds DecodeFeed and (via `stripStats`)
   *  the header's decodes/min + grids-heard figures. */
  decodesRing: SlotRecord[];
  /** The active blocking modem session's mode (e.g. "VARA"), threaded
   *  through unchanged to `BandSubsetPopover`'s `blockingSessionMode` prop
   *  (spec §NewCommands "<mode>" interpolation) — D1 supplies this from the
   *  app's active-modem state. Optional; degrades to BandSubsetPopover's own
   *  "another session" fallback when omitted. */
  blockingSessionMode?: string;
  /**
   * Optional slot for the full `needs-setup` setup surface (Ft8SetupSurface,
   * Task C9a/C9b — in flight concurrently with this task, so this component
   * does not import it directly). When omitted, `needs-setup` renders a
   * minimal placeholder body instead of a blank/absent one, so the component
   * stays self-consistent standalone; D1 is expected to pass
   * `<Ft8SetupSurface .../>` here once both land.
   */
  setupSurface?: ReactNode;
  /** `device-lost`'s compact body carries a "pick another input" link that
   *  opens the full setup surface (spec §States row 6b) — the parent decides
   *  what that means (e.g. force-render Ft8SetupSurface). Optional; the link
   *  simply has nothing to do when omitted. */
  onOpenFullSetup?: () => void;
  /** Injectable "now" for deterministic stats tests; defaults to
   *  `Date.now()` (mirrors `LiveDecodesTab`'s `nowMs` convention). */
  nowMs?: number;
}

export function LiveBandStrip({
  snapshot,
  uiState,
  decodesRing,
  blockingSessionMode,
  setupSurface,
  onOpenFullSetup,
  nowMs,
}: LiveBandStripProps) {
  const { state, flags } = uiState;

  // ---- Collapse: persisted + auto-collapse + force-expand override -------
  const [persistedCollapsed, setPersistedCollapsed] = usePersistedState<boolean>(
    COLLAPSE_STORAGE_KEY,
    false,
    isBoolean,
  );
  const autoCollapseActive = useAutoCollapseByHeight();
  const forceExpand = FORCE_EXPAND_STATES.has(state);

  // Transient, NEVER persisted — scoped to one force-expand episode (spec:
  // "The operator may re-collapse during those states; that choice is not
  // persisted"). Reset whenever a force-expand episode begins.
  const [forceExpandOverrideCollapsed, setForceExpandOverrideCollapsed] = useState(false);
  useEffect(() => {
    if (forceExpand) setForceExpandOverrideCollapsed(false);
  }, [forceExpand]);

  const collapsed = forceExpand ? forceExpandOverrideCollapsed : persistedCollapsed || autoCollapseActive;

  const handleCollapseToggle = useCallback(() => {
    if (forceExpand) {
      setForceExpandOverrideCollapsed((v) => !v);
      return;
    }
    setPersistedCollapsed(!persistedCollapsed);
  }, [forceExpand, persistedCollapsed, setPersistedCollapsed]);

  // ---- Band-subset popover (Flow 1: "limit FT-8 decode to a subset of
  // bands or only one band") — Esc / outside-click close, mirroring the
  // repo's established context-menu idiom (MessageContextMenu.tsx). --------
  const [popoverOpen, setPopoverOpen] = useState(false);
  const popoverAnchorRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!popoverOpen) return undefined;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') setPopoverOpen(false);
    }
    function onMouseDown(e: MouseEvent) {
      const node = popoverAnchorRef.current;
      if (node && !node.contains(e.target as Node)) setPopoverOpen(false);
    }
    document.addEventListener('keydown', onKey);
    document.addEventListener('mousedown', onMouseDown);
    return () => {
      document.removeEventListener('keydown', onKey);
      document.removeEventListener('mousedown', onMouseDown);
    };
  }, [popoverOpen]);

  // ---- Off-state CTA -------------------------------------------------------
  const [starting, setStarting] = useState(false);
  const handleStart = useCallback(() => {
    setStarting(true);
    invoke('ft8_listener_start')
      .catch(() => {
        // Backend truth: a failed start leaves `state` as-is; the next
        // snapshot/change event reflects whatever actually happened.
      })
      .finally(() => setStarting(false));
  }, []);

  // ---- Derived display values ----------------------------------------------
  const band = snapshot?.band ?? '—';
  const effectiveNowMs = nowMs ?? Date.now();
  const { decodesPerMin, gridsHeard } = snapshot
    ? stripStats(decodesRing, snapshot.band, effectiveNowMs)
    : { decodesPerMin: 0, gridsHeard: 0 };

  const dotTone = dotToneFor(state, flags);
  const showLiveBody = LIVE_BODY_STATES.has(state);
  const sweepPaused = snapshot?.sweep.mode === 'fallback-hold';

  return (
    <div className="si-strip" data-testid="ft8-strip" data-state={state} data-collapsed={collapsed}>
      <div className="si-strip__hdr">
        <span
          className={`si-dot${dotTone === 'green' ? '' : ` si-dot--${dotTone}`}`}
          data-testid="ft8-strip-dot"
          data-tone={dotTone}
        />
        <span className="si-strip__title">Live band · FT-8</span>

        <span className="si-strip__chips">
          {snapshot && (
            <ProvenanceChip
              catFixedBand={flags.catFixedBand}
              bandSource={snapshot.bandSource}
              dialHz={snapshot.dialHz}
            />
          )}
          {flags.clockUnsynced && (
            <span className="si-prov si-prov--warn" data-testid="ft8-strip-chip-clock-unsynced">
              CLOCK UNSYNCED
            </span>
          )}
          {flags.jt9Degraded && (
            <span
              className="si-prov si-prov--warn"
              data-testid="ft8-strip-chip-jt9-degraded"
              title={snapshot?.lastFailure ?? undefined}
            >
              JT9 DEGRADED{snapshot?.lastFailure ? ` — ${snapshot.lastFailure}` : ''}
            </span>
          )}
          {state === 'yielded' && (
            <span className="si-prov si-prov--warn" data-testid="ft8-strip-chip-yielded">
              YIELDED
            </span>
          )}
          {state === 'needs-setup' && (
            <span className="si-prov si-prov--warn" data-testid="ft8-strip-chip-needs-setup">
              NEEDS SETUP
            </span>
          )}
          {sweepPaused && (
            <span className="si-prov si-prov--warn" data-testid="ft8-strip-chip-sweep-paused">
              SWEEP PAUSED — radio not responding
            </span>
          )}
        </span>

        <span className="si-strip__stats" data-testid="ft8-strip-stats">
          <span>
            holding <b>{band}</b>
          </span>
          <span>
            dial <b>{formatDialMHz(snapshot?.dialHz)}</b>
          </span>
          <span>
            <b>{Math.round(decodesPerMin)}</b> decodes/min
          </span>
          <span>
            <b>{gridsHeard}</b> grids heard
          </span>
        </span>

        <div className="si-strip__popover-anchor" ref={popoverAnchorRef}>
          <Button
            tone="neutral"
            emphasis="outline"
            size="sm"
            className="si-strip__holding-trigger"
            data-testid="ft8-strip-holding-trigger"
            aria-expanded={popoverOpen}
            disabled={snapshot === null}
            onClick={() => setPopoverOpen((v) => !v)}
          >
            holding {band} ⌄
          </Button>
          {popoverOpen && snapshot && (
            <div className="si-strip__popover" data-testid="ft8-strip-popover">
              <BandSubsetPopover
                sweepConfig={snapshot.sweepConfig}
                heldBand={snapshot.band}
                isListening={snapshot.service.axis === 'listening'}
                fallbackHold={sweepPaused}
                blockingSessionMode={blockingSessionMode}
              />
            </div>
          )}
        </div>

        <Button
          tone="neutral"
          emphasis="outline"
          size="sm"
          className="si-collapse"
          data-testid="ft8-strip-collapse"
          aria-expanded={!collapsed}
          onClick={handleCollapseToggle}
        >
          {collapsed ? '⌃ expand' : '⌄ collapse'}
        </Button>
      </div>

      {!collapsed && (
        <>
          {flags.clockUnsynced && (
            <div className="si-banner si-banner--warn" data-testid="ft8-strip-banner-clock-unsynced">
              ⚠{' '}
              <span>
                <b>System clock is not synchronized.</b> FT-8 slots are UTC-aligned — sync
                chrony/NTP, or decodes may misalign to the wrong slot until time sync returns.
              </span>
            </div>
          )}
          {state === 'wedged' && (
            <div className="si-banner si-banner--err" data-testid="ft8-strip-banner-wedged">
              ⚠ <span>Audio capture is wedged — restart Tuxlink.</span>
            </div>
          )}

          <div className="si-strip__body" data-testid="ft8-strip-body">
            {showLiveBody ? (
              <>
                <div className="si-wf">
                  <Waterfall expanded={!collapsed} />
                  {state === 'yielded' && (
                    <div className="si-wf__overlay" data-testid="ft8-strip-yielded-overlay">
                      ⏸ &nbsp;Session active — FT-8 listening yielded, resumes when it ends
                    </div>
                  )}
                  <div className="si-wf__axis">
                    <span>0</span>
                    <span>1000</span>
                    <span>2000</span>
                    <span>3000 Hz</span>
                  </div>
                </div>
                <div className="si-feed">
                  <DecodeFeed decodesRing={decodesRing} />
                </div>
              </>
            ) : (
              <NonLiveBody
                state={state}
                snapshot={snapshot}
                starting={starting}
                onStart={handleStart}
                setupSurface={setupSurface}
                onOpenFullSetup={onOpenFullSetup}
              />
            )}
          </div>
        </>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Provenance chip — CAT CONFIRMED / OPERATOR-ASSERTED / dashed UNCONFIRMED
// (spec §Strip header enumeration). `catFixedBand` (the flags-overlay field
// this task owns) names the CAT-absent "operator fixed the band" case
// directly; when CAT IS present, `bandSource` distinguishes a confirmed read
// from one still awaiting the operator's dial match.
// ---------------------------------------------------------------------------

function ProvenanceChip({
  catFixedBand,
  bandSource,
  dialHz,
}: {
  catFixedBand: boolean;
  bandSource: Ft8Snapshot['bandSource'];
  dialHz: number;
}) {
  if (catFixedBand) {
    return (
      <span className="si-prov si-prov--warn" data-testid="ft8-strip-chip-band-provenance">
        OPERATOR-ASSERTED
      </span>
    );
  }
  if (bandSource === 'default-unconfirmed') {
    return (
      <span className="si-prov si-prov--unconf" data-testid="ft8-strip-chip-band-provenance">
        UNCONFIRMED — tune your dial to {formatDialMHz(dialHz)}
      </span>
    );
  }
  return (
    <span className="si-prov" data-testid="ft8-strip-chip-band-provenance">
      CAT CONFIRMED
    </span>
  );
}

// ---------------------------------------------------------------------------
// Non-live-body states — off / transitional / needs-setup / device-lost.
// `wedged`'s copy lives entirely in the banner above; this renders nothing
// extra for it (avoids duplicating the same restart copy twice).
// ---------------------------------------------------------------------------

function NonLiveBody({
  state,
  snapshot,
  starting,
  onStart,
  setupSurface,
  onOpenFullSetup,
}: {
  state: Ft8UiState;
  snapshot: Ft8Snapshot | null;
  starting: boolean;
  onStart: () => void;
  setupSurface?: ReactNode;
  onOpenFullSetup?: () => void;
}) {
  switch (state) {
    case 'off':
      return (
        <div className="si-strip__notice" data-testid="ft8-strip-body-off">
          <p>Not listening.</p>
          <Button
            tone="primary"
            emphasis="solid"
            size="sm"
            data-testid="ft8-strip-start-cta"
            disabled={starting}
            onClick={onStart}
          >
            {starting ? 'Starting…' : `Start listening on ${snapshot?.band ?? '20m'} →`}
          </Button>
        </div>
      );
    case 'transitional':
      return (
        <div className="si-strip__notice" data-testid="ft8-strip-body-transitional">
          {snapshot?.service.axis === 'stopping' ? 'Stopping…' : 'Starting…'}
        </div>
      );
    case 'needs-setup':
      return (
        <div className="si-strip__notice" data-testid="ft8-strip-body-needs-setup">
          {setupSurface ?? (
            <p>Setup required — select an audio input (and, optionally, configure CAT) to start listening.</p>
          )}
        </div>
      );
    case 'device-lost':
      return (
        <div className="si-strip__notice" data-testid="ft8-strip-body-device-lost">
          <p>
            Device disconnected — reconnecting…{' '}
            <Button
              tone="neutral"
              emphasis="outline"
              size="xs"
              data-testid="ft8-strip-device-lost-link"
              onClick={() => onOpenFullSetup?.()}
            >
              pick another input
            </Button>
          </p>
        </div>
      );
    case 'wedged':
    default:
      // wedged's copy is the banner above (si-banner--err); nothing further
      // to render here for it. Any unreached default is defensive only —
      // every Ft8UiState is handled above or is a LIVE_BODY_STATES member.
      return null;
  }
}
