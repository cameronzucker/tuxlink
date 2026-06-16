// Right rail for Find-a-Station (design §7): selected-station header → antenna
// aiming hero → path propagation forecast → channels grouped by mode/frequency
// with per-channel reliability + Use →. Replaces the old redundant station list.
// Use → emits emitGatewayPrefill for a channel matching the open modem; other
// channels are listed but their Use → is disabled with a hint (RADIO-1: this
// only fills a form — the operator still clicks Connect).

import type { CSSProperties } from 'react';
import { groupChannelsByMode, channelToDial, channelReliability } from './channelGrouping';
import { bestBandNow, relToTier, tierColorVar } from './reachability';
import { bandLabel, bandForKhz, HF_BANDS } from './bandPlan';
import { emitGatewayPrefill } from '../favorites/prefillEvent';
import { distanceFromGrids, kmToMi } from './distance';
import { gridToLatLon } from '../forms/position/maidenhead';
import type { Station, Channel } from './stationModel';
import type { PathPrediction } from './propagationApi';
import type { PredictionStatus } from './useStationPrediction';
import type { RadioMode, FavoriteDial } from '../favorites/types';

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
   * for an already-open panel.
   */
  onUse?: (dial: FavoriteDial) => void;
  /**
   * Save / unsave a discovered channel as a starred favorite (tuxlink-5016).
   * The parent finds-or-creates the per-mode favorite and toggles its star;
   * omitting the prop hides the affordance (e.g. a standalone harness).
   */
  onSaveFavorite?: (dial: FavoriteDial) => void;
  /** Whether a channel's dial is already a STARRED favorite (drives the ★ fill). */
  isSaved?: (dial: FavoriteDial) => boolean;
}

const mhz = (khz: number): string => (khz / 1000).toFixed(3);
const MODE_LABEL: Record<string, string> = {
  'vara-hf': 'VARA HF',
  'ardop-hf': 'ARDOP HF',
  packet: 'Packet',
  pactor: 'Pactor',
  'robust-packet': 'Robust Packet',
};

/** Great-circle bearing from two grids (deg), for the distance-only fallback. */
function bearingFromGrids(a: string, b: string): number | null {
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

export function StationRail(props: StationRailProps) {
  const { station, prediction, predictionStatus, operatorGrid, utcHour, activePrefillMode } = props;

  if (!station) {
    return <div className="station-finder__rail station-finder__rail--empty">Select a station on the map.</div>;
  }

  const bearing = prediction?.bearingDeg ?? (operatorGrid ? bearingFromGrids(operatorGrid, station.grid) : null);
  const distKm = prediction?.distanceKm ?? (operatorGrid ? distanceFromGrids(operatorGrid, station.grid) : null);
  const distMi = distKm != null ? Math.round(kmToMi(distKm)) : null;
  const best = prediction ? bestBandNow(prediction, utcHour) : null;

  const onUse = (channel: Channel) => {
    const dial = channelToDial(station, channel);
    if (!dial) return;
    // Arm-on-demand (tuxlink-s0r1): AppShell opens the matching modem panel then
    // prefills it. Fall back to a bare emit for an already-open panel in contexts
    // that don't supply the handler (e.g. tests/standalone harness).
    if (props.onUse) props.onUse(dial);
    else emitGatewayPrefill(dial);
  };

  const onSave = (channel: Channel) => {
    const dial = channelToDial(station, channel);
    if (!dial || !props.onSaveFavorite) return;
    props.onSaveFavorite(dial);
  };

  return (
    <div className="station-finder__rail">
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

      <div className="station-finder__aim">
        <div
          className="station-finder__compass"
          style={bearing != null ? ({ ['--bearing']: `${bearing}deg` } as CSSProperties) : undefined}
          aria-hidden
        >
          <span className="station-finder__needle" />
        </div>
        <div>
          <div className="station-finder__big">{bearing != null ? `${Math.round(bearing)}°` : '—'}</div>
          <div className="station-finder__lab">aim antenna</div>
        </div>
        <div className="station-finder__dist" data-testid="aim-distance">
          <div className="station-finder__big">{distMi != null ? `${distMi} mi` : '—'}</div>
          <div className="station-finder__lab">short path</div>
        </div>
      </div>

      {predictionStatus === 'ok' && prediction ? (
        <div className="station-finder__prop">
          <h4>
            Path forecast · you → {station.baseCallsign}
            {best && <span className="station-finder__best">best now: {bandLabel(best.band)}</span>}
          </h4>
          {HF_BANDS.map((b) => {
            const pc = prediction.channels.find((c) => bandForKhz(c.frequencyKhz) === b);
            const rel = pc ? pc.relByHour[utcHour] ?? 0 : null;
            return (
              <div key={b} className={`station-finder__pbar${best?.band === b ? ' is-current' : ''}`}>
                <span className="station-finder__bn">{bandLabel(b)}</span>
                <div className="station-finder__track">
                  <div
                    className="station-finder__fill"
                    style={{
                      width: `${Math.round((rel ?? 0) * 100)}%`,
                      // Colour the bar by reachability tier (good→green … skip→red),
                      // matching the per-channel pip and mock D — not a static fill.
                      background: rel != null ? tierColorVar(relToTier(rel)) : undefined,
                    }}
                  />
                </div>
                <span className="station-finder__pct">{rel != null ? `${Math.round(rel * 100)}%` : '—'}</span>
              </div>
            );
          })}
        </div>
      ) : (
        <div className="station-finder__prop station-finder__prop--degraded">
          {predictionStatus === 'no-location'
            ? 'Set your location in the status bar to see the path forecast.'
            : 'Forecast unavailable — showing channels without reliability.'}
        </div>
      )}

      <div className="station-finder__channels">
        {groupChannelsByMode(station).map((group) => (
          <div key={group.mode}>
            <div className="station-finder__chh">
              <span className={`station-finder__sw station-finder__sw--${group.mode}`} />
              {MODE_LABEL[group.mode] ?? group.mode}
              <span className="station-finder__chh-n">{group.channels.length} ch</span>
            </div>
            {group.channels.map((ch) => {
              const rel = prediction ? channelReliability(ch, prediction, utcHour) : null;
              const dialable = channelToDial(station, ch) != null;
              // The matching modem is already open — purely informational now that
              // Use → arms on demand (tuxlink-s0r1); kept for the "armed" affordance.
              const active = activePrefillMode != null && ch.mode === activePrefillMode;
              return (
                <div
                  key={`${ch.mode}-${ch.frequencyKhz}-${ch.ssid ?? ''}`}
                  className={`station-finder__ch${rel?.tier === 'skip' ? ' is-dim' : ''}`}
                >
                  <span
                    className="station-finder__rel"
                    style={rel ? { background: `var(--reach-${rel.tier})` } : undefined}
                  />
                  <div>
                    <div className="station-finder__f">{mhz(ch.frequencyKhz)} MHz</div>
                    <div className="station-finder__sub">
                      {ch.band === 'vhf-uhf'
                        ? `VHF/UHF · local${ch.ssid ? ` · connect ${ch.ssid}` : ''}`
                        : bandLabel(ch.band ?? '40m')}
                    </div>
                  </div>
                  <span className="station-finder__q">
                    {rel ? `${Math.round(rel.rel * 100)}%` : ch.band === 'vhf-uhf' ? 'LoS?' : '—'}
                  </span>
                  {props.onSaveFavorite && (() => {
                    // Save / unsave this channel as a starred favorite (tuxlink-5016).
                    // Only meaningful for dialable channels (a non-tuxlink mode has
                    // no per-mode favorite to hold). The dial is non-null here
                    // because dialable === (channelToDial != null).
                    const saved = dialable && props.isSaved ? props.isSaved(channelToDial(station, ch)!) : false;
                    return (
                      <button
                        type="button"
                        data-testid={`save-${ch.mode}-${ch.frequencyKhz}`}
                        className={`station-finder__save${saved ? ' is-saved' : ''}`}
                        disabled={!dialable}
                        aria-pressed={saved}
                        title={
                          !dialable
                            ? 'No tuxlink modem for this mode'
                            : saved
                              ? 'Remove from favorites'
                              : 'Save to favorites'
                        }
                        onClick={() => onSave(ch)}
                      >
                        {saved ? '★' : '☆'}
                      </button>
                    );
                  })()}
                  <button
                    type="button"
                    data-testid={`use-${ch.mode}-${ch.frequencyKhz}`}
                    className="station-finder__use"
                    disabled={!dialable}
                    title={
                      !dialable
                        ? 'No tuxlink modem for this mode'
                        : active
                          ? `Prefill the open ${MODE_LABEL[ch.mode]} modem`
                          : `Open the ${MODE_LABEL[ch.mode]} modem and prefill this channel`
                    }
                    onClick={() => onUse(ch)}
                  >
                    Use →
                  </button>
                </div>
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}
