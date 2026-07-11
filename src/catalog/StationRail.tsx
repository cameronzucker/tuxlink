// Right rail for Find-a-Station (design §7): selected-station header → antenna
// aiming hero → path propagation forecast → channels grouped by mode/frequency
// with per-channel reliability + Use →. Replaces the old redundant station list.
// Use → emits emitGatewayPrefill for a channel matching the open modem; other
// channels are listed but their Use → is disabled with a hint (RADIO-1: this
// only fills a form — the operator still clicks Connect).

import type { CSSProperties } from 'react';
import { groupChannelsByMode, channelToDial, channelReliability } from './channelGrouping';
import { rankedDialsFor } from './ranking';
import { bestBandNow, relToTier, tierColorVar } from './reachability';
import { bandLabel, bandForKhz, HF_BANDS } from './bandPlan';
import { emitGatewayPrefill } from '../favorites/prefillEvent';
import { connectFor } from '../connections/connectDispatch';
import { distanceFromGrids, kmToMi } from './distance';
import { gridToLatLon } from '../forms/position/maidenhead';
import type { Station, Channel } from './stationModel';
import type { PathPrediction } from './propagationApi';
import type { PredictionStatus } from './useStationPrediction';
import type { RadioMode, FavoriteDial } from '../favorites/types';
import type { AggregatedPeer } from '../peers/peerModel';
import type {
  Channel as PeerChannel,
  Endpoint as PeerEndpoint,
  ChannelTransport,
  Origin,
  Provenance,
} from '../contacts/types';

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
   * P2P peer rows (Task 23, spec §5) — the panel's `aggregatePeers(usePeers().peers)`
   * output, already filtered by the type toggle + capability hide. Rendered as a
   * distinct list, independent of `station`/`selectedKey`: a gridless/telnet-only
   * peer has no map pin to select, so it must still show up here (untiered) or it
   * is invisible entirely. Omitted/empty renders nothing (no section).
   *
   * Connect here fires a REAL outbound peer dial (Task 23a, Flow 2): the row's
   * Connect invokes `connectFor({ sessionType: 'p2p', protocol })` directly —
   * NOT the gateway `onUse` prefill path — so the dial reaches the same backend
   * command the mode's panel uses with `intent = 'p2p'` and the channel's
   * `via`/`freq` threaded. There is NO CMS fallback on the peer path.
   */
  peers?: AggregatedPeer[];
}

/** Plain-language origin labels (spec §5): no "worked", never the raw enum. */
const ORIGIN_LABEL: Record<Origin, string> = {
  incoming: 'incoming',
  outgoing: 'outgoing',
  manual: 'added',
  aprs: 'APRS',
  unknown: 'unknown origin',
};

const PROVENANCE_LABEL: Record<Provenance, string> = {
  operator: 'operator-added',
  'observed-incoming': 'observed',
  unknown: 'unknown provenance',
};

const PEER_TRANSPORT_LABEL: Record<ChannelTransport, string> = {
  packet: 'Packet',
  ardop: 'ARDOP HF',
  'vara-hf': 'VARA HF',
  'vara-fm': 'VARA FM',
  unknown: 'Unknown',
};

/** Peer ChannelTransport → modem RadioMode; null for a transport with no
 *  prefillable modem (mirrors channelGrouping.ts's radioModeFor, but the
 *  peer wire vocabulary differs from the catalog ListingMode one — 'ardop'
 *  not 'ardop-hf', plus 'vara-fm'). */
function radioModeForPeerTransport(t: ChannelTransport): RadioMode | null {
  if (t === 'ardop') return 'ardop-hf';
  if (t === 'vara-hf' || t === 'vara-fm' || t === 'packet') return t;
  return null;
}

/**
 * Task 23a (Flow 2): fire a REAL outbound P2P dial for a peer RF channel.
 * Reaches the SAME backend command the mode's panel uses via `connectFor`, with
 * `intent = 'p2p'` (so the backend peer recorder, gated on `SessionIntent::P2p`,
 * runs) and the channel's `target`/`via`/`freq` threaded explicitly. `null`
 * transport → no dialable modem, so the caller disables the button.
 *
 * Fire-and-forget: the finder has no inline error surface (it runs with the RF
 * pane closed, like the ribbon Connect), and the backend emits the dial's
 * outcome to the session log. A rejection is swallowed here so an RF failure
 * never throws into React's event handler.
 */
function connectPeerChannel(channel: PeerChannel): void {
  const protocol = radioModeForPeerTransport(channel.transport);
  if (!protocol) return;
  void connectFor(
    { sessionType: 'p2p', protocol },
    {
      target: channel.target_callsign,
      via: channel.via,
      freqHz: channel.freq_hz ?? undefined,
    },
  ).catch(() => {});
}

/**
 * Task 23a (Flow 2): fire a REAL outbound P2P telnet dial for a peer network
 * endpoint. Reaches `telnet_p2p_connect` (the TelnetP2pRadioPanel's command)
 * with the endpoint's host/port and the peer's callsign; `locator` carries the
 * operator grid for the B2F handshake. Fire-and-forget, same rationale as
 * `connectPeerChannel`.
 */
function connectPeerEndpoint(
  peer: AggregatedPeer,
  endpoint: PeerEndpoint,
  operatorGrid: string,
): void {
  void connectFor(
    { sessionType: 'p2p', protocol: 'telnet' },
    {
      target: peer.callsign,
      host: endpoint.host,
      port: endpoint.port,
      locator: operatorGrid || undefined,
    },
  ).catch(() => {});
}

function formatLastConnected(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString();
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
  const { station, prediction, predictionStatus, operatorGrid, utcHour, activePrefillMode, peers } = props;

  // Task 23a (Flow 2): a peer row's Connect fires a REAL P2P dial via
  // connectFor — NOT the gateway `onUse` prefill path AppShell hardcodes to
  // sessionType 'cms'. The dial reaches the mode's backend command with
  // intent='p2p' + the channel's via/freq (connectPeerChannel /
  // connectPeerEndpoint). operatorGrid supplies the telnet handshake locator.
  const peerRows = (
    <PeerRows
      peers={peers ?? []}
      operatorGrid={operatorGrid}
      onConnectChannel={connectPeerChannel}
      onConnectEndpoint={connectPeerEndpoint}
    />
  );

  if (!station) {
    return (
      <div className="station-finder__rail">
        <div className="station-finder__rail--empty">Select a station on the map.</div>
        {peerRows}
      </div>
    );
  }

  const bearing = prediction?.bearingDeg ?? (operatorGrid ? bearingFromGrids(operatorGrid, station.grid) : null);
  const distKm = prediction?.distanceKm ?? (operatorGrid ? distanceFromGrids(operatorGrid, station.grid) : null);
  const distMi = distKm != null ? Math.round(kmToMi(distKm)) : null;
  const best = prediction ? bestBandNow(prediction, utcHour) : null;

  const onUse = (channel: Channel) => {
    const dial = channelToDial(station, channel);
    if (!dial) return;
    // tuxlink-8fkkk Task B: the QSY-on-fail walk needs the station's OTHER
    // channels for this mode, ranked best-first. Compute them here where the
    // station + prediction + utcHour are in scope and pass them alongside the
    // primary dial.
    //
    // The clicked `dial` MUST be the PRIMARY candidate (index 0): the backend
    // treats a non-empty `qsyCandidates` list as OVERRIDING the form's
    // target/freq, so it dials candidates[0] first. `rankedDialsFor` returns
    // channels ranked best-first (and capped), which may NOT be the clicked
    // channel — and could even omit it under the cap. Force the clicked dial to
    // the front, then append the ranked channels minus a duplicate of it, so
    // "Use" on channel X always dials X first and only QSYs to others on
    // failure.
    const ranked = rankedDialsFor(station, dial.mode, prediction, utcHour);
    const sameDial = (a: FavoriteDial, b: FavoriteDial) =>
      a.gateway === b.gateway && a.freq === b.freq;
    const candidates = [dial, ...ranked.filter((d) => !sameDial(d, dial))];
    // Arm-on-demand (tuxlink-s0r1): AppShell opens the matching modem panel then
    // prefills it. Fall back to a bare emit for an already-open panel in contexts
    // that don't supply the handler (e.g. tests/standalone harness).
    if (props.onUse) props.onUse(dial, candidates);
    else emitGatewayPrefill(dial, candidates);
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
      {peerRows}
    </div>
  );
}

/**
 * P2P peer rows (Task 23, spec §5) — rendered as a list independent of the
 * map-pin `station` selection, since a gridless/telnet-only peer has no pin
 * to click. `null` when there are no peers to show (no empty section chrome).
 */
function PeerRows({
  peers,
  operatorGrid,
  onConnectChannel,
  onConnectEndpoint,
}: {
  peers: AggregatedPeer[];
  operatorGrid: string;
  onConnectChannel: (channel: PeerChannel) => void;
  onConnectEndpoint: (peer: AggregatedPeer, endpoint: PeerEndpoint, operatorGrid: string) => void;
}) {
  if (peers.length === 0) return null;
  return (
    <div className="station-finder__peers" data-testid="peer-rows">
      <div className="station-finder__chh">
        Peers
        <span className="station-finder__chh-n">{peers.length}</span>
      </div>
      {peers.map((peer) => (
        <div key={peer.id} className="station-finder__peer" data-testid={`peer-row-${peer.id}`}>
          <div className="station-finder__peer-top">
            <span className="station-finder__call">{peer.callsign}</span>
            <span className="station-finder__peer-origin" data-testid={`peer-origin-${peer.id}`}>
              {ORIGIN_LABEL[peer.origin]}
            </span>
          </div>
          <div className="station-finder__who">
            {peer.grid ? peer.grid : <span data-testid={`peer-untiered-${peer.id}`}>no grid — untiered</span>}
            {peer.lastSeen && ` · last connected ${formatLastConnected(peer.lastSeen)}`}
          </div>

          {peer.endpoints.length > 0 && (
            <div className="station-finder__peer-endpoints">
              {peer.endpoints.map((ep) => (
                <div key={ep.id} className="station-finder__peer-endpoint" data-testid={`peer-endpoint-${ep.id}`}>
                  <span className="station-finder__peer-badge">{PROVENANCE_LABEL[ep.provenance]}</span>
                  <span className="station-finder__peer-ep-addr">
                    {ep.host}:{ep.port}
                  </span>
                  {/* Task 23a: a telnet peer-endpoint dial (Flow 2) — dials the
                      peer's TCP listener over P2P telnet via connectFor. */}
                  <button
                    type="button"
                    data-testid={`peer-endpoint-connect-${ep.id}`}
                    className="station-finder__use"
                    title={`Connect to ${peer.callsign} over telnet`}
                    onClick={() => onConnectEndpoint(peer, ep, operatorGrid)}
                  >
                    Connect →
                  </button>
                </div>
              ))}
            </div>
          )}

          {peer.channels.length > 0 && (
            <div className="station-finder__peer-channels">
              {peer.channels.map((ch, i) => {
                const protocol = radioModeForPeerTransport(ch.transport);
                const label = PEER_TRANSPORT_LABEL[ch.transport] ?? ch.transport;
                return (
                  <div
                    key={`${ch.transport}-${ch.target_callsign}-${ch.freq_hz ?? 'nofreq'}-${i}`}
                    className="station-finder__peer-channel"
                    data-testid={`peer-channel-${peer.id}-${i}`}
                  >
                    <div>
                      <div className="station-finder__f">
                        {label} · {ch.target_callsign}
                      </div>
                      <div className="station-finder__sub">
                        {ch.freq_hz != null ? `${(ch.freq_hz / 1_000_000).toFixed(3)} MHz` : 'freq unknown'}
                      </div>
                    </div>
                    <button
                      type="button"
                      data-testid={`peer-use-${peer.id}-${i}`}
                      className="station-finder__use"
                      disabled={!protocol}
                      title={
                        !protocol
                          ? 'No tuxlink modem for this transport'
                          : `Connect to ${ch.target_callsign} over ${label}`
                      }
                      onClick={() => protocol && onConnectChannel(ch)}
                    >
                      Connect →
                    </button>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
