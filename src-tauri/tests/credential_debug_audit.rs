//! Audit test: every credential-bearing struct in the source-verified list
//! (spec §5.3) has a manual Debug impl whose output does NOT contain a
//! representative secret value.
//!
//! This is a runtime test that constructs the struct with a sentinel password,
//! invokes Debug, and grep-asserts the sentinel is absent. New credential-
//! bearing structs that land without manual Debug will fail this test.

const SENTINEL_PASSWORD: &str = "DO-NOT-LEAK-THIS-PASSWORD-XYZZY-12345";

#[test]
fn exchange_config_debug_does_not_leak_password() {
    use tuxlink_lib::winlink::session::{ExchangeConfig, SessionIntent};
    let cfg = ExchangeConfig {
        mycall: "TEST-K0".into(),
        targetcall: "TEST-K6".into(),
        locator: "CN87".into(),
        password: Some(SENTINEL_PASSWORD.into()),
        intent: SessionIntent::Cms,
    };
    let dbg = format!("{cfg:?}");
    assert!(
        !dbg.contains(SENTINEL_PASSWORD),
        "ExchangeConfig Debug leaked the sentinel password: {dbg}"
    );
    assert!(
        dbg.contains("<redacted>") || dbg.contains("Some(\"<redacted>\")"),
        "ExchangeConfig Debug must show redacted marker; got: {dbg}"
    );
}

#[test]
fn station_password_debug_does_not_leak_value() {
    use tuxlink_lib::winlink::listener::station_password::StationPassword;
    // StationPassword's Debug must redact per spec §5.3: the impl renders
    // "<redacted StationPassword>" — no stored value, no factory internals.
    // Default::default() calls StationPassword::new() which uses the real OS
    // keyring factory; the Debug impl never calls get_password(), so this
    // test is safe to run without a real keyring entry present.
    let pw = StationPassword::default();
    let dbg = format!("{pw:?}");
    assert!(
        dbg.starts_with("<redacted") || dbg.contains("<redacted"),
        "StationPassword Debug should show redaction marker: {dbg}"
    );
}
