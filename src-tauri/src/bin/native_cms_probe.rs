//! Dev probe: validate the NATIVE Winlink client against the real CMS, over
//! TLS-wrapped telnet (default) or plaintext telnet. It runs a full session but
//! transfers no message bodies — it DEFERS every inbound proposal (so the CMS
//! keeps the operator's mail) and sends no outbound.
//!
//! Authorized CMS telnet dev testing — internet transport, not RF, so RADIO-1's
//! per-transmission consent gate (RF under the callsign) does not apply.
//!
//!   cargo run --bin native_cms_probe                 # TLS, cms-z:8773
//!   TUXLINK_CMS_HOST=server.winlink.org cargo run --bin native_cms_probe
//!   TUXLINK_CMS_PORT=8772 TUXLINK_CMS_PLAINTEXT=1 cargo run --bin native_cms_probe

use tuxlink_lib::config;
use tuxlink_lib::winlink::proposal::Answer;
use tuxlink_lib::winlink::session::ExchangeConfig;
use tuxlink_lib::winlink::telnet::{self, Transport};

fn main() {
    let config = config::read_config().expect("read tuxlink config");
    let callsign = config
        .identity
        .callsign
        .clone()
        .expect("config has a callsign")
        .trim()
        .to_uppercase();
    let locator = config.identity.grid.clone().unwrap_or_default();

    let host = std::env::var("TUXLINK_CMS_HOST").unwrap_or_else(|_| "cms-z.winlink.org".to_string());
    let plaintext = std::env::var("TUXLINK_CMS_PLAINTEXT").is_ok();
    let transport = if plaintext {
        Transport::Plaintext
    } else {
        Transport::Tls
    };
    let port: u16 = std::env::var("TUXLINK_CMS_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(if plaintext { 8772 } else { 8773 });

    // The Winlink password (answers the B2F secure-login challenge) from the OS
    // keyring — same entry the wizard/Pat use.
    let password = keyring::Entry::new("tuxlink-pat", &callsign)
        .ok()
        .and_then(|e| e.get_password().ok())
        .filter(|p| !p.is_empty());

    println!("Connecting to {host}:{port} as {callsign} via {transport:?} ...");
    if password.is_none() {
        println!("(no keyring password found — secure login will be skipped/fail)");
    }

    let exchange_config = ExchangeConfig {
        mycall: callsign,
        targetcall: telnet::CMS_TARGET_CALL.to_string(),
        locator,
        password,
    };

    let result = telnet::connect_and_exchange(
        &host,
        port,
        transport,
        &exchange_config,
        Vec::new(),
        &|msg: &str| println!("  · {msg}"),
        &|_| {},
        |proposals| {
            if !proposals.is_empty() {
                println!("CMS offered {} message(s); deferring all", proposals.len());
            }
            proposals.iter().map(|_| Answer::Defer).collect()
        },
    );

    match result {
        Ok(outcome) => println!(
            "\nOK — native client completed a CMS session over {transport:?}. \
             received={} (deferred any offered; no mail transferred).",
            outcome.received.len()
        ),
        Err(e) => println!("\nExchange ended: {e:?}"),
    }
}
