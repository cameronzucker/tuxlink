// src/aprs/aprsTypes.ts
//
// Frontend mirror of the APRS tactical-chat wire shapes emitted by the Rust
// backend (Task 10). MIRROR the serde wire forms EXACTLY — `DeliveryState`
// serializes as camelCase, so the TS union must too.

/// Delivery lifecycle of an outgoing APRS message, as the backend reports it
/// over `aprs-message:state`. Wire forms are camelCase (serde
/// `rename_all = "camelCase"`).
export type DeliveryState = 'sent' | 'acked' | 'timedOut' | 'rejected';

/// Payload of `aprs-message:new` — a received APRS text message. `msgid` is
/// null when the sender's message carried no message number (unacked APRS
/// text).
export interface InboundMsgDto {
  sender: string;
  text: string;
  msgid: string | null;
}

/// Payload of `aprs-message:state` — a delivery-state transition for a
/// previously-sent outgoing message, keyed by its backend-minted `msgid`.
export interface StateChangeDto {
  msgid: string;
  state: DeliveryState;
}

/// A single chat bubble in a thread. `id` is a stable local key for React;
/// `msgid` is the APRS message number (null for inbound messages with no
/// number). `state` is only meaningful for outgoing messages.
export interface ChatMessage {
  id: string;
  direction: 'in' | 'out';
  text: string;
  msgid: string | null;
  state?: DeliveryState;
  at: number;
}

/// A conversation with one remote callsign.
export interface Thread {
  callsign: string;
  messages: ChatMessage[];
}

/// Current APRS station configuration, returned by `aprs_config_get`.
export interface AprsConfigDto {
  sourceSsid: number;
  tocall: string;
  path: string;
}
