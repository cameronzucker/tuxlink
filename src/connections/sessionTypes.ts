export type SessionTypeId = 'cms' | 'radio-only' | 'post-office' | 'p2p' | 'network-po';
export type ProtocolId = 'telnet' | 'packet' | 'vara-hf' | 'vara-fm' | 'ardop-hf' | 'sonde-hf' | 'sonde-fm';
export interface ConnectionKey { sessionType: SessionTypeId; protocol: ProtocolId; }

export interface ProtocolEntry { id: ProtocolId; label: string; built: boolean; }
export interface SessionTypeEntry {
  id: SessionTypeId; label: string; blurb: string; built: boolean; protocols: ProtocolEntry[];
}

const PKT = { id: 'packet' as const, label: 'Packet (AX.25)' };
const TEL = { id: 'telnet' as const, label: 'Telnet' };
const ARD = { id: 'ardop-hf' as const, label: 'ARDOP HF' };
const VHF = { id: 'vara-hf' as const, label: 'VARA HF' };
const VFM = { id: 'vara-fm' as const, label: 'VARA FM' };
// tuxlink-3wwr: Sonde HF/FM — the clean-room modem, surfaced as a "coming soon"
// teaser (always built:false → disabled row + "soon" badge in FolderSidebar).
// No backend; mirrors VARA's placement under the RF session types.
const SHF = { id: 'sonde-hf' as const, label: 'Sonde HF' };
const SFM = { id: 'sonde-fm' as const, label: 'Sonde FM' };

// `built` on a protocol = the (sessionType, protocol) pane has UI + backend today.
export const SESSION_TYPES: SessionTypeEntry[] = [
  {
    id: 'cms',
    label: 'Winlink (CMS)',
    blurb: 'Sync your global mailbox. Credentialed secure-login.',
    built: true,
    protocols: [
      { ...TEL, built: true },
      { ...PKT, built: true },
      { ...ARD, built: true },
      // tuxlink-dfmf Phase 2: VARA HF/FM UI wired for the CMS intent. RF
      // CONNECT (Phase 3) adds the consent-gated peer-dial path; Phase 2
      // surfaces the TCP transport + config to the operator. P2P-VARA
      // stays unbuilt for now — flip once the P2P intent is exercised.
      { ...VHF, built: true },
      { ...VFM, built: true },
      // tuxlink-3wwr: Sonde teaser — coming soon, not yet wired.
      { ...SHF, built: false },
      { ...SFM, built: false },
    ],
  },
  {
    id: 'radio-only',
    label: 'Radio-only',
    blurb: 'RF-only Hybrid network (pool R).',
    // tuxlink-0ye6 Phase 2: radio-only flipped to built:true.
    // ardop-hf + vara-hf + vara-fm are the RF-bearing protocols; their
    // panels are intent-agnostic (same VaraRadioPanel / ArdopRadioPanel
    // surface, just with a radio-only context). Telnet + Packet are not
    // RF-bearing and stay unbuilt for this intent.
    built: true,
    protocols: [
      { ...TEL, built: false },
      { ...PKT, built: false },
      { ...ARD, built: true },
      { ...VHF, built: true },
      { ...VFM, built: true },
      // tuxlink-3wwr: Sonde teaser — coming soon, not yet wired.
      { ...SHF, built: false },
      { ...SFM, built: false },
    ],
  },
  {
    id: 'post-office',
    label: 'Post Office',
    blurb: 'Local RMS Relay store-and-forward (pool L).',
    // tuxlink-6c9y Phase B1: post-office flipped to built:true.
    // Telnet (TEL) is the delivered protocol; Packet (PKT) stays unbuilt.
    built: true,
    protocols: [
      { ...TEL, built: true },
      { ...PKT, built: false },
    ],
  },
  {
    id: 'p2p',
    label: 'Peer-to-peer',
    blurb: 'Direct station — no creds.',
    built: true,
    protocols: [
      { ...PKT, built: true },
      { ...TEL, built: true },
      // tuxlink-kb3s: P2P VARA HF/FM flipped to built:true. The Phase 2
      // surface (TCP open/close + bandwidth config, no transmit) is the
      // same panel rendered for CMS VARA — VaraRadioPanel + useVaraConfig
      // + the visibility router are all intent-agnostic. RF CONNECT-to-
      // peer is Phase 3 (tuxlink-fzl7), parallel to CMS's Phase 3 dial.
      { ...VHF, built: true },
      { ...VFM, built: true },
      // tuxlink-3wwr: Sonde teaser — coming soon, not yet wired.
      { ...SHF, built: false },
      { ...SFM, built: false },
    ],
  },
  {
    id: 'network-po',
    label: 'Network Post Office',
    blurb: 'Local RMS Relay network.',
    // tuxlink-6c9y Phase B1: network-po flipped to built:true.
    // Telnet (TEL) is the delivered protocol; no Packet entry for this intent.
    built: true,
    protocols: [
      { ...TEL, built: true },
    ],
  },
];

export function protocolsFor(id: SessionTypeId): ProtocolEntry[] {
  return SESSION_TYPES.find((s) => s.id === id)?.protocols ?? [];
}

// isBuilt = intent built AND protocol built; a built protocol under an unbuilt intent is not usable.
export function isBuilt(key: ConnectionKey): boolean {
  const intent = SESSION_TYPES.find((s) => s.id === key.sessionType);
  if (!intent?.built) return false;
  return intent.protocols.find((p) => p.id === key.protocol)?.built ?? false;
}

// === Smart auth-failure diagnostics types (tuxlink-7do4, spec §6.3) ===
// These shapes mirror the Rust serde-tagged enums in
// src-tauri/src/winlink/b2f_events.rs. Keep in sync.

export type AttemptId = number;

export type TransportFailureKind =
  | 'dns'
  | 'tcp_refused'
  | 'tcp_timeout'
  | 'tls_handshake';

export type ConnectionPhase = 'pre_handshake' | 'during_handshake' | 'post_handshake';

export type FailureMode =
  | 'network_unreachable'
  | 'client_rejected'
  | 'password_rejected'
  | 'callsign_rejected'
  | 'session_dropped_after_auth'
  | 'temporary_server_unavailability'
  | 'uncategorized';

export type CredentialScope =
  | { kind: 'primary' }
  | { kind: 'aux'; callsign: string }
  | { kind: 'unknown' };

// === Inbound pending-message selection (tuxlink-bsiy, WLE "Review Pending
// Messages" parity) ===
// These shapes mirror the Rust serde structs/enums in the b2f layer.
// snake_case fields match the serde wire contract exactly — do NOT change
// the casing; a mismatch silently breaks deserialization.

/// Disposition for messages the operator did NOT select for download. The Rust
/// enum carries `#[serde(rename_all = "snake_case")]`, so these must be
/// lowercase on the wire. Default is `'hold'`.
export type UnselectedDisposition = 'hold' | 'delete';

/// One proposed inbound message offered before download. Sizes are bytes;
/// sender/subject are unavailable pre-download (proposal phase only carries
/// the MID + sizes), matching the WLE pending-messages columns.
export interface PendingProposalDto {
  mid: string;
  uncompressed_size: number;
  compressed_size: number;
}

/// The operator's selection, sent back to the backend via
/// `cms_resolve_inbound_selection`. `selected_mids` are the MIDs to download;
/// everything else is held or deleted per `disposition`.
export interface InboundSelection {
  selected_mids: string[];
  disposition: UnselectedDisposition;
}

export type B2fEvent =
  | { kind: 'tcp_connected'; host: string; port: number; attempt_id: AttemptId }
  | { kind: 'tls_handshake_started'; attempt_id: AttemptId }
  | { kind: 'tls_handshake_completed'; attempt_id: AttemptId }
  | { kind: 'remote_sid_received'; sid: string; attempt_id: AttemptId }
  | { kind: 'secure_challenge_received'; attempt_id: AttemptId }
  | { kind: 'secure_response_sent'; attempt_id: AttemptId }
  | { kind: 'post_auth_exchange_started'; attempt_id: AttemptId }
  | { kind: 'remote_error_received'; raw: string; attempt_id: AttemptId }
  | { kind: 'handshake_completed'; attempt_id: AttemptId }
  | {
      kind: 'connection_closed';
      phase: ConnectionPhase;
      transport_kind: TransportFailureKind | null;
      attempt_id: AttemptId;
    }
  | {
      kind: 'auth_classified';
      mode: FailureMode;
      raw: string | null;
      attempt_id: AttemptId;
    }
  | {
      kind: 'inbound_proposals_offered';
      request_id: number;
      proposals: PendingProposalDto[];
      attempt_id: AttemptId;
    };
