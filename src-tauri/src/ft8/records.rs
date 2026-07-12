//! Serialized wire shapes for the FT8 events + snapshot (the L3/L4
//! contract, spec §Ring + §Snapshot). Pure DTOs: `From` impls mirror the
//! std-only leaf-crate state types into serde-derived shapes.

use serde::Serialize;

use crate::config::Ft8SweepConfig;
use crate::winlink::ax25::devices::StableAudioId;
use tuxlink_capture::state::{
    BlockedReason, HealthFlags, RingOutcomeKind, ServiceAxis, SlotPhase, Sweep,
};
use tuxlink_jt9::types::Ft8Decode;

/// Band-label provenance (spec §Band provenance): the service never claims a
/// band nobody asserted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BandSource {
    CatConfirmed,
    OperatorAsserted,
    DefaultUnconfirmed,
}

/// One decoded FT8 message on the wire (mirrors `tuxlink_jt9::Ft8Decode`,
/// which cannot derive serde in a dep-free leaf crate).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecodeDto {
    pub slot_utc_ms: u64,
    pub snr_db: i32,
    pub dt_s: f64,
    pub freq_hz: u32,
    pub message: String,
    pub from_call: Option<String>,
    pub to_call: Option<String>,
    pub grid: Option<String>,
    pub partial: bool,
}

impl From<&Ft8Decode> for DecodeDto {
    fn from(d: &Ft8Decode) -> Self {
        Self {
            slot_utc_ms: d.slot_utc_ms,
            snr_db: d.snr_db,
            dt_s: d.dt_s,
            freq_hz: d.freq_hz,
            message: d.message.clone(),
            from_call: d.from_call.clone(),
            to_call: d.to_call.clone(),
            grid: d.grid.clone(),
            partial: d.partial,
        }
    }
}

/// Scheduled-discard classes on the wire (spec §Counter semantics: these
/// count toward NEITHER counter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiscardClassDto {
    FirstSlot,
    QsyTransition,
    ClockAnomaly,
}

/// Per-slot outcome on the wire (spec §Ring). Internally tagged; variant
/// tags kebab-case; payload fields explicitly camelCase-named.
///
/// Deviation from the spec's `Failed(kind)` sketch, recorded: `Failed`
/// carries the Debug-formatted failure STRING so L3/L4 receive the full
/// diagnostic; kind-level matching happens via [`RingOutcome::kind`] /
/// `RingOutcomeKind`, so nothing the spec's kind enum carried is lost.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RingOutcome {
    Decoded,
    BandDead,
    Failed {
        #[serde(rename = "failure")]
        failure: String,
    },
    DroppedBackpressure,
    DroppedLostFrames,
    DroppedStorageError {
        #[serde(rename = "diagnostic")]
        diagnostic: String,
    },
    Discarded {
        #[serde(rename = "class")]
        class: DiscardClassDto,
    },
}

impl RingOutcome {
    /// The counter-classification the leaf-crate machine consumes
    /// (`ListenerMachine::on_slot_outcome`). 1:1 by construction.
    pub fn kind(&self) -> RingOutcomeKind {
        match self {
            RingOutcome::Decoded => RingOutcomeKind::Decoded,
            RingOutcome::BandDead => RingOutcomeKind::BandDead,
            RingOutcome::Failed { .. } => RingOutcomeKind::Failed,
            RingOutcome::DroppedBackpressure => RingOutcomeKind::DroppedBackpressure,
            RingOutcome::DroppedLostFrames => RingOutcomeKind::DroppedLostFrames,
            RingOutcome::DroppedStorageError { .. } => RingOutcomeKind::DroppedStorageError,
            RingOutcome::Discarded { .. } => RingOutcomeKind::Discarded,
        }
    }
}

/// One ring entry (spec §Ring, field-for-field). Every slot boundary yields
/// one — including drops and discards (L4's failure counters and honest
/// recency need them).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotRecord {
    pub slot_utc_ms: u64,
    pub band: String,
    pub dial_hz: u64,
    pub band_source: BandSource,
    pub band_label_confirmed_utc_ms: Option<u64>,
    pub outcome: RingOutcome,
    /// Empty except for `Decoded`.
    pub decodes: Vec<DecodeDto>,
    /// `any(decode.partial)` — salvage provenance.
    pub partial_salvage: bool,
    pub lost_frames: u64,
    pub boundary_skew_frames: u64,
    pub clip_fraction: f32,
    pub rms_dbfs: f32,
    /// Position within the current sweep dwell, when sweeping.
    pub dwell_slot_index: Option<u8>,
}

/// A pickable capture device (spec §Device selection:
/// `available_devices: Vec<{human_name, stable_id}>`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDeviceChoice {
    pub human_name: String,
    pub stable_id: StableAudioId,
    /// The live ALSA `hw:<card_index>,0` name for this card (L3 station-intel
    /// panel: the setup rows show the exact device string the capture path
    /// opens). Same value shape as [`crate::winlink::ax25::devices::ResolvedManagedDevice::alsa_hw`],
    /// resolved fresh at enumeration time via [`crate::winlink::ax25::devices::alsa_hw_name`].
    pub alsa_hw: String,
}

/// Sweep configuration on the wire (spec header additive-changes (1): the
/// L3 popover shows the sweep the operator configured, not just the live
/// [`SweepStatusDto`]). Mirrors [`Ft8SweepConfig`] field-for-field —
/// deliberately a separate DTO (not `#[derive(Serialize)]` on the config
/// struct itself) so config-file shape and wire shape can diverge without
/// coupling.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SweepConfigDto {
    pub enabled: bool,
    pub bands: Vec<String>,
    pub dwell_slots: u8,
}

impl From<&Ft8SweepConfig> for SweepConfigDto {
    fn from(c: &Ft8SweepConfig) -> Self {
        Self { enabled: c.enabled, bands: c.bands.clone(), dwell_slots: c.dwell_slots }
    }
}

// ---- state-machine mirrors (leaf-crate types cannot derive serde) --------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BlockedReasonDto {
    DeviceAbsent,
    NeedsDeviceSelection,
    WsjtxAbsent,
    UnsupportedSampleRate,
    CaptureWedged,
}

impl From<BlockedReason> for BlockedReasonDto {
    fn from(r: BlockedReason) -> Self {
        match r {
            BlockedReason::DeviceAbsent => Self::DeviceAbsent,
            BlockedReason::NeedsDeviceSelection => Self::NeedsDeviceSelection,
            BlockedReason::WsjtxAbsent => Self::WsjtxAbsent,
            BlockedReason::UnsupportedSampleRate => Self::UnsupportedSampleRate,
            BlockedReason::CaptureWedged => Self::CaptureWedged,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "axis", rename_all = "kebab-case")]
pub enum ServiceAxisDto {
    Stopped,
    Starting,
    Listening,
    Yielded,
    Blocked {
        #[serde(rename = "reason")]
        reason: BlockedReasonDto,
    },
    Stopping,
}

impl From<ServiceAxis> for ServiceAxisDto {
    fn from(a: ServiceAxis) -> Self {
        match a {
            ServiceAxis::Stopped => Self::Stopped,
            ServiceAxis::Starting => Self::Starting,
            ServiceAxis::Listening => Self::Listening,
            ServiceAxis::Yielded => Self::Yielded,
            ServiceAxis::Blocked(r) => Self::Blocked { reason: r.into() },
            ServiceAxis::Stopping => Self::Stopping,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthFlagsDto {
    pub clock_unsynced: bool,
    pub cat_fixed_band: bool,
    pub jt9_degraded: bool,
}

impl From<HealthFlags> for HealthFlagsDto {
    fn from(f: HealthFlags) -> Self {
        Self {
            clock_unsynced: f.clock_unsynced,
            cat_fixed_band: f.cat_fixed_band,
            jt9_degraded: f.jt9_degraded,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SlotPhaseDto {
    WaitingFirstSlot,
    Decoded,
    BandDead,
}

impl From<SlotPhase> for SlotPhaseDto {
    fn from(p: SlotPhase) -> Self {
        match p {
            SlotPhase::WaitingFirstSlot => Self::WaitingFirstSlot,
            SlotPhase::Decoded => Self::Decoded,
            SlotPhase::BandDead => Self::BandDead,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SweepModeDto {
    Inactive,
    Active,
    FallbackHold,
}

/// Sweep status on the wire (spec §Snapshot:
/// `SweepStatus { mode, band_idx, dwell_progress }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SweepStatusDto {
    pub mode: SweepModeDto,
    pub band_idx: Option<usize>,
    pub dwell_progress: Option<u8>,
}

impl From<Sweep> for SweepStatusDto {
    fn from(s: Sweep) -> Self {
        match s {
            Sweep::Inactive => Self { mode: SweepModeDto::Inactive, band_idx: None, dwell_progress: None },
            Sweep::Active { band_idx, dwell_progress } => Self {
                mode: SweepModeDto::Active,
                band_idx: Some(band_idx),
                dwell_progress: Some(dwell_progress),
            },
            Sweep::FallbackHold { .. } => {
                Self { mode: SweepModeDto::FallbackHold, band_idx: None, dwell_progress: None }
            }
        }
    }
}

/// The `ft8-listening:change` payload (spec §Events: axis + flags + phase +
/// band + sweep summary).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ft8ListeningChange {
    pub service: ServiceAxisDto,
    pub flags: HealthFlagsDto,
    pub slot_phase: SlotPhaseDto,
    pub band: String,
    pub dial_hz: u64,
    pub sweep: SweepStatusDto,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the exact JSON tag/field shapes the L3 frontend will parse.
    /// serde `rename_all` on an ENUM renames variant TAGS only — this test
    /// is the project-mandated shape pin (serde_rename_all_enum_fields).
    #[test]
    fn ring_outcome_serde_shape_is_pinned() {
        let j = |o: &RingOutcome| serde_json::to_string(o).unwrap();
        assert_eq!(j(&RingOutcome::Decoded), r#"{"kind":"decoded"}"#);
        assert_eq!(j(&RingOutcome::BandDead), r#"{"kind":"band-dead"}"#);
        assert_eq!(
            j(&RingOutcome::Failed { failure: "Timeout".into() }),
            r#"{"kind":"failed","failure":"Timeout"}"#
        );
        assert_eq!(j(&RingOutcome::DroppedBackpressure), r#"{"kind":"dropped-backpressure"}"#);
        assert_eq!(j(&RingOutcome::DroppedLostFrames), r#"{"kind":"dropped-lost-frames"}"#);
        assert_eq!(
            j(&RingOutcome::DroppedStorageError { diagnostic: "ENOSPC".into() }),
            r#"{"kind":"dropped-storage-error","diagnostic":"ENOSPC"}"#
        );
        assert_eq!(
            j(&RingOutcome::Discarded { class: DiscardClassDto::QsyTransition }),
            r#"{"kind":"discarded","class":"qsy-transition"}"#
        );
    }

    #[test]
    fn service_axis_serde_shape_is_pinned() {
        let j = |a: &ServiceAxisDto| serde_json::to_string(a).unwrap();
        assert_eq!(j(&ServiceAxisDto::Listening), r#"{"axis":"listening"}"#);
        assert_eq!(
            j(&ServiceAxisDto::Blocked { reason: BlockedReasonDto::NeedsDeviceSelection }),
            r#"{"axis":"blocked","reason":"needs-device-selection"}"#
        );
        assert_eq!(
            j(&ServiceAxisDto::Blocked { reason: BlockedReasonDto::CaptureWedged }),
            r#"{"axis":"blocked","reason":"capture-wedged"}"#
        );
    }

    #[test]
    fn every_ring_outcome_maps_to_its_counter_kind() {
        use tuxlink_capture::state::RingOutcomeKind as K;
        assert_eq!(RingOutcome::Decoded.kind(), K::Decoded);
        assert_eq!(RingOutcome::BandDead.kind(), K::BandDead);
        assert_eq!(RingOutcome::Failed { failure: String::new() }.kind(), K::Failed);
        assert_eq!(RingOutcome::DroppedBackpressure.kind(), K::DroppedBackpressure);
        assert_eq!(RingOutcome::DroppedLostFrames.kind(), K::DroppedLostFrames);
        assert_eq!(
            RingOutcome::DroppedStorageError { diagnostic: String::new() }.kind(),
            K::DroppedStorageError
        );
        assert_eq!(
            RingOutcome::Discarded { class: DiscardClassDto::FirstSlot }.kind(),
            K::Discarded
        );
    }

    #[test]
    fn band_source_and_sweep_shapes_are_pinned() {
        assert_eq!(
            serde_json::to_string(&BandSource::DefaultUnconfirmed).unwrap(),
            r#""default-unconfirmed""#
        );
        let s: SweepStatusDto =
            tuxlink_capture::state::Sweep::Active { band_idx: 2, dwell_progress: 5 }.into();
        assert_eq!(
            serde_json::to_string(&s).unwrap(),
            r#"{"mode":"active","bandIdx":2,"dwellProgress":5}"#
        );
    }

    /// L3 additive-fields shape pin (tuxlink-b026z.4 Task A1): `AudioDeviceChoice`
    /// carries `alsaHw`; `SweepConfigDto` mirrors `Ft8SweepConfig` in camelCase.
    #[test]
    fn audio_device_choice_and_sweep_config_dto_shapes_are_pinned() {
        let dev = AudioDeviceChoice {
            human_name: "USB Audio CODEC".into(),
            stable_id: StableAudioId {
                kind: crate::winlink::ax25::devices::StableIdKind::ByIdSymlink,
                value: "usb-codec-00".into(),
            },
            alsa_hw: "hw:1,0".into(),
        };
        let v = serde_json::to_value(&dev).unwrap();
        assert_eq!(v["alsaHw"], "hw:1,0");
        assert_eq!(v["humanName"], "USB Audio CODEC");

        let cfg = Ft8SweepConfig {
            enabled: true,
            bands: vec!["20m".into(), "40m".into()],
            dwell_slots: 8,
        };
        let dto: SweepConfigDto = (&cfg).into();
        assert_eq!(
            serde_json::to_string(&dto).unwrap(),
            r#"{"enabled":true,"bands":["20m","40m"],"dwellSlots":8}"#
        );
    }
}
