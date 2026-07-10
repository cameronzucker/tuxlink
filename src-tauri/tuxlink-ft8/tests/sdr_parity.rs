//! M3 oracle-parity harness (plan T3.3): decode each committed real RTL-SDR
//! capture and score it against its AP-disabled WSJT-X `jt9 -8` reference log via
//! the permanent [`tuxlink_ft8::oracle`] comparator.
//!
//! # L0 SPIKE OUTCOME (2026-07-07): exit gate NOT met at single pass → jt9 fallback
//!
//! The L0 exit gate was **parity ≥ 85 %** with **zero false decodes** on an
//! ordinary AND a crowded capture. Measured single-pass parity on the committed
//! real captures is **1/5 (40m ordinary)** and **0/2 (20m quiet)** — while `jt9`
//! recovers 5/5 and 2/2 from the same WAVs. Diagnosis (see the session handoff
//! 2026-07-07): the shortfall is **weak-signal coarse TIME localization**, not
//! the sync floor — a floor-free decode targeted at the exact reference carriers
//! still fails, because the coarse metric mislocates the frame start (`t0`)
//! beyond the ±40 ms fine-refine window on −14…−19 dB signals.
//!
//! **Operator decision (2026-07-07):** the L0 clean-room-decoder spike is a
//! **NO-GO**; Station Intelligence falls back to depending on the proven external
//! decoder (`jt9`/`wsjtr`). This crate is retained as a working, tested reference
//! / learning artifact. Revisit only if the jt9 dependency proves problematic or
//! the project matures with spare capacity. The missing lever is a robust
//! sub-sample time-synchronization stage (ft8_lib/WB2FKO `sync8d`-class).
//!
//! # What these tests DO guarantee
//!
//! The one property that holds and matters for a passive intelligence feed is
//! **zero false decodes**: the decoder must never report a station `jt9` did not.
//! The gated tests below assert exactly that at the shipped default floor. Recall
//! is a documented known gap, not asserted (single pass is shelved). The
//! `floor_calibration_diag` (ignored) reproduces the parity + `t0` evidence for a
//! future revisit.
//!
//! Fixtures + reference provenance: `tests/fixtures/sdr/README.md`.

use std::path::PathBuf;
use tuxlink_ft8::oracle::{compare, parse_reference_log, ParityResult};
use tuxlink_ft8::sync::{coarse_candidates, decode_samples};

fn sdr_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures/sdr");
    p
}

/// Load a committed SDR capture WAV as f32 samples (12 kHz / mono / 16-bit).
fn load_capture(stem: &str) -> Vec<f32> {
    let mut path = sdr_dir();
    path.push(format!("{stem}.wav"));
    let mut reader =
        hound::WavReader::open(&path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, 12_000, "{stem}: capture must be 12 kHz");
    assert_eq!(spec.channels, 1, "{stem}: capture must be mono");
    assert_eq!(spec.bits_per_sample, 16, "{stem}: capture must be 16-bit");
    reader
        .samples::<i16>()
        .map(|s| s.expect("read sample") as f32)
        .collect()
}

/// Decode a capture and score it against its `.jt9-d3-ap-off.txt` reference log,
/// printing a full report (visible with `--nocapture`).
fn score_capture(stem: &str) -> ParityResult {
    let samples = load_capture(stem);
    let decoded: Vec<String> = decode_samples(&samples, 12_000)
        .into_iter()
        .map(|d| d.message)
        .collect();

    let mut ref_path = sdr_dir();
    ref_path.push(format!("{stem}.jt9-d3-ap-off.txt"));
    let ref_text = std::fs::read_to_string(&ref_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", ref_path.display()));
    let reference = parse_reference_log(&ref_text);

    let result = compare(&decoded, &reference);
    println!("\n=== {stem} ===");
    println!("  {}", result.summary());
    if !result.missed.is_empty() {
        println!("  MISSED ({}):", result.missed.len());
        for m in &result.missed {
            println!("    - {m}");
        }
    }
    if !result.false_decodes.is_empty() {
        println!("  FALSE ({}):", result.false_decodes.len());
        for m in &result.false_decodes {
            println!("    + {m}");
        }
    }
    result
}

// ── Floor-calibration diagnostic (M3 carry-forward #4) ──────────────────────
// Run explicitly: `cargo test -p tuxlink-ft8 --test sdr_parity -- --ignored
//                  --nocapture floor_calibration_diag`

#[test]
#[ignore = "diagnostic: prints candidate metrics + parity-vs-floor sweep"]
fn floor_calibration_diag() {
    use tuxlink_ft8::channelize::compute_spectrogram;
    use tuxlink_ft8::message::HashTable;
    use tuxlink_ft8::sync::try_decode_candidate;

    for stem in [
        "ft8-40m-ordinary-20260706T121215Z",
        "ft8-20m-quiet-20260706T121400Z",
    ] {
        let samples = load_capture(stem);
        let spec = compute_spectrogram(&samples, 12_000);
        let cands = coarse_candidates(&spec);
        println!("\n########## {stem} ##########");
        println!("total coarse candidates: {}", cands.len());
        println!("top 12 by sync metric:");
        for c in cands.iter().take(12) {
            println!("  fc={:7.1} Hz  metric={:6.2} dB  t0={:.0}", c.freq_hz, c.sync_metric, c.start_sample);
        }
        // Metric of the strongest candidate near each reference carrier.
        let mut ref_path = sdr_dir();
        ref_path.push(format!("{stem}.jt9-d3-ap-off.txt"));
        let ref_text = std::fs::read_to_string(&ref_path).unwrap();
        println!("candidate nearest each reference-log carrier:");
        for line in ref_text.lines() {
            let toks: Vec<&str> = line.split_whitespace().collect();
            if toks.len() < 3 {
                continue;
            }
            if let Ok(freq) = toks[2].parse::<f64>() {
                let near = cands
                    .iter()
                    .min_by(|a, b| {
                        (a.freq_hz - freq)
                            .abs()
                            .total_cmp(&(b.freq_hz - freq).abs())
                    });
                if let Some(c) = near {
                    println!(
                        "  ref {freq:7.1} Hz  -> nearest cand fc={:7.1} (|Δ|={:5.1})  metric={:6.2} dB",
                        c.freq_hz,
                        (c.freq_hz - freq).abs(),
                        c.sync_metric
                    );
                }
            }
        }
        let reference = parse_reference_log(&ref_text);

        // Targeted decode: for each reference carrier, try to decode the coarse
        // candidates within ±8 Hz of it, IGNORING the metric floor. This isolates
        // "given the right frequency, does a candidate decode?" — i.e. whether the
        // bottleneck is coarse localization/floor (candidates decode, metric just
        // ranks them low) or the demod itself (nothing decodes even on-frequency).
        println!("targeted decode at reference carriers (floor ignored):");
        let mut hash = HashTable::new();
        let mut targeted: Vec<String> = Vec::new();
        for line in ref_text.lines() {
            let toks: Vec<&str> = line.split_whitespace().collect();
            if toks.len() < 3 {
                continue;
            }
            let Ok(freq) = toks[2].parse::<f64>() else { continue };
            let mut got: Option<String> = None;
            for c in cands.iter().filter(|c| (c.freq_hz - freq).abs() <= 8.0) {
                if let Some(d) = try_decode_candidate(&samples, c, &mut hash) {
                    got = Some(d.message);
                    break;
                }
            }
            if let Some(m) = &got {
                targeted.push(m.clone());
            }
            println!("  ref {freq:7.1} Hz -> {got:?}");
        }
        let tr = compare(&targeted, &reference);
        println!("  TARGETED parity: {}", tr.summary());

        // Bounded floor-free probe: how many of the top-40 candidates decode at
        // all (any frequency)? Approximates "lower the floor to admit 40 cands".
        let mut hash2 = HashTable::new();
        let top: Vec<String> = cands
            .iter()
            .take(40)
            .filter_map(|c| try_decode_candidate(&samples, c, &mut hash2).map(|d| d.message))
            .collect();
        let topr = compare(&top, &reference);
        println!("  TOP-40 floor-free parity: {}", topr.summary());
    }
}

// ── Zero-false regression guard on every committed capture ──────────────────
// The shipped default decoder must NEVER emit a message `jt9` did not (no
// invented stations). Recall is a documented known gap (see module docs — L0
// spike NO-GO, jt9 fallback), so it is printed but not asserted. If a future
// revisit lowers the floor or strengthens acquisition, these guard against a
// regression that starts fabricating decodes.

#[test]
fn zero_false_40m_ordinary() {
    let r = score_capture("ft8-40m-ordinary-20260706T121215Z");
    assert_eq!(r.false_count(), 0, "false decodes: {:?}", r.false_decodes);
}

#[test]
fn zero_false_20m_quiet() {
    let r = score_capture("ft8-20m-quiet-20260706T121400Z");
    assert_eq!(r.false_count(), 0, "false decodes: {:?}", r.false_decodes);
}

#[test]
fn zero_false_40m_crowded() {
    let r = score_capture("ft8-40m-crowded-20260706T121300Z");
    assert_eq!(r.false_count(), 0, "false decodes: {:?}", r.false_decodes);
}

#[test]
fn zero_false_20m_busier() {
    let r = score_capture("ft8-20m-busier-20260706T121415Z");
    assert_eq!(r.false_count(), 0, "false decodes: {:?}", r.false_decodes);
}
