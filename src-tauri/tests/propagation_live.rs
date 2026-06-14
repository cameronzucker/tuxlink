//! Gated end-to-end live test — runs real voacapl binary against real itshfbc data.
//!
//! This is the U1 acceptance gate for the voacapl-prediction feature. All tests
//! are `#[ignore]` so normal `cargo test` (and CI without voacapl installed) skips
//! them. Run explicitly with:
//!
//! ```sh
//! cargo test --manifest-path src-tauri/Cargo.toml --test propagation_live -- --ignored --nocapture
//! ```
//!
//! Environment overrides (both optional; defaults resolve via $HOME):
//!   TUXLINK_VOACAPL_BIN     path to the voacapl binary
//!                           (default: $HOME/.local/bin/voacapl)
//!   TUXLINK_VOACAPL_ITSHFBC path to the itshfbc data root
//!                           (default: $HOME/itshfbc)
//!
//! Example with explicit paths:
//!   TUXLINK_VOACAPL_BIN=/usr/local/bin/voacapl \
//!   TUXLINK_VOACAPL_ITSHFBC=/opt/itshfbc \
//!   cargo test --manifest-path src-tauri/Cargo.toml --test propagation_live -- --ignored --nocapture

use std::path::PathBuf;
use tuxlink_lib::propagation::{
    deck,
    engine::{run_voacapl, EnginePaths},
    parse::parse_voacapx_out,
    PredictionInputs,
};

/// Resolve the voacapl binary path from the env or the default install location.
fn resolve_binary() -> PathBuf {
    if let Ok(val) = std::env::var("TUXLINK_VOACAPL_BIN") {
        return PathBuf::from(val);
    }
    // Default: $HOME/.local/bin/voacapl
    let home = std::env::var("HOME").expect("$HOME not set");
    PathBuf::from(home).join(".local/bin/voacapl")
}

/// Resolve the itshfbc data root from the env or the default install location.
fn resolve_itshfbc() -> PathBuf {
    if let Ok(val) = std::env::var("TUXLINK_VOACAPL_ITSHFBC") {
        return PathBuf::from(val);
    }
    // Default: $HOME/itshfbc
    let home = std::env::var("HOME").expect("$HOME not set");
    PathBuf::from(home).join("itshfbc")
}

/// F5 acceptance gate: assert the three critical database files are present
/// before attempting a run. These are required for voacapl to start.
///
/// Missing any of them produces a hard abort in the Fortran runtime with an
/// opaque message like "version.w32: file does not exist" — catching them here
/// provides a clear failure message and guards against a partial itshfbc deploy.
fn assert_f5_database_files(itshfbc_root: &std::path::Path) {
    let required = [
        "database/version.w32",
        "database/voacap.def",
        "database/north_pole.txt",
    ];
    for rel in &required {
        let path = itshfbc_root.join(rel);
        assert!(
            path.exists(),
            "F5 guard: itshfbc database file missing: {}\n\
             Full path: {}\n\
             The itshfbc data tree is incomplete. Run `makeitshfbc` or check the \
             TUXLINK_VOACAPL_ITSHFBC env var.",
            rel,
            path.display()
        );
    }
}

/// End-to-end test: DM43→DM34 circuit, 6 frequencies, voacapl + parse pipeline.
///
/// This test exercises the full production path:
///   PredictionInputs → deck::build_deck → engine::run_voacapl → parse::parse_voacapx_out
///
/// Assertions:
///   - F5: itshfbc database files are present before running.
///   - 6 channels produced (one per input frequency).
///   - Each channel has exactly 24 REL/SNR/MUFday values.
///   - F1: channels[1].frequency_khz == 7103.0 and channels[2].frequency_khz == 7108.0
///     (these two adjacent frequencies must NOT collapse to a single channel).
///   - bearing_deg ≈ 301.65 (±1.0) — DM43 → DM34 great-circle bearing.
///   - distance_km ≈ 215.2 (±1.0) — DM43 → DM34 path distance.
#[test]
#[ignore]
fn live_dm43_to_dm34_full_pipeline() {
    let binary = resolve_binary();
    let itshfbc_root = resolve_itshfbc();

    println!("=== propagation_live: live_dm43_to_dm34_full_pipeline ===");
    println!("  binary:      {}", binary.display());
    println!("  itshfbc:     {}", itshfbc_root.display());

    // F5: assert the database files are present before running.
    assert_f5_database_files(&itshfbc_root);
    println!("  F5 guard:    database files present — OK");

    let inputs = PredictionInputs {
        tx_grid: "DM43".to_string(),
        rx_grid: "DM34".to_string(),
        frequencies_khz: vec![3590.0, 7103.0, 7108.0, 10147.0, 14103.0, 14115.0],
        year: 2026,
        month: 6,
        ssn: 100.0,
        tx_power_w: 100.0,
        req_snr_db: 73.0,
        // Legacy stock antennas — this live test exercises the voacapl engine
        // path, not the antenna-preset selection; keep the captured deck inputs.
        tx_antenna_voa: "const17.voa".to_string(),
        rx_antenna_voa: "swwhip.voa".to_string(),
        tx_antenna_voa_content: None,
        noise_dbw: 145.0,
    };

    // Build the deck.
    let deck_text = deck::build_deck(&inputs).expect("build_deck failed");
    println!("  deck built:  {} bytes", deck_text.len());

    // Run voacapl (scratch in temp_dir — acceptable for a test; production uses
    // the app cache dir per engine.rs comments, but temp_dir is fine here).
    let paths = EnginePaths {
        binary: binary.clone(),
        itshfbc_root: itshfbc_root.clone(),
    };
    let scratch = std::env::temp_dir();
    let raw_out = run_voacapl(&paths, &deck_text, &scratch)
        .expect("run_voacapl failed");
    println!("  voacapl out: {} bytes", raw_out.len());

    // Parse the output.
    let active_freqs: Vec<f64> = inputs.frequencies_khz.clone();
    let result = parse_voacapx_out(&raw_out, &active_freqs, inputs.ssn, inputs.year, inputs.month)
        .expect("parse_voacapx_out failed");

    println!("  bearing:     {:.2}°", result.bearing_deg);
    println!("  distance:    {:.1} km", result.distance_km);
    println!("  channels:    {}", result.channels.len());
    for ch in &result.channels {
        println!(
            "    {:.1} kHz  voacap={:.2} MHz  rel[0]={:.2}  rel[12]={:.2}",
            ch.frequency_khz,
            ch.voacap_mhz,
            ch.rel_by_hour[0],
            ch.rel_by_hour[12],
        );
    }

    // ── Assertions ────────────────────────────────────────────────────────────

    // 6 channels — one per input frequency.
    assert_eq!(
        result.channels.len(),
        6,
        "expected 6 channels (one per input frequency), got {}",
        result.channels.len()
    );

    // Each channel must have exactly 24 values per vector.
    for (i, ch) in result.channels.iter().enumerate() {
        assert_eq!(
            ch.rel_by_hour.len(),
            24,
            "channel {i}: rel_by_hour must have 24 entries, got {}",
            ch.rel_by_hour.len()
        );
        assert_eq!(
            ch.snr_by_hour.len(),
            24,
            "channel {i}: snr_by_hour must have 24 entries, got {}",
            ch.snr_by_hour.len()
        );
        assert_eq!(
            ch.mufday_by_hour.len(),
            24,
            "channel {i}: mufday_by_hour must have 24 entries, got {}",
            ch.mufday_by_hour.len()
        );
    }

    // F1: 7103.0 kHz and 7108.0 kHz must NOT collapse — they appear in separate
    // channels at indices 1 and 2 respectively.
    assert_eq!(
        result.channels[1].frequency_khz,
        7103.0,
        "F1: channels[1].frequency_khz should be 7103.0, got {}",
        result.channels[1].frequency_khz
    );
    assert_eq!(
        result.channels[2].frequency_khz,
        7108.0,
        "F1: channels[2].frequency_khz should be 7108.0, got {}",
        result.channels[2].frequency_khz
    );

    // Bearing ≈ 301.65° (±1.0°) — DM43 → DM34 great-circle.
    let bearing_err = (result.bearing_deg - 301.65_f64).abs();
    assert!(
        bearing_err <= 1.0,
        "bearing_deg {:.4} differs from expected 301.65 by {:.4}° (tolerance ±1.0°)",
        result.bearing_deg,
        bearing_err
    );

    // Distance ≈ 215.2 km (±1.0 km).
    let distance_err = (result.distance_km - 215.2_f64).abs();
    assert!(
        distance_err <= 1.0,
        "distance_km {:.2} differs from expected 215.2 by {:.2} km (tolerance ±1.0 km)",
        result.distance_km,
        distance_err
    );

    println!("=== PASS ===");
}
