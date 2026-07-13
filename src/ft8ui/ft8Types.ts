/**
 * ft8Types.ts — the COMPLETE Rust↔TS wire-shape contract for the FT8 / Station
 * Intelligence L3 panel (Task B1, plan tuxlink-b026z.4 §Frontend data layer).
 *
 * Every type here is transcribed field-for-field from the Rust serde DTOs, all
 * of which derive `#[serde(rename_all = "camelCase")]` (enums use kebab-case or
 * camelCase as noted per-type). Source structs (the source of truth):
 *   - src-tauri/src/ft8/records.rs      — BandSource, DecodeDto, DiscardClassDto,
 *                                          RingOutcome, SlotRecord, AudioDeviceChoice,
 *                                          SweepConfigDto, BlockedReasonDto,
 *                                          ServiceAxisDto, HealthFlagsDto, SlotPhaseDto,
 *                                          SweepModeDto, SweepStatusDto, Ft8ListeningChange
 *   - src-tauri/src/ft8/service.rs      — Ft8Snapshot
 *   - src-tauri/src/ft8/commands.rs     — Ft8CmdError, CatProbeDto, SubDto
 *   - src-tauri/src/ft8/meter.rs        — MeterDto
 *   - src-tauri/src/geomag/mod.rs       — DeclDto
 *   - src-tauri/src/ft8/waterfall.rs    — WaterfallBatch
 *   - src-tauri/src/winlink/ax25/devices.rs — StableAudioId, StableIdKind
 *   - src-tauri/src/ft8/events.rs       — event-name constants (re-exported below)
 *
 * No Phase-C task should need to read Phase-A Rust to learn a wire type — this
 * file is the single frontend home for the contract. A camelCase / rename drift
 * in a Rust DTO is caught by the composed-seam golden-fixture test
 * (useFt8Listener.test.ts §composed-seam), not by a user.
 */

// ---------------------------------------------------------------------------
// Event name constants (mirror src-tauri/src/ft8/events.rs)
// ---------------------------------------------------------------------------

/** `ft8-decodes:slot` — one payload per slot boundary (SlotRecord). */
export const FT8_SLOT_EVENT = 'ft8-decodes:slot' as const;
/** `ft8-listening:change` — axis/flags/phase/band/sweep summary (Ft8ListeningChange). */
export const FT8_LISTENING_EVENT = 'ft8-listening:change' as const;
/** `ft8-waterfall:columns` — one payload per FFT batch (WaterfallBatch). */
export const FT8_WATERFALL_EVENT = 'ft8-waterfall:columns' as const;

/** The `ft8_listener_snapshot` Tauri command name. */
export const FT8_SNAPSHOT_COMMAND = 'ft8_listener_snapshot' as const;

// ---------------------------------------------------------------------------
// Leaf enums / scalar wire shapes
// ---------------------------------------------------------------------------

/**
 * Band-label provenance (records.rs `BandSource`, serde kebab-case). The service
 * never claims a band nobody asserted.
 */
export type BandSource = 'cat-confirmed' | 'operator-asserted' | 'default-unconfirmed';

/** Scheduled-discard classes (records.rs `DiscardClassDto`, serde kebab-case). */
export type DiscardClassDto = 'first-slot' | 'qsy-transition' | 'clock-anomaly';

/** How a stable audio id was derived (devices.rs `StableIdKind`, serde camelCase). */
export type StableIdKind = 'byIdSymlink' | 'usbVidPidSerial' | 'cardIdHash';

/** Blocked-reason variants (records.rs `BlockedReasonDto`, serde kebab-case). */
export type BlockedReasonDto =
  | 'device-absent'
  | 'needs-device-selection'
  | 'wsjtx-absent'
  | 'unsupported-sample-rate'
  | 'capture-wedged';

/** Slot-phase (records.rs `SlotPhaseDto`, serde kebab-case). */
export type SlotPhaseDto = 'waiting-first-slot' | 'decoded' | 'band-dead';

/** Sweep mode (records.rs `SweepModeDto`, serde kebab-case). */
export type SweepModeDto = 'inactive' | 'active' | 'fallback-hold';

/**
 * Live-meter state (meter.rs — `MeterDto.state` is a bare String on the wire).
 * A busy device is surfaced HERE as `'in-use'` (an Ok meter value), NOT as an
 * `Ft8CmdError { kind: 'device-in-use' }` — that error kind is never produced.
 */
export type MeterState = 'live' | 'silent' | 'in-use' | 'error';

// ---------------------------------------------------------------------------
// Tagged unions
// ---------------------------------------------------------------------------

/**
 * Per-slot outcome (records.rs `RingOutcome`) — serde internally tagged on
 * `kind`, variant tags kebab-case, payload fields explicitly camelCase.
 */
export type RingOutcome =
  | { kind: 'decoded' }
  | { kind: 'band-dead' }
  | { kind: 'failed'; failure: string }
  | { kind: 'dropped-backpressure' }
  | { kind: 'dropped-lost-frames' }
  | { kind: 'dropped-storage-error'; diagnostic: string }
  | { kind: 'discarded'; class: DiscardClassDto };

/**
 * Service axis (records.rs `ServiceAxisDto`) — serde tagged on `axis`, kebab-case.
 */
export type ServiceAxisDto =
  | { axis: 'stopped' }
  | { axis: 'starting' }
  | { axis: 'listening' }
  | { axis: 'yielded' }
  | { axis: 'blocked'; reason: BlockedReasonDto }
  | { axis: 'stopping' };

// ---------------------------------------------------------------------------
// Record / struct wire shapes (all serde camelCase)
// ---------------------------------------------------------------------------

/** One decoded FT8 message (records.rs `DecodeDto`). */
export interface DecodeDto {
  slotUtcMs: number;
  snrDb: number;
  dtS: number;
  freqHz: number;
  message: string;
  fromCall: string | null;
  toCall: string | null;
  grid: string | null;
  partial: boolean;
}

/** A stable audio-device id (devices.rs `StableAudioId`). */
export interface StableAudioId {
  kind: StableIdKind;
  value: string;
}

/** A pickable capture device (records.rs `AudioDeviceChoice`). */
export interface AudioDeviceChoice {
  humanName: string;
  stableId: StableAudioId;
  /** The live ALSA `hw:<card_index>,0` name the capture path opens. */
  alsaHw: string;
}

/** Operator-configured CAT sweep (records.rs `SweepConfigDto`). */
export interface SweepConfigDto {
  enabled: boolean;
  bands: string[];
  dwellSlots: number;
}

/** Health flags (records.rs `HealthFlagsDto`). */
export interface HealthFlagsDto {
  clockUnsynced: boolean;
  catFixedBand: boolean;
  jt9Degraded: boolean;
}

/** Live sweep status (records.rs `SweepStatusDto`). */
export interface SweepStatusDto {
  mode: SweepModeDto;
  bandIdx: number | null;
  dwellProgress: number | null;
}

/** One ring entry (records.rs `SlotRecord`). Every slot boundary yields one. */
export interface SlotRecord {
  slotUtcMs: number;
  band: string;
  dialHz: number;
  bandSource: BandSource;
  bandLabelConfirmedUtcMs: number | null;
  outcome: RingOutcome;
  /** Empty except for a `decoded` outcome. */
  decodes: DecodeDto[];
  /** `any(decode.partial)` — salvage provenance. */
  partialSalvage: boolean;
  lostFrames: number;
  boundarySkewFrames: number;
  clipFraction: number;
  rmsDbfs: number;
  /** Position within the current sweep dwell, when sweeping; else null. */
  dwellSlotIndex: number | null;
}

/** The `ft8-listening:change` payload (records.rs `Ft8ListeningChange`). */
export interface Ft8ListeningChange {
  service: ServiceAxisDto;
  flags: HealthFlagsDto;
  slotPhase: SlotPhaseDto;
  band: string;
  dialHz: number;
  sweep: SweepStatusDto;
}

/**
 * The `ft8_listener_snapshot` full state (service.rs `Ft8Snapshot`) — the L3/L4
 * contract, field-for-field.
 */
export interface Ft8Snapshot {
  service: ServiceAxisDto;
  flags: HealthFlagsDto;
  slotPhase: SlotPhaseDto;
  band: string;
  dialHz: number;
  bandSource: BandSource;
  bandLabelConfirmedUtcMs: number | null;
  sweep: SweepStatusDto;
  engineVersion: string | null;
  nConsecutive: number;
  kConsecutive: number;
  lastSlotUtcMs: number | null;
  lastFailure: string | null;
  availableDevices: AudioDeviceChoice[] | null;
  ringTail: SlotRecord[];
  /** The operator-configured CAT sweep; independent of the live `sweep` above. */
  sweepConfig: SweepConfigDto;
  configuredDeviceName: string | null;
}

/** Live-meter sample (meter.rs `MeterDto`). */
export interface MeterDto {
  rmsDbfs: number;
  state: MeterState;
  /** ALSA diagnostic for `state === 'error'` (absent otherwise — the Rust side
   *  skips serializing None). Surfaced by the setup surface so "meter
   *  unavailable" says WHY (rate/format vs busy vs vanished). */
  detail?: string;
}

/** CAT-probe result (commands.rs `CatProbeDto`). */
export interface CatProbeDto {
  dialHz: number;
  band: string;
}

/** Magnetic-declination result (geomag/mod.rs `DeclDto`). */
export interface DeclDto {
  declDeg: number;
  modelEpoch: string;
  validUntil: string;
}

/** One waterfall column batch (waterfall.rs `WaterfallBatch`). */
export interface WaterfallBatch {
  seq: number;
  firstColUtcMs: number;
  cols: number[][];
}

/** Waterfall-subscribe token (commands.rs `SubDto`). */
export interface SubDto {
  subscriptionId: string;
}

/**
 * The 8 error kinds the FT8 commands actually EMIT (commands.rs — `kind` is a
 * bare String on the wire so the UI branches on a known tag but tolerates a
 * future/unknown one). NOTE: `device-in-use` is deliberately absent — a busy
 * device is an Ok `MeterDto.state === 'in-use'`, never an error kind.
 *
 * The `(string & {})` arm keeps the literal-union autocomplete while allowing
 * any string at the type level; the consuming UI treats an unrecognized kind as
 * a generic failure.
 */
export type Ft8CmdErrorKind =
  | 'device-reserved'
  | 'device-not-found'
  | 'modem-busy'
  | 'rig-not-configured'
  | 'probe-timeout'
  | 'invalid-grid'
  | 'invalid-band'
  | 'internal-error'
  // eslint-disable-next-line @typescript-eslint/ban-types
  | (string & {});

/** Structured command error (commands.rs `Ft8CmdError`). */
export interface Ft8CmdError {
  kind: Ft8CmdErrorKind;
  detail: string;
}

// ---------------------------------------------------------------------------
// Derived (non-wire) types — Task B2 / B3 own the authoritative definitions
// ---------------------------------------------------------------------------

/**
 * The 9-member UI-state union (plan §Frontend data layer). Task B2
 * (`deriveUiState`) owns the AUTHORITATIVE derivation semantics; this literal
 * union is declared here so B1's hook + every Phase-C consumer can name the
 * states. B2 finalizes the mapping from raw axis/phase/flags → these members.
 */
export type Ft8UiState =
  | 'off'
  | 'transitional'
  | 'needs-setup'
  | 'device-lost'
  | 'wedged'
  | 'yielded'
  | 'waiting-first-slot'
  | 'band-dead'
  | 'decoding';

/**
 * Derived UI flags (plan §Frontend data layer). PLACEHOLDER shape — Task B2
 * (`deriveUiState`) owns the authoritative definition and may extend this. The
 * three health flags mirror `HealthFlagsDto` so the hook's return type is
 * usable now; B2 finalizes any additional derived flags.
 */
export interface Ft8Flags {
  clockUnsynced: boolean;
  catFixedBand: boolean;
  jt9Degraded: boolean;
}

/**
 * Per-band openness dot (plan §Frontend data layer, §Openness). AUTHORITATIVE
 * shape — finalized by Task B3 (`deriveBandActivity` in `deriveBandActivity.ts`).
 * Consumed by C4 (chip dots), C3 (BandMatrix row dots), C7 (strip stats).
 *
 * - `tier`: 'hot' (>=8 decodes/min), 'warm' (>=1 decodes/min), 'quiet' (sampled,
 *   evidence present, but below 1/min), or 'no-data' (no evidence — either
 *   nothing sampled yet, or every slot for this band was non-evidence
 *   (`discarded` / `dropped-*`) or provenance-excluded (`default-unconfirmed`)).
 *   A dot never claims knowledge it lacks: only `decoded` / `band-dead` ring
 *   outcomes on a provenance-confirmed band count as evidence.
 * - `opacity`: fades with `sampledAgoMs`, floored at 0.4 — never fully invisible
 *   once there IS evidence, but `no-data` renders at 0 (nothing to show).
 * - `sampledAgoMs`: `nowMs - lastEvidenceSlotUtcMs`; `null` when `tier` is
 *   `'no-data'` (no evidence slot to measure from).
 * - `dwellSlots`: count of provenance-confirmed ring slots attributed to this
 *   band (any outcome) — the sample size backing the dot, for consumer context.
 */
export interface BandDot {
  tier: 'hot' | 'warm' | 'quiet' | 'no-data';
  opacity: number;
  sampledAgoMs: number | null;
  dwellSlots: number;
}
