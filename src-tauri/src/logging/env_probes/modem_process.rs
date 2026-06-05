//! Modem process state probe — VARA HF and ARDOP process detection
//! (spec §9.3).
//!
//! RADIO-1 CRITICAL: This probe does NOT spawn modems. It reads existing
//! process state via `pgrep` only. Process invocation is delegated entirely
//! to the `run_with_deadline` helper in env_probes/mod.rs — this file
//! contains no direct process-spawning calls. Compile-time enforcement in
//! tests/probes_no_tx_apis.rs.

use crate::logging::env_probes::{run_with_deadline, ProbeGate, ProbeSnapshot};
use chrono::Utc;
use serde_json::json;

pub static GATE: ProbeGate = ProbeGate::new();

pub fn run(trigger: &str) -> ProbeSnapshot {
    let timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    // pgrep -x varahf — exact name match
    let varahf_output = run_with_deadline("pgrep", &["-x", "varahf"]);
    let varahf_running = varahf_output
        .as_deref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    let varahf_pid: Option<u32> = varahf_output.as_deref().and_then(|s| {
        s.trim().lines().next().and_then(|l| l.trim().parse().ok())
    });

    // pgrep -x ardopc — exact name match; also try ardop variant
    let ardopc_output = run_with_deadline("pgrep", &["-x", "ardopc"])
        .or_else(|| run_with_deadline("pgrep", &["-x", "ardop"]));
    let ardopc_running = ardopc_output
        .as_deref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    let ardopc_pid: Option<u32> = ardopc_output.as_deref().and_then(|s| {
        s.trim().lines().next().and_then(|l| l.trim().parse().ok())
    });

    let result = json!({
        "trigger": trigger,
        "varahf_running": varahf_running,
        "ardopc_running": ardopc_running,
        "varahf_pid": varahf_pid,
        "ardopc_pid": ardopc_pid,
    });

    ProbeSnapshot {
        probe: "modem_process".into(),
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
        assert_eq!(snap.probe, "modem_process");
        let r = &snap.result;
        assert!(r.get("varahf_running").is_some());
        assert!(r.get("ardopc_running").is_some());
    }
}
