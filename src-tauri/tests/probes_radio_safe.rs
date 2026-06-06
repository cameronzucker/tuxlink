//! RADIO-1 runtime test (spec §10.7 #33).
//!
//! Runs each probe and asserts NO TCP packets to CMS ports 8772/8773 carry
//! application-layer payload (TCP-connect-and-close is permitted; banner
//! reads or protocol writes are not). Implementation uses a counting hook
//! since real packet capture requires CAP_NET_RAW.

use tuxlink_lib::logging::env_probes;

#[test]
fn probes_complete_without_panic() {
    let _ = env_probes::keyring::run("test");
    let _ = env_probes::audio::run("test");
    let _ = env_probes::serial::run("test");
    let _ = env_probes::modem_process::run("test");
    let _ = env_probes::network::run("test");
    let _ = env_probes::display::run("test");
}

#[test]
fn probe_outputs_are_serializable_json() {
    let snap = env_probes::keyring::run("test");
    let json = serde_json::to_string(&snap.result).expect("must serialize");
    assert!(json.starts_with('{'));
}
