// src/aprs/aprsTypes.ts
//
// Frontend mirror of the APRS tactical-chat wire shapes emitted by the Rust
// backend. MIRROR the serde wire forms EXACTLY — `DeliveryState` serializes as
// camelCase, so the TS union must too.
//
// APRS is a single OPEN CHANNEL (party line), not a set of conversations. The
// UI model is one flat, time-ordered feed of every message heard on the channel
// plus our own sends — see `ChannelMessage`. There is no per-callsign thread.

/// Delivery lifecycle of an outgoing APRS message, as the backend reports it
/// over `aprs-message:state`. Wire forms are camelCase (serde
/// `rename_all = "camelCase"`).
export type DeliveryState = 'sent' | 'acked' | 'timedOut' | 'rejected';

/// Payload of `aprs-message:new` — a received APRS text message. `addressee` is
/// the callsign the message was directed to, or `""` for a broadcast (no
/// addressee / blank 9-space field on the wire). `msgid` is null when the
/// sender's message carried no message number (unacked APRS text).
export interface InboundMsgDto {
  sender: string;
  addressee: string;
  text: string;
  msgid: string | null;
}

/// Payload of `aprs-message:state` — a delivery-state transition for a
/// previously-sent outgoing message, keyed by its backend-minted tracking id.
export interface StateChangeDto {
  msgid: string;
  state: DeliveryState;
}

/// A single message on the open channel — inbound (heard) or outbound (sent by
/// us), in one flat time-ordered feed.
///
/// `to` is the addressee callsign, or `null` for a broadcast (rendered `→ all`).
/// `state` is meaningful only for outbound messages: directed sends progress
/// `sent → acked / timedOut`; broadcasts are fire-and-forget and only ever
/// report `sent` (no delivery checkmark).
export interface ChannelMessage {
  /// Stable local React key. For outbound this is the backend tracking id
  /// (real msgid for directed, `b`-prefixed for broadcast); for inbound it is
  /// the msgid when present, else a synthetic local id.
  id: string;
  direction: 'in' | 'out';
  /// Sending station's callsign.
  from: string;
  /// Addressee callsign, or `null` for a broadcast (`→ all`).
  to: string | null;
  text: string;
  /// APRS message number (null when none). For outbound this matches the
  /// backend tracking id used to reconcile `aprs-message:state`.
  msgid: string | null;
  /// Outbound delivery state only. Undefined for inbound.
  state?: DeliveryState;
  /// Local epoch-ms when tuxlink received (inbound) or sent (outbound) this
  /// message. Honest client-stamp — NOT a claimed origin time.
  at: number;
  /// Local epoch-ms when the `acked` transition arrived. Set only on ACK so the
  /// UI can show "Acked HH:MM" (the round-trip close time). Undefined otherwise.
  ackedAt?: number;
}

/// A station heard on the channel, for the recipient dropdown. `lastHeard` is
/// the local epoch-ms of the most recent inbound message from `call`.
export interface HeardStation {
  call: string;
  lastHeard: number;
}

/// Payload of `aprs-position:new` — a position report decoded from a frame heard
/// on the channel (RX-only). Mirrors the Rust `InboundPos` (serde camelCase):
/// `sender` is the transmitting callsign; lat/lon/symbol/comment are exactly
/// what was decoded off the wire (RF-honesty — no estimated location).
export interface InboundPosDto {
  sender: string;
  lat: number;
  lon: number;
  symbolTable: string;
  symbolCode: string;
  comment: string;
  /// APRS position-ambiguity level (0–4) decoded off the wire. `0` is a
  /// full-precision fix; higher means the sender masked low-order minute digits,
  /// so the map must plot a region of uncertainty, not a false-exact pin.
  ambiguity: number;
}

/// A heard station's most-recent decoded position, accumulated by
/// `useAprsPositions` and plotted on the Tac Chat map. Deduped by `call`
/// (latest-position-wins). `at` is the local epoch-ms when this fix was heard.
export interface HeardPosition {
  call: string;
  lat: number;
  lon: number;
  symbolTable: string;
  symbolCode: string;
  comment: string;
  at: number;
  /// APRS position-ambiguity level (0–4) carried from the decoded report, so the
  /// map can plot an uncertainty region for masked fixes instead of a sharp pin.
  ambiguity: number;
}

/// Current APRS station configuration, returned by `aprs_config_get`.
export interface AprsConfigDto {
  sourceSsid: number;
  tocall: string;
  path: string;
}
