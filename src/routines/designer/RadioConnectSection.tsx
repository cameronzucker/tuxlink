/**
 * RadioConnectSection — the `radio.connect` step's dedicated inspector body
 * (tuxlink-fg0em), replacing the generic key/value grid that let operators
 * type params the action does not accept ("target", "freq_hz") and band
 * strings the runtime resolver rejects.
 *
 * Grounded in how the action actually works (`routines/actions/radio.rs`):
 *
 *  - Transport precedence: a configured packet KISS link is dialed FIRST —
 *    bands are inert there. Otherwise exactly one of ARDOP/VARA must be
 *    configured, and an HF dial REQUIRES at least one band (empty bands is
 *    the packet-dial shape, not "any band"). The "Runs on" line states all
 *    of this from `config_read.routine_hf_modem` instead of letting the
 *    first run fail.
 *  - Stations: ordered chips (duplicates legal — the runtime walks the
 *    exact sequence; rows are keyed and removed BY INDEX) + an inline
 *    picker over the station-finder cache and per-mode favorites.
 *  - Bands: toggle chips over `HF_BANDS`, the same vocabulary the backend
 *    ParamSpec enforces at save time (case-insensitively, matching the
 *    runtime's lookup).
 *
 * Ownership rule (adrev 5.5/5.6 P1): any owned param whose shape this
 * surface cannot express — a whole-value step ref (`"$s2.callsigns"`) in
 * `stations` OR `bands`, a non-number `listen_before_tx_s` — is rendered
 * read-only or disclosed, and PRESERVED VERBATIM by every commit unless the
 * operator explicitly replaces that specific param. Unknown keys (`rig`, …)
 * are always preserved and disclosed. "Edit as JSON" (the StepInspector
 * toggle) remains the escape hatch for every such shape.
 *
 * C4 distance source: operator grid comes from `position_current_fix`
 * (full precision), never the precision-reduced status surfaces.
 */
import { useEffect, useMemo, useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';

import { HF_BANDS, type Band } from '../../catalog/bandPlan';
import { distanceFromGrids, kmToMi } from '../../catalog/distance';
import { aggregateStations, type Station } from '../../catalog/stationModel';
import { useStations } from '../../catalog/useStations';
import type { ListingMode } from '../../catalog/stationTypes';
import { useFavorites } from '../../favorites/useFavorites';
import type { Favorite } from '../../favorites/types';

export interface RadioConnectSectionProps {
  /** The step's committed params object (never mutated in place). */
  params: Record<string, unknown>;
  /** Full replacement commit — the caller wraps it into a StepPatch. */
  onChange: (params: Record<string, unknown>) => void;
}

/** `config_read`'s slice this section consumes (wire shape pinned by the
 *  Rust test `routine_hf_modem_view_wire_shape`). */
interface HfModemView {
  kind: 'packet' | 'vara' | 'ardop' | 'both' | 'none';
  bandwidth_hz?: number | null;
}

function asStringArray(v: unknown): string[] | null {
  return Array.isArray(v) && v.every((e) => typeof e === 'string') ? (v as string[]) : null;
}

/** Finder listing modes for the configured modem; both/none/packet fall back
 *  to both HF modes so the picker still works while Settings is unresolved. */
function finderModes(modem: HfModemView | undefined): ListingMode[] {
  if (modem?.kind === 'vara') return ['vara-hf'];
  if (modem?.kind === 'ardop') return ['ardop-hf'];
  return ['vara-hf', 'ardop-hf'];
}

export function RadioConnectSection({ params, onChange }: RadioConnectSectionProps) {
  const stationsRaw = params['stations'];
  const stations = asStringArray(stationsRaw) ?? [];
  /** `"$s2.callsigns"` whole-value ref — render, don't edit. */
  const stationsRef = typeof stationsRaw === 'string' ? stationsRaw : null;
  const bandsRaw = params['bands'];
  const bands = asStringArray(bandsRaw) ?? [];
  /** Whole-value ref in bands is just as legal (validator permits `$` refs
   *  for list params) — render read-only, preserve on every commit. */
  const bandsRef = typeof bandsRaw === 'string' ? bandsRaw : null;
  const listenRaw = params['listen_before_tx_s'];
  const listen = typeof listenRaw === 'number' && Number.isInteger(listenRaw) ? listenRaw : undefined;
  /** A malformed (non-integer) listen value this surface can't express —
   *  preserved until the operator explicitly replaces it via the field. */
  const listenUneditable = listenRaw !== undefined && listen === undefined;

  /** Keys this section never owns — preserved verbatim and disclosed. */
  const extraKeys = Object.keys(params).filter(
    (k) => !['stations', 'bands', 'listen_before_tx_s'].includes(k),
  );

  const [listenText, setListenText] = useState(listen === undefined ? '' : String(listen));
  // Same-step external updates (JSON-mode roundtrip, model edits) must
  // resync the local text — this component is NOT remounted for same-step
  // prop changes (adrev 5.6 P2).
  useEffect(() => {
    setListenText(listen === undefined ? '' : String(listen));
  }, [listen]);

  /**
   * Build + commit the full replacement params object. Every owned param
   * not explicitly replaced by `next` keeps its CURRENT raw value (refs and
   * malformed shapes included); unknown keys always carry over verbatim.
   * `next.listen === null` means "explicitly clear the key".
   */
  function commit(next: {
    stations?: string[];
    bands?: string[];
    listen?: number | null;
  }) {
    const out: Record<string, unknown> = {};
    out['stations'] = next.stations ?? stationsRaw ?? [];
    const bandsOut = next.bands ?? bandsRaw;
    if (bandsRef !== null && next.bands === undefined) {
      out['bands'] = bandsRef;
    } else if (Array.isArray(bandsOut) ? bandsOut.length > 0 : bandsOut !== undefined) {
      if (Array.isArray(bandsOut) && bandsOut.length === 0) {
        // omit empty array
      } else {
        out['bands'] = bandsOut;
      }
    }
    if ('listen' in next) {
      if (next.listen !== null && next.listen !== undefined) {
        out['listen_before_tx_s'] = next.listen;
      }
      // explicit clear/replace: the old raw value (malformed included) is
      // NOT preserved — the operator acted on this exact param.
    } else if (listenRaw !== undefined) {
      out['listen_before_tx_s'] = listenRaw;
    }
    for (const k of extraKeys) out[k] = params[k];
    onChange(out);
  }

  // ---- "Runs on" — the transport the run will actually use ----
  const modemQuery = useQuery({
    queryKey: ['config_read'],
    queryFn: () => invoke<{ routine_hf_modem?: HfModemView }>('config_read'),
  });
  const modem = modemQuery.data?.routine_hf_modem;
  /** Config settled = success or error (older backend without the field
   *  degrades to the fallback modes, but only ONCE it's settled). */
  const modemSettled = modemQuery.isSuccess || modemQuery.isError;

  // ---- picker state + data ----
  const [pickerOpen, setPickerOpen] = useState(false);
  const [pickerTab, setPickerTab] = useState<'finder' | 'favorites'>('finder');
  const [search, setSearch] = useState('');
  const { listings, loading, error, fetch } = useStations();
  /** Fetch guard keyed by the EFFECTIVE mode set (adrev consensus P2): an
   *  early open with config unresolved must not permanently pin the wrong
   *  modes — the fetch waits for config to settle, and a later mode change
   *  refetches. */
  const fetchedForRef = useRef<string | null>(null);
  useEffect(() => {
    if (!pickerOpen || pickerTab !== 'finder' || !modemSettled) return;
    const modes = finderModes(modem);
    const key = modes.join('|');
    if (fetchedForRef.current === key) return;
    fetchedForRef.current = key;
    fetch(modes);
  }, [pickerOpen, pickerTab, modemSettled, modem, fetch]);

  const fixQuery = useQuery({
    queryKey: ['position_current_fix'],
    queryFn: () => invoke<{ grid: string | null }>('position_current_fix'),
  });
  const operatorGrid = fixQuery.data?.grid ?? null;

  const varaFavs = useFavorites('vara-hf');
  const ardopFavs = useFavorites('ardop-hf');
  const favorites: Favorite[] =
    modem?.kind === 'vara'
      ? varaFavs.favorites
      : modem?.kind === 'ardop'
        ? ardopFavs.favorites
        : [...varaFavs.favorites, ...ardopFavs.favorites];
  const favoriteGateways = useMemo(
    () => new Set(favorites.map((f) => f.gateway.toUpperCase())),
    [favorites],
  );

  const finderRows = useMemo(() => {
    const all = aggregateStations(listings);
    const q = search.trim().toUpperCase();
    const withMeta = all
      .map((s: Station) => {
        const stationBands = [
          ...new Set(
            s.channels.map((c) => c.band).filter((b): b is Band => b != null && b !== 'vhf-uhf'),
          ),
        ];
        const km = operatorGrid ? distanceFromGrids(operatorGrid, s.grid) : null;
        return { station: s, stationBands, mi: km == null ? null : Math.round(kmToMi(km)) };
      })
      .filter(({ station, stationBands }) => {
        if (q && !station.baseCallsign.includes(q) && !station.grid.toUpperCase().startsWith(q)) {
          return false;
        }
        // Only gateways with a cached dial on a selected band; no bands
        // selected (or a bands ref) = no band filter.
        if (bands.length > 0 && !stationBands.some((b) => bands.includes(b))) return false;
        return true;
      })
      .sort((a, b) => (a.mi ?? Number.MAX_SAFE_INTEGER) - (b.mi ?? Number.MAX_SAFE_INTEGER));
    return withMeta;
  }, [listings, search, bands, operatorGrid]);
  const VISIBLE_ROWS = 12;

  function addStation(call: string) {
    const c = call.trim().toUpperCase();
    if (!c || stations.includes(c)) return;
    commit({ stations: [...stations, c] });
  }

  function removeStationAt(index: number) {
    commit({ stations: stations.filter((_, i) => i !== index) });
  }

  return (
    <div className="rc-section" data-testid="radio-connect-section">
      {modem && (
        <div
          className={`rc-runson ${modem.kind === 'both' || modem.kind === 'none' ? 'rc-runson-warn' : ''}`}
          data-testid="rc-runson"
        >
          {modem.kind === 'packet' && (
            <>
              <span className="rc-dot" />
              <span className="mono">Packet (KISS)</span>
              <span className="rc-src">takes precedence · bands unused</span>
            </>
          )}
          {modem.kind === 'vara' && (
            <>
              <span className="rc-dot" />
              <span className="mono">
                VARA HF{modem.bandwidth_hz ? ` · ${modem.bandwidth_hz} Hz` : ''}
              </span>
              <span className="rc-src">from Settings → Modem</span>
            </>
          )}
          {modem.kind === 'ardop' && (
            <>
              <span className="rc-dot" />
              <span className="mono">ARDOP HF</span>
              <span className="rc-src">from Settings → Modem</span>
            </>
          )}
          {modem.kind === 'both' && (
            <span className="rc-warntext">
              Both ARDOP and VARA are configured — runs will refuse. Configure ONE HF modem
              (Settings → Modem).
            </span>
          )}
          {modem.kind === 'none' && (
            <span className="rc-warntext">
              No transport configured (Settings → Modem) — this step has nothing to dial.
            </span>
          )}
        </div>
      )}

      <div className="rc-label">
        STATIONS <span className="rc-req">required</span>
        <span className="rc-hint">tried in order</span>
      </div>
      <div className="rc-chips" data-testid="rc-stations">
        {stationsRef ? (
          <span className="rc-chip rc-chip-ref mono" data-testid="rc-stations-ref">
            {stationsRef}
            <span className="rc-refnote">step output — edit as JSON to change</span>
          </span>
        ) : (
          stations.map((s, i) => (
            <span className="rc-chip mono" data-testid={`rc-station-${i}-${s}`} key={`${i}|${s}`}>
              <span className="rc-ord">{i + 1}</span>
              {s}
              <button
                type="button"
                className="rc-x"
                aria-label={`Remove station ${i + 1} (${s})`}
                onClick={() => removeStationAt(i)}
              >
                ✕
              </button>
            </span>
          ))
        )}
        {!stationsRef && (
          <button
            type="button"
            className={`rc-add ${pickerOpen ? 'rc-add-active' : ''}`}
            data-testid="rc-add-station"
            onClick={() => setPickerOpen((o) => !o)}
          >
            + Add station
          </button>
        )}
      </div>

      {pickerOpen && !stationsRef && (
        <div className="rc-picker" data-testid="rc-picker">
          <div className="rc-ptabs" role="tablist">
            <button
              type="button"
              role="tab"
              aria-selected={pickerTab === 'finder'}
              className={`rc-ptab ${pickerTab === 'finder' ? 'rc-ptab-on' : ''}`}
              onClick={() => setPickerTab('finder')}
            >
              Finder
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={pickerTab === 'favorites'}
              className={`rc-ptab ${pickerTab === 'favorites' ? 'rc-ptab-on' : ''}`}
              onClick={() => setPickerTab('favorites')}
            >
              Favorites
            </button>
          </div>

          {pickerTab === 'finder' && (
            <>
              <input
                className="rc-search mono"
                data-testid="rc-search"
                placeholder="Search callsign or grid…"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
              {loading && <div className="rc-pnote">Fetching stations…</div>}
              {error && <div className="rc-pnote rc-perr">{error}</div>}
              {!loading && !error && finderRows.length === 0 && (
                <div className="rc-pnote">
                  No cached gateways match{bands.length > 0 ? ' the selected bands' : ''} — run
                  Find a Station, or clear the search.
                </div>
              )}
              {finderRows.slice(0, VISIBLE_ROWS).map(({ station, stationBands, mi }) => (
                <button
                  type="button"
                  className="rc-prow"
                  data-testid={`rc-finder-${station.baseCallsign}`}
                  key={`${station.baseCallsign}|${station.grid}`}
                  onClick={() => addStation(station.baseCallsign)}
                >
                  <span className="rc-call mono">{station.baseCallsign}</span>
                  <span className="rc-meta">
                    {station.grid}
                    {mi != null ? ` · ${mi} mi` : ''}
                  </span>
                  <span className="rc-pbands">
                    {stationBands.map((b) => (
                      <span className="rc-pband mono" key={b}>
                        {b}
                      </span>
                    ))}
                  </span>
                  <span
                    className={`rc-star ${favoriteGateways.has(station.baseCallsign) ? '' : 'rc-star-off'}`}
                  >
                    {favoriteGateways.has(station.baseCallsign) ? '★' : '☆'}
                  </span>
                </button>
              ))}
              {finderRows.length > VISIBLE_ROWS && (
                <div className="rc-pnote">
                  +{finderRows.length - VISIBLE_ROWS} more — refine the search
                </div>
              )}
              <div className="rc-pfoot">
                Stations from the finder cache · only gateways with a cached dial on a selected
                band are listed · dial freqs resolve from the cache at run time
              </div>
            </>
          )}

          {pickerTab === 'favorites' && (
            <>
              {favorites.length === 0 && (
                <div className="rc-pnote">No starred favorites for the configured HF mode.</div>
              )}
              {favorites.map((f) => (
                <button
                  type="button"
                  className="rc-prow"
                  data-testid={`rc-fav-${f.gateway}`}
                  key={f.id}
                  onClick={() => addStation(f.gateway)}
                >
                  <span className="rc-call mono">{f.gateway}</span>
                  <span className="rc-meta">
                    {f.band ?? ''}
                    {f.freq ? ` · ${f.freq}` : ''}
                  </span>
                  <span className="rc-star">★</span>
                </button>
              ))}
            </>
          )}
        </div>
      )}

      <div className="rc-label">
        BANDS <span className="rc-hint">HF dials need at least one · empty = packet-dial shape</span>
      </div>
      {bandsRef ? (
        <div className="rc-chips">
          <span className="rc-chip rc-chip-ref mono" data-testid="rc-bands-ref">
            {bandsRef}
            <span className="rc-refnote">step output — edit as JSON to change</span>
          </span>
        </div>
      ) : (
        <div className="rc-bands" data-testid="rc-bands">
          {HF_BANDS.map((b) => (
            <button
              type="button"
              key={b}
              className={`rc-band mono ${bands.includes(b) ? 'rc-band-on' : ''}`}
              data-testid={`rc-band-${b}`}
              aria-pressed={bands.includes(b)}
              onClick={() =>
                commit({
                  bands: bands.includes(b) ? bands.filter((x) => x !== b) : [...bands, b],
                })
              }
            >
              {b}
            </button>
          ))}
        </div>
      )}

      <div className="rc-numrow">
        <span className="rc-label rc-numlabel">listen_before_tx_s</span>
        <input
          className="rc-num mono"
          data-testid="rc-listen"
          inputMode="numeric"
          value={listenText}
          onChange={(e) => setListenText(e.target.value)}
          onBlur={() => {
            const t = listenText.trim();
            if (t === '') {
              commit({ listen: null });
              return;
            }
            const n = Number(t);
            // Backend contract is Option<u64> — integers only (adrev 5.6 P2).
            if (Number.isInteger(n) && n >= 0) {
              commit({ listen: n });
            } else {
              setListenText(listen === undefined ? '' : String(listen));
            }
          }}
        />
        {listenUneditable && (
          <span className="rc-hint" data-testid="rc-listen-uneditable">
            current value is not an integer — kept as-is until replaced
          </span>
        )}
      </div>

      {extraKeys.length > 0 && (
        <div className="rc-extras" data-testid="rc-extras">
          also set: {extraKeys.join(', ')} — edit as JSON
        </div>
      )}
    </div>
  );
}
