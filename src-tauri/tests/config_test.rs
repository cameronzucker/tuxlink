// Tests for tuxlink-4mt — see docs/superpowers/specs/2026-05-18-task-2-config-impl-design.md §6
// for the full 24-test matrix and the design rationale.

use tuxlink_lib::config::{validate_identity, validate_identity_describe};

// ============================================================================
// Phase 1 — validate_identity + describe-helper (loose-validator rules)
// ============================================================================

#[test]
fn test_validate_identity_loose_rules_accept() {
    assert!(validate_identity("W4PHS"));
    assert!(validate_identity("W4PHS-7"));
    assert!(validate_identity("EOC-1"));
    assert!(validate_identity("BAOFENG-FM-01"));
    assert!(validate_identity("LabBench-3"));
    assert!(validate_identity("W"));                    // 1 char OK
    assert!(validate_identity(&"X".repeat(32)));        // exactly 32 chars OK
}

#[test]
fn test_validate_identity_loose_rules_reject_each_class() {
    assert!(!validate_identity(""), "empty rejected");
    assert!(!validate_identity("W4 PHS"), "internal whitespace rejected");
    assert!(!validate_identity(&"X".repeat(33)), ">32 chars rejected");
    assert!(!validate_identity("W4PHS\x07"), "non-ASCII-printable (BEL) rejected");
    assert!(!validate_identity("W4PHS\x7F"), "DEL rejected");
    assert!(!validate_identity("Ünïcödë"), "non-ASCII rejected");
}

#[test]
fn test_validate_identity_describe_returns_first_rule_violated() {
    // Rule order per spec §3.2: empty → ASCII → whitespace → length
    assert_eq!(validate_identity_describe(""), Some("must not be empty"));
    assert_eq!(validate_identity_describe("Ünï"), Some("must be ASCII-printable"));
    assert_eq!(validate_identity_describe("W4 PHS"), Some("must not contain whitespace"));
    assert_eq!(validate_identity_describe(&"X".repeat(33)), Some("must be ≤32 chars"));
}

#[test]
fn test_validate_identity_describe_precedence_multi_violation() {
    // Per plan-review R2 P2-3: test PRECEDENCE — inputs violating multiple rules
    // should produce the FIRST-rule error. R2 P1-3's actionable-error-first claim
    // is the load-bearing semantic; regression that swapped rule order (e.g., length
    // first) would pass single-violation tests but fail these.
    // 40-char string containing whitespace → whitespace fires before length.
    let ws_long: String = std::iter::repeat("X ").take(20).collect();
    assert_eq!(validate_identity_describe(&ws_long), Some("must not contain whitespace"),
        "whitespace check must fire before length check");
    // 40-char non-ASCII string → ASCII fires before length.
    let non_ascii_long: String = std::iter::repeat("Ü").take(40).collect();
    assert_eq!(validate_identity_describe(&non_ascii_long), Some("must be ASCII-printable"),
        "ASCII check must fire before length check");
}

#[test]
fn test_validate_identity_describe_returns_none_on_accept() {
    assert_eq!(validate_identity_describe("W4PHS"), None);
    assert_eq!(validate_identity_describe("EOC-1"), None);
    assert_eq!(validate_identity_describe(&"X".repeat(32)), None);
}

#[test]
fn test_validate_identity_consistency_with_describe() {
    // validate_identity == validate_identity_describe(s).is_none()
    for s in &["W4PHS", "EOC-1", "", "W4 PHS", "Ünï", &"X".repeat(33)] {
        let by_bool = validate_identity(s);
        let by_describe = validate_identity_describe(s).is_none();
        assert_eq!(by_bool, by_describe, "consistency violation for input {:?}", s);
    }
}
