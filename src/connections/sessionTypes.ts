export type SessionTypeId = 'cms' | 'radio-only' | 'post-office' | 'p2p' | 'network-po';
export type ProtocolId = 'telnet' | 'packet' | 'vara-hf' | 'vara-fm' | 'ardop-hf';
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
      { ...VHF, built: false },
      { ...VFM, built: false },
    ],
  },
  {
    id: 'radio-only',
    label: 'Radio-only',
    blurb: 'RF-only Hybrid network (pool R).',
    built: false,
    protocols: [
      { ...TEL, built: false },
      { ...PKT, built: false },
      { ...VHF, built: false },
      { ...VFM, built: false },
    ],
  },
  {
    id: 'post-office',
    label: 'Post Office',
    blurb: 'Local RMS Relay store-and-forward (pool L).',
    built: false,
    protocols: [
      { ...TEL, built: false },
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
      { ...VHF, built: false },
      { ...VFM, built: false },
    ],
  },
  {
    id: 'network-po',
    label: 'Network Post Office',
    blurb: 'Local RMS Relay network.',
    built: false,
    protocols: [
      { ...TEL, built: false },
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
