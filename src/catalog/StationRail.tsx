// Right rail for Find-a-Station (design §7): selected-station header → antenna
// aiming hero → path propagation forecast → channels grouped by mode/frequency
// with per-channel reliability + Use →. Replaces the old redundant station list.
// Use → emits emitGatewayPrefill for a channel matching the open modem; other
// channels are listed but their Use → is disabled with a hint (RADIO-1: this
// only fills a form — the operator still clicks Connect).

import { useState, type CSSProperties } from 'react';
import { groupChannelsByMode, channelToDial, channelReliability } from './channelGrouping';
import { rankedDialsFor } from './ranking';
import { bestBandNow, relToTier, tierColorVar } from './reachability';
import { bandLabel, bandForKhz, HF_BANDS } from './bandPlan';
import { emitGatewayPrefill } from '../favorites/prefillEvent';
import { distanceFromGrids, kmToMi } from './distance';
import { gridToLatLon } from '../forms/position/maidenhead';
import type { Station, Channel } from './stationModel';
import type { PathPrediction } from './propagationApi';
import type { PredictionStatus } from './useStationPrediction';
import type { RadioMode, FavoriteDial } from '../favorites/types';
import type { AggregatedPeer } from '../peers/peerModel';
import {
  connectPeerChannel,
  connectPeerEndpoint,
  radioModeForPeerTransport,
} from '../peers/connectPeer';
import { validateCallsign } from '../wizard/validators';
import { parseFreqInputToHz } from '../radio/modes/freq';
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
  /**
   * Show the "Dial a station" manual-dial affordance (Task T-G, spec
   * §AMENDMENT pt. 7) — the ONLY way to dial a callsign the operator has not
   * heard, from an empty peer roster. Gated on the SAME `finder_peers`
   * capability bit that shows the Peer rows (`useP2pCapabilities` in
   * StationFinderPanel) — not on whether any peers happen to be visible right
   * now, so the affordance still renders when `peers` is empty (Flow 2(b)).
   * Defaults to false (hidden) when omitted — matches the peer rows' hide,
   * not disable, posture.
   */
  p2pDialEnabled?: boolean;
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

/** Format a SUCCESS timestamp (`last_ok`) for display. Renamed from
 *  `formatLastConnected` (T-F Part 3): its input is now the success-only
 *  `lastOk`, not the failure-bumping `lastSeen`, so the name states the truth. */
function formatReachedAt(iso: string): string {
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
  const { station, prediction, predictionStatus, operatorGrid, utcHour, activePrefillMode, peers, p2pDialEnabled } = props;

  // Task 23a (Flow 2): a peer row's Connect fires a REAL P2P dial via
  // connectFor — NOT the gateway `onUse` prefill path AppShell hardcodes to
  // sessionType 'cms'. The dial reaches the mode's backend command with
  // intent='p2p' + the channel's via/freq (connectPeerChannel /
  // connectPeerEndpoint). operatorGrid supplies the telnet handshake locator.
  //
  // Task T-G: the manual-dial affordance sits ABOVE the peer list — it must
  // render even when `peers` is empty (an unheard station has no roster row
  // yet), so it cannot be gated on `peers.length`. It reuses the EXACT same
  // connectPeerChannel/connectPeerEndpoint seam as the rows below.
  const peerRows = (
    <>
      {p2pDialEnabled && (
        <ManualDialForm
          operatorGrid={operatorGrid}
          onConnectChannel={connectPeerChannel}
          onConnectEndpoint={connectPeerEndpoint}
        />
      )}
      <PeerRows
        peers={peers ?? []}
        operatorGrid={operatorGrid}
        onConnectChannel={connectPeerChannel}
        onConnectEndpoint={(peer, ep, grid) => connectPeerEndpoint(peer.callsign, ep, grid, peer.id)}
      />
    </>
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
            {/* Success-only recency (T-F Part 0): a failed dial must never show
                as "reached". `lastOk` is the honest instant; absent it, the row
                says so plainly rather than mislabeling a failed-attempt time. */}
            {peer.lastOk
              ? ` · last reached ${formatReachedAt(peer.lastOk)}`
              : ' · not reached yet'}
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

/** The p2p-capable transport set the connect seam supports for a manual dial
 *  (spec §AMENDMENT pt. 7). Four RF transports route through
 *  `connectPeerChannel` (a `Channel`-shaped dispatch); `telnet` routes through
 *  `connectPeerEndpoint` (host/port, no freq/via) — it is not a
 *  `ChannelTransport` member, so it is handled as a distinct branch below. */
const P2P_DIAL_TRANSPORTS: { value: ChannelTransport | 'telnet'; label: string }[] = [
  { value: 'vara-hf', label: 'VARA HF' },
  { value: 'vara-fm', label: 'VARA FM' },
  { value: 'ardop', label: 'ARDOP HF' },
  { value: 'packet', label: 'Packet' },
  { value: 'telnet', label: 'Telnet' },
];

/**
 * "Dial a station" (Task T-G, spec §AMENDMENT pt. 7) — a compact manual-dial
 * affordance next to the finder's Peer rows: type a callsign that has never
 * been heard and dial it directly, for the empty-roster case Flow 2(b)
 * depends on. PURE FRONTEND per the task brief: it collects the dial
 * parameters and dispatches through the EXISTING peer-connect seam
 * (`connectPeerChannel` / `connectPeerEndpoint`) — the SAME functions the
 * peer rows below use — so the backend observation recorder (armed on every
 * p2p dial attempt, success or failure) auto-creates the unconfirmed contact.
 * No new persistence code; no new dispatch path.
 *
 * The `Channel`/`Endpoint` objects built here carry placeholder values for
 * fields `connectPeerChannel`/`connectPeerEndpoint` never read (`counts`,
 * `last_seen`, `last_ok`, `id`, …) — those functions only consume
 * `transport`/`target_callsign`/`via`/`freq_hz` (channel) or `host`/`port`
 * (endpoint); the backend's OWN observation record is the real, authoritative
 * write (T-B), not anything constructed here.
 */
function ManualDialForm({
  operatorGrid,
  onConnectChannel,
  onConnectEndpoint,
}: {
  operatorGrid: string;
  onConnectChannel: (channel: PeerChannel) => void;
  onConnectEndpoint: (callsign: string, endpoint: PeerEndpoint, operatorGrid: string) => void;
}) {
  const [callsign, setCallsign] = useState('');
  const [transport, setTransport] = useState<ChannelTransport | 'telnet'>('vara-hf');
  const [freqMhz, setFreqMhz] = useState('');
  const [via, setVia] = useState('');
  const [host, setHost] = useState('');
  const [port, setPort] = useState('');
  const [error, setError] = useState<string | null>(null);

  const isTelnet = transport === 'telnet';
  const isPacket = transport === 'packet';

  // Clears every field after a dispatch (mirrors GroupEditor's "+ Add" raw-
  // callsign idiom: type → commit → clear, ready for the next entry). The
  // brief's own free-persistence claim is why: retry after this dial lives in
  // the Recent/finder rows this SAME dial just created a record for, not in
  // this box — unlike the RF panels' persistent target field (built for
  // repeated redial of the SAME gateway), a manual dial here is a one-shot
  // "reach this new station" action.
  const reset = () => {
    setCallsign('');
    setFreqMhz('');
    setVia('');
    setHost('');
    setPort('');
    setError(null);
  };

  const onConnect = () => {
    const typed = callsign.trim().toUpperCase();
    const callErr = validateCallsign(typed);
    if (callErr) {
      setError(callErr);
      return;
    }

    if (isTelnet) {
      const trimmedHost = host.trim();
      const portNum = Number(port.trim());
      if (!trimmedHost) {
        setError('Host is required for a telnet dial.');
        return;
      }
      if (!Number.isInteger(portNum) || portNum < 1 || portNum > 65535) {
        setError('Port must be 1–65535.');
        return;
      }
      const endpoint: PeerEndpoint = {
        id: 'manual-dial',
        host: trimmedHost,
        port: portNum,
        provenance: 'operator',
        last_seen: new Date().toISOString(),
        last_ok: null,
      };
      onConnectEndpoint(typed, endpoint, operatorGrid);
      reset();
      return;
    }

    // RF transport (vara-hf/vara-fm/ardop/packet). Freq is OPTIONAL — same
    // rule as the RF panels' Manual freq field: an empty/unparseable MHz
    // string yields freq_hz: null, which connectFor threads as an undefined
    // freqHz (direct dial, no pre-audio CAT tune) rather than blocking Connect.
    const freqHz = parseFreqInputToHz(freqMhz);
    // Packet-only digipeater path, 0-2 entries — same cap as the PacketRadioPanel
    // Connect sub-section's relay chips.
    const viaList = isPacket
      ? via.split(',').map((v) => v.trim()).filter(Boolean).slice(0, 2)
      : [];
    const channel: PeerChannel = {
      transport,
      target_callsign: typed,
      via: viaList,
      freq_hz: freqHz,
      bandwidth: null,
      direction: 'outgoing',
      counts: { ok: 0, fail: 0 },
      last_seen: new Date().toISOString(),
      last_ok: null,
      last_ok_direction: null,
    };
    onConnectChannel(channel);
    reset();
  };

  return (
    <div className="station-finder__dial" data-testid="manual-dial-form">
      <div className="station-finder__chh">Dial a station</div>
      <div className="station-finder__dial-row">
        <input
          type="text"
          data-testid="manual-dial-callsign"
          className="station-finder__dial-call"
          placeholder="Callsign"
          value={callsign}
          onChange={(e) => {
            setCallsign(e.target.value.toUpperCase());
            setError(null);
          }}
          autoComplete="off"
          spellCheck={false}
        />
        <select
          data-testid="manual-dial-transport"
          className="station-finder__dial-transport"
          value={transport}
          onChange={(e) => {
            setTransport(e.target.value as ChannelTransport | 'telnet');
            setError(null);
          }}
        >
          {P2P_DIAL_TRANSPORTS.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
        {isTelnet ? (
          <>
            <input
              type="text"
              data-testid="manual-dial-host"
              className="station-finder__dial-host"
              placeholder="Host"
              value={host}
              onChange={(e) => setHost(e.target.value)}
              autoComplete="off"
            />
            <input
              type="text"
              inputMode="numeric"
              data-testid="manual-dial-port"
              className="station-finder__dial-port"
              placeholder="Port"
              value={port}
              onChange={(e) => setPort(e.target.value)}
              autoComplete="off"
            />
          </>
        ) : (
          <input
            type="text"
            inputMode="decimal"
            data-testid="manual-dial-freq"
            className="station-finder__dial-freq"
            placeholder="MHz"
            value={freqMhz}
            onChange={(e) => setFreqMhz(e.target.value)}
            autoComplete="off"
          />
        )}
        {isPacket && (
          <input
            type="text"
            data-testid="manual-dial-via"
            className="station-finder__dial-via"
            placeholder="via (0–2, comma-sep)"
            value={via}
            onChange={(e) => setVia(e.target.value)}
            autoComplete="off"
          />
        )}
        <button
          type="button"
          data-testid="manual-dial-connect"
          className="station-finder__use"
          title={`Dial ${callsign.trim() || 'a station'} over ${P2P_DIAL_TRANSPORTS.find((o) => o.value === transport)?.label ?? transport}`}
          onClick={onConnect}
        >
          Connect →
        </button>
      </div>
      {error && (
        <div className="station-finder__dial-error" data-testid="manual-dial-error" role="alert">
          {error}
        </div>
      )}
    </div>
  );
}
