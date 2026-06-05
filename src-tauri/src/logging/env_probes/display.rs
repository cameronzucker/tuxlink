//! Display environment probe — Wayland/X11 + WebKitGTK version + GPU vendor
//! (spec §9.3).
//!
//! RADIO-1: read-only. No writes; only state queries.

use crate::logging::env_probes::{run_with_deadline, safe_env_value, ProbeGate, ProbeSnapshot};
use chrono::Utc;
use serde_json::json;

pub static GATE: ProbeGate = ProbeGate::new();

pub fn run(trigger: &str) -> ProbeSnapshot {
    let timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    // Display server environment variables
    let wayland_display = safe_env_value("WAYLAND_DISPLAY");
    let x11_display = safe_env_value("DISPLAY");

    // WebKitGTK version: try webkitgtk-6.0 --version first, fallback dpkg-query
    let webkitgtk_version = probe_webkitgtk_version();

    // GPU vendor: run glxinfo and grep in Rust (no shell pipe)
    let gpu_vendor = probe_gpu_vendor();

    let result = json!({
        "trigger": trigger,
        "wayland_display": wayland_display,
        "x11_display": x11_display,
        "webkitgtk_version": webkitgtk_version,
        "gpu_vendor": gpu_vendor,
    });

    ProbeSnapshot {
        probe: "display".into(),
        timestamp,
        trigger: trigger.into(),
        result,
    }
}

fn probe_webkitgtk_version() -> Option<String> {
    // Try pkg-config first (most reliable)
    if let Some(out) = run_with_deadline("pkg-config", &["--modversion", "webkit2gtk-4.1"]) {
        let v = out.trim().to_string();
        if !v.is_empty() {
            return Some(v);
        }
    }
    // Fallback: dpkg-query for libwebkit2gtk-4.1-0
    let dpkg_out = run_with_deadline(
        "dpkg-query",
        &["-W", "-f=${Version}", "libwebkit2gtk-4.1-0"],
    )?;
    let v = dpkg_out.trim().to_string();
    if v.is_empty() {
        // Also try libwebkitgtk-6.0
        let dpkg_out2 = run_with_deadline(
            "dpkg-query",
            &["-W", "-f=${Version}", "libwebkitgtk-6.0-4"],
        )?;
        let v2 = dpkg_out2.trim().to_string();
        if !v2.is_empty() { Some(v2) } else { None }
    } else {
        Some(v)
    }
}

fn probe_gpu_vendor() -> Option<String> {
    // Run glxinfo and extract "OpenGL vendor string" in Rust
    let out = run_with_deadline("glxinfo", &["-B"])?;
    out.lines()
        .find_map(|l| {
            let lower = l.to_lowercase();
            if lower.contains("opengl vendor") || lower.contains("vendor string") {
                l.split(':').nth(1).map(|v| v.trim().to_string())
            } else {
                None
            }
        })
        .or_else(|| {
            // Fallback: try glxinfo (full) and look for "OpenGL vendor string"
            let out2 = run_with_deadline("glxinfo", &[])?;
            out2.lines().find_map(|l| {
                l.to_lowercase()
                    .contains("opengl vendor string")
                    .then(|| l.split(':').nth(1).map(|v| v.trim().to_string()))
                    .flatten()
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_produces_non_empty_json() {
        let snap = run("test");
        assert_eq!(snap.probe, "display");
        let r = &snap.result;
        assert!(r.get("wayland_display").is_some() || r.get("x11_display").is_some());
        assert!(r.get("webkitgtk_version").is_some());
        assert!(r.get("gpu_vendor").is_some());
    }
}
