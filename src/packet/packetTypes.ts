// src/packet/packetTypes.ts
// Mirrors the P3 (Rust) PacketConfigDto + packet_* commands serialization shapes.
// Consume only — P3 owns these definitions. Field names are the WIRE contract.
//
// IMPORTANT: The P3 DTO is FLAT camelCase (not nested link/ax25 objects).
// Rust struct: #[serde(rename_all = "camelCase")] PacketConfigDto
// Wire keys: ssid, listenDefault, linkKind ("Tcp"|"Serial"|null),
//            tcpHost, tcpPort, serialDevice, serialBaud,
//            txdelay, persistence, slotTime, paclen, maxframe, t1Ms, n2Retries
//
// Command signatures (P3 Rust param names → JS invoke arg keys):
//   packet_config_get() → PacketConfigDto
//   packet_config_set(dto: PacketConfigDto) → void
//   packet_connect(call: string, path: string[]) → void
//   packet_set_listen(enabled: boolean) → void

/** KISS link kind. "Tcp" | "Serial" | "Bluetooth" | null (no link configured).
 *  - "Serial": USB COM device (/dev/ttyUSB0).
 *  - "Bluetooth": in-app RFCOMM socket to a radio MAC (tuxlink-nx2) — no
 *    rfcomm bind / no /dev/rfcommN. The SPP channel is resolved from SDP at
 *    connect time. (Replaces the old "BT = Serial + /dev/rfcomm0" affordance,
 *    whose serialport TTY open the radio tore down → "Broken pipe".) */
export type PacketLinkKind = 'Tcp' | 'Serial' | 'Bluetooth' | 'Managed' | 'UvproNative';

/** How a {@link StableAudioId} was derived — the camelCased projection of the
 *  Rust `StableIdKind` enum (`#[serde(rename_all = "camelCase")]`):
 *  `ByIdSymlink` → 'byIdSymlink', `UsbVidPidSerial` → 'usbVidPidSerial',
 *  `CardIdHash` → 'cardIdHash'. (Verified against
 *  src-tauri/src/winlink/ax25/devices.rs `StableIdKind`.) */
export type StableIdKind = 'byIdSymlink' | 'usbVidPidSerial' | 'cardIdHash';

/** Boot-order-independent identity for a managed-modem audio device. Mirrors the
 *  Rust `StableAudioId` struct (`#[serde(rename_all = "camelCase")]`) → flat
 *  `{ kind, value }`. `value` is the by-id basename, the `vid:pid:serial`
 *  triple, or a `cardid:<hash>` fallback depending on `kind`. */
export interface StableAudioId {
  kind: StableIdKind;
  value: string;
}

/** A PTT keying method for a managed-modem audio device. Mirrors the Rust
 *  `PttChoice` enum, which is INTERNALLY TAGGED
 *  (`#[serde(rename_all = "camelCase", tag = "kind")]`): the variant rides a
 *  `kind` discriminator alongside the variant's fields, NOT a nested object.
 *  `Cm108Hid { hidraw_path }` → `{ kind: 'cm108Hid', hidrawPath }`;
 *  `SerialRts { tty }` → `{ kind: 'serialRts', tty }`. (Verified against
 *  src-tauri/src/winlink/ax25/devices.rs `PttChoice`.) */
export type PttChoice =
  | { kind: 'cm108Hid'; hidrawPath: string }
  | { kind: 'serialRts'; tty: string };

/** One managed-modem audio device from `packet_list_audio_devices` (P7.1).
 *  Mirrors the Rust `ManagedAudioDeviceDto` (`#[serde(rename_all = "camelCase")]`).
 *  `pttCandidates` is ranked; the first entry is the default PTT choice. */
export interface ManagedAudioDeviceDto {
  /** Friendly name the picker shows, e.g. "C-Media USB Audio Device (DigiRig)". */
  humanName: string;
  /** ALSA `plughw:CARD=<id>,DEV=0` name backing this device. */
  alsaPlughw: string;
  /** Stable identity persisted in config when this device is chosen. */
  stableId: StableAudioId;
  /** Ranked PTT candidates; `[0]` is the default. Empty when none discoverable. */
  pttCandidates: PttChoice[];
}

/** Flat, camelCase-on-wire P3 PacketConfigDto.
 *  Matches Rust #[serde(rename_all = "camelCase")] PacketConfigDto. */
export interface PacketConfigDto {
  /** SSID 0–15. GLOBAL + STICKY (persisted via packet_config_set). */
  ssid: number;
  /** Default-on listen flag (arm for incoming calls when idle). */
  listenDefault: boolean;
  /** Link kind: "Tcp" | "Serial" | "Bluetooth" | "Managed" | null when not yet
   *  configured. "Managed" is the managed-Dire-Wolf path (P5/P7) — its sound
   *  card + PTT ride `managedAudioDevice` / `managedPtt` below. */
  linkKind: PacketLinkKind | null;
  /** TCP host (non-null when linkKind === "Tcp"). */
  tcpHost: string | null;
  /** TCP port (non-null when linkKind === "Tcp"). */
  tcpPort: number | null;
  /** Serial device path (non-null when linkKind === "Serial"). */
  serialDevice: string | null;
  /** Serial host-link baud rate (non-null when linkKind === "Serial";
   *  distinct from over-air 1200 baud). */
  serialBaud: number | null;
  /** Radio Bluetooth MAC, e.g. "38:D2:00:01:55:5C" (non-null when
   *  linkKind === "Bluetooth"). The RFCOMM socket connects directly to this.
   *  Optional on the wire: an older payload without `btMac` still parses
   *  (backend uses `#[serde(default)]`); the panel selector is a follow-up. */
  btMac?: string | null;
  /** Resolved audio device for `linkKind === "Managed"` (managed Dire Wolf).
   *  Structured so the StableAudioId `kind`+`value` survive the round-trip.
   *  Optional on the wire (backend uses `#[serde(default)]`), like `btMac`. */
  managedAudioDevice?: StableAudioId | null;
  /** Resolved PTT keying method for `linkKind === "Managed"`. The internally-
   *  tagged PttChoice. Optional on the wire (`#[serde(default)]`), like `btMac`. */
  managedPtt?: PttChoice | null;
  /** AX.25 TXDELAY (units: 10 ms). */
  txdelay: number;
  /** AX.25 persistence parameter. */
  persistence: number;
  /** AX.25 slot time (units: 10 ms). */
  slotTime: number;
  /** AX.25 packet length. */
  paclen: number;
  /** AX.25 maximum outstanding I-frames. */
  maxframe: number;
  /** AX.25 T1 retransmit timer (milliseconds). */
  t1Ms: number;
  /** AX.25 N2 retry count. */
  n2Retries: number;
}
