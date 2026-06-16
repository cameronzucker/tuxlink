//! Offline generator: runs nec2c over the antenna catalog × height grid and emits
//! Type-14 .voa pattern files for Find-a-Station Phase 1. NOT part of the app/CI
//! build — run manually by a developer when the catalog or geometries change:
//!
//!   cd tools/pattern-gen && cargo run
//!
//! Requires `nec2c` on PATH. Writes src-tauri/src/propagation/patterns/*.voa, which
//! the runtime include_str!s. The emitter is the Phase 0 type14.rs, path-included
//! below so generated files are byte-identical to what the app validates.
//! See docs/design/2026-06-15-find-a-station-antenna-phase1-picker-PLAN.md.

// One source of truth: include the real Phase 0 emitter (no copy). Its golden test
// uses include_str! (file-relative), so it keeps passing under this crate too.
#[path = "../../../src-tauri/src/propagation/type14.rs"]
mod type14;

use type14::{FreqBlock, Type14Pattern, N_BLOCKS, N_GAINS};

/// voacapl maps Type-14 block index → frequency by RECORD NUMBER = integer MHz:
/// `ifreq = freqarea(1)` (float MHz truncated), `read(14, rec=ifreq)`, then linear
/// interpolation to rec=ifreq+1 (voacapl src voacapw/antcalc.for:183). So **block i = i MHz**,
/// records 1..30. We tabulate NEC gains at 1..30 MHz (block i ↔ FREQS_MHZ[i-1]).
/// Verified against voacapl source 2026-06-15; "Freqs 2-30" (antcalc.for) is the useful HF subset.
const FREQS_MHZ: [f64; N_BLOCKS] = [
    1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0,
    16.0, 17.0, 18.0, 19.0, 20.0, 21.0, 22.0, 23.0, 24.0, 25.0, 26.0, 27.0, 28.0, 29.0, 30.0,
];

fn main() {
    eprintln!(
        "gen_antenna_patterns: {} freq blocks (1..30 MHz, block i = i MHz), target 20 patterns",
        N_BLOCKS
    );
    // Generation driver filled in Task B4.
    let _ = (
        FreqBlock { efficiency: 0.0, gains: vec![0.0; N_GAINS] },
        Type14Pattern { title: String::new(), blocks: vec![] },
        FREQS_MHZ,
    );
}
