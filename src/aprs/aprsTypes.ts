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
  /// `'message'` for a true APRS text message; `'raw'` for a non-message frame's
  /// verbatim info field surfaced for the monitor feed (the UI decodes raw rows
  /// into a readable line — see `aprsDecode`). Absent on legacy payloads ⇒
  /// treated as `'message'`.
  kind?: 'message' | 'raw';
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
  /// `'message'` (chat) vs `'raw'` (a decoded monitor row for a non-message
  /// frame). Outbound is always `'message'`. Defaults to `'message'`.
  kind: 'message' | 'raw';
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
  /// For an OBJECT (`;`) / ITEM (`)`) report, the named entity this position
  /// describes (a weather object, event marker, ARES asset, …). The map labels
  /// the pin by this rather than the reporting `sender`. Absent for a station's
  /// own beacon (the backend omits it when `None`).
  name?: string | null;
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

/// One analog telemetry channel from a heard `aprs-telemetry:new` frame.
export interface TelemetryChannelDto {
  name: string;
  unit: string;
  raw: number;
  value: number;
  /// True when `value` was EQNS-scaled to an engineering unit; false when no
  /// EQNS is known and `value` is the raw count (so the UI labels it honestly).
  scaled: boolean;
}

/// One binary telemetry channel.
export interface TelemetryBitDto {
  name: string;
  value: boolean;
  /// The channel's defined active sense from `BITS.` (default true).
  sense: boolean;
}

/// A heard APRS telemetry frame, enriched with the station's known PARM/UNIT/
/// EQNS/BITS definitions. Backend event: `aprs-telemetry:new`. RF-honesty: only
/// channels present on the wire are included; an unscaled channel reports its raw
/// count with `scaled: false`. Consumed by the telemetry panel (fast-follow).
export interface InboundTelemetryDto {
  station: string;
  seq: number | null;
  analog: TelemetryChannelDto[];
  digital: TelemetryBitDto[];
  project: string;
  comment: string;
}

/// A heard APRS weather report, decoded from either a positionless weather
/// report (DTI `_`) or a `_`-symbol position report's comment. Backend event:
/// `aprs-weather:new`, mirroring the Rust `WeatherReport` (serde camelCase).
///
/// RF-honesty: every measurement is optional — a field absent from the wire is
/// `null`, never a fabricated 0. `humidityPct` of 100 may arrive on the wire as
/// `h00` (already decoded to 100 by the backend). Units are ham-conventional
/// (mph / °F / inches / hPa / W·m⁻²); a metric toggle is a panel concern.
///
/// The source-reactive panel (tuxlink-2phz fast-follow) derives its display
/// channels from these fields: wind direction (`windDirectionDeg`), wind speed
/// (`windSpeedMph`), wind gust (`windGustMph`), temperature (`temperatureF`),
/// humidity (`humidityPct`), pressure (`pressureHpa`), rain (`rain1hIn` /
/// `rain24hIn` / `rainSinceMidnightIn`), luminosity (`luminosityWm2`), and snow
/// (`snowIn`) — rendering only the channels actually present.
export interface WeatherReportDto {
  /// Reporting station callsign-SSID.
  station: string;
  windDirectionDeg: number | null;
  windSpeedMph: number | null;
  windGustMph: number | null;
  temperatureF: number | null;
  humidityPct: number | null;
  pressureHpa: number | null;
  rain1hIn: number | null;
  rain24hIn: number | null;
  rainSinceMidnightIn: number | null;
  luminosityWm2: number | null;
  snowIn: number | null;
  /// Free-text comment trailing the parsable WX run (station/software id), or "".
  comment: string;
}

/// Current APRS station configuration, returned by `aprs_config_get`.
export interface AprsConfigDto {
  sourceSsid: number;
  tocall: string;
  path: string;
}
