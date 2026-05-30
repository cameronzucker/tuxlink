// Wire-mirror of src-tauri/src/modem_status.rs. Field names match the Rust
// #[serde(rename_all = "camelCase")] output.
export type ModemState =
  | 'stopped'
  | 'spawning'
  | 'initializing'
  | 'idle'
  | 'connecting'
  | 'connected-irs'
  | 'connected-iss'
  | 'disconnecting'
  | 'error';

export interface ArqFlags {
  busy: boolean;
  rx: boolean;
  tx: boolean;
}

export interface ModemStatus {
  state: ModemState;
  peer: string | null;
  mode: string | null;
  widthHz: number | null;
  pttBackend: string | null;     // "rts" | "cat" | "vox"
  snDb: number | null;
  vuDbfs: number | null;
  throughputBps: number | null;
  bytesRx: number;
  bytesTx: number;
  uptimeSec: number;
  arqFlags: ArqFlags;
  lastError: string | null;
}

export const STOPPED: ModemStatus = {
  state: 'stopped',
  peer: null, mode: null, widthHz: null, pttBackend: null,
  snDb: null, vuDbfs: null, throughputBps: null,
  bytesRx: 0, bytesTx: 0, uptimeSec: 0,
  arqFlags: { busy: false, rx: false, tx: false },
  lastError: null,
};
