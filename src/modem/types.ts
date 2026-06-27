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
  /**
   * ardopcf Quality score (0..=100), populated from PINGACK / PING events.
   * `null` until the first ping has been observed; held across the rest of
   * the session as the last-known reading. Read by the Signal section's
   * "Quality" big-number indicator (spec §5.3). Closes tuxlink-1637.
   */
  quality: number | null;
  /**
   * Live VFO frequency (Hz) read back from the rig over CAT while a session
   * is connected (rig-control LIVE-VFO POLL). Present only on the DRA-100
   * keep-serial path with `live_vfo_poll` enabled; `null` otherwise (and
   * until the first read lands). The ARDOP frequency element shows this live
   * MHz when set, falling back to the configured/idle frequency.
   */
  rigFreqHz: number | null;
}

export const STOPPED: Readonly<ModemStatus> = {
  state: 'stopped',
  peer: null, mode: null, widthHz: null, pttBackend: null,
  snDb: null, vuDbfs: null, throughputBps: null,
  bytesRx: 0, bytesTx: 0, uptimeSec: 0,
  arqFlags: { busy: false, rx: false, tx: false },
  lastError: null,
  quality: null,
  rigFreqHz: null,
};
