/**
 * RadioConnectSection — the `radio.connect` step's dedicated inspector body
 * (tuxlink-fg0em), replacing the generic key/value grid that let operators
 * type params the action does not accept ("target", "freq_hz") and band
 * strings the runtime resolver rejects.
 *
 * Grounded in how the action actually works (`routines/actions/radio.rs`):
 * the step's ONLY radio params are `stations` (gateway callsigns, tried in
 * order) and optional `bands` (closed vocabulary walked per station) —
 * mode and dial frequency are DERIVED at run time from Settings → Modem and
 * the station cache. This section states that instead of hiding it:
 *
 *  - "Runs on" line: the configured HF modem, mirrored from
 *    `config_read.routine_hf_modem` (the runtime's exactly-one rule, incl.
 *    the both/none refusal states surfaced at authoring time).
 *  - Stations: ordered chips + an inline picker over the REAL selection
 *    surfaces — the station-finder cache and the per-mode favorites — never
 *    blind free text. A whole-value step ref (`"$s2.callsigns"`) renders as
 *    a read-only ref chip (edit as JSON to change).
 *  - Bands: toggle chips over `HF_BANDS` (the same vocabulary the backend
 *    ParamSpec now enforces at save time).
 *
 * Params this section does not own (`rig`, unknown keys) are preserved
 * verbatim on every commit and disclosed in a muted note — nothing is
 * silently hidden; "edit as JSON" (the StepInspector toggle) remains the
 * escape hatch for every shape this surface cannot express.
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

/** The keys this section owns; everything else is preserved verbatim. */
const OWNED_KEYS = ['stations', 'bands', 'listen_before_tx_s'] as const;

/** `config_read`'s slice this section consumes (wire shape pinned by the
 *  Rust test `routine_hf_modem_view_wire_shape`). */
interface HfModemView {
  kind: 'vara' | 'ardop' | 'both' | 'none';
  bandwidth_hz?: number | null;
}

function asStringArray(v: unknown): string[] | null {
  return Array.isArray(v) && v.every((e) => typeof e === 'string') ? (v as string[]) : null;
}

/** Finder listing modes for the configured modem; both/none fall back to
 *  both HF modes so the picker still works while Settings is unresolved. */
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
  const bands = asStringArray(params['bands']) ?? [];
  const listenRaw = params['listen_before_tx_s'];
  const listen = typeof listenRaw === 'number' ? listenRaw : undefined;

  /** Keys preserved verbatim (incl. owned keys whose shape this surface
   *  cannot express, e.g. a non-number listen value). */
  const extraKeys = Object.keys(params).filter(
    (k) =>
      !(OWNED_KEYS as readonly string[]).includes(k) ||
      (k === 'listen_before_tx_s' && listenRaw !== undefined && listen === undefined),
  );

  const [listenText, setListenText] = useState(listen === undefined ? '' : String(listen));

  function commit(next: {
    stations?: string[] | string;
    bands?: string[];
    listen?: number | undefined;
  }) {
    const out: Record<string, unknown> = {};
    const outStations = next.stations ?? stationsRef ?? stations;
    out['stations'] = outStations;
    const outBands = next.bands ?? bands;
    if (outBands.length > 0) out['bands'] = outBands;
    const outListen = 'listen' in next ? next.listen : listen;
    if (outListen !== undefined) out['listen_before_tx_s'] = outListen;
    for (const k of extraKeys) out[k] = params[k];
    onChange(out);
  }

  // ---- "Runs on" — the modem the run will actually dial ----
  const modemQuery = useQuery({
    queryKey: ['config_read'],
    queryFn: () => invoke<{ routine_hf_modem?: HfModemView }>('config_read'),
  });
  const modem = modemQuery.data?.routine_hf_modem;

  // ---- picker state + data ----
  const [pickerOpen, setPickerOpen] = useState(false);
  const [pickerTab, setPickerTab] = useState<'finder' | 'favorites'>('finder');
  const [search, setSearch] = useState('');
  const { listings, loading, error, fetch } = useStations();
  const fetchedRef = useRef(false);
  useEffect(() => {
    if (pickerOpen && pickerTab === 'finder' && !fetchedRef.current) {
      fetchedRef.current = true;
      fetch(finderModes(modem));
    }
  }, [pickerOpen, pickerTab, fetch, modem]);

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
          ...new Set(s.channels.map((c) => c.band).filter((b): b is Band => b != null && b !== 'vhf-uhf')),
        ];
        const km = operatorGrid ? distanceFromGrids(operatorGrid, s.grid) : null;
        return { station: s, stationBands, mi: km == null ? null : Math.round(kmToMi(km)) };
      })
      .filter(({ station, stationBands }) => {
        if (q && !station.baseCallsign.includes(q) && !station.grid.toUpperCase().startsWith(q)) {
          return false;
        }
        // Only gateways with a cached dial on a selected band (mock footer
        // rule); no bands selected = no band filter.
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

  return (
    <div className="rc-section" data-testid="radio-connect-section">
      {modem && (
        <div
          className={`rc-runson ${modem.kind === 'both' || modem.kind === 'none' ? 'rc-runson-warn' : ''}`}
          data-testid="rc-runson"
        >
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
              No HF modem configured (Settings → Modem) — HF dials cannot run.
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
            <span className="rc-chip mono" data-testid={`rc-station-${s}`} key={s}>
              <span className="rc-ord">{i + 1}</span>
              {s}
              <button
                type="button"
                className="rc-x"
                aria-label={`Remove station ${s}`}
                onClick={() => commit({ stations: stations.filter((x) => x !== s) })}
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
        BANDS <span className="rc-hint">empty = any cached band</span>
      </div>
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
              commit({ listen: undefined });
              return;
            }
            const n = Number(t);
            if (Number.isFinite(n) && n >= 0) {
              commit({ listen: n });
            } else {
              setListenText(listen === undefined ? '' : String(listen));
            }
          }}
        />
      </div>

      {extraKeys.length > 0 && (
        <div className="rc-extras" data-testid="rc-extras">
          also set: {extraKeys.join(', ')} — edit as JSON
        </div>
      )}
    </div>
  );
}
