// TypeScript projection of the UV-Pro native control types
// (`src-tauri/src/winlink/ax25/uvpro/model.rs` + `commands.rs`). The Rust side
// serializes with `#[serde(rename_all = "camelCase")]`; keep these keys in sync.
//
// These mirror the `uvpro_*` Tauri commands + the `uvpro:status` broadcast event,
// the always-live control surface for the unified native UV-Pro profile
// (tuxlink-ve3j; backend tuxlink-7my9 / nx95).

/** Connection lifecycle of the native control session (Rust `ConnState`,
 *  `rename_all = "lowercase"`). */
export type ConnState = 'disconnected' | 'connecting' | 'connected';

/** A live snapshot of the radio's control state. Broadcast on `uvpro:status`
 *  every ~2 s while connected, and returned by the connect / get-status commands.
 *  Optional fields are absent (`skip_serializing_if`) until the radio reports them. */
export interface UvproStatus {
  state: ConnState;
  deviceModel?: string;
  firmware?: string;
  currentChannelId?: number;
  rxMhz?: number;
  txMhz?: number;
  mode?: string;
  bandwidth?: string;
  channelName?: string;
  isTx: boolean;
  isRx: boolean;
  squelchOpen: boolean;
  powerOn: boolean;
  gpsLocked: boolean;
  rssi?: number;
  batteryPercent?: number;
  /** Set when the session is NOT connected because an EXTERNAL holder owns the
   *  radio's single Bluetooth host (e.g. the phone app, or tuxlink's own KISS
   *  packet path). Per the unified-model design, a real `linkBusyHolder` now means
   *  an external holder — not a self-inflicted mode-switch. */
  linkBusyHolder?: string;
}

/** A channel-memory entry (Rust `UvproChannel`). Switching channel is the
 *  canonical "change the radio's frequency/mode" operation — each memory carries
 *  its own rx/tx frequency, modulation and bandwidth. */
export interface UvproChannel {
  channelId: number;
  name: string;
  rxMhz: number;
  txMhz: number;
  mode: string;
  bandwidth: string;
  txDisable: boolean;
}

/** Frontend-facing error shape a rejected `uvpro_*` command resolves to
 *  (Rust `UvproCommandError`): a stable `kind` + a human `message`. */
export interface UvproCommandError {
  kind: string;
  message: string;
}

/** The Tauri event name the status broadcaster emits on (mirrors `modem:status`). */
export const UVPRO_STATUS_EVENT = 'uvpro:status';

/** Best-effort extraction of a human message from an unknown rejected-invoke
 *  value: a `UvproCommandError` object, a plain string, or an `Error`. */
export function uvproErrorMessage(err: unknown): string {
  if (typeof err === 'string') return err;
  if (err && typeof err === 'object' && 'message' in err) {
    const m = (err as { message?: unknown }).message;
    if (typeof m === 'string') return m;
  }
  if (err instanceof Error) return err.message;
  return 'The UV-Pro command failed.';
}
