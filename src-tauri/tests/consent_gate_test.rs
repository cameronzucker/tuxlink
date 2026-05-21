//! Consent-gate unit tests (tuxlink-nk7, Task 6 Step 1).
//!
//! Pure I/O: a `std::io::Cursor` stands in for stdin, a `Vec<u8>` for stdout.
//! NO network, NO transmission, NO keyring. These are the Part-97-safe tests
//! that gate the `live_cms_smoke` binary's consent logic.

use std::io::Cursor;
use tuxlink_lib::consent_gate::{check_consent, ConsentOutcome};

fn plan() -> tuxlink_lib::consent_gate::TransmissionPlan {
    tuxlink_lib::consent_gate::TransmissionPlan {
        target: "SERVICE@winlink.org".into(),
        session_count: 1,
        expected_duration_s: 30,
        content: "short test body".into(),
        freq_mode_band: "telnet over IP; no RF".into(),
        callsign: "W4PHS".into(),
    }
}

#[test]
fn test_exact_go_grants_consent() {
    let mut input = Cursor::new(b"go\n".to_vec());
    let mut output = Vec::new();
    let outcome = check_consent(&plan(), &mut input, &mut output);
    assert!(matches!(outcome, ConsentOutcome::Granted));
}

#[test]
fn test_uppercase_go_does_not_grant_consent() {
    let mut input = Cursor::new(b"GO\n".to_vec());
    let mut output = Vec::new();
    let outcome = check_consent(&plan(), &mut input, &mut output);
    assert!(matches!(outcome, ConsentOutcome::Aborted));
}

#[test]
fn test_empty_input_aborts() {
    let mut input = Cursor::new(b"".to_vec());
    let mut output = Vec::new();
    let outcome = check_consent(&plan(), &mut input, &mut output);
    assert!(matches!(outcome, ConsentOutcome::Aborted));
}

#[test]
fn test_any_other_input_aborts() {
    for s in &[b"yes\n".as_ref(), b"y\n".as_ref(), b"\n".as_ref(), b"go now\n".as_ref(), b" go\n".as_ref(), b"go \n".as_ref()] {
        let mut input = Cursor::new(s.to_vec());
        let mut output = Vec::new();
        let outcome = check_consent(&plan(), &mut input, &mut output);
        assert!(matches!(outcome, ConsentOutcome::Aborted), "input {:?} must abort", s);
    }
}

#[test]
fn test_banner_mentions_all_scoped_plan_fields() {
    let mut input = Cursor::new(b"go\n".to_vec());
    let mut output = Vec::new();
    let _ = check_consent(&plan(), &mut input, &mut output);
    let banner = String::from_utf8(output).unwrap();
    assert!(banner.contains("SERVICE@winlink.org"));
    assert!(banner.contains("W4PHS"));
    assert!(banner.contains("1"));       // session count
    assert!(banner.contains("30"));      // duration
    assert!(banner.contains("short test body"));
    assert!(banner.contains("telnet"));
    assert!(banner.contains("Part 97"));
}
