// Right rail for Find-a-Station (design §7): a `Station | Live decodes` tab
// shell (plan tuxlink-b026z.4 Task C5, spec §Rail) fronting two panes:
//   - Station tab (existing behavior, preserved verbatim): selected-station
//     header → antenna aiming hero → path propagation forecast → channels
//     grouped by mode/frequency with per-channel reliability + Use →.
//   - Live decodes tab (`LiveDecodesTab.tsx`, new): station-centric
//     aggregation over the FT8 decode ring, independent of the map selection.
// Use → emits emitGatewayPrefill for a channel matching the open modem; other
// channels are listed but their Use → is disabled with a hint (RADIO-1: this
// only fills a form — the operator still clicks Connect).

import { useEffect, useState, type CSSProperties } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { BandMatrix } from './BandMatrix';
import { distanceFromGrids, kmToMi } from './distance';
import { gridToLatLon, type LatLon } from '../forms/position/maidenhead';
import { LiveDecodesTab } from './LiveDecodesTab';
import { useStatusData } from '../shell/useStatus';
import type { Station } from './stationModel';
import type { PathPrediction } from './propagationApi';
import type { PredictionStatus } from './useStationPrediction';
import type { RadioMode, FavoriteDial } from '../favorites/types';
import type { SlotRecord, DeclDto, BandDot } from '../ft8ui/ft8Types';

export interface StationRailProps {
  station: Station | null;
  prediction: PathPrediction | null;
  predictionStatus: PredictionStatus;
  operatorGrid: string;
  utcHour: number;
  /** The open modem that can consume a prefill, or undefined if none. */
  activePrefillMode?: RadioMode;
  /**
   * Handle "Use →" for a channel. AppShell arms (opens) the matching modem panel
   * on demand and then prefills it, so the operator does not have to open the
   * modem first (tuxlink-s0r1). When omitted, falls back to emitting the prefill
   * for an already-open panel. `candidates` is the Find-a-Station ranked list of
   * the station's other channels for this mode (tuxlink-8fkkk Task B) — the
   * panel sends it as `qsyCandidates` for the backend QSY-on-fail walk.
   */
  onUse?: (dial: FavoriteDial, candidates?: FavoriteDial[]) => void;
  /**
   * Save / unsave a discovered channel as a starred favorite (tuxlink-5016).
   * The parent finds-or-creates the per-mode favorite and toggles its star;
   * omitting the prop hides the affordance (e.g. a standalone harness).
   */
  onSaveFavorite?: (dial: FavoriteDial) => void;
  /** Whether a channel's dial is already a STARRED favorite (drives the ★ fill). */
  isSaved?: (dial: FavoriteDial) => boolean;
  /**
   * The FT8 decode ring backing the "Live decodes" tab (Task B1's
   * `useFt8Listener().decodesRing`). C5's scope is the tab shell + the
   * LiveDecodesTab component only — the caller wires the live hook value in
   * (Task D1, "wire the panel body"); a caller that omits this (e.g. today's
   * StationFinderPanel, or a harness/test) sees the tab's empty state rather
   * than a crash.
   */
  decodesRing?: SlotRecord[];
  /**
   * Pan the map to a station's grid-derived coordinate — fired by a Live
   * decodes row click AFTER the row's grid clears the null-guarded
   * `gridToLatLon` (a malformed/garbage over-the-air grid never reaches this
   * callback). Omitted ⇒ the click still computes but has nowhere to act
   * (Task D1 wires the real map pan); never throws either way.
   */
  onPanToGrid?: (ll: LatLon) => void;
  /**
   * Live per-band FT-8 openness dots (design §Openness) for the Station tab's
   * BandMatrix rows (tuxlink-b026z.4 Task C3) — `useFt8Listener().bandActivity`,
   * itself Task B3's `deriveBandActivity` output. Optional so BandMatrix stays
   * presentational pre-D1-wiring; omitting it renders every eligible row with
   * a hollow no-data dot.
   */
  bandActivity?: Map<string, BandDot>;
}

const MODE_LABEL: Record<string, string> = {
  'vara-hf': 'VARA HF',
  'ardop-hf': 'ARDOP HF',
  packet: 'Packet',
  pactor: 'Pactor',
  'robust-packet': 'Robust Packet',
};

/** Great-circle bearing from two grids (deg), for the distance-only fallback.
 *  Exported for `LiveDecodesTab`'s mi·brg column (same operator↔station math,
 *  now also applied to a heard station's grid instead of the selected one). */
export function bearingFromGrids(a: string, b: string): number | null {
  const pa = gridToLatLon(a);
  const pb = gridToLatLon(b);
  if (!pa || !pb) return null;
  const lat1 = (pa.lat * Math.PI) / 180;
  const lat2 = (pb.lat * Math.PI) / 180;
  const dLon = ((pb.lon - pa.lon) * Math.PI) / 180;
  const y = Math.sin(dLon) * Math.cos(lat2);
  const x = Math.cos(lat1) * Math.sin(lat2) - Math.sin(lat1) * Math.cos(lat2) * Math.cos(dLon);
  return ((Math.atan2(y, x) * 180) / Math.PI + 360) % 360;
}

/** `true - decl` (declination east-positive), normalized to [0, 360) and
 *  rounded to a whole degree for display. Compass convention: an exact-0
 *  wrap renders as `360°`, never `0°` (a compass rose has no 0 tick — spec
 *  §Declination). */
function magneticBearing(trueDeg: number, declDeg: number): number {
  const wrapped = (((trueDeg - declDeg) % 360) + 360) % 360;
  const rounded = Math.round(wrapped) % 360;
  return rounded === 0 ? 360 : rounded;
}

/** The aim hero's declination provenance line (spec §Declination example:
 *  `declination +9.7° E · WMM2025 · from <operator grid> · updates with your
 *  location`), with a drift note appended when the model's `validUntil` has
 *  passed — the hero still renders the (now-stale) value, never blanks. */
function declProvenance(decl: DeclDto, grid: string): string {
  const dir = decl.declDeg >= 0 ? 'E' : 'W';
  const sign = decl.declDeg >= 0 ? '+' : '-';
  const mag = Math.abs(decl.declDeg).toFixed(1);
  const base = `declination ${sign}${mag}° ${dir} · ${decl.modelEpoch} · from ${grid} · updates with your location`;
  const validMs = Date.parse(decl.validUntil);
  const expired = Number.isFinite(validMs) && validMs < Date.now();
  return expired ? `${base} · model expired — declination may drift ~0.1°/yr` : base;
}

type RailTab = 'station' | 'live';

export function StationRail(props: StationRailProps) {
  const { decodesRing = [], operatorGrid, onPanToGrid } = props;
  const [tab, setTab] = useState<RailTab>('station');

  return (
    <div className="station-finder__rail">
      <div className="station-finder__railtabs" role="tablist" aria-label="Station rail view">
        <button
          type="button"
          role="tab"
          aria-selected={tab === 'station'}
          className={`station-finder__railtab${tab === 'station' ? ' is-active' : ''}`}
          data-testid="rail-tab-station"
          onClick={() => setTab('station')}
        >
          Station
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={tab === 'live'}
          className={`station-finder__railtab${tab === 'live' ? ' is-active' : ''}`}
          data-testid="rail-tab-live"
          onClick={() => setTab('live')}
        >
          Live decodes
        </button>
      </div>
      {tab === 'station' ? (
        <StationTabPane {...props} />
      ) : (
        <LiveDecodesTab decodesRing={decodesRing} operatorGrid={operatorGrid} onPanTo={onPanToGrid} />
      )}
    </div>
  );
}

/** The Station tab's content — unchanged from pre-tab-shell StationRail behavior. */
function StationTabPane(props: StationRailProps) {
  const { station, prediction, predictionStatus, operatorGrid, utcHour } = props;

  // Task C6 (aim hero + magnetic declination, spec §Declination): the LIVE
  // operator grid — useStatusData().grid, NOT config_read (the tuxlink-fnzr
  // bug class: a one-shot config read misses a GPS fix that arrives after
  // mount). Declination depends only on the operator's OWN position, not on
  // the selected station, so this runs unconditionally (before the `!station`
  // early return below) — by the time a station is picked, the declination is
  // usually already resolved, no per-selection fetch latency.
  const { grid: liveGrid } = useStatusData();
  const [decl, setDecl] = useState<DeclDto | null>(null);

  useEffect(() => {
    let cancelled = false;
    if (!liveGrid) {
      setDecl(null);
      return;
    }
    invoke<DeclDto>('magnetic_declination', { grid: liveGrid })
      .then((dto) => {
        if (cancelled) return;
        // Defensive shape check: a test double or a future backend contract
        // drift that resolves something other than a real DeclDto degrades
        // the same as an explicit error, rather than rendering "NaN° M".
        setDecl(dto && typeof dto.declDeg === 'number' ? dto : null);
      })
      .catch(() => {
        // invalid-grid / internal-error (Ft8CmdError), or no Tauri context
        // (tests/dev browser) — degrade to the plain true-bearing display;
        // never throw, never spin.
        if (!cancelled) setDecl(null);
      });
    return () => {
      cancelled = true;
    };
  }, [liveGrid]);

  if (!station) {
    return (
      <div className="station-finder__rail--empty" data-testid="rail-pane-station-empty">
        Select a station on the map.
      </div>
    );
  }

  const bearing = prediction?.bearingDeg ?? (operatorGrid ? bearingFromGrids(operatorGrid, station.grid) : null);
  const distKm = prediction?.distanceKm ?? (operatorGrid ? distanceFromGrids(operatorGrid, station.grid) : null);
  const distMi = distKm != null ? Math.round(kmToMi(distKm)) : null;

  return (
    <div className="station-finder__railpane" data-testid="rail-pane-station">
      <header className="station-finder__sta">
        <div className="station-finder__sta-top">
          <span className="station-finder__call">{station.baseCallsign}</span>
          <span className="station-finder__badges">
            {station.modes.map((m) => (
              <span key={m} className="station-finder__mb">
                <span className={`station-finder__sw station-finder__sw--${m}`} />
                {MODE_LABEL[m] ?? m}
              </span>
            ))}
          </span>
        </div>
        <div className="station-finder__who">
          {[station.sysopName, station.location, station.grid].filter(Boolean).join(' · ')}
        </div>
      </header>

      {/* Task C6 (aim hero + magnetic declination, spec §Declination): compass
          needle stays TRUE-referenced (matches the map); the numeric readout
          prefers magnetic (what a compass shows) once declination resolves,
          falling back to the plain true bearing while decl is unavailable
          (no live grid, still loading, or a degraded invoke) — never blanks a
          bearing the rail already has. */}
      <div className="station-finder__aim">
        <div className="station-finder__aimrow">
          <div
            className="station-finder__compass"
            style={bearing != null ? ({ ['--bearing']: `${bearing}deg` } as CSSProperties) : undefined}
            aria-hidden
          >
            <span className="station-finder__needle" />
          </div>
          <div>
            <div className="station-finder__big" data-testid="aim-bearing">
              {bearing == null
                ? '—'
                : decl != null
                  ? `${magneticBearing(bearing, decl.declDeg)}° M`
                  : `${Math.round(bearing)}°`}
            </div>
            {bearing != null && decl != null && (
              <div className="station-finder__aim-true" data-testid="aim-bearing-true">
                {Math.round(bearing)}° T
              </div>
            )}
            <div className="station-finder__lab">aim antenna</div>
          </div>
          <div className="station-finder__dist" data-testid="aim-distance">
            <div className="station-finder__big">{distMi != null ? `${distMi} mi` : '—'}</div>
            <div className="station-finder__lab">short path</div>
          </div>
        </div>
        {bearing != null && decl != null && liveGrid && (
          <div className="station-finder__aim-decl" data-testid="aim-declination">
            {declProvenance(decl, liveGrid)}
          </div>
        )}
      </div>

      {/* Task C3 (BandMatrix mount, spec §Rail Station tab): one row per HF
          band + VHF — openness dot · VOACAP bar+% · dial chips. Supersedes
          the pre-C3 "path forecast" bars + "channels grouped by mode" list
          (moved into BandMatrix verbatim — see BandMatrix.tsx for the
          rankedDialsFor/channelToDial + candidates[0] + sibling-☆ contracts). */}
      <BandMatrix
        station={station}
        prediction={prediction}
        predictionStatus={predictionStatus}
        utcHour={utcHour}
        bandActivity={props.bandActivity}
        onUse={props.onUse}
        onSaveFavorite={props.onSaveFavorite}
        isSaved={props.isSaved}
      />
    </div>
  );
}
