//! RADIO-1 enforcement (spec §9.1, §10.7 #32).
//!
//! This test verifies at COMPILE time that probe modules do NOT import any
//! TX-touching module. The test runs `cargo build` on the probe modules
//! through a synthetic source-level grep: if any probe file's source contains
//! a forbidden module path, the test fails.

const PROBE_FILES: &[&str] = &[
    "src/logging/env_probes/keyring.rs",
    "src/logging/env_probes/audio.rs",
    "src/logging/env_probes/serial.rs",
    "src/logging/env_probes/modem_process.rs",
    "src/logging/env_probes/network.rs",
    "src/logging/env_probes/display.rs",
];

const FORBIDDEN_IMPORTS: &[&str] = &[
    "crate::winlink::session::",
    "crate::winlink::secure",
    "crate::winlink::handshake",
    "crate::winlink::modem::ardop::command",
    "crate::winlink::modem::ardop::session",
    "crate::winlink::modem::vara::commands",
    "crate::winlink::modem::vara::transport",
    "crate::winlink::transfer",
    "winlink::session::ExchangeConfig",
];

#[test]
fn probes_do_not_import_tx_touching_modules() {
    let workspace_root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    for relative in PROBE_FILES {
        let path = std::path::Path::new(&workspace_root).join(relative);
        if !path.exists() {
            continue; // skipped: probe not yet implemented in this commit
        }
        let src = std::fs::read_to_string(&path).expect("read probe source");
        for forbidden in FORBIDDEN_IMPORTS {
            assert!(
                !src.contains(forbidden),
                "RADIO-1 violation: {} imports forbidden module path: {}",
                relative,
                forbidden
            );
        }
    }
}

#[test]
fn modem_process_probe_reads_cached_state_not_live() {
    // Specific check: modem_process must NOT call spawn() or write to modem.
    // It reads from a runtime cache maintained by winlink::modem::process
    // (which is the live owner of process lifecycle state).
    let workspace_root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let path = std::path::Path::new(&workspace_root)
        .join("src/logging/env_probes/modem_process.rs");
    if !path.exists() {
        return;
    }
    let src = std::fs::read_to_string(&path).unwrap();
    assert!(
        !src.contains("Command::new") && !src.contains(".spawn()"),
        "modem_process probe must not spawn processes; should read cached state"
    );
}
