//! APRS telemetry accumulation + emit DTO (engine layer).
//!
//! The pure decoder ([`super::telemetry`]) turns wire bytes into a
//! [`TelemetryData`] reading or a [`TelemetryDefinition`] fragment. Naming,
//! unit-labelling, and EQNS scaling are inherently *stateful* — a station's
//! `PARM`/`UNIT`/`EQNS`/`BITS` definitions arrive in separate frames (often on a
//! slower cadence than the data), so a listener routinely hears a `T#` report
//! before the definitions that name its channels. This module holds that
//! per-station state and produces the [`InboundTelemetry`] the UI panel renders.
//!
//! RF-honesty: only channels actually present on the wire are emitted, an analog
//! field that was blank is dropped (never a fabricated `0`), and a channel whose
//! station has sent no `EQNS` is reported as a `raw` count with `scaled = false`
//! rather than implying an engineering value that was never defined.

use std::collections::HashMap;

use super::telemetry::{apply_eqns, TelemetryData, TelemetryDefinition};

/// One scaled analog telemetry channel, ready for display.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryChannel {
    /// Channel name from `PARM`, or a positional `A1`…`A5` fallback when the
    /// station has not (yet) defined names.
    pub name: String,
    /// Unit/label from `UNIT`, or empty when undefined.
    pub unit: String,
    /// Raw analog count straight off the wire.
    pub raw: f64,
    /// Engineering value: `EQNS`-scaled when coefficients are known, else equal
    /// to `raw` (see `scaled`).
    pub value: f64,
    /// `true` when `value` was scaled by a defined `EQNS` quadratic; `false` when
    /// no `EQNS` is known and `value` is the raw count (so the UI can label it
    /// honestly as a raw reading rather than an engineering unit).
    pub scaled: bool,
}

/// One binary telemetry channel.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryBit {
    /// Bit name from `PARM` (binary labels follow the 5 analog labels), or a
    /// positional `B1`…`B8` fallback.
    pub name: String,
    /// The bit's state as decoded off the wire.
    pub value: bool,
    /// The channel's defined "active" sense from `BITS.` (default `true` when no
    /// `BITS` definition is known), so the panel can show which state is the
    /// significant one without inverting the honestly-decoded `value`.
    pub sense: bool,
}

/// A heard telemetry frame, enriched with whatever definitions the station has
/// transmitted so far. Serializes camelCase as `aprs-telemetry:new`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboundTelemetry {
    /// The telemetry station (callsign-SSID): the `T#` sender, or the addressee
    /// of a definition message.
    pub station: String,
    /// Sequence id, or `None` for the non-numeric `MIC` marker.
    pub seq: Option<u16>,
    /// Present analog channels (absent/blank wire fields are dropped).
    pub analog: Vec<TelemetryChannel>,
    /// Binary channels in wire order.
    pub digital: Vec<TelemetryBit>,
    /// `BITS.` project title, or empty.
    pub project: String,
    /// Free-text comment trailing the data report.
    pub comment: String,
}

/// Per-station accumulated definitions + the most recent data report.
#[derive(Default)]
struct StationDefs {
    /// `PARM` labels: up to 5 analog then up to 8 binary (13 positions).
    parm: Vec<String>,
    /// `UNIT` labels, positionally aligned with the analog `parm` labels.
    unit: Vec<String>,
    /// `EQNS` coefficients, 3 (`a,b,c`) per analog channel in wire order.
    eqns: Vec<f64>,
    /// `BITS.` per-channel active sense, in wire order.
    bit_sense: Vec<bool>,
    /// `BITS.` project title.
    project: String,
    /// Latest data report, retained so a late-arriving definition can re-emit a
    /// now-named DTO for data already heard.
    last_data: Option<TelemetryData>,
}

/// Accumulates per-station telemetry definitions and enriches each data report.
#[derive(Default)]
pub struct TelemetryStore {
    by_station: HashMap<String, StationDefs>,
}

impl TelemetryStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a `T#` data report from `station` and return the enriched DTO. A
    /// data report is always emittable (channels fall back to positional names
    /// when no definitions are known yet).
    pub fn ingest_data(&mut self, station: &str, data: TelemetryData) -> InboundTelemetry {
        let defs = self.by_station.entry(station.to_string()).or_default();
        defs.last_data = Some(data.clone());
        build_dto(station, &data, defs)
    }

    /// Record a definition fragment for `station`. Returns an enriched DTO **only
    /// if** a prior data report exists for that station (so the panel can re-label
    /// data already on screen); otherwise `None` — a definition with no data yet
    /// is nothing to display.
    pub fn ingest_definition(
        &mut self,
        station: &str,
        def: TelemetryDefinition,
    ) -> Option<InboundTelemetry> {
        let defs = self.by_station.entry(station.to_string()).or_default();
        match def {
            TelemetryDefinition::Parm(v) => defs.parm = v,
            TelemetryDefinition::Unit(v) => defs.unit = v,
            TelemetryDefinition::Eqns(v) => defs.eqns = v,
            TelemetryDefinition::Bits { sense, project } => {
                defs.bit_sense = sense;
                defs.project = project;
            }
        }
        defs.last_data
            .clone()
            .map(|data| build_dto(station, &data, defs))
    }
}

/// Build the display DTO for `data` using the station's known `defs`.
fn build_dto(station: &str, data: &TelemetryData, defs: &StationDefs) -> InboundTelemetry {
    let mut analog = Vec::new();
    for (i, raw_opt) in data.analog.iter().enumerate() {
        // RF-honesty: a blank analog field is absent, not zero — drop it.
        let raw = match raw_opt {
            Some(r) => *r,
            None => continue,
        };
        let name = defs
            .parm
            .get(i)
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| format!("A{}", i + 1));
        let unit = defs.unit.get(i).cloned().unwrap_or_default();
        // EQNS carries 3 coefficients per analog channel: [a_i, b_i, c_i].
        let (value, scaled) = match (
            defs.eqns.get(3 * i),
            defs.eqns.get(3 * i + 1),
            defs.eqns.get(3 * i + 2),
        ) {
            (Some(a), Some(b), Some(c)) => (apply_eqns(raw, *a, *b, *c), true),
            _ => (raw, false),
        };
        analog.push(TelemetryChannel { name, unit, raw, value, scaled });
    }

    let mut digital = Vec::new();
    for (i, bit) in data.digital.iter().enumerate() {
        // Binary PARM labels follow the 5 analog labels (positions 5..=12).
        let name = defs
            .parm
            .get(5 + i)
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| format!("B{}", i + 1));
        let sense = defs.bit_sense.get(i).copied().unwrap_or(true);
        digital.push(TelemetryBit { name, value: *bit, sense });
    }

    InboundTelemetry {
        station: station.to_string(),
        seq: data.seq,
        analog,
        digital,
        project: defs.project.clone(),
        comment: data.comment.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(seq: u16, analog: &[Option<f64>], digital: &[bool]) -> TelemetryData {
        TelemetryData {
            seq: Some(seq),
            analog: analog.to_vec(),
            digital: digital.to_vec(),
            comment: String::new(),
        }
    }

    #[test]
    fn data_with_no_definitions_uses_positional_names_and_raw_values() {
        let mut store = TelemetryStore::new();
        let dto = store.ingest_data("N0CALL-5", data(1, &[Some(199.0), Some(7.4)], &[true, false]));
        assert_eq!(dto.station, "N0CALL-5");
        assert_eq!(dto.seq, Some(1));
        assert_eq!(dto.analog.len(), 2);
        assert_eq!(dto.analog[0].name, "A1");
        assert_eq!(dto.analog[0].raw, 199.0);
        assert_eq!(dto.analog[0].value, 199.0);
        assert!(!dto.analog[0].scaled);
        assert_eq!(dto.analog[1].name, "A2");
        assert_eq!(dto.digital.len(), 2);
        assert_eq!(dto.digital[0].name, "B1");
        assert!(dto.digital[0].value);
        assert!(!dto.digital[1].value);
    }

    #[test]
    fn blank_analog_channel_is_dropped_not_zeroed() {
        let mut store = TelemetryStore::new();
        // A2 blank → only A1 and A3 are present, and A3 keeps its index-based name.
        let dto = store.ingest_data("W1AW", data(2, &[Some(12.0), None, Some(34.0)], &[]));
        assert_eq!(dto.analog.len(), 2);
        assert_eq!(dto.analog[0].name, "A1");
        assert_eq!(dto.analog[1].name, "A3");
        assert_eq!(dto.analog[1].raw, 34.0);
    }

    #[test]
    fn definitions_then_data_yields_named_scaled_channels() {
        let mut store = TelemetryStore::new();
        // Define before any data: PARM names, UNIT labels, EQNS scaling.
        assert!(store
            .ingest_definition(
                "K4CJX",
                TelemetryDefinition::Parm(vec!["Vbat".into(), "Temp".into()])
            )
            .is_none());
        store.ingest_definition(
            "K4CJX",
            TelemetryDefinition::Unit(vec!["Volts".into(), "degC".into()]),
        );
        // A1: value = 0.075*raw → 200 → 15.0 V.  A2: value = 1*raw - 100.
        store.ingest_definition(
            "K4CJX",
            TelemetryDefinition::Eqns(vec![0.0, 0.075, 0.0, 0.0, 1.0, -100.0]),
        );
        let dto = store.ingest_data("K4CJX", data(3, &[Some(200.0), Some(125.0)], &[]));
        assert_eq!(dto.analog[0].name, "Vbat");
        assert_eq!(dto.analog[0].unit, "Volts");
        assert!(dto.analog[0].scaled);
        assert!((dto.analog[0].value - 15.0).abs() < 1e-9);
        assert_eq!(dto.analog[1].name, "Temp");
        assert!((dto.analog[1].value - 25.0).abs() < 1e-9);
    }

    #[test]
    fn data_then_late_definition_reemits_named_dto() {
        let mut store = TelemetryStore::new();
        // Data arrives first → positional names, no re-emit trigger.
        let first = store.ingest_data("N0CALL", data(4, &[Some(50.0)], &[true]));
        assert_eq!(first.analog[0].name, "A1");
        // The PARM definition arrives later → re-emit the already-heard reading,
        // now named (and the binary label too).
        let reemit = store
            .ingest_definition(
                "N0CALL",
                TelemetryDefinition::Parm(vec![
                    "Solar".into(),
                    "".into(),
                    "".into(),
                    "".into(),
                    "".into(),
                    "Door".into(),
                ]),
            )
            .expect("a prior reading exists, so a re-emit is produced");
        assert_eq!(reemit.seq, Some(4));
        assert_eq!(reemit.analog[0].name, "Solar");
        assert_eq!(reemit.digital[0].name, "Door");
    }

    #[test]
    fn definition_with_no_prior_data_does_not_emit() {
        let mut store = TelemetryStore::new();
        assert!(store
            .ingest_definition("NOBODY", TelemetryDefinition::Unit(vec!["Volts".into()]))
            .is_none());
    }

    #[test]
    fn bits_project_title_is_carried_onto_the_dto() {
        let mut store = TelemetryStore::new();
        store.ingest_data("AB1CD", data(5, &[Some(1.0)], &[]));
        let dto = store
            .ingest_definition(
                "AB1CD",
                TelemetryDefinition::Bits {
                    sense: vec![true, false],
                    project: "Solar Shed".into(),
                },
            )
            .unwrap();
        assert_eq!(dto.project, "Solar Shed");
    }

    #[test]
    fn stations_are_kept_separate() {
        let mut store = TelemetryStore::new();
        store.ingest_definition("AAA", TelemetryDefinition::Parm(vec!["VoltsA".into()]));
        store.ingest_definition("BBB", TelemetryDefinition::Parm(vec!["VoltsB".into()]));
        let a = store.ingest_data("AAA", data(1, &[Some(1.0)], &[]));
        let b = store.ingest_data("BBB", data(1, &[Some(2.0)], &[]));
        assert_eq!(a.analog[0].name, "VoltsA");
        assert_eq!(b.analog[0].name, "VoltsB");
    }
}
