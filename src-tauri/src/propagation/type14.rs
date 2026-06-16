//! VOACAP **Type-14** point-to-point antenna pattern emitter.
//!
//! A Type-14 `.voa` file carries a *real* elevation-gain table — 30 frequency
//! blocks, each an efficiency value plus 91 gains (dBi) at elevation angles
//! 0°..90° in 1° steps. This is the honest antenna model the Find-a-Station
//! prediction epic feeds voacapl, replacing the parametric IONCAP type-codes
//! (22/23/24) that the product-name presets collapsed onto. Design:
//! `docs/design/2026-06-15-find-a-station-antenna-real-patterns.md`.
//!
//! ## Why a byte-exact fixed-format emitter (not list-directed)
//!
//! voacapl's antenna loader parses the text `.voa` into a direct-access binary
//! scratch (`read(14,rec=ifreq)` at `voacapw/antcalc.for:184`). The text is read
//! with a **fixed Fortran format**, not list-directed — a flat / space-joined /
//! LF layout fails with *"Bad value during floating point read"*. The layout
//! this emitter reproduces, measured byte-for-byte against voacapl's own
//! `antennas/samples/sample.14`:
//!
//! - **CRLF** line endings throughout; the file ends with a trailing CRLF.
//! - A 5-line header (`3 parameters`: Max Gain, Antenna Type=14, Frequency).
//! - 30 frequency blocks. Each block = 10 lines:
//!   - line 0: `%2d` block index + `%6.2f` efficiency + one space + 10×`%7.3f` gains
//!   - lines 1..8: 9-space indent + 10×`%7.3f` gains
//!   - line 9: 9-space indent + 1×`%7.3f` gain  (10+8×10+1 = 91)
//! - Gains are **F7.3 with legacy leading-zero suppression**: `0.0`→`"   .000"`,
//!   `-0.072`→`"  -.072"` (g77-style; voacapl's own samples are written this way).
//!
//! Contrast with [`super::antenna::operator_voa_content`], which emits the
//! parametric types 22/23/24 — those *are* read list-directed, so column
//! alignment there does not matter. Type-14 is the strict path.
//!
//! The companion reference implementation + voacapl round-trip verification live
//! in `dev/scratch/type14_ref.py`; the committed golden fixture
//! `testdata/type14_hiang_golden.voa` is a high-angle pattern this emitter
//! reproduces byte-for-byte (asserted in tests) and that voacapl ingests to drive
//! a 215 km NVIS path to high reliability (peak SNR 59 dB vs 28 dB for the
//! zenith-null low-angle control — the 31 dB delta matches the +6/−25 dBi gain
//! difference at the ~70° takeoff angle).
//!
//! Phase 0 (this module) is the foundation only — it does **not** switch the
//! runtime default off the IONCAP path; Phases 1–3 (C/B/A) source real patterns.

/// Elevation gains per frequency block: 0°..90° inclusive, 1° steps.
pub const N_GAINS: usize = 91;
/// Frequency blocks in a Type-14 file (voacapl's fixed expectation).
pub const N_BLOCKS: usize = 30;
/// Title line maximum (voacapl truncates/wraps beyond this).
pub const MAX_TITLE_CHARS: usize = 70;

/// One frequency block: an efficiency value (dB) and 91 elevation gains (dBi).
#[derive(Debug, Clone, PartialEq)]
pub struct FreqBlock {
    /// Per-frequency efficiency, rendered F6.2 (e.g. `-1.70`, or `0.00`→`   .00`).
    pub efficiency: f64,
    /// 91 gains in dBi at elevation 0°..90°. Must be exactly [`N_GAINS`] long.
    pub gains: Vec<f64>,
}

/// A full Type-14 pattern: a title plus exactly [`N_BLOCKS`] frequency blocks.
#[derive(Debug, Clone, PartialEq)]
pub struct Type14Pattern {
    /// Title line (≤ [`MAX_TITLE_CHARS`]); a trailing `:comment` is conventional.
    pub title: String,
    /// Exactly 30 frequency blocks.
    pub blocks: Vec<FreqBlock>,
}

/// Reasons a [`Type14Pattern`] cannot be rendered to a valid `.voa`.
///
/// The value-range / finiteness variants exist because voacapl reads the gain
/// table with **fixed-width** Fortran fields (`F7.3` gains, `F6.2` efficiency):
/// a value whose rendered form overflows its field would *widen* the column and
/// silently shift every subsequent value voacapl reads — a plausible-but-wrong
/// pattern, not a loud failure. The emitter is the last line of defence against
/// that, so it refuses rather than emits. Callers feeding real NEC output (whose
/// deep nulls can run below −100 dBi) must clamp to the representable floor
/// (≥ −99.999 dBi, physically the noise floor) *before* emitting.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum Type14Error {
    #[error("expected {N_BLOCKS} frequency blocks, got {0}")]
    BlockCount(usize),
    #[error("block {block} has {got} gains, expected {N_GAINS}")]
    GainCount { block: usize, got: usize },
    #[error("title is {0} chars, exceeds the {MAX_TITLE_CHARS}-char limit")]
    TitleTooLong(usize),
    #[error("block {block} gain[{index}] is not finite ({value})")]
    NonFiniteGain { block: usize, index: usize, value: f64 },
    #[error("block {block} gain[{index}] = {value} overflows the F7.3 field")]
    GainOutOfRange { block: usize, index: usize, value: f64 },
    #[error("block {block} efficiency is not finite ({value})")]
    NonFiniteEfficiency { block: usize, value: f64 },
    #[error("block {block} efficiency = {value} overflows the F6.2 field")]
    EfficiencyOutOfRange { block: usize, value: f64 },
}

/// Suppress a single leading zero in the integer part, g77-style:
/// `"0.000"`→`".000"`, `"-0.072"`→`"-.072"`. Leaves `"-11.700"`, `"6.407"` as-is.
fn suppress_leading_zero(s: &str) -> String {
    if let Some(rest) = s.strip_prefix("0.") {
        format!(".{rest}")
    } else if let Some(rest) = s.strip_prefix("-0.") {
        format!("-.{rest}")
    } else {
        s.to_string()
    }
}

/// Format a gain as a 7-wide F7.3 field with leading-zero suppression.
/// Negative zero is normalised to positive so it renders `"   .000"`, never
/// `"  -.000"` (matches voacapl's sample files).
fn f7_3(v: f64) -> String {
    let v = if v == 0.0 { 0.0 } else { v };
    format!("{:>7}", suppress_leading_zero(&format!("{v:.3}")))
}

/// Format an efficiency as a 6-wide F6.2 field with leading-zero suppression.
fn f6_2(v: f64) -> String {
    let v = if v == 0.0 { 0.0 } else { v };
    format!("{:>6}", suppress_leading_zero(&format!("{v:.2}")))
}

impl Type14Pattern {
    /// Render to the fixed-format Type-14 `.voa` text (CRLF, trailing CRLF).
    ///
    /// Validates the title length, the block count (must be 30), and each
    /// block's gain count (must be 91) before emitting; returns [`Type14Error`]
    /// rather than producing a file voacapl would silently mis-parse.
    pub fn to_voa(&self) -> Result<String, Type14Error> {
        if self.title.chars().count() > MAX_TITLE_CHARS {
            return Err(Type14Error::TitleTooLong(self.title.chars().count()));
        }
        if self.blocks.len() != N_BLOCKS {
            return Err(Type14Error::BlockCount(self.blocks.len()));
        }
        for (i, b) in self.blocks.iter().enumerate() {
            let block = i + 1;
            if b.gains.len() != N_GAINS {
                return Err(Type14Error::GainCount {
                    block,
                    got: b.gains.len(),
                });
            }
            // Efficiency must be finite and fit its F6.2 column.
            if !b.efficiency.is_finite() {
                return Err(Type14Error::NonFiniteEfficiency {
                    block,
                    value: b.efficiency,
                });
            }
            if f6_2(b.efficiency).len() > 6 {
                return Err(Type14Error::EfficiencyOutOfRange {
                    block,
                    value: b.efficiency,
                });
            }
            // Every gain must be finite and fit its F7.3 column; a value that
            // overflowed would widen the field and shift all following columns,
            // so voacapl would silently read a different gain table.
            for (j, g) in b.gains.iter().enumerate() {
                if !g.is_finite() {
                    return Err(Type14Error::NonFiniteGain {
                        block,
                        index: j,
                        value: *g,
                    });
                }
                if f7_3(*g).len() > 7 {
                    return Err(Type14Error::GainOutOfRange {
                        block,
                        index: j,
                        value: *g,
                    });
                }
            }
        }

        let mut lines: Vec<String> = Vec::with_capacity(5 + N_BLOCKS * 10);
        lines.push(self.title.clone());
        lines.push(" 3     3 parameters".to_string());
        lines.push("  0.00  [ 1] Max Gain dBi..:".to_string());
        lines.push("  14    [ 2] Antenna Type..: 30 x (efficiency + 91 gain values) follow".to_string());
        lines.push("10.00   [ 3] Frequency".to_string());

        for (i, b) in self.blocks.iter().enumerate() {
            let cells: Vec<String> = b.gains.iter().map(|g| f7_3(*g)).collect();
            // First line: index + efficiency + one space + first 10 gains.
            let mut first = format!("{:2}{} ", i + 1, f6_2(b.efficiency));
            first.push_str(&cells[0..10].concat());
            lines.push(first);
            // Continuation lines: 9-space indent + up to 10 gains.
            let mut idx = 10;
            while idx < N_GAINS {
                let end = (idx + 10).min(N_GAINS);
                let mut cont = " ".repeat(9);
                cont.push_str(&cells[idx..end].concat());
                lines.push(cont);
                idx += 10;
            }
        }

        let mut out = lines.join("\r\n");
        out.push_str("\r\n");
        Ok(out)
    }
}

/// Parse block `block` (1..=[`N_BLOCKS`], i.e. `block` MHz) of a Type-14 `.voa`
/// back into its 91 elevation gains (`gains[i]` = gain at elevation `i`°). This is
/// the inverse of [`Type14Pattern::to_voa`]'s block layout: 5 header lines, then 10
/// lines per block, with gains as 7-wide F7.3 fields starting at byte offset 9 on
/// every line. Used by the antenna-pattern preview (a read-only projection of the
/// same data that feeds voacapl). ASCII content, so byte offset == char offset.
pub fn read_block_gains(voa: &str, block: usize) -> Result<Vec<f64>, String> {
    if block < 1 || block > N_BLOCKS {
        return Err(format!("block {block} out of range 1..={N_BLOCKS}"));
    }
    let lines: Vec<&str> = voa.lines().collect();
    let start = 5 + (block - 1) * 10; // 5 header lines + 10 lines/block
    let end = start + 10;
    if lines.len() < end {
        return Err(format!(
            "truncated Type-14: need {end} lines for block {block}, have {}",
            lines.len()
        ));
    }
    let mut gains = Vec::with_capacity(N_GAINS);
    for line in &lines[start..end] {
        if line.len() < 9 {
            continue; // shorter than the gain offset → no fields
        }
        let body = &line[9..];
        let mut i = 0;
        while i + 7 <= body.len() {
            let field = body[i..i + 7].trim();
            if !field.is_empty() {
                gains.push(
                    field
                        .parse::<f64>()
                        .map_err(|_| format!("bad gain field {field:?} in block {block}"))?,
                );
            }
            i += 7;
        }
    }
    if gains.len() != N_GAINS {
        return Err(format!(
            "block {block}: parsed {} gains, expected {N_GAINS}",
            gains.len()
        ));
    }
    Ok(gains)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The high-angle synthetic pattern that produced the committed golden
    /// fixture: +6 dBi at elevations ≥45°, −25 dBi below, identical across all
    /// 30 frequency blocks, efficiency 0.00. Mirrors `dev/scratch/type14_ref.py
    /// synth high` exactly.
    fn synth_high() -> Type14Pattern {
        let gains: Vec<f64> = (0..91)
            .map(|deg| if deg >= 45 { 6.0 } else { -25.0 })
            .collect();
        let blocks = (0..N_BLOCKS)
            .map(|_| FreqBlock {
                efficiency: 0.00,
                gains: gains.clone(),
            })
            .collect();
        Type14Pattern {
            title: "tuxlink-test high-angle  :synthetic Type-14 verification pattern".to_string(),
            blocks,
        }
    }

    #[test]
    fn read_block_gains_round_trips_to_voa() {
        // Per-block-distinct (constant b) so block indexing is verified, plus the
        // clamped-floor edge (-99.999) and a positive value. Integers + -99.999 are
        // their own 3-decimal round-trip, so assert_eq is exact.
        let blocks: Vec<FreqBlock> = (0..N_BLOCKS)
            .map(|b| {
                let mut gains = vec![b as f64; N_GAINS];
                gains[0] = -99.999; // elevation 0 (horizon) floor
                gains[90] = 6.0; // elevation 90 (zenith)
                FreqBlock {
                    efficiency: 0.00,
                    gains,
                }
            })
            .collect();
        let p = Type14Pattern {
            title: "round-trip".to_string(),
            blocks,
        };
        let voa = p.to_voa().unwrap();
        for b in 1..=N_BLOCKS {
            let got = read_block_gains(&voa, b).unwrap();
            assert_eq!(got, p.blocks[b - 1].gains, "block {b} gains mismatch");
        }
        assert!(read_block_gains(&voa, 0).is_err());
        assert!(read_block_gains(&voa, N_BLOCKS + 1).is_err());
    }

    #[test]
    fn f7_3_is_width_seven_with_leading_zero_suppressed() {
        assert_eq!(f7_3(0.0), "   .000");
        assert_eq!(f7_3(-0.072), "  -.072");
        assert_eq!(f7_3(0.472), "   .472");
        assert_eq!(f7_3(6.0), "  6.000");
        assert_eq!(f7_3(6.407), "  6.407");
        assert_eq!(f7_3(-11.7), "-11.700");
        assert_eq!(f7_3(-25.0), "-25.000");
        // negative zero must not render a sign
        assert_eq!(f7_3(-0.0), "   .000");
        // every field is exactly 7 chars wide
        for v in [0.0, -0.072, 6.407, -11.7, -25.0, 10.138] {
            assert_eq!(f7_3(v).len(), 7, "f7_3({v}) not width 7");
        }
    }

    #[test]
    fn f6_2_is_width_six_with_leading_zero_suppressed() {
        assert_eq!(f6_2(0.0), "   .00");
        assert_eq!(f6_2(-1.70), " -1.70");
        assert_eq!(f6_2(0.0).len(), 6);
        assert_eq!(f6_2(-1.70).len(), 6);
    }

    #[test]
    fn to_voa_reproduces_voacapl_accepted_golden_byte_for_byte() {
        // The golden was generated by the verified Python reference and ingested
        // by voacapl on the 215 km NVIS deck (high-angle → REL 1.00). If this
        // assertion holds, the Rust emitter produces bytes voacapl accepts.
        let golden = include_str!("testdata/type14_hiang_golden.voa");
        assert_eq!(synth_high().to_voa().unwrap(), golden);
    }

    #[test]
    fn header_is_exactly_five_known_lines() {
        let voa = synth_high().to_voa().unwrap();
        let lines: Vec<&str> = voa.split("\r\n").collect();
        assert_eq!(lines[0], "tuxlink-test high-angle  :synthetic Type-14 verification pattern");
        assert_eq!(lines[1], " 3     3 parameters");
        assert_eq!(lines[2], "  0.00  [ 1] Max Gain dBi..:");
        assert_eq!(lines[3], "  14    [ 2] Antenna Type..: 30 x (efficiency + 91 gain values) follow");
        assert_eq!(lines[4], "10.00   [ 3] Frequency");
    }

    #[test]
    fn emits_thirty_blocks_of_ten_lines_with_crlf() {
        let voa = synth_high().to_voa().unwrap();
        assert!(voa.ends_with("\r\n"), "must end with trailing CRLF");
        assert!(!voa.contains('\n') || voa.contains("\r\n"), "no bare LF");
        // 5 header + 30*10 data lines, plus a trailing empty from the final CRLF.
        let parts: Vec<&str> = voa.split("\r\n").collect();
        assert_eq!(parts.len(), 5 + N_BLOCKS * 10 + 1);
        assert_eq!(parts.last(), Some(&""));
    }

    #[test]
    fn block_first_line_carries_index_efficiency_and_first_ten_gains() {
        let voa = synth_high().to_voa().unwrap();
        let lines: Vec<&str> = voa.split("\r\n").collect();
        // Block 1 first data line (index 5): gains 0..9 are all -25 dBi.
        let b1 = lines[5];
        assert_eq!(&b1[0..9], " 1   .00 "); // %2d + F6.2(0) + space
        assert_eq!(b1, format!(" 1   .00 {}", "-25.000".repeat(10)));
        assert_eq!(b1.len(), 79);
        // Block 30 first data line starts at 5 + 29*10.
        let b30 = lines[5 + 29 * 10];
        assert_eq!(&b30[0..9], "30   .00 ");
    }

    #[test]
    fn the_45_degree_transition_line_mixes_low_and_high_gain() {
        // Within a block, line 4 holds gains 40..49: 40-44 below 45° (−25),
        // 45-49 at/above 45° (+6). 9-space indent + 10 F7.3 cells.
        let voa = synth_high().to_voa().unwrap();
        let lines: Vec<&str> = voa.split("\r\n").collect();
        let line4 = lines[5 + 4]; // block 1, 5th line
        let expected = format!("{}{}{}", " ".repeat(9), "-25.000".repeat(5), "  6.000".repeat(5));
        assert_eq!(line4, expected);
        assert_eq!(line4.len(), 79);
    }

    #[test]
    fn last_line_of_a_block_holds_the_single_91st_gain() {
        let voa = synth_high().to_voa().unwrap();
        let lines: Vec<&str> = voa.split("\r\n").collect();
        // Block 1 line 9 (gains[90]): elevation 90° ≥ 45 → +6 dBi.
        let last = lines[5 + 9];
        assert_eq!(last, format!("{}  6.000", " ".repeat(9)));
        assert_eq!(last.len(), 16);
    }

    #[test]
    fn rejects_wrong_block_count() {
        let mut p = synth_high();
        p.blocks.truncate(29);
        assert_eq!(p.to_voa(), Err(Type14Error::BlockCount(29)));
    }

    #[test]
    fn rejects_wrong_gain_count() {
        let mut p = synth_high();
        p.blocks[7].gains.truncate(90);
        assert_eq!(
            p.to_voa(),
            Err(Type14Error::GainCount { block: 8, got: 90 })
        );
    }

    #[test]
    fn rejects_overlong_title() {
        let mut p = synth_high();
        p.title = "x".repeat(71);
        assert_eq!(p.to_voa(), Err(Type14Error::TitleTooLong(71)));
    }

    #[test]
    fn rejects_non_finite_gain() {
        // NaN/inf must be refused, not formatted as '    NaN'/'    inf' inside
        // the field. `matches!` (not assert_eq!) because NaN != NaN under
        // PartialEq would defeat an equality assertion on the carried value.
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut p = synth_high();
            p.blocks[3].gains[60] = bad;
            assert!(
                matches!(
                    p.to_voa(),
                    Err(Type14Error::NonFiniteGain {
                        block: 4,
                        index: 60,
                        ..
                    })
                ),
                "non-finite gain {bad} must be refused"
            );
        }
    }

    #[test]
    fn rejects_gain_that_overflows_the_f7_3_field() {
        // A deep NEC null below -100 dBi renders "-100.000" (8 chars) and would
        // shift every following column; +1000 dBi likewise. Both must error.
        for bad in [-100.0, -120.5, 1000.0] {
            let mut p = synth_high();
            p.blocks[0].gains[0] = bad;
            assert_eq!(
                p.to_voa(),
                Err(Type14Error::GainOutOfRange {
                    block: 1,
                    index: 0,
                    value: bad
                })
            );
        }
    }

    #[test]
    fn accepts_extreme_but_field_representable_values() {
        // The boundary values that still fit F7.3 / F6.2 must NOT be rejected.
        let mut p = synth_high();
        p.blocks[0].gains[0] = -99.999; // 7 chars exactly
        p.blocks[0].gains[1] = 999.999; // 7 chars exactly
        p.blocks[0].efficiency = -99.99; // 6 chars exactly
        assert!(p.to_voa().is_ok());
    }

    #[test]
    fn rejects_non_finite_efficiency() {
        let mut p = synth_high();
        p.blocks[2].efficiency = f64::NAN;
        assert!(matches!(
            p.to_voa(),
            Err(Type14Error::NonFiniteEfficiency { block: 3, .. })
        ));
    }

    #[test]
    fn rejects_efficiency_that_overflows_the_f6_2_field() {
        let mut p = synth_high();
        p.blocks[1].efficiency = -100.0; // "-100.00" is 7 chars, overflows F6.2
        assert_eq!(
            p.to_voa(),
            Err(Type14Error::EfficiencyOutOfRange {
                block: 2,
                value: -100.0
            })
        );
    }
}
