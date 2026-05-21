//! Dev probe: validate the NATIVE Winlink client against the real CMS over
//! telnet. Reads the real handshake (no credentials, sends nothing); if a
//! keyring password is available it also performs the secure login and a
//! transfer-nothing session (defers every inbound proposal, sends no outbound).
//!
//! This is authorized CMS *telnet* dev testing — internet transport, not RF, so
//! RADIO-1's per-transmission consent gate (which covers RF under the callsign)
//! does not apply. It transfers no message bodies and queues no mail.
//!
//!   cargo run --bin native_cms_probe

use std::io::{BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

use tuxlink_lib::config;
use tuxlink_lib::winlink::proposal::Answer;
use tuxlink_lib::winlink::{handshake, secure, session, telnet};

// The development CMS, which accepts unregistered client types. The production
// servers (server.winlink.org) reject unknown client SIDs and redirect here.
const HOST: &str = "cms-z.winlink.org";
const PORT: u16 = 8772;

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

    println!("Connecting to {HOST}:{PORT} as {callsign} (telnet, plaintext) ...");
    let stream = TcpStream::connect((HOST, PORT)).expect("tcp connect");
    stream.set_read_timeout(Some(Duration::from_secs(20))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(20))).ok();
    let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
    let mut writer = stream;

    // 1. Clear the telnet "post office" login (callsign + fixed CMSTelnet
    //    password). This uses no station secret.
    telnet::telnet_login(&mut reader, &mut writer, &callsign).expect("telnet login");
    println!("telnet login sent (callsign + fixed CMS password)");

    // 2. Read the real CMS B2F handshake.
    let remote = handshake::read_remote_handshake(&mut reader).expect("read CMS handshake");
    println!("--- handshake parsed ---");
    println!("  SID codes        : {}", remote.sid);
    println!("  forwarders       : {:?}", remote.forwarders);
    println!("  password challenge: {}", remote.challenge.is_some());

    // 2. Try the keyring password.
    let password = keyring::Entry::new("tuxlink-pat", &callsign)
        .ok()
        .and_then(|e| e.get_password().ok())
        .filter(|p| !p.is_empty());

    if remote.challenge.is_some() && password.is_none() {
        println!(
            "\nNo keyring password readable for {callsign}; skipping authentication.\n\
             Handshake PARSE validated against the real CMS. Closing (sent nothing)."
        );
        return;
    }

    let token = remote
        .challenge
        .as_deref()
        .zip(password.as_deref())
        .map(|(challenge, pw)| secure::secure_login_response(challenge, pw));

    // 3. Send our handshake (with the secure-login token if challenged).
    let our_handshake =
        handshake::build_handshake(&callsign, telnet::CMS_TARGET_CALL, &locator, token.as_deref());
    writer.write_all(&our_handshake).expect("send handshake");
    println!("\n--- authenticated; running a transfer-nothing session ---");

    // 4. A bounded set of turns, deferring every inbound proposal so nothing is
    //    downloaded and the CMS keeps the operator's mail; we send no outbound.
    let mut remote_no_messages = false;
    let mut my_turn = true;
    for _ in 0..8 {
        if my_turn {
            let out = session::send_turn(&mut reader, &mut writer, &[], remote_no_messages)
                .expect("send turn");
            println!("  send turn  -> quit_sent={}", out.quit_sent);
            if out.quit_sent {
                break;
            }
        } else {
            let out = session::receive_turn(&mut reader, &mut writer, |props| {
                println!("  CMS offered {} message(s); deferring all", props.len());
                props.iter().map(|_| Answer::Defer).collect()
            })
            .expect("receive turn");
            println!(
                "  recv turn  -> remote_no_messages={} remote_quit={}",
                out.remote_no_messages, out.remote_quit
            );
            remote_no_messages = out.remote_no_messages;
            if out.remote_quit {
                break;
            }
        }
        my_turn = !my_turn;
    }
    println!("\nDone — native client completed a real CMS session. No mail transferred.");
}
