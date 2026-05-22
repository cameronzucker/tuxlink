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

/** KISS link kind. "Tcp" | "Serial" | null (no link configured).
 *  USB serial AND Bluetooth-RFCOMM both use "Serial" — the UI 3-segment
 *  (TCP/USB/BT) is a UI affordance; only two wire kinds exist. */
export type PacketLinkKind = 'Tcp' | 'Serial';

/** Flat, camelCase-on-wire P3 PacketConfigDto.
 *  Matches Rust #[serde(rename_all = "camelCase")] PacketConfigDto. */
export interface PacketConfigDto {
  /** SSID 0–15. GLOBAL + STICKY (persisted via packet_config_set). */
  ssid: number;
  /** Default-on listen flag (arm for incoming calls when idle). */
  listenDefault: boolean;
  /** KISS link kind: "Tcp" | "Serial" | null when not yet configured. */
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
