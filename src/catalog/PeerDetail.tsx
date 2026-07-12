// PeerDetail — the selected PEER's connect rows in the Station tab (the peers↔L3
// reconciliation, operator-approved render
// docs/design/mockups/2026-07-11-station-intel-l3/station-intel-l3-peer-selected.html).
//
// The peer analog of `BandMatrix`: same rail, same tab, same row/chip ELEMENTS
// (`station-finder__bmrow` / `__bmchip` / `__chipuse` / `__sw--<mode>`), so a peer
// reads as a station rather than a bolted-on surface. It is a SEPARATE component
// (not a BandMatrix mode) because a peer's rows are per-TRANSPORT, not per-BAND: a
// peer carries the exact channels/endpoints its contact record knows, with no VOACAP
// band sweep to lay them across.
//
// PREFILL, NEVER DIAL. A row's click hands the peer's target/freq/via/host/port to
// the matching modem pane and stops there — the operator connects from the pane, the
// same two-step a gateway's "Use →" uses. This is the deliberate CORRECTION over the
// shipped-then-dropped peers finder rail, whose `connectPeerChannel` /
// `connectPeerEndpoint` fired a REAL outbound RF dial straight from the browse
// surface with the modem pane closed — a connect path unlike every other mode in the
// app, and a live transmitter keyed from a list. Those functions still serve
// ContactsPanel's own reachability block; the finder never calls them.
//
// `onUsePeer` omitted → falls back to a bare `emitPeerPrefill`, mirroring
// `BandMatrix`'s own fallback-to-`emitGatewayPrefill` contract (keeps the component
// renderable standalone in tests / a harness).

import { emitPeerPrefill, type PeerPrefill } from '../peers/peerPrefillEvent';
import { radioModeForPeerTransport } from '../peers/connectPeer';
import type { AggregatedPeer } from '../peers/peerModel';
import type { RadioMode } from '../favorites/types';

/** Row label per dialable transport. Covers every `RadioMode` (incl. the two the
 *  catalog's MODE_LABEL omits — `vara-fm` and `telnet`), so a row never falls back
 *  to a raw wire string. */
const PEER_MODE_LABEL: Record<RadioMode, string> = {
  'vara-hf': 'VARA HF',
  'vara-fm': 'VARA FM',
  'ardop-hf': 'ARDOP HF',
  packet: 'Packet',
  telnet: 'Telnet',
};

/** Hz → MHz, 3dp — the dial format the panels' freq field parses (mirrors
 *  BandMatrix's kHz `mhz()` helper, but a peer `Channel.freq_hz` is in Hz). */
const mhz = (hz: number): string => (hz / 1_000_000).toFixed(3);

export interface PeerDetailProps {
  peer: AggregatedPeer;
  /** Prefill handler (AppShell arms the modem under the P2P intent, then emits).
   *  Omitted → bare `emitPeerPrefill`. NEVER a dial. */
  onUsePeer?: (prefill: PeerPrefill) => void;
}

export function PeerDetail({ peer, onUsePeer }: PeerDetailProps) {
  const use = (prefill: PeerPrefill) => {
    if (onUsePeer) onUsePeer(prefill);
    else emitPeerPrefill(prefill);
  };

  // Only channels whose transport maps to a real modem are dialable; an 'unknown'
  // transport has no pane to prefill, so it is omitted rather than rendered dead.
  const rfRows = peer.channels.flatMap((ch) => {
    const mode = radioModeForPeerTransport(ch.transport);
    return mode ? [{ ch, mode }] : [];
  });

  const hasAny = rfRows.length > 0 || peer.endpoints.length > 0;

  return (
    <div className="station-finder__bandmatrix" data-testid="peer-detail">
      <div className="station-finder__bmheader" data-testid="peer-detail-header">
        Connect to {peer.callsign}
      </div>

      {!hasAny && (
        <div className="station-finder__bmrow" data-testid="peer-detail-empty">
          <span className="station-finder__bmnone">no channel</span>
        </div>
      )}

      {rfRows.map(({ ch, mode }, i) => (
        <div
          className="station-finder__bmrow"
          key={`ch-${mode}-${ch.freq_hz ?? 'nofreq'}-${i}`}
          data-testid={`peer-row-${mode}`}
        >
          <span className="station-finder__bn">{PEER_MODE_LABEL[mode]}</span>
          <div className="station-finder__bmchips">
            <span className="station-finder__bmchip">
              <button
                type="button"
                className="station-finder__chipuse"
                data-testid={`peer-use-${mode}`}
                title={`Prefill the ${PEER_MODE_LABEL[mode]} pane to connect ${ch.target_callsign} — you still press Connect there`}
                onClick={() =>
                  use({
                    mode,
                    target: ch.target_callsign,
                    freqHz: ch.freq_hz ?? undefined,
                    // Packet relay path — the pane's via/relay chips consume it.
                    via: ch.via.length > 0 ? ch.via : undefined,
                    contactId: peer.id,
                  })
                }
              >
                <span className={`station-finder__sw station-finder__sw--${mode}`} />
                {ch.freq_hz != null ? mhz(ch.freq_hz) : ch.target_callsign}
              </button>
            </span>
          </div>
        </div>
      ))}

      {peer.endpoints.map((ep) => (
        <div className="station-finder__bmrow" key={ep.id} data-testid={`peer-row-telnet-${ep.id}`}>
          <span className="station-finder__bn">{PEER_MODE_LABEL.telnet}</span>
          <div className="station-finder__bmchips">
            <span className="station-finder__bmchip">
              <button
                type="button"
                className="station-finder__chipuse"
                data-testid={`peer-use-telnet-${ep.id}`}
                title={`Prefill the Telnet pane to connect ${peer.callsign} — you still press Connect there`}
                onClick={() =>
                  use({
                    mode: 'telnet',
                    // A telnet endpoint dial targets the PEER's callsign (the B2F
                    // handshake partner); host/port carry the address.
                    target: peer.callsign,
                    host: ep.host,
                    port: ep.port,
                    contactId: peer.id,
                  })
                }
              >
                <span className="station-finder__sw station-finder__sw--telnet" />
                {ep.host}:{ep.port}
              </button>
            </span>
          </div>
        </div>
      ))}
    </div>
  );
}
