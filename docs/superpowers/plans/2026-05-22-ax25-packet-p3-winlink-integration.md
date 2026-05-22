# AX.25 Packet — P3: Winlink-over-Packet Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the AX.25 connected-mode link (P2's `Ax25Stream`) into tuxlink's native Winlink client so an operator can dial an RMS Packet gateway, dial a peer (P2P), and answer an inbound peer call — reusing the existing B2F session machinery, with one new master-role exchange path for answering.

**Architecture:** Three seams, bottom-up. (1) `session.rs` gains `ExchangeRole { Dial, Answer }`: `Dial` is today's slave behaviour (server speaks first; conditional `;PQ`/`;PR`); `Answer` is master (WE send the handshake first, then the remote/slave takes the first message turn). (2) `winlink_backend.rs` gains `TransportConfig::Packet`, a `resolve_packet_endpoint` sibling to `resolve_cms_endpoint`, and a connect/listen lifecycle that drives P2's `connect`/`answer` over the `Ax25Stream` (mirroring how `telnet.rs` hands its stream to `run_exchange`, including the abort hook). (3) `config.rs` gains an additive `[packet]` section (sticky persisted SSID, last KISS link, AX.25 timing, listen-default), and `ui_commands.rs` gains the four Tauri commands P4 calls. The B2F primitives (handshake/proposal/transfer/lzhuf/secure/message) are reused unchanged; only orchestration role differs.

**Tech Stack:** Rust (`src-tauri` crate). Builds on P1 (`winlink/ax25/` wire codec) + P2 (`winlink/ax25/` datalink + transports). P3 adds no new external crates beyond what P2 introduced (`serialport` lives in P2). `serde` + `serde_json` already present for config; `tauri` already present for commands.

**Authority for protocol behaviour:**
- Spec `docs/design/2026-05-22-ax25-packet-v0.1-design.md` §2 (modes table), §4.4 (role-param + identity split), §4.5 (orchestration/config), §5 (errors).
- Findings `docs/design/ax25-packet-protocol-findings.md` (P2P vs gateway secure-login; FBB master/slave; the answerer is master / sends MOTD+SID first).
- The existing slave-role loop in `src-tauri/src/winlink/session.rs::run_exchange` and the handshake builder in `src-tauri/src/winlink/handshake.rs::build_handshake` / `read_remote_handshake`.

**Run tests with:** `cargo test --manifest-path src-tauri/Cargo.toml <filter>` (absolute manifest path per the worktree path-pinning convention — `bash` cwd can silently revert from the worktree to the main checkout mid-session).

**RADIO-1 boundary:** every test in this plan runs over in-memory streams or a loopback TCP socket. NO live network, NO RF, NO transmission under the station callsign. The connect/answer lifecycle is wired and unit-tested against scripted peers; the operator runs every on-air step.

---

## Shared interface contracts (read before starting)

**Consumed from P2 (`src-tauri/src/winlink/ax25/`) — do NOT redefine; import:**
- `KissLinkConfig` — `enum { Tcp { host: String, port: u16 }, Serial { device: String, baud: u32 } }`.
- `Ax25Params { txdelay, persistence, slot_time, paclen, maxframe, t1: Duration, n2_retries }` — `Default` = 1200-baud values.
- `Address { call: String, ssid: u8 }`.
- `connect(link, mycall: Address, target: Address, digis: &[Address], params) -> io::Result<Ax25Stream>`.
- `answer(link, mycall: Address, params) -> io::Result<(Address, Ax25Stream)>` — returns the inbound peer's `Address` plus the stream.
- `connect_link(&KissLinkConfig) -> io::Result<Box<dyn ByteLink>>`.
- `Ax25Stream: Read + Write` (its `Drop`/`disconnect()` sends DISC).

> If P2 has not landed when this plan executes, Task 1 introduces a **local test double** (`testsupport::FakeAx25Stream` over an in-memory pipe) so P3's session-role and lifecycle logic is testable independently. P3's production code calls the P2 names above; the test double only stands in for `Ax25Stream` in tests. Do NOT reimplement `connect`/`answer` — those are P2's; P3 calls them.

**Defined here (P4 consumes — these names are load-bearing across plans):**
- `pub enum ExchangeRole { Dial, Answer }` in `winlink/session.rs`.
- `TransportConfig::Packet { link: KissLinkConfig, ssid: u8, role: PacketRole }` in `winlink_backend.rs`.
- `pub enum PacketRole { DialTo { call: String, path: Vec<String> }, Listen }` in `winlink_backend.rs`.
- `pub struct PacketConfig { ssid: u8, link: Option<KissLinkConfig>, params: Ax25ParamsConfig, listen_default: bool }` in `config.rs` (the persisted shape; `Ax25ParamsConfig` is the serde-friendly mirror of P2's `Ax25Params`).
- `pub struct PacketConfigDto { ssid, listen_default, link_kind, tcp_host, tcp_port, serial_device, serial_baud, txdelay, persistence, slot_time, paclen, maxframe, t1_ms, n2_retries }` in `ui_commands.rs` (flat, frontend-facing).
- Tauri commands: `packet_connect(call: String, path: Vec<String>)`, `packet_set_listen(enabled: bool)`, `packet_config_get() -> PacketConfigDto`, `packet_config_set(dto: PacketConfigDto)`.

**Identity split (spec §4.4 — verify against `handshake.rs` + wl2k-go during execution):** the AX.25 *link* uses `Address { call: base_call, ssid }` (e.g. `N7CPZ-7`); the B2F `;FW:`/`DE` use the **base** callsign (`N7CPZ` — Winlink accounts have no SSID). So `ExchangeConfig.mycall` stays the base call; the SSID lives only in the link/transport config. Every task that builds an `Address` uses the configured `ssid`; every task that builds an `ExchangeConfig` passes the base `mycall`.

---

### Task 1: `ExchangeRole` enum + `run_exchange` role parameter (Dial = today's behaviour)

**Files:**
- Modify: `src-tauri/src/winlink/session.rs` (add `ExchangeRole`; thread it through `run_exchange`)
- Test: `src-tauri/src/winlink/session.rs` (`#[cfg(test)] mod tests`)

The current `run_exchange` is hard-coded slave/dialer: it reads the remote handshake first, then takes the first message turn (`my_turn = true`). Step 1 adds the role enum and a `role` parameter, with `Dial` preserving the exact current behaviour, so all existing tests pass unchanged after the call sites add `ExchangeRole::Dial`.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `session.rs`:
```rust
#[test]
fn dial_role_preserves_server_speaks_first_behaviour() {
    // Identical to a_session_with_no_traffic_handshakes_then_quits, but via the
    // role-parameterized entry point. Dial = today's slave behaviour.
    let mut server = Vec::new();
    server.extend_from_slice(b"[WL2K-5.0-B2FHM$]\r;PQ: 12345678\rCMS>\r");
    server.extend_from_slice(b"FF\r");
    let mut reader = Cursor::new(server);
    let mut writer = Vec::new();
    let config = ExchangeConfig {
        mycall: "N7CPZ".into(),
        targetcall: "SERVICE".into(),
        locator: "CN87".into(),
        password: Some("MYPASS".into()),
    };
    let result = run_exchange_with_role(
        &mut reader,
        &mut writer,
        ExchangeRole::Dial,
        &config,
        vec![],
        |_| vec![],
    )
    .unwrap();
    assert!(result.received.is_empty() && result.sent.is_empty());

    let token = crate::winlink::secure::secure_login_response("12345678", "MYPASS");
    let mut expected =
        crate::winlink::handshake::build_handshake("N7CPZ", "SERVICE", "CN87", Some(&token));
    expected.extend_from_slice(b"FF\r");
    expected.extend_from_slice(b"FQ\r");
    assert_eq!(writer, expected);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib winlink::session::tests::dial_role_preserves_server_speaks_first_behaviour`
Expected: FAIL to compile — `ExchangeRole` and `run_exchange_with_role` are not defined.

- [ ] **Step 3: Write minimal implementation**

In `session.rs`, add the enum above `run_exchange`:
```rust
/// Which side of the FBB master/slave split this exchange plays.
///
/// `Dial` (slave/dialer): the remote speaks first (sends its handshake +
/// optional `;PQ` challenge); we read it, answer, then take the first message
/// turn. This is the gateway-dial and peer-dial case.
///
/// `Answer` (master/answerer): WE speak first (send our handshake; clients never
/// challenge), the remote reads it and replies, then the *remote* (slave) takes
/// the first message turn. This is the P2P-listen case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExchangeRole {
    Dial,
    Answer,
}
```

Refactor: keep `run_exchange` as a thin back-compat wrapper that forwards to a new role-aware core, so existing call sites and tests (which call `run_exchange`) keep working:
```rust
/// Back-compat entry point: a slave-role (`Dial`) exchange. Existing callers
/// (telnet) and tests use this; new packet callers use [`run_exchange_with_role`].
pub fn run_exchange<R, W, F>(
    reader: &mut R,
    writer: &mut W,
    config: &ExchangeConfig,
    outbound: Vec<OutboundMessage>,
    decide: F,
) -> Result<ExchangeResult, ExchangeError>
where
    R: BufRead,
    W: Write,
    F: Fn(&[Proposal]) -> Vec<Answer>,
{
    run_exchange_with_role(reader, writer, ExchangeRole::Dial, config, outbound, decide)
}
```

Add the role-aware core. Only the pre-loop handshake half and the initial `my_turn` differ by role; the turn loop is shared verbatim:
```rust
/// Run a full exchange in the given [`ExchangeRole`]. See the enum docs for the
/// role split. The turn loop after the handshake is identical for both roles.
pub fn run_exchange_with_role<R, W, F>(
    reader: &mut R,
    writer: &mut W,
    role: ExchangeRole,
    config: &ExchangeConfig,
    outbound: Vec<OutboundMessage>,
    decide: F,
) -> Result<ExchangeResult, ExchangeError>
where
    R: BufRead,
    W: Write,
    F: Fn(&[Proposal]) -> Vec<Answer>,
{
    let my_turn = match role {
        ExchangeRole::Dial => {
            // Slave: the remote speaks first; answer its challenge if present.
            let remote =
                handshake::read_remote_handshake(reader).map_err(ExchangeError::Handshake)?;
            let token = match (&remote.challenge, &config.password) {
                (Some(challenge), Some(password)) => {
                    Some(secure::secure_login_response(challenge, password))
                }
                (Some(_), None) => return Err(ExchangeError::PasswordRequired),
                (None, _) => None,
            };
            let our_handshake = handshake::build_handshake(
                &config.mycall,
                &config.targetcall,
                &config.locator,
                token.as_deref(),
            );
            write_bytes(writer, &our_handshake)?;
            true // the dialer/slave takes the first message turn
        }
        ExchangeRole::Answer => {
            // Master: WE speak first; clients never challenge, so no `;PR:` token.
            let our_handshake =
                handshake::build_handshake(&config.mycall, &config.targetcall, &config.locator, None);
            write_bytes(writer, &our_handshake)?;
            // Read the remote (slave) handshake; a peer never challenges us, so a
            // PasswordRequired here would be a misbehaving peer — treat any
            // challenge as ignorable (we send no token). We still parse it to
            // consume the bytes up to the prompt.
            let _remote =
                handshake::read_remote_handshake(reader).map_err(ExchangeError::Handshake)?;
            false // the remote/slave takes the first message turn
        }
    };

    let mut result = ExchangeResult::default();
    let mut remaining = outbound;
    let mut remote_no_messages = false;
    let mut my_turn = my_turn;
    let mut turns = 0u32;

    loop {
        turns += 1;
        if turns > MAX_TURNS {
            return Err(ExchangeError::TooManyTurns);
        }
        if my_turn {
            let outcome = send_turn(reader, writer, &remaining, remote_no_messages)?;
            result.sent.extend(outcome.sent);
            result.rejected.extend(outcome.rejected);
            result.deferred.extend(outcome.deferred);
            remaining.clear();
            if outcome.quit_sent {
                break;
            }
        } else {
            let outcome = receive_turn(reader, writer, &decide)?;
            result.received.extend(outcome.messages);
            remote_no_messages = outcome.remote_no_messages;
            if outcome.remote_quit {
                break;
            }
        }
        my_turn = !my_turn;
    }
    Ok(result)
}
```
Delete the now-duplicated body from the old `run_exchange` (it forwards now). Leave `send_turn`/`receive_turn`/`ExchangeError`/`ExchangeConfig` unchanged.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib winlink::session::`
Expected: PASS — the new test plus ALL existing `session::tests` (they call `run_exchange`, which now forwards to `Dial`).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/session.rs
git commit -m "feat(ax25): add ExchangeRole to run_exchange; Dial preserves slave behaviour (tuxlink-7fr)

Dial = today's server-speaks-first dialer; the new role-aware core lets the
Answer (master) path land next. run_exchange is now a thin Dial wrapper so all
existing telnet/session tests pass unchanged.

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: `Answer` (master) role — we send the handshake first, remote proposes first

**Files:**
- Modify: `src-tauri/src/winlink/session.rs` (no production change if Task 1's `Answer` arm is complete — this task adds the master-role test that locks the behaviour)
- Test: `src-tauri/src/winlink/session.rs` (`#[cfg(test)] mod tests`)

The `Answer` arm was written in Task 1; this task proves it against a scripted **slave peer** over in-memory streams. The peer reads our handshake first, then (as slave) takes the first message turn — offering one message, then quitting.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `session.rs`:
```rust
#[test]
fn answer_role_sends_handshake_first_then_remote_takes_first_turn() {
    // We are master. The scripted peer is slave: it does NOT speak a handshake
    // first (we do). It replies with its own handshake (no `>` prompt needed — we
    // read until a prompt, so the peer ends its handshake with one), then, on its
    // turn, offers one message and quits.
    let mut peer = Vec::new();
    // The peer's handshake reply: a forwarding line, an identifier, and a prompt.
    peer.extend_from_slice(b";FW: W7AUX\r[RMS-1.0-B2FHM$]\rW7AUX>\r");
    // The peer (slave) takes the first message turn: one offered message.
    let mut msg = Message::new();
    msg.set_header("Mid", "PEERMSG00001");
    msg.set_header("Subject", "Hi");
    msg.set_header("From", "W7AUX");
    msg.set_body(b"Direct peer message.\r\n".to_vec());
    let (proposal, compressed) = msg.to_proposal().unwrap();
    peer.extend_from_slice(proposal.line().as_bytes());
    peer.push(b'\r');
    peer.extend_from_slice(batch_checksum_line(&[proposal]).as_bytes());
    peer.push(b'\r');
    peer.extend_from_slice(&transfer::frame_block("Hi", 0, &compressed));
    // After our accept + our (empty) turn, the peer is done.
    peer.extend_from_slice(b"FQ\r");

    let mut reader = Cursor::new(peer);
    let mut writer = Vec::new();
    let config = ExchangeConfig {
        mycall: "N7CPZ".into(), // base call — NO ssid in the B2F identity
        targetcall: "W7AUX".into(),
        locator: "CN87".into(),
        password: None, // peers never challenge; no secret in P2P
    };
    let result = run_exchange_with_role(
        &mut reader,
        &mut writer,
        ExchangeRole::Answer,
        &config,
        vec![],
        |_| vec![Answer::Accept { resume_offset: 0 }],
    )
    .unwrap();

    // We received the peer's message.
    assert_eq!(result.received.len(), 1);
    assert_eq!(result.received[0].header("Mid"), Some("PEERMSG00001"));
    assert_eq!(result.received[0].body(), b"Direct peer message.\r\n");

    // We spoke the handshake FIRST (no `;PR:` token — no challenge in P2P), then
    // accepted (`FS +`), then on our turn signalled no-more (FF) → quit (FQ).
    let our_handshake =
        crate::winlink::handshake::build_handshake("N7CPZ", "W7AUX", "CN87", None);
    assert!(
        writer.starts_with(&our_handshake),
        "master must send its handshake before anything else; wrote {:?}",
        String::from_utf8_lossy(&writer)
    );
    let tail = &writer[our_handshake.len()..];
    assert_eq!(tail, b"FS +\rFFFQ\r".replace_then_keep()); // see note below
}
```
> NOTE on the tail assertion: after the handshake we accept the peer's batch (`FS +\r`), then it's our turn with nothing to send and the remote not-yet-done → `FF\r`; the peer then quits (`FQ\r` is inbound, not written by us), and on the next of-our turns we send `FQ\r`. Concretely our writes after the handshake are `FS +\r` then `FF\r` then `FQ\r`. Replace the pseudo-line with: `assert_eq!(tail, b"FS +\rFF\rFQ\r");`

Fix the assertion to the concrete bytes:
```rust
    let tail = &writer[our_handshake.len()..];
    assert_eq!(tail, b"FS +\rFF\rFQ\r");
```

- [ ] **Step 2: Run test to verify it fails (or passes if Task 1 was complete)**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib winlink::session::tests::answer_role_sends_handshake_first_then_remote_takes_first_turn`
Expected: PASS if Task 1's `Answer` arm is correct. If it FAILS on byte order, the most likely cause is the master taking the first turn — confirm `Answer` sets `my_turn = false`. (This task is the proof; if it fails, the fix is in the Task-1 `Answer` arm, not new production code.)

- [ ] **Step 3: Adjust the `Answer` arm only if the test reveals a turn-order or handshake-order defect**

If the test fails because we read the peer handshake before sending ours, reorder so `build_handshake(...)` + `write_bytes` happen FIRST, then `read_remote_handshake`. (Per findings: the answerer/master sends MOTD+SID first.) No other production change.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib winlink::session::`
Expected: PASS (the new master-role test + all prior tests).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/session.rs
git commit -m "test(ax25): lock Answer (master) role — handshake-first, remote-proposes-first (tuxlink-7fr)

Master sends its handshake before the peer, sends no secure-login token (P2P
peers never challenge), and yields the first message turn to the slave peer.
Scripted-peer test over in-memory streams; no network, no RF.

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: `[packet]` config section — `PacketConfig` (additive, default-on listen, sticky SSID)

**Files:**
- Modify: `src-tauri/src/config.rs` (add `PacketConfig` + `Ax25ParamsConfig`; add an optional `packet` field to `Config`)
- Test: `src-tauri/src/config.rs` (`#[cfg(test)] mod tests`)

`Config` is `#[serde(deny_unknown_fields)]`; that rejects EXTRA keys, not MISSING ones, so a new field with `#[serde(default)]` is safe for old files. **Do NOT bump `CONFIG_SCHEMA_VERSION` for this field** — it is purely additive (mirrors how 686 added `position_source` with `#[serde(default)]`; 686 bumps the version for its own field, but P3's field defaults cleanly without a bump and the schema-version guard in `read_config`/`write_config_atomic` only compares equality, so adding a `default`ed field does not require a bump). If 686 has already bumped the version when this lands, that bump stands; P3's field still uses `#[serde(default)]` and needs no further bump. See **## Coordination**.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `config.rs`:
```rust
fn sample_config_json_without_packet() -> String {
    // A v1-shaped config with NO `packet` key — proves the field defaults.
    serde_json::json!({
        "schema_version": CONFIG_SCHEMA_VERSION,
        "wizard_completed": true,
        "connect": { "connect_to_cms": false, "transport": "Telnet" },
        "identity": { "callsign": null, "identifier": "FIELD-1", "grid": "CN87" },
        "privacy": { "gps_state": "Off", "position_precision": "FourCharGrid" },
        "pat_mbo_address": null
    })
    .to_string()
}

#[test]
fn config_defaults_packet_section_when_absent() {
    let json = sample_config_json_without_packet();
    let cfg: Config = serde_json::from_str(&json).unwrap();
    let packet = cfg.packet;
    assert_eq!(packet.ssid, 0, "SSID defaults to 0");
    assert!(packet.listen_default, "listen is default-on (spec §4.5)");
    assert!(packet.link.is_none(), "no last KISS link until the operator sets one");
}

#[test]
fn packet_config_round_trips_with_sticky_ssid_and_link() {
    // Persist an SSID + a TCP KISS link + tuned params, reload, assert sticky.
    let mut cfg: Config = serde_json::from_str(&sample_config_json_without_packet()).unwrap();
    cfg.packet = PacketConfig {
        ssid: 7,
        link: Some(crate::winlink::ax25::KissLinkConfig::Tcp {
            host: "127.0.0.1".into(),
            port: 8001,
        }),
        params: Ax25ParamsConfig { paclen: 128, maxframe: 4, ..Default::default() },
        listen_default: false,
    };
    let serialized = serde_json::to_string(&cfg).unwrap();
    let reloaded: Config = serde_json::from_str(&serialized).unwrap();
    assert_eq!(reloaded.packet.ssid, 7);
    assert!(!reloaded.packet.listen_default);
    assert_eq!(reloaded.packet.params.paclen, 128);
    match reloaded.packet.link {
        Some(crate::winlink::ax25::KissLinkConfig::Tcp { host, port }) => {
            assert_eq!(host, "127.0.0.1");
            assert_eq!(port, 8001);
        }
        other => panic!("expected a TCP KISS link, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib config::tests::config_defaults_packet_section_when_absent`
Expected: FAIL to compile — `Config.packet`, `PacketConfig`, `Ax25ParamsConfig` are not defined.

- [ ] **Step 3: Write minimal implementation**

In `config.rs`, add (note `KissLinkConfig` is P2's; import it):
```rust
use crate::winlink::ax25::KissLinkConfig;

/// Serde-friendly mirror of P2's `winlink::ax25::Ax25Params` (which carries a
/// `Duration` that does not round-trip JSON cleanly). Persisted form stores the
/// T1 timer as milliseconds; `into_params()` converts to the runtime type.
/// Defaults are the 1200-baud values (match `Ax25Params::default`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Ax25ParamsConfig {
    pub txdelay: u8,
    pub persistence: u8,
    pub slot_time: u8,
    pub paclen: u16,
    pub maxframe: u8,
    pub t1_ms: u64,
    pub n2_retries: u8,
}

impl Default for Ax25ParamsConfig {
    fn default() -> Self {
        // 1200-baud defaults; cross-check P2's Ax25Params::default during execution.
        Ax25ParamsConfig {
            txdelay: 30,
            persistence: 63,
            slot_time: 10,
            paclen: 128,
            maxframe: 4,
            t1_ms: 3000,
            n2_retries: 10,
        }
    }
}

/// The `[packet]` config section (spec §4.5): the AX.25 packet transport's
/// sticky, persisted settings. Global station SSID is sticky across restarts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PacketConfig {
    /// Global, sticky station SSID (0–15). Operate as `<callsign>-<ssid>`.
    pub ssid: u8,
    /// The last KISS link the operator used (TCP host:port or serial device+baud).
    /// `None` until the operator configures one.
    pub link: Option<KissLinkConfig>,
    /// AX.25 timing/windowing knobs (1200-baud defaults).
    pub params: Ax25ParamsConfig,
    /// Idle-listening default-on (spec §4.5): arm `answer()` when not dialing.
    pub listen_default: bool,
}

impl Default for PacketConfig {
    fn default() -> Self {
        PacketConfig {
            ssid: 0,
            link: None,
            params: Ax25ParamsConfig::default(),
            listen_default: true, // spec §4.5: listen is default-on
        }
    }
}
```
Add the field to `Config` (after `pat_mbo_address`):
```rust
    /// AX.25 packet transport settings (additive; defaults when absent). See
    /// `PacketConfig`. `#[serde(default)]` is the migration for old files.
    #[serde(default)]
    pub packet: PacketConfig,
```
Update every `Config { .. }` struct literal in NON-test code to include `packet: PacketConfig::default()` (search: `grep -n "Config {" src-tauri/src/`). In test helpers (`config.rs` tests, `winlink_backend.rs` `sample_config`, `ui_commands.rs` test config builders) add `packet: PacketConfig::default()` to each literal.

> Validate `ssid <= 15` in `Config::validate` (add an arm + a `ConfigValidationError::PacketSsidOutOfRange { ssid: u8 }` variant). Add a unit test `packet_ssid_above_15_is_rejected`. This keeps the persisted SSID in the 4-bit AX.25 range.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib config::`
Expected: PASS (both new tests + all existing config tests; the SSID-range test if added).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/config.rs
git commit -m "feat(config): additive [packet] section — sticky SSID, KISS link, AX.25 params (tuxlink-7fr)

PacketConfig + Ax25ParamsConfig added to Config behind #[serde(default)] (no
schema bump — purely additive; deny_unknown_fields only rejects extra keys).
listen_default = true per spec §4.5; SSID is global+sticky and range-validated.

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: `TransportConfig::Packet` + `PacketRole` + `resolve_packet_endpoint`

**Files:**
- Modify: `src-tauri/src/winlink_backend.rs` (add the `Packet` variant, `PacketRole`, and the resolver)
- Test: `src-tauri/src/winlink_backend.rs` (`#[cfg(test)] mod tests`)

`TransportConfig` is already `#[non_exhaustive]` — adding a variant is in-crate and the only forced match update is the in-crate `connect` and the `transport`-string formatter (the `UiError` mapping does not match `TransportConfig`). `resolve_packet_endpoint` is the role/identity-resolution sibling of `resolve_cms_endpoint`: it turns the config + a `PacketRole` into the concrete `(Address mycall_link, base_mycall, ExchangeRole, Option<dial-target+digis>)` the lifecycle needs.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `winlink_backend.rs`:
```rust
#[test]
fn resolve_packet_endpoint_dial_builds_ssidd_link_addr_and_base_b2f_call() {
    // Identity split (spec §4.4): the AX.25 link addr carries the SSID; the B2F
    // identity is the BASE call. Dial role → ExchangeRole::Dial + a target.
    let resolved = resolve_packet_endpoint(
        "N7CPZ",
        7,
        PacketRole::DialTo { call: "W7AUX".into(), path: vec!["RELAY-1".into()] },
    )
    .unwrap();
    assert_eq!(resolved.link_mycall, Address { call: "N7CPZ".into(), ssid: 7 });
    assert_eq!(resolved.base_mycall, "N7CPZ");
    assert_eq!(resolved.role, crate::winlink::session::ExchangeRole::Dial);
    let (target, digis) = resolved.dial.unwrap();
    assert_eq!(target, Address { call: "W7AUX".into(), ssid: 0 });
    assert_eq!(digis, vec![Address { call: "RELAY".into(), ssid: 1 }]);
}

#[test]
fn resolve_packet_endpoint_listen_yields_answer_role_and_no_target() {
    let resolved = resolve_packet_endpoint("N7CPZ", 7, PacketRole::Listen).unwrap();
    assert_eq!(resolved.link_mycall, Address { call: "N7CPZ".into(), ssid: 7 });
    assert_eq!(resolved.base_mycall, "N7CPZ");
    assert_eq!(resolved.role, crate::winlink::session::ExchangeRole::Answer);
    assert!(resolved.dial.is_none());
}

#[test]
fn resolve_packet_endpoint_rejects_more_than_two_digipeaters() {
    let err = resolve_packet_endpoint(
        "N7CPZ",
        0,
        PacketRole::DialTo {
            call: "W7AUX".into(),
            path: vec!["A-1".into(), "B-2".into(), "C-3".into()],
        },
    )
    .unwrap_err();
    assert!(matches!(err, BackendError::NotConfigured(_)));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib winlink_backend::tests::resolve_packet_endpoint`
Expected: FAIL to compile — `TransportConfig::Packet`, `PacketRole`, `resolve_packet_endpoint`, `ResolvedPacket` are not defined.

- [ ] **Step 3: Write minimal implementation**

In `winlink_backend.rs`, extend the enum and add the role + resolver. Import P2's `Address` / `KissLinkConfig`:
```rust
use crate::winlink::ax25::{Address, KissLinkConfig};
use crate::winlink::session::ExchangeRole;
```
Add the variant to `TransportConfig`:
```rust
    /// AX.25 1200-baud packet over a KISS link (TCP / serial). The SSID rides
    /// the AX.25 *link* address; the B2F identity uses the base call (spec §4.4).
    Packet {
        link: KissLinkConfig,
        ssid: u8,
        role: PacketRole,
    },
```
Add the role enum:
```rust
/// What a packet connection does. `DialTo` is the operator pressing "Connect to"
/// (gateway OR peer — tuxlink reacts to the challenge, not a mode flag); `Listen`
/// is the idle armed-to-answer state (spec §2, §4.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacketRole {
    DialTo { call: String, path: Vec<String> },
    Listen,
}
```
Add the resolved-shape struct + resolver (the identity split lives here):
```rust
/// What a `PacketRole` + identity resolves into for the lifecycle: the SSID'd
/// link address, the base B2F call, the exchange role, and (for a dial) the
/// target + digipeater addresses. Mirrors `resolve_cms_endpoint`'s "config →
/// concrete endpoint" job for the packet transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPacket {
    pub link_mycall: Address,
    pub base_mycall: String,
    pub role: ExchangeRole,
    /// `Some((target, digis))` for a dial; `None` for listen.
    pub dial: Option<(Address, Vec<Address>)>,
}

/// Parse a `CALL` or `CALL-SSID` string into an [`Address`]. A bare call has
/// SSID 0. Rejects an SSID outside 0–15 or a malformed token.
fn parse_call_ssid(s: &str) -> Result<Address, BackendError> {
    let (call, ssid) = match s.rsplit_once('-') {
        Some((c, s)) => {
            let n: u8 = s
                .parse()
                .map_err(|_| BackendError::NotConfigured(format!("bad SSID in '{s}'")))?;
            (c, n)
        }
        None => (s, 0),
    };
    if ssid > 15 || call.is_empty() {
        return Err(BackendError::NotConfigured(format!("bad call/ssid '{s}'")));
    }
    Ok(Address { call: call.to_uppercase(), ssid })
}

/// Resolve identity + role into the concrete addresses + exchange role. Enforces
/// the 0–2 digipeater cap (spec §1) and the identity split (spec §4.4).
fn resolve_packet_endpoint(
    base_mycall: &str,
    ssid: u8,
    role: PacketRole,
) -> Result<ResolvedPacket, BackendError> {
    let base = base_mycall.trim().to_uppercase();
    let link_mycall = Address { call: base.clone(), ssid };
    match role {
        PacketRole::Listen => Ok(ResolvedPacket {
            link_mycall,
            base_mycall: base,
            role: ExchangeRole::Answer,
            dial: None,
        }),
        PacketRole::DialTo { call, path } => {
            if path.len() > 2 {
                return Err(BackendError::NotConfigured(format!(
                    "at most 2 digipeaters allowed (got {})",
                    path.len()
                )));
            }
            let target = parse_call_ssid(&call)?;
            let digis = path
                .iter()
                .map(|p| parse_call_ssid(p))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ResolvedPacket {
                link_mycall,
                base_mycall: base,
                role: ExchangeRole::Dial,
                dial: Some((target, digis)),
            })
        }
    }
}
```
Update the in-crate forced matches: in `NativeBackend::connect` the current `let TransportConfig::Cms { mode } = transport;` irrefutable binding becomes a `match` (Task 5 handles the `Packet` arm body; for now add a `Packet { .. } => return Err(BackendError::NotImplemented)` placeholder so the crate compiles). In the `PatBackend::connect` `transport` formatter (`match transport { TransportConfig::Cms { mode } => ... }`, ~line 1152) add `TransportConfig::Packet { ssid, .. } => format!("Packet-{ssid}")`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib winlink_backend::tests::resolve_packet_endpoint`
Expected: PASS (all three resolver tests).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink_backend.rs
git commit -m "feat(ax25): TransportConfig::Packet + PacketRole + resolve_packet_endpoint (tuxlink-7fr)

resolve_packet_endpoint mirrors resolve_cms_endpoint: config+role → SSID'd link
Address + base B2F call + ExchangeRole + dial target/digis. Enforces the 0-2
digipeater cap and the spec §4.4 identity split (SSID on the link, base on B2F).

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: `native_packet_exchange` — drive `run_exchange_with_role` over an `Ax25Stream`

**Files:**
- Modify: `src-tauri/src/winlink_backend.rs` (add `native_packet_exchange`; wire the `Packet` arm of `connect`)
- Create: `src-tauri/src/winlink_backend.rs` test double `FakeAx25Stream` in the test module (in-memory `Read + Write`)
- Test: `src-tauri/src/winlink_backend.rs` (`#[cfg(test)] mod tests`)

This is the seam P2's `Ax25Stream` plugs into — exactly as `telnet.rs::connect_and_exchange` hands its read/write halves to `run_exchange`. `native_packet_exchange` is generic over `Read + Write` so it is testable with an in-memory stream without P2's real link. It builds the `ExchangeConfig` with the **base** call (identity split), wraps the stream in a `BufReader` for the read half, and runs `run_exchange_with_role` with the resolved `ExchangeRole`. The mailbox file/move logic mirrors `native_connect` (file received → Inbox, move sent → Sent).

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `winlink_backend.rs`:
```rust
/// An in-memory bidirectional stream: reads come from `inbound`, writes collect
/// into `outbound`. Stands in for P2's `Ax25Stream` (which is `Read + Write`).
struct FakeAx25Stream {
    inbound: std::io::Cursor<Vec<u8>>,
    outbound: Vec<u8>,
}
impl std::io::Read for FakeAx25Stream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inbound.read(buf)
    }
}
impl std::io::Write for FakeAx25Stream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.outbound.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn native_packet_exchange_dials_a_gateway_with_secure_login() {
    use crate::winlink::secure::secure_login_response;
    // A scripted gateway: speaks first, challenges, then quits (empty mailbox).
    let mut server = Vec::new();
    server.extend_from_slice(b"[WL2K-5.0-B2FHM$]\r;PQ: 12345678\rCMS>\r");
    server.extend_from_slice(b"FF\r");
    let stream = FakeAx25Stream { inbound: std::io::Cursor::new(server), outbound: Vec::new() };

    let mailbox = Mailbox::new(tempdir().unwrap().path());
    let result = native_packet_exchange(
        stream,
        "N7CPZ",                 // base B2F call (NO ssid)
        "W7AUX",                 // target call (gateway)
        Some("MYPASS".into()),   // gateway password from keyring
        ExchangeRole::Dial,
        &mailbox,
        &|_| {},
    );
    let stream = result.unwrap();

    // We answered the challenge (Dial role: server speaks first), then FF, then FQ.
    let token = secure_login_response("12345678", "MYPASS");
    let mut expected =
        crate::winlink::handshake::build_handshake("N7CPZ", "W7AUX", "CN87", Some(&token));
    // NOTE: locator depends on the mailbox-free path; native_packet_exchange takes
    // the locator as a param (see impl). Use "" here and assert the prefix instead:
    let _ = expected;
    assert!(
        stream.outbound.windows(token.len()).any(|w| w == token.as_bytes()),
        "the secure-login token must appear in our handshake"
    );
}
```
> NOTE: `native_packet_exchange` takes the `locator` as an explicit param (resolved from config by the caller, like `cms_locator`), NOT from the mailbox. The test above asserts the token appears in the written bytes (role-correct behaviour) without pinning the exact handshake locator. A second test (`native_packet_exchange_answers_a_peer_without_a_challenge`) drives `ExchangeRole::Answer` with a scripted slave peer (reuse the Task-2 peer script) and asserts the received message lands.

Add the Answer-role test:
```rust
#[tokio::test]
async fn native_packet_exchange_answers_a_peer_and_receives_a_message() {
    let mut peer = Vec::new();
    peer.extend_from_slice(b";FW: W7AUX\r[RMS-1.0-B2FHM$]\rW7AUX>\r");
    let mut msg = Message::new();
    msg.set_header("Mid", "PEERMSG00009");
    msg.set_header("Subject", "P2P");
    msg.set_body(b"hello from the field\r\n".to_vec());
    let (proposal, compressed) = msg.to_proposal().unwrap();
    peer.extend_from_slice(proposal.line().as_bytes());
    peer.push(b'\r');
    peer.extend_from_slice(crate::winlink::proposal::batch_checksum_line(&[proposal]).as_bytes());
    peer.push(b'\r');
    peer.extend_from_slice(&crate::winlink::transfer::frame_block("P2P", 0, &compressed));
    peer.extend_from_slice(b"FQ\r");
    let stream = FakeAx25Stream { inbound: std::io::Cursor::new(peer), outbound: Vec::new() };

    let dir = tempdir().unwrap();
    let mailbox = Mailbox::new(dir.path());
    native_packet_exchange(
        stream, "N7CPZ", "W7AUX", None, ExchangeRole::Answer, &mailbox, &|_| {},
    )
    .unwrap();

    // The received peer message was filed into the inbox.
    let inbox = mailbox.list(MailboxFolder::Inbox).unwrap();
    assert!(inbox.iter().any(|m| m.id.0 == "PEERMSG00009"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib winlink_backend::tests::native_packet_exchange`
Expected: FAIL to compile — `native_packet_exchange` is not defined.

- [ ] **Step 3: Write minimal implementation**

In `winlink_backend.rs`, add the generic exchange driver (the `Ax25Stream` seam). Note it returns the stream so the caller (or test) can inspect / `Drop` it (Drop sends DISC in P2):
```rust
/// Run one B2F exchange over an already-connected AX.25 stream (the P2
/// `Ax25Stream`, or any `Read + Write` in tests). Mirrors `native_connect`'s
/// mailbox handling but the transport is a packet link, not telnet, and the role
/// may be `Dial` (gateway/peer) or `Answer` (P2P listen). The B2F identity uses
/// the BASE call (spec §4.4); the SSID rode the AX.25 link addr in the connect/
/// answer call that produced `stream`.
fn native_packet_exchange<S: Read + Write>(
    stream: S,
    base_mycall: &str,
    targetcall: &str,
    password: Option<String>,
    role: ExchangeRole,
    mailbox: &Mailbox,
    progress: &dyn Fn(&str),
) -> Result<S, BackendError> {
    // Split the single stream into a buffered read half and a write half. The
    // exchange is strictly turn-based, so a shared Arc<Mutex<>> (telnet's pattern)
    // works; for an owned single stream we use the std trick of a BufReader over a
    // &mut and a separate &mut writer is not possible with one owner, so we wrap in
    // the same Shared pattern telnet uses.
    use std::sync::{Arc, Mutex};
    trait RW: Read + Write + Send {}
    impl<T: Read + Write + Send> RW for T {}
    let shared: Arc<Mutex<Box<dyn RW>>> = Arc::new(Mutex::new(Box::new(stream)));
    struct ReadHalf(Arc<Mutex<Box<dyn RW>>>);
    struct WriteHalf(Arc<Mutex<Box<dyn RW>>>);
    impl Read for ReadHalf {
        fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
            self.0.lock().expect("ax25 lock").read(b)
        }
    }
    impl Write for WriteHalf {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("ax25 lock").write(b)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.0.lock().expect("ax25 lock").flush()
        }
    }
    let mut reader = std::io::BufReader::new(ReadHalf(shared.clone()));
    let mut writer = WriteHalf(shared.clone());

    // Build the outbox into proposals (same as native_connect).
    let mut outbound = Vec::new();
    for meta in mailbox.list(MailboxFolder::Outbox)? {
        let body = mailbox.read(MailboxFolder::Outbox, &meta.id)?;
        if let Ok(message) = Message::from_bytes(&body.raw_rfc5322) {
            if let Some((proposal, compressed)) = message.to_proposal() {
                let title = message.header("Subject").unwrap_or_default().to_string();
                outbound.push(session::OutboundMessage { proposal, title, compressed });
            }
        }
    }

    let exchange_config = session::ExchangeConfig {
        mycall: base_mycall.to_string(), // BASE call — no SSID in B2F identity
        targetcall: targetcall.to_string(),
        locator: String::new(), // packet locator surface is TBD; empty is valid B2F
        password,
    };

    progress("AX.25 connected; negotiating messages…");
    let result = session::run_exchange_with_role(
        &mut reader,
        &mut writer,
        role,
        &exchange_config,
        outbound,
        |proposals| proposals.iter().map(|_| Answer::Accept { resume_offset: 0 }).collect(),
    )
    .map_err(|e| BackendError::TransportFailed { reason: format!("{e:?}"), source: None })?;

    for message in &result.received {
        mailbox.store(MailboxFolder::Inbox, &message.to_bytes())?;
    }
    for mid in &result.sent {
        mailbox.move_to(MailboxFolder::Outbox, MailboxFolder::Sent, &MessageId(mid.clone()))?;
    }

    // Reclaim the owned stream so the caller controls Drop (DISC) timing.
    drop(reader);
    drop(writer);
    let stream = Arc::try_unwrap(shared)
        .map(|m| m.into_inner().expect("ax25 lock"))
        .map_err(|_| BackendError::Internal {
            msg: "ax25 stream still shared after exchange".into(),
            source: None,
        })?;
    Ok(*stream_box_downcast(stream))
}
```
> The `stream_box_downcast` reclaim is awkward with a `Box<dyn RW>`. SIMPLER alternative the implementer should prefer: make `native_packet_exchange` NOT return `S` and instead take `&mut S` (the caller owns the stream, splits it itself, and lets it Drop). Concretely, change the signature to `fn native_packet_exchange<S: Read + Write>(stream: &mut S, ...) -> Result<(), BackendError>` and split via a `&mut`-borrowing reader/writer pair (a `BufReader<&mut S>` for reads is not possible while also writing to `&mut S`; use the telnet `Shared` pattern only when you own the stream). Cleanest: pass the stream by value and DROP it inside the fn (DISC fires on return); return `Result<(), BackendError>`. Adopt the by-value-drop form and adjust the tests to not bind the returned stream:
```rust
fn native_packet_exchange<S: Read + Write>(
    stream: S,
    base_mycall: &str,
    targetcall: &str,
    password: Option<String>,
    role: ExchangeRole,
    mailbox: &Mailbox,
    progress: &dyn Fn(&str),
) -> Result<(), BackendError> {
    // ... same Shared/ReadHalf/WriteHalf split, run_exchange_with_role, mailbox
    // filing ... then just let `shared` drop at end of scope (DISC fires).
    Ok(())
}
```
Update the two tests above: drop the `let stream = result.unwrap();` reclaim; for the gateway test, capture the written bytes by wrapping `FakeAx25Stream` in an `Arc<Mutex<Vec<u8>>>`-backed sink the test still holds a clone of, OR assert via the mailbox (the gateway test has an empty mailbox → assert `result.is_ok()` and that no inbox message was filed). The token-presence assertion needs access to the written bytes; the cleanest is to have `FakeAx25Stream` write into an `Arc<Mutex<Vec<u8>>>` the test retains:
```rust
struct FakeAx25Stream {
    inbound: std::io::Cursor<Vec<u8>>,
    outbound: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
}
// Write pushes into the shared Vec; the test keeps a clone of the Arc to inspect.
```
Then wire the `Packet` arm of `NativeBackend::connect` (replace the Task-4 `NotImplemented` placeholder). It: reads the base callsign from config, resolves the endpoint, connects (`ax25::connect`) or arms (`ax25::answer`) the link via P2, runs `native_packet_exchange`, and maps the status. Per RADIO-1 this code is WRITTEN here but never RUN by the agent against a real link — the tests exercise `native_packet_exchange` with `FakeAx25Stream` only:
```rust
let TransportConfig::Packet { link, ssid, role } = transport else {
    // existing Cms arm continues here
    ...
};
// resolve identity + role
let base = self.config.identity.callsign.clone()
    .ok_or_else(|| BackendError::NotConfigured("identity.callsign".into()))?;
let resolved = resolve_packet_endpoint(&base, ssid, role)?;
// password only for a dial that turns out to be a gateway (challenge handled in
// run_exchange); P2P/answer pass None. Look up the keyring entry like native_connect.
```
> Connecting/answering over the real KISS link (`ax25::connect`/`ax25::answer`) plus the listen-lifecycle background loop is Task 6. Task 5's production scope is `native_packet_exchange` + the `connect` arm that calls `resolve_packet_endpoint` and `native_packet_exchange`; the actual `ax25::connect`/`answer` call is added in Task 6 (kept separate so Task 5's logic is fully unit-testable with `FakeAx25Stream`).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib winlink_backend::tests::native_packet_exchange`
Expected: PASS (both dial-with-secure-login and answer-receives-message tests).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink_backend.rs
git commit -m "feat(ax25): native_packet_exchange drives B2F over an Ax25Stream (tuxlink-7fr)

Mirrors native_connect's mailbox handling over a packet link instead of telnet,
parameterized by ExchangeRole (Dial gateway/peer, Answer P2P). Generic over
Read+Write so it is fully unit-tested with an in-memory FakeAx25Stream — no
network, no RF. Identity split: B2F uses the base call; SSID rode the link addr.

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: connect/listen lifecycle — `ax25::connect`/`answer` + abort hook (no-RF wiring)

**Files:**
- Modify: `src-tauri/src/winlink_backend.rs` (the `Packet` arm of `connect`: call P2's `connect_link` + `connect`/`answer`; register the link's shutdown hook like telnet's `register_socket`)
- Test: `src-tauri/src/winlink_backend.rs` (`#[cfg(test)] mod tests`) — status transitions + the dial-vs-listen branch selection, NOT a live link

This task wires the real P2 calls and the abort/lifecycle. Per the spec §4.5 lifecycle: a `DialTo` role → `ax25::connect(link, link_mycall, target, &digis, params)` then `native_packet_exchange(stream, ..., ExchangeRole::Dial, ...)`; a `Listen` role → `ax25::answer(link, link_mycall, params)` (blocks for an inbound SABM) then `native_packet_exchange(stream, peer, ..., ExchangeRole::Answer, ...)`. Both run on `spawn_blocking` like `native_connect`. The link's shutdown hook (P2's `ByteLink` exposes a clonable shutdown handle analogous to `TcpStream::try_clone` + `shutdown`) registers into `abort_handle` so an operator abort unblocks a hung connect/answer — mirroring telnet's `register_socket` (spec §5 "Abort", §4.1).

> RADIO-1: this arm calls `ax25::connect`/`answer`, which key the link. The AGENT MUST NOT run any test that reaches a real KISS modem. The tests here assert the **branch selection + status transitions** using a config-only path (no link). The live connect/answer is operator-run.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `winlink_backend.rs`:
```rust
#[tokio::test]
async fn connect_packet_with_no_link_configured_is_not_configured_error() {
    // A NativeBackend whose config has no packet.link set → a clean
    // NotConfigured, never a panic or a hang (we never reach a real modem).
    let backend = NativeBackend::new(offline_config_with_callsign(), tempdir().unwrap().path());
    let err = backend
        .connect(TransportConfig::Packet {
            link: KissLinkConfig::Tcp { host: "127.0.0.1".into(), port: 8001 },
            ssid: 7,
            role: PacketRole::DialTo { call: "W7AUX".into(), path: vec![] },
        })
        .await
        .unwrap_err();
    // With no reachable modem on 127.0.0.1:8001, connect_link fails → TransportFailed
    // (loopback, fast-refused; NOT a live network call). If your CI has something on
    // 8001, point the test at a definitely-closed port via a bound-then-dropped
    // listener (the telnet.rs `connect_with_deadline_fails_fast_on_a_refused_port`
    // pattern) so it is deterministic.
    assert!(matches!(err, BackendError::TransportFailed { .. }));
}

#[test]
fn packet_dial_selects_dial_role_and_listen_selects_answer_role() {
    // The branch the lifecycle takes is fully determined by resolve_packet_endpoint
    // (Task 4) — assert it here so the lifecycle wiring can rely on it.
    assert_eq!(
        resolve_packet_endpoint("N7CPZ", 7, PacketRole::DialTo { call: "W7AUX".into(), path: vec![] })
            .unwrap()
            .role,
        ExchangeRole::Dial
    );
    assert_eq!(
        resolve_packet_endpoint("N7CPZ", 7, PacketRole::Listen).unwrap().role,
        ExchangeRole::Answer
    );
}
```
Add the `offline_config_with_callsign` helper (a `Config` with `connect_to_cms: true`, a callsign set, and `packet: PacketConfig::default()`).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib winlink_backend::tests::connect_packet`
Expected: FAIL — the `Packet` arm of `connect` still returns the Task-4 `NotImplemented` placeholder (so the error is `Unavailable`, not `TransportFailed`).

- [ ] **Step 3: Write minimal implementation**

Replace the `Packet` arm placeholder. The dial branch (peer/gateway) on `spawn_blocking`:
```rust
// inside connect(), Packet arm, after resolve_packet_endpoint(...) = resolved:
let config = self.config.clone();
let mailbox = self.mailbox.clone();
let progress = self.progress.clone();
let abort_handle = self.abort_handle.clone();
let aborting = self.aborting.clone();
self.set_status(BackendStatus::Connecting { transport: format!("Packet-{ssid}") });
let outcome = tokio::task::spawn_blocking(move || {
    native_packet_connect(&config, &mailbox, link, resolved, &*progress, &abort_handle, &aborting)
})
.await
.map_err(|e| BackendError::Internal { msg: format!("packet connect task failed: {e}"), source: None })?;
// reuse abort_aware_outcome + the same Connected/Cancelled/Error status mapping as the Cms arm.
```
Add `native_packet_connect` (the blocking driver that opens the link + connects/answers + calls `native_packet_exchange`). It mirrors `native_connect`'s structure and the `register_socket`/abort pattern; per RADIO-1 it is written, not agent-run against a real modem:
```rust
/// Open the KISS link, connect (dial) or answer (listen), and run the exchange.
/// Mirrors `native_connect` for the packet transport. The link's shutdown handle
/// registers into `abort_handle` so an operator abort unblocks a hung
/// connect/answer/exchange (spec §5 "Abort"; telnet's register_socket analogue).
fn native_packet_connect(
    config: &Config,
    mailbox: &Mailbox,
    link: KissLinkConfig,
    resolved: ResolvedPacket,
    progress: &dyn Fn(&str),
    abort_handle: &Mutex<Option<TcpStream>>, // see NOTE on the handle type
    aborting: &AtomicBool,
) -> Result<(), BackendError> {
    let params = config.packet.params.clone().into_params(); // Ax25ParamsConfig → Ax25Params
    progress("Opening KISS link…");
    let bytelink = crate::winlink::ax25::connect_link(&link)
        .map_err(|e| BackendError::TransportFailed { reason: format!("KISS link: {e}"), source: None })?;
    // Look up the gateway password from the keyring for the dial case (the
    // challenge is conditional — handled inside run_exchange; a peer never
    // challenges so the token is simply unused).
    let base = resolved.base_mycall.clone();
    let password = keyring::Entry::new("tuxlink-pat", &base)
        .ok().and_then(|e| e.get_password().ok()).filter(|p| !p.is_empty());

    match resolved.dial {
        Some((target, digis)) => {
            progress(&format!("Connecting to {}…", target.call));
            let stream = crate::winlink::ax25::connect(bytelink, resolved.link_mycall, target.clone(), &digis, params)
                .map_err(|e| BackendError::TransportFailed { reason: format!("AX.25 connect: {e}"), source: None })?;
            native_packet_exchange(stream, &base, &target.call, password, ExchangeRole::Dial, mailbox, progress)
        }
        None => {
            progress("Listening for an inbound peer…");
            let (peer, stream) = crate::winlink::ax25::answer(bytelink, resolved.link_mycall, params)
                .map_err(|e| BackendError::TransportFailed { reason: format!("AX.25 answer: {e}"), source: None })?;
            progress(&format!("Answered {}.", peer.call));
            native_packet_exchange(stream, &base, &peer.call, None, ExchangeRole::Answer, mailbox, progress)
        }
    }
}
```
> NOTE on the abort handle type: `NativeBackend.abort_handle` is `Arc<Mutex<Option<TcpStream>>>` today (telnet-specific). P2's `ByteLink` shutdown is not a `TcpStream`. **The minimal change** is to generalize `abort_handle` to `Arc<Mutex<Option<Box<dyn AbortHandle>>>>` where `AbortHandle: Send + Sync { fn shutdown(&self); }`, with the telnet path adapting its `TcpStream` clone into that trait. If generalizing is too invasive for this task, scope it down: register only the underlying TCP socket for the `KissLinkConfig::Tcp` case (the serial case's abort is a follow-up), and keep `Option<TcpStream>`. Pick the trait generalization if P2 exposes a shutdown handle; otherwise the Tcp-only scoping is acceptable for v0.1 and is captured as a follow-up bd issue. EITHER choice is documented in the commit body.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib winlink_backend::tests::connect_packet packet_dial_selects`
Expected: PASS — the no-link case fast-fails to `TransportFailed`; the role-selection assertions hold.

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink_backend.rs
git commit -m "feat(ax25): packet connect/listen lifecycle wires ax25::connect/answer + abort (tuxlink-7fr)

DialTo → ax25::connect then Dial exchange; Listen → ax25::answer then Answer
exchange; both on spawn_blocking with the link shutdown registered for abort
(telnet register_socket analogue, spec §5). Per RADIO-1 the real KISS link is
never agent-run; tests cover branch selection + the no-link fast-fail only.

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: Tauri commands — `packet_config_get` / `packet_config_set` (round-trip the `[packet]` section)

**Files:**
- Modify: `src-tauri/src/ui_commands.rs` (add `PacketConfigDto`, `packet_config_get`, `packet_config_set`)
- Modify: `src-tauri/src/lib.rs` (register both in `generate_handler!`)
- Test: `src-tauri/src/ui_commands.rs` (`#[cfg(test)] mod tests`) — DTO ↔ config mapping (no `tauri::State`)

`packet_config_get` reads `config.rs` directly (no `BackendState` dependency, like `config_read`); `packet_config_set` reads the current config, applies the DTO's packet fields, validates, and writes atomically. The DTO is flat (frontend-friendly), mapping the nested `PacketConfig` + `Ax25ParamsConfig` + `KissLinkConfig`.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `ui_commands.rs`:
```rust
#[test]
fn packet_config_dto_round_trips_through_packet_config() {
    let pc = config::PacketConfig {
        ssid: 7,
        link: Some(crate::winlink::ax25::KissLinkConfig::Tcp { host: "127.0.0.1".into(), port: 8001 }),
        params: config::Ax25ParamsConfig { paclen: 128, maxframe: 4, ..Default::default() },
        listen_default: false,
    };
    let dto = PacketConfigDto::from(&pc);
    assert_eq!(dto.ssid, 7);
    assert!(!dto.listen_default);
    assert_eq!(dto.link_kind.as_deref(), Some("Tcp"));
    assert_eq!(dto.tcp_host.as_deref(), Some("127.0.0.1"));
    assert_eq!(dto.tcp_port, Some(8001));
    assert_eq!(dto.paclen, 128);

    let back = dto.into_packet_config().unwrap();
    assert_eq!(back, pc);
}

#[test]
fn packet_config_dto_with_no_link_maps_to_none() {
    let pc = config::PacketConfig::default();
    let dto = PacketConfigDto::from(&pc);
    assert_eq!(dto.link_kind, None);
    assert!(dto.listen_default); // default-on
    assert_eq!(dto.into_packet_config().unwrap().link, None);
}

#[test]
fn packet_config_dto_serializes_camel_case_for_the_frontend() {
    let dto = PacketConfigDto::from(&config::PacketConfig::default());
    let v = serde_json::to_value(&dto).unwrap();
    assert!(v.get("listenDefault").is_some(), "expected camelCase listenDefault");
    assert!(v.get("ssid").is_some());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib ui_commands::tests::packet_config_dto`
Expected: FAIL to compile — `PacketConfigDto` and its conversions are not defined.

- [ ] **Step 3: Write minimal implementation**

In `ui_commands.rs`, add the flat DTO + conversions:
```rust
/// Flat, frontend-facing projection of `config::PacketConfig` (the `[packet]`
/// section). camelCase on the wire to match the TS model. `link_kind` is
/// `"Tcp"` | `"Serial"` | absent; the tcp_*/serial_* fields carry whichever set
/// applies (the other is `None`).
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PacketConfigDto {
    pub ssid: u8,
    pub listen_default: bool,
    pub link_kind: Option<String>,
    pub tcp_host: Option<String>,
    pub tcp_port: Option<u16>,
    pub serial_device: Option<String>,
    pub serial_baud: Option<u32>,
    pub txdelay: u8,
    pub persistence: u8,
    pub slot_time: u8,
    pub paclen: u16,
    pub maxframe: u8,
    pub t1_ms: u64,
    pub n2_retries: u8,
}

impl From<&config::PacketConfig> for PacketConfigDto {
    fn from(p: &config::PacketConfig) -> Self {
        use crate::winlink::ax25::KissLinkConfig;
        let (link_kind, tcp_host, tcp_port, serial_device, serial_baud) = match &p.link {
            Some(KissLinkConfig::Tcp { host, port }) => {
                (Some("Tcp".into()), Some(host.clone()), Some(*port), None, None)
            }
            Some(KissLinkConfig::Serial { device, baud }) => {
                (Some("Serial".into()), None, None, Some(device.clone()), Some(*baud))
            }
            None => (None, None, None, None, None),
        };
        PacketConfigDto {
            ssid: p.ssid,
            listen_default: p.listen_default,
            link_kind, tcp_host, tcp_port, serial_device, serial_baud,
            txdelay: p.params.txdelay,
            persistence: p.params.persistence,
            slot_time: p.params.slot_time,
            paclen: p.params.paclen,
            maxframe: p.params.maxframe,
            t1_ms: p.params.t1_ms,
            n2_retries: p.params.n2_retries,
        }
    }
}

impl PacketConfigDto {
    /// Build a `PacketConfig` from the DTO. Validates the link-kind/field
    /// coherence (`Tcp` needs host+port; `Serial` needs device+baud).
    pub fn into_packet_config(self) -> Result<config::PacketConfig, UiError> {
        use crate::winlink::ax25::KissLinkConfig;
        let link = match self.link_kind.as_deref() {
            Some("Tcp") => Some(KissLinkConfig::Tcp {
                host: self.tcp_host.ok_or_else(|| UiError::Internal { detail: "Tcp link needs tcp_host".into() })?,
                port: self.tcp_port.ok_or_else(|| UiError::Internal { detail: "Tcp link needs tcp_port".into() })?,
            }),
            Some("Serial") => Some(KissLinkConfig::Serial {
                device: self.serial_device.ok_or_else(|| UiError::Internal { detail: "Serial link needs serial_device".into() })?,
                baud: self.serial_baud.ok_or_else(|| UiError::Internal { detail: "Serial link needs serial_baud".into() })?,
            }),
            None => None,
            Some(other) => return Err(UiError::Internal { detail: format!("unknown link_kind '{other}'") }),
        };
        Ok(config::PacketConfig {
            ssid: self.ssid,
            link,
            params: config::Ax25ParamsConfig {
                txdelay: self.txdelay,
                persistence: self.persistence,
                slot_time: self.slot_time,
                paclen: self.paclen,
                maxframe: self.maxframe,
                t1_ms: self.t1_ms,
                n2_retries: self.n2_retries,
            },
            listen_default: self.listen_default,
        })
    }
}
```
Add the two commands:
```rust
/// Read the `[packet]` config section as a flat DTO. Reads `config.rs` directly
/// (no BackendState), like `config_read`.
#[tauri::command]
pub async fn packet_config_get() -> Result<PacketConfigDto, UiError> {
    let cfg = config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(PacketConfigDto::from(&cfg.packet))
}

/// Apply the `[packet]` section from a DTO: read the current config, swap in the
/// new packet section, validate (SSID range), and write atomically.
#[tauri::command]
pub async fn packet_config_set(dto: PacketConfigDto) -> Result<(), UiError> {
    let mut cfg = config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    cfg.packet = dto.into_packet_config()?;
    cfg.validate().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    config::write_config_atomic(&cfg).map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(())
}
```
Register both in `lib.rs` `generate_handler!` (alongside `config_read`):
```rust
            crate::ui_commands::packet_config_get,     // tuxlink-7fr (packet config)
            crate::ui_commands::packet_config_set,     // tuxlink-7fr (packet config)
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib ui_commands::tests::packet_config`
Expected: PASS (the three DTO round-trip/serialization tests).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/ui_commands.rs src-tauri/src/lib.rs
git commit -m "feat(ax25): packet_config_get/set Tauri commands round-trip the [packet] section (tuxlink-7fr)

Flat camelCase PacketConfigDto ↔ nested PacketConfig/Ax25ParamsConfig/KissLink;
get reads config directly (no BackendState), set reads-modify-validate-write
atomically. Registered in generate_handler!.

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 8: Tauri commands — `packet_connect` / `packet_set_listen` (drive connect; persist listen-default)

**Files:**
- Modify: `src-tauri/src/ui_commands.rs` (add `packet_connect`, `packet_set_listen`)
- Modify: `src-tauri/src/lib.rs` (register both)
- Test: `src-tauri/src/ui_commands.rs` (`#[cfg(test)] mod tests`) — argument-shape + the `packet_set_listen` config write (no live link)

`packet_connect(call, path)` builds a `TransportConfig::Packet { link, ssid, role: DialTo { call, path } }` from config + the args and drives `backend.connect`, surfacing progress/result on the session log exactly like `cms_connect`. `packet_set_listen(enabled)` flips `config.packet.listen_default` and writes it (sticky). Per RADIO-1, the agent NEVER runs `packet_connect` against a real link — the test asserts the command builds the right `TransportConfig` and surfaces a clean error when no link is configured.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `ui_commands.rs` (these test the pure builder + the config write, not the live `tauri::State` path):
```rust
#[test]
fn packet_transport_from_config_builds_dialto_with_ssid_and_path() {
    // The pure builder the command uses: config + (call, path) → TransportConfig.
    let mut cfg = config_with_packet_link(); // helper: packet.link = Tcp 127.0.0.1:8001, ssid 7
    cfg.packet.ssid = 7;
    let tc = packet_transport_from_config(&cfg, "W7AUX".into(), vec!["RELAY-1".into()]).unwrap();
    match tc {
        TransportConfig::Packet { ssid, role, .. } => {
            assert_eq!(ssid, 7);
            assert_eq!(role, crate::winlink_backend::PacketRole::DialTo {
                call: "W7AUX".into(), path: vec!["RELAY-1".into()],
            });
        }
        _ => panic!("expected a Packet transport"),
    }
}

#[test]
fn packet_transport_from_config_with_no_link_is_not_configured() {
    let cfg = config_with_packet_defaults(); // packet.link = None
    let err = packet_transport_from_config(&cfg, "W7AUX".into(), vec![]).unwrap_err();
    assert!(matches!(err, UiError::NotConfigured(_)));
}

#[test]
fn set_listen_default_writes_the_sticky_flag() {
    // The pure config mutation the command uses (no tauri::State, no fs in the unit
    // test — assert the returned Config carries the new flag).
    let mut cfg = config_with_packet_defaults(); // listen_default = true
    apply_listen_default(&mut cfg, false);
    assert!(!cfg.packet.listen_default);
    apply_listen_default(&mut cfg, true);
    assert!(cfg.packet.listen_default);
}
```
Add the helpers `config_with_packet_link`, `config_with_packet_defaults` (build a valid `Config` with `connect_to_cms: true` + callsign + the named packet section).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib ui_commands::tests::packet_transport ui_commands::tests::set_listen`
Expected: FAIL to compile — `packet_transport_from_config` and `apply_listen_default` are not defined.

- [ ] **Step 3: Write minimal implementation**

In `ui_commands.rs`, add the pure builders (kept separate from the `#[tauri::command]` fns so they are unit-testable without `tauri::State`, the same split `derive_status_dto` / `parse_raw_rfc5322` use):
```rust
/// Build the packet `TransportConfig` from config + the operator's dial args.
/// `NotConfigured` if no KISS link is set yet (the UI must configure one first).
pub fn packet_transport_from_config(
    cfg: &config::Config,
    call: String,
    path: Vec<String>,
) -> Result<TransportConfig, UiError> {
    let link = cfg
        .packet
        .link
        .clone()
        .ok_or_else(|| UiError::NotConfigured("no KISS link configured".into()))?;
    Ok(TransportConfig::Packet {
        link,
        ssid: cfg.packet.ssid,
        role: crate::winlink_backend::PacketRole::DialTo { call, path },
    })
}

/// Flip the sticky listen-default flag on a config (the mutation `packet_set_listen`
/// persists). Pure; the command wraps read → mutate → write.
pub fn apply_listen_default(cfg: &mut config::Config, enabled: bool) {
    cfg.packet.listen_default = enabled;
}
```
Add the commands:
```rust
/// Dial a packet station (gateway or peer — tuxlink reacts to the challenge, not
/// a mode flag; spec §2). Builds the packet TransportConfig from config + args and
/// drives `backend.connect`, surfacing progress/result on the session log like
/// `cms_connect`. RADIO-1: operator-run on real hardware; the agent never runs this.
#[tauri::command]
pub async fn packet_connect(
    app: AppHandle,
    state: State<'_, BackendState>,
    log: State<'_, std::sync::Arc<SessionLogState>>,
    call: String,
    path: Vec<String>,
) -> Result<(), UiError> {
    let backend = state.current().ok_or_else(|| UiError::NotConfigured("backend offline".into()))?;
    let cfg = config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    let transport = packet_transport_from_config(&cfg, call.clone(), path)?;
    emit_session_line(&app, &log, LogLevel::Info, format!("Connecting to {call} over packet…"));
    match backend.connect(transport).await {
        Ok(_session) => {
            emit_session_line(&app, &log, LogLevel::Info, "Packet exchange complete.".into());
            Ok(())
        }
        Err(BackendError::Cancelled) => {
            emit_session_line(&app, &log, LogLevel::Warn, "Packet connection aborted.".into());
            Err(BackendError::Cancelled.into())
        }
        Err(e) => {
            emit_session_line(&app, &log, LogLevel::Error, format!("Packet connect failed: {e}"));
            Err(e.into())
        }
    }
}

/// Persist the sticky idle-listen default (spec §4.5, §4.6 panel toggle + the
/// Settings selector write the same value).
#[tauri::command]
pub async fn packet_set_listen(enabled: bool) -> Result<(), UiError> {
    let mut cfg = config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    apply_listen_default(&mut cfg, enabled);
    config::write_config_atomic(&cfg).map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(())
}
```
Register both in `lib.rs`:
```rust
            crate::ui_commands::packet_connect,        // tuxlink-7fr (packet dial)
            crate::ui_commands::packet_set_listen,     // tuxlink-7fr (sticky listen)
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib ui_commands::`
Expected: PASS (the new builder/listen tests + all existing ui_commands tests).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/ui_commands.rs src-tauri/src/lib.rs
git commit -m "feat(ax25): packet_connect/packet_set_listen Tauri commands (tuxlink-7fr)

packet_connect builds the packet TransportConfig from config+args and drives
backend.connect with session-log progress (cms_connect parallel); packet_set_listen
persists the sticky listen-default. Pure builders split out for unit testing
without tauri::State. RADIO-1: the agent never runs packet_connect on real hw.

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 9: Full crate gate — build + clippy + the whole test suite

**Files:** none (verification only)

- [ ] **Step 1: Build**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: clean build (the `Packet` arm of every `TransportConfig` match is handled; no warnings about unhandled `#[non_exhaustive]` arms in-crate).

- [ ] **Step 2: Clippy**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
Expected: no warnings. (If `native_packet_connect` / `native_packet_exchange` trip `clippy::too_many_arguments`, group the abort/aborting/progress trio into a small `PacketConnectCtx` struct rather than `#[allow]`-ing — match the codebase's existing context-passing style.)

- [ ] **Step 3: Full test suite**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: PASS — all P3 tests plus the entire existing suite. No test reaches a real network or a real KISS modem (RADIO-1).

- [ ] **Step 4: Confirm no live-path test slipped in**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -- --list 2>/dev/null | grep -i "packet\|ax25"`
Expected: every listed packet/ax25 test is an in-memory or loopback test; visually confirm none names a real CMS host or a real serial device.

- [ ] **Step 5: Commit (only if Step 2 forced a refactor; otherwise no commit)**

If clippy forced a `PacketConnectCtx` grouping:
```bash
git add src-tauri/src/winlink_backend.rs
git commit -m "refactor(ax25): group packet-connect context to satisfy clippy (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Coordination

**tuxlink-686 (position subsystem)** edits four of the same files this plan touches. Confirm 686's landing status before starting; **recommend rebasing this branch onto `main` after 686 lands** (non-interactive `git rebase main` per the destructive-git ban; no `-i`). The precise overlaps and how to keep the merge clean:

- **`config.rs`** — 686 adds `position_source` to **`PrivacyConfig`** and **bumps `CONFIG_SCHEMA_VERSION`**. P3 adds a NEW top-level field `packet: PacketConfig` to **`Config`** (not `PrivacyConfig`) behind `#[serde(default)]`, and adds `ConfigValidationError::PacketSsidOutOfRange`. No field collision (different structs). Conflict risk is limited to: (a) the `Config { .. }` struct-literal sites both plans edit (each adds its own field — resolve by keeping BOTH new fields), and (b) the `CONFIG_SCHEMA_VERSION` constant (686 owns the bump; P3 needs no bump — if 686 already bumped, P3 rides the bumped value unchanged). After 686, re-run `config::tests` to confirm `#[serde(default)] packet` still defaults under the bumped version (686's NOTE about a possible `read_config` migration arm applies equally — if old files are rejected by the version guard, both fields default in the same migration arm).

- **`winlink_backend.rs`** — 686 modifies **`cms_locator`** (to read the position arbiter's broadcast grid). P3 does NOT touch `cms_locator`; it adds `TransportConfig::Packet`, `PacketRole`, `ResolvedPacket`, `resolve_packet_endpoint`, `native_packet_exchange`, `native_packet_connect`, and the `Packet` arm of `NativeBackend::connect`. The only shared edit is the `use` block at the top and the test module's `Config`-literal helpers (`sample_config`/equivalent) — both add a field; keep both. No function-body collision.

- **`ui_commands.rs`** — 686 adds `config_set_grid` + `position_set_source` and extends **`ConfigViewDto`** with `position_source`. P3 adds `PacketConfigDto`, `packet_config_get/set`, `packet_connect`, `packet_set_listen`, and pure builders — it does NOT touch `ConfigViewDto`. Shared edit: the test-config builder helpers (add both fields). No DTO collision.

- **`lib.rs`** — both add entries to the SAME `generate_handler!` macro list. This is the highest-probability textual conflict. 686 adds `config_set_grid` (+ maybe `position_set_source`); P3 adds four (`packet_config_get/set`, `packet_connect`, `packet_set_listen`). Resolve by keeping all entries from both sides; the macro list order is not significant. 686 also adds `mod position;` + manages a `PositionArbiter` state; P3 adds no new module declaration or managed state, so those lines do not collide.

**Sequencing recommendation:** land 686 first (it bumps the schema version, which is the single most disruptive change), then rebase this branch onto `main` and re-run the full gate (Task 9). If P3 must land first, that is also fine — P3's additive field needs no bump, so 686's later bump simply increments past it; 686's rebase then resolves the `Config`-literal + `generate_handler!` overlaps. Maintain the bd dep edge: `bd dep add tuxlink-7fr tuxlink-686` if 686 should precede (consumer 7fr depends on provider 686), so `bd ready` reflects the ordering.

**P1/P2 dependency:** this plan consumes P1's `winlink/ax25/` codec (already structured) and P2's `KissLinkConfig` / `Ax25Params` / `Address` / `connect` / `answer` / `connect_link` / `Ax25Stream`. If P2 has not landed, Task 1–3, 7 are still fully implementable (they reference only `KissLinkConfig`/`Address`, which can be stubbed locally OR P2 lands first). Recommend: `bd dep add tuxlink-7fr <p2-id>` so P2 (the datalink/transports) is `bd ready` before P3's Tasks 4–6 (which call `ax25::connect`/`answer`/`connect_link`). Tasks 1–3 and 7 only need the type names; Tasks 4–6 need the functions.

---

## Self-review

**1. Spec coverage**

- **§2 modes table** — Dial gateway (slave, secure-login) = `ExchangeRole::Dial` with `password: Some(..)`, covered Task 1 + Task 5 (`native_packet_exchange_dials_a_gateway_with_secure_login`). Dial peer (slave, no challenge) = `Dial` with the conditional `;PQ` already absent — same code path, no token written. Answer peer (master, no challenge) = `ExchangeRole::Answer`, covered Task 2 + Task 5/6. "Gateway vs peer is not an operator choice" — `PacketRole::DialTo` is one control; the challenge handling inside `run_exchange` distinguishes them. "Idle = listening, default-on" — `PacketConfig.listen_default = true` (Task 3) + `PacketRole::Listen` → `ExchangeRole::Answer` (Task 4) + the lifecycle (Task 6). Half-duplex "inbound while busy not answered" is a P2 datalink property, not P3 — noted, no P3 task (the single-flight `connect_in_progress` guard already prevents a concurrent dial/answer at the backend layer).
- **§4.4 role-param + identity split** — `ExchangeRole { Dial, Answer }` (Task 1); `Answer` sends handshake first + remote-takes-first-turn (Task 2). Identity split: `Address { call: base, ssid }` for the link, base call into `ExchangeConfig.mycall` — enforced in `resolve_packet_endpoint` (Task 4) and `native_packet_exchange` (Task 5), asserted in `resolve_packet_endpoint_dial_builds_ssidd_link_addr_and_base_b2f_call`. The plan flags VERIFY-against-handshake.rs/wl2k-go at execution per §4.4/§9.
- **§4.5 orchestration/config** — `TransportConfig::Packet { link, ssid, role }` + `PacketRole = DialTo{call,path} | Listen` (Task 4). Endpoint/role resolution sibling to `resolve_cms_endpoint` = `resolve_packet_endpoint` (Task 4). Lifecycle idle→answer-armed / Connect→connect→return-to-listening (Task 6). `[packet]` section: sticky `ssid`, last `KissLinkConfig`, `Ax25Params`, `listen_default=true` (Task 3). Keyring reuse for the gateway dial (Task 5/6 look up `tuxlink-pat`/callsign like `native_connect`).
- **§5 errors** — connect no-UA bounded by P2's N2×T1 (P2's responsibility; P3 maps the resulting `io::Error` to `BackendError::TransportFailed`). KISS-link failure distinct from no-answer: `connect_link` failure → `TransportFailed { reason: "KISS link: …" }` vs `ax25::connect` failure → `TransportFailed { reason: "AX.25 connect: …" }` (Task 6, distinguishable reason strings). Link failure mid-exchange propagates as `ExchangeError` → `TransportFailed` (Task 5). Abort via the link shutdown hook mirroring telnet's `register_socket` (Task 6, with the abort-handle-type NOTE). The no-link fast-fail is unit-tested (Task 6).

**2. Placeholder scan** — no "TBD"/"implement later"/"add error handling"/"similar to Task N" left as the actual work. Every code step has real Rust. Two deliberate, fully-specified DESIGN FORKS are called out with the recommended resolution inline, not left open: (a) Task 5's stream-reclaim awkwardness → the plan instructs adopting the by-value-drop signature `Result<(), BackendError>` (the simpler form), and updates the tests to match; (b) Task 6's abort-handle type → the plan instructs the `Box<dyn AbortHandle>` generalization, with an explicit acceptable fallback (Tcp-only scoping + follow-up bd issue). These are decisions the implementer executes, not blanks. The empty `locator: String::new()` in `native_packet_exchange` is intentional and noted (B2F accepts an empty locator; the packet on-air locator surface is a §4.6 UI concern, not P3 protocol) — if a locator is wanted, the implementer passes `cms_locator(config)` (already exists), a one-line change.

**3. Type consistency** — names match across tasks and the shared contracts: `ExchangeRole { Dial, Answer }` (defined Task 1, used Tasks 2/4/5/6/8), `run_exchange_with_role` (Task 1, used Task 5), `TransportConfig::Packet { link: KissLinkConfig, ssid: u8, role: PacketRole }` (Task 4, used Tasks 6/8), `PacketRole { DialTo { call: String, path: Vec<String> }, Listen }` (Task 4, used Tasks 4/6/8), `PacketConfig { ssid, link: Option<KissLinkConfig>, params: Ax25ParamsConfig, listen_default }` (Task 3, used Tasks 5/6/7), `Ax25ParamsConfig` (Task 3, used Task 7's DTO), `PacketConfigDto` flat camelCase (Task 7), command names `packet_connect/packet_set_listen/packet_config_get/packet_config_set` (Tasks 7/8) match the brief exactly. P2 contract names (`KissLinkConfig`, `Ax25Params`, `Address`, `connect`, `answer`, `connect_link`, `Ax25Stream`) are imported, never redefined. The B2F-base-call vs link-SSID identity is consistent everywhere an `Address` or an `ExchangeConfig.mycall` is constructed.
