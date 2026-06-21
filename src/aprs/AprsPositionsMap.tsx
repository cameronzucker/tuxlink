// APRS Tac Chat positions map (tuxlink-6vgt; migrated to Leaflet in tuxlink-6kdw).
// Plots the positions of stations HEARD on the open channel — one pin per station
// at its decoded lat/lon, with a callsign label and a comment popup. RF-honesty:
// every pin is a real, decoded fix (no estimated locations); a station appears
// only after its beacon is heard.
//
// RF-honesty refinements (tuxlink-f717):
//   - Ambiguous fixes (APRS position-ambiguity > 0, where the sender masked
//     low-order minute digits) are drawn as an UNCERTAINTY REGION — a translucent
//     circle sized to circumscribe the masked resolution box (radius ×√2 the cell
//     half-width) — instead of a false-exact pin, so the map never claims more
//     precision than the wire carried.
//   - Pins age: a station not re-heard within STALE_MS dims to a greyscale sprite,
//     and the popup shows how long ago it was last heard, so a stale fix is not
//     read as current.
//
// Leaflet re-expression (tuxlink-6kdw): each station owns a per-call bundle
// (L.featureGroup) of a pin (a `divIcon` carrying the authentic APRS sprite as a
// PNG data URL + the callsign label), an uncertainty L.circle (ambiguous fixes
// only), and a WX badge `divIcon` (weather stations only). The category filter
// adds/removes whole bundles — no orphan halos, and NO per-frame style mutation
// (the MapLibre `setFilter`-on-`styledata` "drunk map" failure class is gone with
// the engine). Markers are created once per station and updated in place across
// re-renders (no churn). Pin sprite identity is grim-verified; jsdom asserts the
// stable sprite id via the data attribute.

import { useEffect, useMemo, useRef, useState } from 'react';
import L from 'leaflet';
import { LeafletMap } from '../map/LeafletMap';
import { useLeafletMap } from '../map/LeafletMapContext';
import { useLeafletLayerGroup } from '../map/leafletHooks';
import { usePersistedViewport } from '../map/usePersistedViewport';
import { LeafletRecenterControl } from '../map/LeafletRecenterControl';
import { gridToLatLon } from '../forms/position/maidenhead';
import { lookupAprsSymbol } from './aprsSymbols';
import { spriteDataUrl, spriteIdFor, greyIdOf, whenSheetsReady } from '../map/aprsSprites';
import type { HeardPosition } from './aprsTypes';
import { joinWxStations, badgeContent, type WxStation } from './wxStations';
import { CATEGORIES, categoryByKey } from './stationCategories';
import { composeWxSitrep } from './wxSitrep';
import type { EnvStation } from './envStations';
import { saveDraft } from '../compose/useDraft';
import { newDraftId } from '../routing';
import { invoke } from '@tauri-apps/api/core';
import { reportFrontendError } from '../frontendErrorLog';
import { resolveDigipeatPath, type PathSegment } from './digipeatPath';
import { DigipeatPathLayer } from './DigipeatPathLayer';
import './AprsPositionsMap.css';

export interface AprsPositionsMapProps {
  /// Heard stations' latest positions (one per callsign), from useAprsPositions.
  positions: HeardPosition[];
  /// Operator Maidenhead grid (statusData.ui_grid). First-run map center; the
  /// recenter control flies here. Empty / absent = no known position.
  operatorGrid?: string;
  /// Heard environmental view-models (weather/telemetry), from useEnvStations.
  /// Joined with positions to render WX badges (ni5b). Absent = no WX overlay.
  envStations?: EnvStation[];
  /// Click a WX badge → focus that station's Station Data card (ni5b).
  onFocusStation?: (call: string) => void;
}

/// First-run / recenter zoom on the operator. APRS is local VHF → a LOCAL-area
/// zoom (metro/county), not StationFinderMap's continental Z6.
const OPERATOR_ZOOM = 10;

/// A fix not re-heard within this long is shown dimmed (and its age surfaced in
/// the popup). The hook drops it entirely after a longer TTL.
const STALE_MS = 15 * 60 * 1000;
/// Cadence for recomputing "now" so staleness updates without new traffic.
const NOW_TICK_MS = 30 * 1000;

/// Uncertainty radius (latitude minutes) per APRS ambiguity level. Level N masks
/// the lowest N minute digits → the fix lies anywhere in a box half this many
/// minutes wide: L1 ±0.05′, L2 ±0.5′, L3 ±5′, L4 ±30′ (1°).
const AMBIGUITY_HALF_MINUTES = [0, 0.05, 0.5, 5, 30];
const METERS_PER_MINUTE_LAT = 1852;

/// Half-width, in metres, of the ambiguity cell for a given level — the "±"
/// distance shown in the popup. `0` for a full-precision fix (level 0).
export function ambiguityRadiusMeters(level: number): number {
  const l = Math.max(0, Math.min(4, Math.floor(level)));
  return AMBIGUITY_HALF_MINUTES[l] * METERS_PER_MINUTE_LAT;
}

/// The decoded coordinate is the LOW corner of the ambiguity cell (the parser
/// zero-fills masked minute digits), so plot the cell CENTRE — half a cell toward
/// increasing magnitude on each axis — and let the region circumscribe the box.
/// A full-precision fix is returned unchanged.
function cellCenter(p: { lat: number; lon: number; ambiguity: number }): { lat: number; lon: number } {
  const l = Math.max(0, Math.min(4, Math.floor(p.ambiguity)));
  const offDeg = AMBIGUITY_HALF_MINUTES[l] / 60;
  if (offDeg === 0) return { lat: p.lat, lon: p.lon };
  return {
    lat: p.lat + Math.sign(p.lat) * offDeg,
    lon: p.lon + Math.sign(p.lon) * offDeg,
  };
}

/// Human "last heard" age, e.g. "just now", "3 min ago", "2 h ago".
function formatAge(ms: number): string {
  if (ms < 60_000) return 'just now';
  const min = Math.floor(ms / 60_000);
  if (min < 60) return `${min} min ago`;
  const h = Math.floor(min / 60);
  return `${h} h ago`;
}

/// "± ~Xkm" / "± ~Xm" precision note for an ambiguous fix's popup.
function ambiguityNote(level: number): string {
  const r = ambiguityRadiusMeters(level);
  const approx = r >= 1000 ? `~${(r / 1000).toFixed(r >= 10000 ? 0 : 1)} km` : `~${Math.round(r)} m`;
  return `approximate position (±${approx})`;
}

const esc = (s: string): string =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/// Build the pin `divIcon`: the authentic sprite (colour, or greyscale when
/// stale) + the callsign label. `data-sprite` carries the STABLE sprite id (the
/// grey id when stale) so jsdom can assert identity + staleness without real
/// pixels; `data-call`/`data-lat`/`data-lon` expose the fix for assertions. The
/// ambiguous shrink (icon scale 0.7) mirrors the MapLibre icon-size.
function pinIcon(p: HeardPosition, c: { lat: number; lon: number }, stale: boolean): L.DivIcon {
  const sym = lookupAprsSymbol(p.symbolTable, p.symbolCode);
  const colorId = spriteIdFor(p.symbolTable, p.symbolCode, sym.overlay);
  const id = stale ? greyIdOf(colorId) : colorId;
  const url = spriteDataUrl(p.symbolTable, p.symbolCode, sym.overlay, stale);
  const scale = p.ambiguity > 0 ? 0.7 : 1;
  const px = Math.round(32 * scale);
  const html =
    `<img class="aprs-pin${stale ? ' aprs-pin--stale' : ''}" data-sprite="${esc(id)}" ` +
    `data-call="${esc(p.call)}" data-lat="${c.lat}" data-lon="${c.lon}" ` +
    `style="width:${px}px;height:${px}px" src="${url}" alt="">` +
    `<span class="aprs-pin-label">${esc(p.call)}</span>`;
  return L.divIcon({ className: 'aprs-pin-icon', html, iconSize: [px, px], iconAnchor: [px / 2, px / 2] });
}

/// Build the WX badge `divIcon` (temperature-led chip), positioned above the pin.
function badgeIcon(w: WxStation): L.DivIcon {
  const b = badgeContent(w.env);
  const text = b.glyph ? `${b.primary} ${b.glyph}` : b.primary;
  const html = `<span class="aprs-wx-chip" data-call="${esc(w.call)}">${esc(text)}</span>`;
  return L.divIcon({ className: 'aprs-wx-badge-icon', html, iconSize: [0, 0], iconAnchor: [0, 34] });
}

/// One station's owned Leaflet layers, grouped so the category filter can add/
/// remove the whole bundle atomically (no orphan halo/label).
interface Bundle {
  group: L.FeatureGroup;
  pin: L.Marker;
  circle: L.Circle | null;
  badge: L.Marker | null;
  /// Cache key of the inputs that determine the pin icon (sprite + stale + amb
  /// scale), so the icon is re-baked only when one actually changes.
  iconKey: string;
  visible: boolean;
}

/// Key for the pin icon: sprite identity + stale + ambiguous-shrink. Re-bake only
/// when this changes (not every NOW_TICK / reconcile).
function pinIconKey(p: HeardPosition, stale: boolean): string {
  const sym = lookupAprsSymbol(p.symbolTable, p.symbolCode);
  return `${spriteIdFor(p.symbolTable, p.symbolCode, sym.overlay)}|${stale ? 's' : 'f'}|${p.ambiguity > 0 ? 'a' : 'e'}`;
}

/// Manages the per-station bundles + the click/hover-driven popup and WX card.
function MapOverlays({
  positions,
  wx,
  category,
  onFocusStation,
  operator,
}: {
  positions: HeardPosition[];
  wx: WxStation[];
  category: string;
  onFocusStation?: (call: string) => void;
  operator: { lat: number; lon: number } | null;
}) {
  const map = useLeafletMap();
  const group = useLeafletLayerGroup(map);

  const [popupCall, setPopupCall] = useState<string | null>(null);
  // The WX station whose info card is open. Click-pinned + dismissed via the card's
  // × button — NOT hover: hover-dismiss (mouseout) proved unreliable in WebKitGTK
  // with the 0-size divIcon badge, leaving the card stuck open (operator smoke).
  const [wxCardCall, setWxCardCall] = useState<string | null>(null);

  // Re-tick "now" so pins age (grey) and the popup age stays roughly current.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), NOW_TICK_MS);
    return () => clearInterval(id);
  }, []);

  const byCall = useMemo(() => new Map(positions.map((p) => [p.call, p])), [positions]);
  const byCallRef = useRef(byCall);
  byCallRef.current = byCall;
  const wxByCall = useMemo(() => new Map(wx.map((w) => [w.call, w])), [wx]);

  // Hold interaction state + callbacks in refs so the reconcile effect does NOT
  // depend on them. A pin/badge click (popupCall/wxCardCall) or a fresh
  // onFocusStation identity (AppShell passes an inline arrow → new every parent
  // render) must NOT re-run the whole layer reconcile: reconciling on every
  // click/parent-render churned layers and could mutate them mid Leaflet zoom/pan
  // animation → an intermittent throw that tripped the app ErrorBoundary (operator
  // soft-crash). Reconcile now runs ONLY on data changes (positions/wx/category/now).
  const onFocusRef = useRef(onFocusStation);
  onFocusRef.current = onFocusStation;
  const popupCallRef = useRef(popupCall);
  popupCallRef.current = popupCall;
  const wxCardCallRef = useRef(wxCardCall);
  wxCardCallRef.current = wxCardCall;

  // Path state for the digipeat-path animation layer (tuxlink-qnu6 / cn84).
  const [tracePath, setTracePath] = useState<PathSegment[] | null>(null);

  // Resolve a callsign's digipeat path to drawable segments. Kept in a ref so
  // the reconcile effect (which creates pin hover handlers) does NOT depend on
  // it and does not re-run when positions/operator change after mount. The ref
  // is updated every render so the handlers always see current data.
  const resolve = (call: string): PathSegment[] | null => {
    const p = byCallRef.current.get(call);
    if (!p) return null;
    // Object/item reports carry the RELAYING station's via-chain, not the
    // object's own — tracing a path from an object pin would fabricate its RF
    // source. The store tags these `isObject` (it does not drop them); honor the
    // HeardPosition invariant and never trace one.
    if (p.isObject) return null;
    const src = { ...cellCenter(p), call: p.call };
    const via = p.via ?? [];
    const located = new Map(positions.map((q) => [q.call, cellCenter(q)]));
    const segs = resolveDigipeatPath({ src, via, located, operator });
    return segs.length ? segs : null;
  };
  const resolveRef = useRef(resolve);
  resolveRef.current = resolve;

  // Live trigger: when the newest frame (highest `at`) advances, fire the trace
  // for that station once. The layer's bounded animation fades and stops on its
  // own; no loop needed here. The two triggers are hover + a genuinely-new frame
  // — so on first mount we SEED the high-water mark from the existing backlog
  // WITHOUT firing (a cached frame that may be hours old is not "new"); only
  // frames that arrive after mount animate.
  const newestSeenRef = useRef<{ at: number; call: string } | null>(null);
  useEffect(() => {
    // Only real stations can drive a path trace — object/item reports carry the
    // relayer's via-chain (see `resolve`), so they never become the trigger frame.
    const traceable = positions.filter((p) => !p.isObject);
    if (traceable.length === 0) return;
    let newest = traceable[0];
    for (const p of traceable) {
      if (p.at > newest.at) newest = p;
    }
    const prev = newestSeenRef.current;
    if (!prev) {
      // First mount: seed the backlog's high-water mark, do not auto-play.
      newestSeenRef.current = { at: newest.at, call: newest.call };
      return;
    }
    if (newest.at > prev.at) {
      newestSeenRef.current = { at: newest.at, call: newest.call };
      setTracePath(resolveRef.current(newest.call));
    }
  }, [positions]);

  const bundlesRef = useRef<Map<string, Bundle>>(new Map());
  // Uncertainty discs use an SVG renderer (not the map's canvas): they are few
  // (ambiguous fixes only), so SVG's cost is negligible, and it renders in DOM —
  // robust under software-GL WebKitGTK and unit-inspectable in jsdom (the map's
  // preferCanvas vector path has no 2D context there).
  const rendererRef = useRef<L.Renderer | null>(null);
  if (!rendererRef.current) rendererRef.current = L.svg({ padding: 2 });

  // Diff-based reconciliation: create new stations, update existing markers IN
  // PLACE (stable identity — no churn/leak/flicker), remove dropped ones. Runs on
  // positions/wx/category/now change.
  useEffect(() => {
    if (!map || !group) return;
    const bundles = bundlesRef.current;
    const wxCalls = new Set(wx.map((w) => w.call));
    const live = new Set(positions.map((p) => p.call));

    const makeCircle = (p: HeardPosition, c: { lat: number; lon: number }): L.Circle =>
      L.circle([c.lat, c.lon], {
        radius: ambiguityRadiusMeters(p.ambiguity) * Math.SQRT2,
        renderer: rendererRef.current ?? undefined,
        color: '#f0c24a',
        weight: 1,
        dashArray: '2 2',
        fillColor: '#f0c24a',
        fillOpacity: 0.12,
      });
    const makeBadge = (call: string, w: WxStation, c: { lat: number; lon: number }): L.Marker => {
      const m = L.marker([c.lat, c.lon], { icon: badgeIcon(w) });
      // Click opens the on-map WX card (dismissed via its × button) AND focuses the
      // station's dock card. No hover handlers — hover-dismiss is unreliable here.
      m.on('click', () => {
        onFocusRef.current?.(call);
        setWxCardCall(call);
      });
      return m;
    };

    // Guard every Leaflet layer mutation: a transient throw (e.g. add/remove that
    // lands mid zoom/pan animation) is logged + skipped, never crashed to the app
    // ErrorBoundary (operator soft-crash). Mirrors mapHooks' safeEnsure/safeTeardown.
    const safe = (what: string, fn: () => void): void => {
      try {
        fn();
      } catch (e) {
        reportFrontendError(
          'aprs-map-reconcile',
          `${what}: ${e instanceof Error ? e.message : String(e)}`,
          e instanceof Error ? e.stack : undefined,
        );
      }
    };

    // Remove dropped stations entirely.
    for (const [call, b] of bundles) {
      if (!live.has(call)) {
        safe(`drop ${call}`, () => group.removeLayer(b.group));
        bundles.delete(call);
        if (popupCallRef.current === call) setPopupCall(null);
        if (wxCardCallRef.current === call) setWxCardCall(null);
      }
    }

    for (const p of positions) {
      safe(`reconcile ${p.call}`, () => {
        const c = cellCenter(p);
        const stale = now - p.at > STALE_MS;
        const isWeather = wxCalls.has(p.call);
        const w = wxByCall.get(p.call);
        let b = bundles.get(p.call);

        if (!b) {
          const pin = L.marker([c.lat, c.lon], { icon: pinIcon(p, c, stale) });
          pin.on('click', () => {
            if (byCallRef.current.has(p.call)) setPopupCall(p.call);
          });
          pin.on('mouseover', () => setTracePath(resolveRef.current(p.call)));
          pin.on('mouseout', () => setTracePath(null));
          const circle = p.ambiguity > 0 ? makeCircle(p, c) : null;
          const badge = w ? makeBadge(p.call, w, c) : null;
          // Pin draws above the uncertainty disc: add circle first, then pin, then badge.
          const groupLayers: L.Layer[] = circle ? [circle, pin] : [pin];
          if (badge) groupLayers.push(badge);
          b = { group: L.featureGroup(groupLayers), pin, circle, badge, iconKey: pinIconKey(p, stale), visible: true };
          bundles.set(p.call, b);
        } else {
          // FULL reconcile of every sub-layer (a re-beacon can change position,
          // staleness, symbol, ambiguity level, or weather readings — Codex impl P1).
          b.pin.setLatLng([c.lat, c.lon]);
          const key = pinIconKey(p, stale);
          if (key !== b.iconKey) {
            b.pin.setIcon(pinIcon(p, c, stale)); // sprite / stale / ambiguous-shrink changed
            b.iconKey = key;
          }
          // Uncertainty disc: create / update radius+centre / remove as ambiguity changes.
          if (p.ambiguity > 0) {
            if (!b.circle) {
              b.circle = makeCircle(p, c);
              b.group.addLayer(b.circle);
            } else {
              b.circle.setLatLng([c.lat, c.lon]);
              b.circle.setRadius(ambiguityRadiusMeters(p.ambiguity) * Math.SQRT2);
            }
          } else if (b.circle) {
            b.group.removeLayer(b.circle);
            b.circle = null;
          }
          // WX badge: create / refresh reading text / remove as weather appears/changes/clears.
          if (w) {
            if (!b.badge) {
              b.badge = makeBadge(p.call, w, c);
              b.group.addLayer(b.badge);
            } else {
              b.badge.setLatLng([c.lat, c.lon]);
              b.badge.setIcon(badgeIcon(w)); // refresh the reading (chip text)
            }
          } else if (b.badge) {
            b.group.removeLayer(b.badge);
            b.badge = null;
          }
        }

        // Visibility per the active category (no orphan — the whole bundle moves).
        const visible = categoryByKey(category).matches({ call: p.call, isWeather });
        if (visible && !group.hasLayer(b.group)) group.addLayer(b.group);
        else if (!visible && group.hasLayer(b.group)) group.removeLayer(b.group);
        b.visible = visible;
      });
    }
  }, [map, group, positions, wx, wxByCall, category, now]);

  // whenSheetsReady re-bake (tuxlink-r8sm / R3 P0): the sprite sheets decode
  // asynchronously; the first synchronous bake on mount yields transparent icons.
  // Without re-baking, a Leaflet `divIcon`'s data-URL `<img>` never re-decodes →
  // pins stay BLANK forever. Re-bake every pin's icon once the sheets are ready.
  useEffect(() => {
    if (!map) return;
    const stop = whenSheetsReady(() => {
      for (const [call, b] of bundlesRef.current) {
        const p = byCallRef.current.get(call);
        if (!p) continue;
        const stale = Date.now() - p.at > STALE_MS;
        b.pin.setIcon(pinIcon(p, cellCenter(p), stale));
        b.iconKey = pinIconKey(p, stale);
      }
    });
    return stop;
  }, [map]);

  // Live-derived bodies: a re-beacon updates them; a pruned station closes them.
  const selected = popupCall ? byCall.get(popupCall) : undefined;
  const wxSelected = wxCardCall ? wxByCall.get(wxCardCall) : undefined;

  return (
    <>
      {selected && <PositionPopup fix={selected} now={now} onClose={() => setPopupCall(null)} />}
      {wxSelected && <WxCard wx={wxSelected} onClose={() => setWxCardCall(null)} />}
      <DigipeatPathLayer path={tracePath} />
    </>
  );
}

function PositionPopup({ fix, now, onClose }: { fix: HeardPosition; now: number; onClose: () => void }) {
  const symbol = lookupAprsSymbol(fix.symbolTable, fix.symbolCode);
  return (
    <div className="aprs-positions-map__popup" role="status" data-testid="aprs-position-popup">
      <button
        type="button"
        className="aprs-positions-map__popup-close"
        aria-label="Dismiss"
        onClick={onClose}
      >
        ×
      </button>
      <span className="aprs-positions-map__popup-call">{fix.call}</span>
      <span className="aprs-positions-map__popup-symbol" data-testid="aprs-position-symbol">
        <span className="aprs-positions-map__popup-symbol-glyph" aria-hidden="true">
          {symbol.glyph}
        </span>
        {symbol.overlay ? `${symbol.name} (overlay ${symbol.overlay})` : symbol.name}
      </span>
      <span className="aprs-positions-map__popup-age" data-testid="aprs-position-age">
        last heard {formatAge(Math.max(0, now - fix.at))}
      </span>
      {fix.ambiguity > 0 && (
        <span className="aprs-positions-map__popup-ambiguity" data-testid="aprs-position-ambiguity">
          {ambiguityNote(fix.ambiguity)}
        </span>
      )}
      {fix.comment && <span className="aprs-positions-map__popup-comment">{fix.comment}</span>}
    </div>
  );
}

function WxCard({ wx, onClose }: { wx: WxStation; onClose: () => void }) {
  return (
    <div className="aprs-wx-card" role="status" data-testid="aprs-wx-card">
      <button
        type="button"
        className="aprs-wx-card__close"
        aria-label="Dismiss"
        data-testid="aprs-wx-card-close"
        onClick={onClose}
      >
        ×
      </button>
      <span className="aprs-wx-card__call">{wx.call}</span>
      <ul className="aprs-wx-card__list">
        {wx.env.channels.map((c) => (
          <li key={c.key}>
            {c.label}: {Math.round(c.value)}
            {c.unit ? ` ${c.unit}` : ''}
          </li>
        ))}
        {wx.env.rain?.in1h != null && <li>Rain 1h: {wx.env.rain.in1h}&quot;</li>}
      </ul>
    </div>
  );
}

/// The operator's own position pin ("you") — a blue-ringed dot drawn distinctly so
/// it never reads as a heard station. Sourced from the operator grid, not a
/// decoded beacon (it does not violate the map's RF-honesty).
function OperatorPin({ location }: { location: { lat: number; lon: number } | null }) {
  const map = useLeafletMap();
  const group = useLeafletLayerGroup(map);
  useEffect(() => {
    if (!map || !group || !location) return;
    const marker = L.marker([location.lat, location.lon], {
      icon: L.divIcon({
        className: 'aprs-you-pin-icon',
        html: '<span class="aprs-you-pin" data-testid="aprs-you-pin"></span>',
        iconSize: [18, 18],
        iconAnchor: [9, 9],
      }),
      interactive: false,
      keyboard: false,
    });
    group.addLayer(marker);
    return () => {
      group.removeLayer(marker);
    };
  }, [map, group, location?.lat, location?.lon]);
  return null;
}

/// "Weather SITREP" (tuxlink-hepq): aggregate heard WX stations into a Winlink-
/// ready local-area situation report and open a prefilled compose window. Disabled
/// until at least one WX station is heard.
function WxSitrepControl({ wx, operatorGrid }: { wx: WxStation[]; operatorGrid?: string }) {
  const compose = () => {
    const { subject, body } = composeWxSitrep(wx, { nowMs: Date.now(), operatorGrid });
    const draftId = newDraftId();
    saveDraft({ draftId, to: '', subject, body, requestAck: false });
    void invoke('compose_window_open', { draftId });
  };
  return (
    <button
      type="button"
      className="aprs-wx-sitrep"
      data-testid="aprs-wx-sitrep"
      disabled={wx.length === 0}
      title={
        wx.length === 0
          ? 'No weather stations heard yet'
          : 'Compose a Winlink weather situation report from heard stations'
      }
      onClick={compose}
    >
      Weather SITREP
    </button>
  );
}

/// The category filter control ("weather mode"): a small select in the map corner.
function WxFilterControl({ category, onChange }: { category: string; onChange: (key: string) => void }) {
  return (
    <div className="aprs-wx-filter" data-testid="aprs-wx-filter">
      <label className="aprs-wx-filter__label">
        Show{' '}
        <select value={category} onChange={(e) => onChange(e.target.value)} data-testid="aprs-wx-filter-select">
          {CATEGORIES.map((c) => (
            <option key={c.key} value={c.key}>
              {c.label}
            </option>
          ))}
        </select>
      </label>
    </div>
  );
}

export function AprsPositionsMap({ positions, operatorGrid, envStations, onFocusStation }: AprsPositionsMapProps) {
  const me = operatorGrid ? gridToLatLon(operatorGrid) : null;
  // tuxlink-dwzu: remember + restore the operator's last viewport. First run (no
  // saved view) centers on the operator at the local zoom — never the mid-Atlantic
  // world view — falling back to the world view only when no operator grid is known.
  const { saved, onViewportChange } = usePersistedViewport('tuxlink:map-viewport:aprs');
  const initialCenter = saved ? saved.center : (me ?? undefined);
  const initialZoom = saved ? saved.zoom : me ? OPERATOR_ZOOM : 2;
  const [category, setCategory] = useState('all');
  const wx = useMemo(() => joinWxStations(envStations ?? [], positions), [envStations, positions]);

  // Export PNG is removed this phase (tuxlink-a7qt): naive Leaflet canvas
  // compositing omits DOM markers + per-tile pane transforms; a proper DOM-aware
  // snapshot is tracked separately. The Winlink-text Weather SITREP stays.

  return (
    <div className="aprs-positions-map" data-testid="aprs-positions-map">
      <WxFilterControl category={category} onChange={setCategory} />
      <LeafletMap initialCenter={initialCenter} initialZoom={initialZoom} onViewportChange={onViewportChange}>
        <MapOverlays positions={positions} wx={wx} category={category} onFocusStation={onFocusStation} operator={me} />
        <OperatorPin location={me} />
        <WxSitrepControl wx={wx} operatorGrid={operatorGrid || undefined} />
        <LeafletRecenterControl target={me} zoom={OPERATOR_ZOOM} />
      </LeafletMap>
    </div>
  );
}
