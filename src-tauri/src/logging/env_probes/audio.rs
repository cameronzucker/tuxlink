//! Audio environment probe — PipeWire / PulseAudio / DigiRig detection
//! (spec §9.3).
//!
//! RADIO-1: read-only. Queries audio subsystem state only; no writes.

use crate::logging::env_probes::{run_with_deadline, ProbeGate, ProbeSnapshot};
use chrono::Utc;
use serde_json::json;

pub static GATE: ProbeGate = ProbeGate::new();

pub fn run(trigger: &str) -> ProbeSnapshot {
    let timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    // PipeWire running?
    let pipewire_running = run_with_deadline("pw-cli", &["info", "0"])
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

    // PulseAudio server name (from pactl stat)
    let pulse_server = run_with_deadline("pactl", &["info"])
        .and_then(|out| {
            out.lines()
                .find_map(|l| l.strip_prefix("Server Name:").map(|v| v.trim().to_string()))
        });

    // Sinks
    let sinks_raw = run_with_deadline("pactl", &["list", "short", "sinks"]).unwrap_or_default();
    let sinks_count = sinks_raw.lines().filter(|l| !l.trim().is_empty()).count();

    // Cards
    let cards_raw = run_with_deadline("pactl", &["list", "short", "cards"]).unwrap_or_default();
    let cards_count = cards_raw.lines().filter(|l| !l.trim().is_empty()).count();

    // DigiRig detection: case-insensitive match of "digirig" in card or sink names
    let digirig_detected = sinks_raw.to_lowercase().contains("digirig")
        || cards_raw.to_lowercase().contains("digirig");

    let result = json!({
        "trigger": trigger,
        "pipewire_running": pipewire_running,
        "pulse_server": pulse_server,
        "sinks_count": sinks_count,
        "cards_count": cards_count,
        "digirig_detected": digirig_detected,
    });

    ProbeSnapshot {
        probe: "audio".into(),
        timestamp,
        trigger: trigger.into(),
        result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_produces_non_empty_json() {
        let snap = run("test");
        assert_eq!(snap.probe, "audio");
        let r = &snap.result;
        assert!(r.get("pipewire_running").is_some());
        assert!(r.get("sinks_count").is_some());
        assert!(r.get("digirig_detected").is_some());
    }
}
