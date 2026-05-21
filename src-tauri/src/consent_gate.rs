//! Part 97 consent gate for the operator-only `live_cms_smoke` binary.
//!
//! bd issue: tuxlink-nk7 (v0.0.1 plan Task 6)
//!
//! The consent-gate logic is pure: it renders a scoped transmission plan to a
//! `Write` sink, reads one line from a `Read` source, and returns whether the
//! operator granted consent. Consent is granted ONLY when the operator types
//! exactly `go` (no surrounding whitespace, no case variation) and presses
//! Enter. This module touches NO network, NO keyring, and initiates NO
//! transmission — it is unit-testable with `std::io::Cursor` stand-ins, which
//! is how the Part-97-safe `consent_gate_test.rs` exercises it.
//!
//! See docs/live-cms-testing-policy.md and docs/pitfalls/implementation-pitfalls.md
//! §0 (RADIO-1) for the rationale: no automation, test, CI job, or AI agent
//! initiates a transmission under the operator's callsign without explicit,
//! scoped, per-invocation consent at the moment of the run.

use std::io::{BufRead, BufReader, Read, Write};

/// A single scoped transmission plan presented to the operator for consent.
/// Every field is surfaced in the consent banner so the operator authorizes a
/// specific, fully-described activity — not a blanket "transmit something."
pub struct TransmissionPlan {
    pub target: String,
    pub session_count: u32,
    pub expected_duration_s: u32,
    pub content: String,
    pub freq_mode_band: String,
    pub callsign: String,
}

/// Outcome of the consent gate. `Granted` means the operator typed exactly
/// `go`; `Aborted` means anything else (including empty input or an I/O error
/// reading stdin) — fail-closed: no transmission unless explicitly granted.
pub enum ConsentOutcome {
    Granted,
    Aborted,
}

/// Render the Part 97 consent banner to `output`, read one line from `input`,
/// and return [`ConsentOutcome::Granted`] iff that line is exactly `go`
/// (after stripping a single trailing newline / CRLF — and nothing else).
///
/// Any I/O error reading the line, an empty input (EOF), or any other text
/// returns [`ConsentOutcome::Aborted`]. The gate is deliberately strict: no
/// case-folding, no whitespace-trimming, no "yes"/"y" synonyms — the operator
/// must type the exact token. This is a Part 97 control-operator gate, not a
/// convenience prompt.
pub fn check_consent<R: Read, W: Write>(plan: &TransmissionPlan, input: R, mut output: W) -> ConsentOutcome {
    let _ = writeln!(
        output,
        "\nWARNING: Live amateur radio transmission.\n\
         This tool will transmit on the amateur radio network under callsign {callsign}.\n\n\
         Planned activity:\n  \
         - Target: {target}\n  \
         - Session count: {count}\n  \
         - Expected duration: {duration} s\n  \
         - Transmission content: {content}\n  \
         - Frequency / mode / band: {fmb}\n\n\
         By typing \"go\" and pressing Enter, you confirm:\n  \
         - You are the station licensee (or their authorized deputy).\n  \
         - You accept responsibility under 47 CFR Part 97 for these transmissions.\n  \
         - You consent to this specific run only; no future run is authorized.\n  \
         - You will monitor for completion.\n\n\
         Type \"go\" to proceed, anything else to abort:\n> ",
        callsign = plan.callsign,
        target = plan.target,
        count = plan.session_count,
        duration = plan.expected_duration_s,
        content = plan.content,
        fmb = plan.freq_mode_band,
    );
    let _ = output.flush();

    let mut reader = BufReader::new(input);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return ConsentOutcome::Aborted;
    }
    // Strip exactly one trailing newline (Unix) or CRLF (Windows), nothing else.
    // "go" exactly; no surrounding whitespace, no case variation.
    let trimmed = line.strip_suffix('\n').unwrap_or(&line).strip_suffix('\r').unwrap_or(line.strip_suffix('\n').unwrap_or(&line));
    if trimmed == "go" {
        ConsentOutcome::Granted
    } else {
        let _ = writeln!(output, "Aborted — no transmission occurred.");
        ConsentOutcome::Aborted
    }
}
