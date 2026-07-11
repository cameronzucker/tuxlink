// connectPeer — the ONE P2P dial seam (Task 23a), extracted from StationRail
// (T-F) so both the finder rail AND the ContactsPanel contact-detail
// reachability block dispatch through the exact same path. NEVER reimplement
// the dispatch: every peer Connect reaches the mode's real backend command via
// `connectFor({ sessionType: 'p2p', protocol })` with the channel's
// target/via/freq threaded, so the backend peer recorder (gated on
// `SessionIntent::P2p`) runs and the wire is indistinguishable from the panel's
// Start → Send/Receive. RADIO-1: the operator's click IS the consent — no added
// modal, identical to the finder rows (no-tuxlink-added-safeguards).

import { connectFor } from '../connections/connectDispatch';
import type { Channel, Endpoint, ChannelTransport } from '../contacts/types';
import type { RadioMode } from '../favorites/types';

/**
 * Peer `ChannelTransport` → modem `RadioMode`; `null` for a transport with no
 * prefillable modem. The peer wire vocabulary differs from the catalog
 * `ListingMode` one — `'ardop'` not `'ardop-hf'`, plus `'vara-fm'`.
 */
export function radioModeForPeerTransport(t: ChannelTransport): RadioMode | null {
  if (t === 'ardop') return 'ardop-hf';
  if (t === 'vara-hf' || t === 'vara-fm' || t === 'packet') return t;
  return null;
}

/**
 * Fire a REAL outbound P2P dial for a peer RF channel (Flow 2). Reaches the
 * SAME backend command the mode's panel uses via `connectFor`, with
 * `intent = 'p2p'` and the channel's `target`/`via`/`freq` threaded explicitly.
 * A `null` transport → no dialable modem, so the caller disables the button.
 *
 * Fire-and-forget: the surfaces that call this run with the RF pane closed
 * (like the ribbon Connect); the backend emits the dial's outcome to the
 * session log. A rejection is swallowed so an RF failure never throws into a
 * React event handler.
 */
export function connectPeerChannel(channel: Channel): void {
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
 * Fire a REAL outbound P2P telnet dial for a peer network endpoint (Flow 2).
 * Reaches `telnet_p2p_connect` (the TelnetP2pRadioPanel's command) with the
 * endpoint's host/port and the peer's callsign; `locator` carries the operator
 * grid for the B2F handshake. Fire-and-forget, same rationale as
 * `connectPeerChannel`.
 */
export function connectPeerEndpoint(
  callsign: string,
  endpoint: Endpoint,
  operatorGrid: string,
  contactId?: string,
): void {
  void connectFor(
    { sessionType: 'p2p', protocol: 'telnet' },
    {
      target: callsign,
      host: endpoint.host,
      port: endpoint.port,
      locator: operatorGrid || undefined,
      // FIX-1: thread the contact + endpoint identity so the backend gates the
      // stored password on Provenance::Operator. Both travel together or not at
      // all — a manual / hand-typed dial (no `contactId`) sends neither, so the
      // backend attaches no stored password. The backend is authoritative; this
      // threading only enables the legitimate operator-endpoint dial.
      contactId,
      endpointId: contactId ? endpoint.id : undefined,
    },
  ).catch(() => {});
}
