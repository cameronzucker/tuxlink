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

use std::collections::BTreeMap;
use std::io::Write;
use std::process::Command;
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

// ---------------------------------------------------------------------------
// nec2c subprocess + radiation-pattern parser
// ---------------------------------------------------------------------------

/// Run nec2c on a card deck, returning the output-file text.
fn run_nec2c(deck: &str) -> std::io::Result<String> {
    let dir = std::env::temp_dir();
    let stamp = format!("{}_{:p}", std::process::id(), deck);
    let inp = dir.join(format!("tux_nec_{stamp}.nec"));
    let out = dir.join(format!("tux_nec_{stamp}.out"));
    std::fs::File::create(&inp)?.write_all(deck.as_bytes())?;
    let status = Command::new("nec2c")
        .arg(format!("-i{}", inp.display()))
        .arg(format!("-o{}", out.display()))
        .status()?;
    if !status.success() {
        return Err(std::io::Error::other("nec2c exited non-zero"));
    }
    let text = std::fs::read_to_string(&out)?;
    let _ = std::fs::remove_file(&inp);
    let _ = std::fs::remove_file(&out);
    Ok(text)
}

/// Parse the RADIATION PATTERNS table; return THETA(deg) → TOTAL gain (dBi), raw (unclamped).
/// Columns (whitespace-delimited): THETA PHI VERTC HORIZ TOTAL ... ; TOTAL is index 4 (0-based).
fn parse_total_gains(out: &str) -> Result<BTreeMap<u32, f64>, String> {
    let start = out
        .find("RADIATION PATTERNS")
        .ok_or("no radiation-pattern block in nec2c output")?;
    let mut map = BTreeMap::new();
    for line in out[start..].lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 5 {
            if let (Ok(theta), Ok(total)) = (cols[0].parse::<f64>(), cols[4].parse::<f64>()) {
                if (0.0..=90.0).contains(&theta) {
                    map.insert(theta.round() as u32, total);
                }
            }
        }
    }
    if map.is_empty() {
        return Err("no data rows parsed from radiation-pattern block".into());
    }
    Ok(map)
}

// ---------------------------------------------------------------------------
// Elevation-vector assembly + null clamp + Type14Pattern builder
// ---------------------------------------------------------------------------

/// Clamp to the Type-14 F7.3 floor so to_voa() never errors on a deep NEC null
/// (nec2c prints -999.99 sentinels; a real null can go far below -100 dBi).
fn clamp_gain(g: f64) -> f64 {
    if !g.is_finite() {
        return -99.999;
    }
    g.clamp(-99.999, 999.999)
}

/// gains[i] = gain at elevation i degrees (i in 0..=90). elevation i = theta (90 - i),
/// matching the Phase 0 golden convention (higher index = higher takeoff angle).
fn elevation_vector(by_theta: &BTreeMap<u32, f64>) -> Vec<f64> {
    (0..=90u32)
        .map(|elev| {
            let theta = 90 - elev;
            clamp_gain(by_theta.get(&theta).copied().unwrap_or(-99.999))
        })
        .collect()
}

/// Build a Type14Pattern by running nec2c at each of the 30 freqs for a fixed geometry.
/// `deck_at(freq_mhz)` returns the full nec2c deck for that frequency.
fn build_pattern(title: &str, deck_at: impl Fn(f64) -> String) -> Result<Type14Pattern, String> {
    let mut blocks = Vec::with_capacity(N_BLOCKS);
    for &f in FREQS_MHZ.iter() {
        let out = run_nec2c(&deck_at(f)).map_err(|e| format!("nec2c {f} MHz: {e}"))?;
        let by_theta = parse_total_gains(&out)?;
        blocks.push(FreqBlock {
            efficiency: 0.0,
            gains: elevation_vector(&by_theta),
        });
    }
    Ok(Type14Pattern {
        title: title.chars().take(type14::MAX_TITLE_CHARS).collect(),
        blocks,
    })
}

// ---------------------------------------------------------------------------
// Antenna geometries (RF-critical). Each is a real NEC deck; nec2c computes the
// gains. All over poor/dry-desert ground (GN 2 ... 3.0 0.001). Modeling choices
// documented per antenna; the F2 Codex RF round reviews them before "shipped".
// Elevation cut: RP 0 91 1 ... sweeps theta 0..90 (1deg) = the 91 Type-14 points.
// ---------------------------------------------------------------------------

const GROUND_POOR: &str = "GN 2 0 0 0 3.0 0.001";
const RP_ELEV: &str = "RP 0 91 1 1000 0.0 0.0 1.0 0.0"; // theta sweep, normalized power gain
const RP_ELEV_BORESIGHT: &str = "RP 0 91 1 0 0.0 0.0 1.0 0.0"; // raw gain (yagi, phi=0 boresight)

/// NVIS wire dipole / OCFD: flat center-fed 20 m horizontal wire at `apex_m`.
/// Pure high-angle (overhead) lobe at low height — the regional/NVIS entry.
fn deck_nvis_dipole(freq_mhz: f64, apex_m: f64) -> String {
    format!(
        "CM tuxlink nvis-wire-dipole 20m flat @ {apex_m:.1}m, poor ground\nCE\n\
         GW 1 61 -10.0 0 {apex_m:.3} 10.0 0 {apex_m:.3} 0.001\nGE -1\n{GROUND_POOR}\n\
         EX 0 1 31 0 1.0 0.0\nFR 0 1 0 0 {freq_mhz:.3} 0\n{RP_ELEV}\nEN\n"
    )
}

/// EFHW / sloper: single ~18 m wire sloping from a high feedpoint (`apex_m`) down to
/// 2 m, end-fed (seg 1). Tilted lobe with mixed polarization + some low-angle — distinct
/// from the flat dipole by its slope. Pattern is geometry-dominated; the high feed
/// impedance off-resonance doesn't change the elevation lobe (efficiency isn't a Type-14 axis).
fn deck_efhw_sloper(freq_mhz: f64, apex_m: f64) -> String {
    format!(
        "CM tuxlink efhw-sloper 18m, apex {apex_m:.1}m -> 2m, end-fed, poor ground\nCE\n\
         GW 1 61 0.0 0 {apex_m:.3} 18.0 0 2.0 0.001\nGE -1\n{GROUND_POOR}\n\
         EX 0 1 1 0 1.0 0.0\nFR 0 1 0 0 {freq_mhz:.3} 0\n{RP_ELEV}\nEN\n"
    )
}

/// Portable dipole (linked / inverted-V): apex (center) at `apex_m`, legs drooping to
/// `end_z`. Two wires meeting at the apex, fed at the junction. Compromise takeoff angle,
/// near-omni azimuth — between the flat dipole and a vertical.
fn deck_portable_dipole(freq_mhz: f64, apex_m: f64) -> String {
    let end_z = (apex_m - 3.0).max(2.0);
    format!(
        "CM tuxlink portable inverted-V 20m, apex {apex_m:.1}m -> ends {end_z:.1}m, poor ground\nCE\n\
         GW 1 31 -10.0 0 {end_z:.3} 0.0 0 {apex_m:.3} 0.001\n\
         GW 2 31 0.0 0 {apex_m:.3} 10.0 0 {end_z:.3} 0.001\nGE -1\n{GROUND_POOR}\n\
         EX 0 1 31 0 1.0 0.0\nFR 0 1 0 0 {freq_mhz:.3} 0\n{RP_ELEV}\nEN\n"
    )
}

/// Beam / Yagi: monoband 3-element design fixed for ~14 MHz (lengths/spacing in metres),
/// boom along x, elements parallel to y, at apex `apex_m`, boresight (phi=0) elevation cut.
/// Swept across 1..30 MHz it is off-design (degrades to three wires) away from 14 MHz —
/// the honest behavior of a real monoband beam. Driven element fed at center.
fn deck_yagi(freq_mhz: f64, apex_m: f64) -> String {
    // 14 MHz design (lambda 21.4 m): reflector 10.7, driven 10.06, director 9.6; spacing 3.2 m.
    format!(
        "CM tuxlink 3-el yagi (14MHz design) @ {apex_m:.1}m boresight, poor ground\nCE\n\
         GW 1 41 0.0 -5.35 {apex_m:.3} 0.0 5.35 {apex_m:.3} 0.005\n\
         GW 2 41 3.2 -5.03 {apex_m:.3} 3.2 5.03 {apex_m:.3} 0.005\n\
         GW 3 41 6.4 -4.80 {apex_m:.3} 6.4 4.80 {apex_m:.3} 0.005\nGE -1\n{GROUND_POOR}\n\
         EX 0 2 21 0 1.0 0.0\nFR 0 1 0 0 {freq_mhz:.3} 0\n{RP_ELEV_BORESIGHT}\nEN\n"
    )
}

/// Ground-mounted vertical monopole of `len_m` (base 0.1 m), with 4 radials (10 m, 0.05 m
/// high) over poor soil. Fed at the base. Taller = lower takeoff angle. Used for all three
/// verticals at different lengths; height is NOT an operator axis (ground-mounted geometry).
fn deck_vertical(freq_mhz: f64, len_m: f64) -> String {
    let nseg = ((len_m * 6.0).round() as i32).max(9);
    let mut s = format!(
        "CM tuxlink vertical {len_m:.2}m monopole + 4 radials, poor ground\nCE\n\
         GW 1 {nseg} 0 0 0.1 0 0 {top:.3} 0.001\n",
        top = 0.1 + len_m
    );
    for (i, (x, y)) in [(10.0_f64, 0.0_f64), (0.0, 10.0), (-10.0, 0.0), (0.0, -10.0)]
        .iter()
        .enumerate()
    {
        s.push_str(&format!(
            "GW {tag} 9 0 0 0.05 {x:.3} {y:.3} 0.05 0.001\n",
            tag = i + 2
        ));
    }
    s.push_str(&format!(
        "GE -1\n{GROUND_POOR}\nEX 0 1 1 0 1.0 0.0\nFR 0 1 0 0 {freq_mhz:.3} 0\n{RP_ELEV}\nEN\n"
    ));
    s
}

fn deck_base_vertical(freq_mhz: f64) -> String { deck_vertical(freq_mhz, 10.0) } // tall multiband
fn deck_portable_whip(freq_mhz: f64) -> String { deck_vertical(freq_mhz, 3.0) }  // short portable
fn deck_mobile_whip(freq_mhz: f64) -> String { deck_vertical(freq_mhz, 1.5) }    // short loaded mobile (proxy)

/// Neutral pattern for `unknown`: flat 0 dBi at all elevations/freqs — honest "not modeled".
fn unknown_pattern() -> Type14Pattern {
    let block = FreqBlock { efficiency: 0.0, gains: vec![0.0; N_GAINS] };
    Type14Pattern {
        title: "tuxlink unknown/generic neutral pattern (not modeled)".into(),
        blocks: vec![block; N_BLOCKS],
    }
}

/// Operator height grid for horizontal antennas (apex, metres). Mirrors HEIGHT_GRID_M
/// in the runtime patterns module.
const HEIGHT_GRID_M: [f64; 4] = [2.5, 4.0, 6.0, 9.0];

/// Output dir for committed .voa files (resolved from the generator manifest dir).
fn out_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../src-tauri/src/propagation/patterns")
}

fn write_voa(name: &str, p: &Type14Pattern) {
    let voa = p.to_voa().unwrap_or_else(|e| panic!("emit {name}: {e}"));
    let dir = out_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.voa"));
    std::fs::write(&path, voa).unwrap();
    eprintln!("wrote {}", path.display());
}

fn main() {
    eprintln!(
        "gen_antenna_patterns: {} freq blocks (1..30 MHz, block i = i MHz), target 20 patterns",
        N_BLOCKS
    );
    if Command::new("nec2c").arg("-v").output().is_err() {
        eprintln!("ERROR: nec2c not on PATH — install it (sudo apt install nec2c)");
        std::process::exit(1);
    }

    // Horizontal antennas: <preset>__<apex*10, 3-digit>.voa over the height grid.
    let horizontals: [(&str, fn(f64, f64) -> String); 4] = [
        ("efhw-sloper", deck_efhw_sloper),
        ("nvis-wire-dipole", deck_nvis_dipole),
        ("resonant-portable-dipole", deck_portable_dipole),
        ("beam-yagi", deck_yagi),
    ];
    for (preset, deck) in horizontals {
        for h in HEIGHT_GRID_M {
            let name = format!("{preset}__{:03}", (h * 10.0).round() as u32);
            let title = format!("tuxlink {preset} {h}m poor-ground");
            let p = build_pattern(&title, |f| deck(f, h))
                .unwrap_or_else(|e| panic!("build {name}: {e}"));
            write_voa(&name, &p);
        }
    }

    // Vertical antennas: ground-mounted, no height axis.
    let verticals: [(&str, fn(f64) -> String); 3] = [
        ("portable-vertical-whip", deck_portable_whip),
        ("base-vertical-radials", deck_base_vertical),
        ("mobile-hf-whip", deck_mobile_whip),
    ];
    for (preset, deck) in verticals {
        let title = format!("tuxlink {preset} ground-mounted poor-ground");
        let p = build_pattern(&title, deck).unwrap_or_else(|e| panic!("build {preset}: {e}"));
        write_voa(preset, &p);
    }

    // Neutral fallback.
    write_voa("unknown", &unknown_pattern());
    eprintln!("done: 20 patterns written to {}", out_dir().display());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A slice of real nec2c output: header + 4 data rows (THETA 0,1,89,90).
    const SAMPLE: &str = "\
                             ---------- RADIATION PATTERNS -----------

 ---- ANGLES -----     ----- POWER GAINS -----       ---- POLARIZATION ----
  THETA      PHI       VERTC    HORIZ    TOTAL       AXIAL      TILT  SENSE
 DEGREES   DEGREES        DB       DB       DB       RATIO   DEGREES
    0.00      0.00      3.68  -999.99     3.68      0.0000      0.00 LINEAR
    1.00      0.00      3.68  -999.99     3.68      0.0000      0.00 LINEAR
   89.00      0.00     -8.20  -999.99    -8.20      0.0000      0.00 LINEAR
   90.00      0.00   -999.99  -999.99  -999.99      0.0000      0.00 LINEAR
";

    #[test]
    fn parses_total_gain_by_theta() {
        let gains = parse_total_gains(SAMPLE).unwrap();
        assert_eq!(gains.get(&0).copied(), Some(3.68));
        assert_eq!(gains.get(&89).copied(), Some(-8.20));
        assert_eq!(gains.get(&90).copied(), Some(-999.99)); // sentinel preserved pre-clamp
    }

    #[test]
    fn parse_errors_without_pattern_block() {
        assert!(parse_total_gains("no table here").is_err());
    }

    #[test]
    fn clamp_floors_sentinels_and_nonfinite() {
        assert_eq!(clamp_gain(-999.99), -99.999);
        assert_eq!(clamp_gain(f64::NEG_INFINITY), -99.999);
        assert_eq!(clamp_gain(f64::NAN), -99.999);
        assert_eq!(clamp_gain(3.68), 3.68); // in-range untouched
    }

    #[test]
    fn assembles_91_point_elevation_with_clamp() {
        let mut by_theta = BTreeMap::new();
        for t in 0..=90u32 {
            by_theta.insert(t, 3.0);
        }
        by_theta.insert(90, -999.99); // theta 90 (= elevation 0) null sentinel
        let gains = elevation_vector(&by_theta);
        assert_eq!(gains.len(), N_GAINS); // 91
        assert!(gains[0] >= -99.999); // elevation 0 = theta 90 -> clamped from -999.99
        assert_eq!(gains[90], 3.0); // elevation 90 = theta 0 -> 3.0
    }

    #[test]
    fn elevation_indexing_matches_phase0_high_angle_convention() {
        // theta small (overhead) -> high elevation index; assert index 90 = theta 0.
        let mut by_theta = BTreeMap::new();
        for t in 0..=90u32 {
            by_theta.insert(t, if t <= 20 { 6.0 } else { -25.0 });
        }
        let gains = elevation_vector(&by_theta);
        assert_eq!(gains[90], 6.0); // zenith (theta 0)
        assert_eq!(gains[80], 6.0); // elevation 80 = theta 10 -> still 6.0
        assert_eq!(gains[0], -25.0); // horizon (theta 90)
    }

    /// Physics regression guard (gated on nec2c): a low horizontal wire favors the
    /// zenith; a ground-mounted vertical has a zenith null. Block 14 = 14 MHz.
    #[test]
    fn geometry_physics_is_directionally_honest() {
        if Command::new("nec2c").arg("-v").output().is_err() {
            return;
        }
        let nvis_low = build_pattern("nvis 2.5m", |f| deck_nvis_dipole(f, 2.5)).unwrap();
        let nvis_high = build_pattern("nvis 9m", |f| deck_nvis_dipole(f, 9.0)).unwrap();
        let vert = build_pattern("vert", deck_base_vertical).unwrap();
        let zenith = |p: &Type14Pattern| p.blocks[13].gains[90]; // block 14, elevation 90°
        // Low NVIS wire beats a vertical overhead (NVIS vs zenith-null).
        assert!(zenith(&nvis_low) > zenith(&vert) + 20.0, "wire should beat vertical at zenith");
        // Lower wire concentrates more power overhead than a higher one.
        assert!(zenith(&nvis_low) > zenith(&nvis_high), "low wire favors higher angle");
        // Vertical zenith is a deep (clamped) null.
        assert!(zenith(&vert) <= -90.0, "vertical should null overhead");
    }

    #[test]
    fn built_pattern_emits_valid_voa() {
        // Gate on nec2c presence so the suite passes on machines without it.
        if Command::new("nec2c").arg("-v").output().is_err() {
            return;
        }
        // Trivial horizontal dipole deck (inline; real geometries land in Task B).
        let deck_at = |f: f64| {
            format!(
                "CM probe\nCE\nGW 1 21 -5.3 0 6.0 5.3 0 6.0 0.001\nGE -1\n\
                 GN 2 0 0 0 3.0 0.001\nEX 0 1 11 0 1.0 0.0\n\
                 FR 0 1 0 0 {f:.3} 0\nRP 0 91 1 1000 0.0 0.0 1.0 0.0\nEN\n"
            )
        };
        let p = build_pattern("tuxlink probe dipole 6m poor", deck_at).unwrap();
        assert_eq!(p.blocks.len(), N_BLOCKS);
        assert!(p.to_voa().is_ok(), "emitter rejected a generated pattern");
    }
}
