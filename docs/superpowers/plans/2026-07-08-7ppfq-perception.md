# tuxlink-7ppfq Perception (VARA reachability + active-modem SoT) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the agent honest perception of the VARA modem (is its cmd port reachable? is a real VARA answering?) and of *which* modem is actually running vs which the operator selected, so agent-driven VARA testing stops seeing a stale `kind:"ardop"`.

**Architecture:** Two contracts, perception-only (no CONNECT, no transmit, no VARA *launch* — launch is split to tuxlink-u269g). Contract 1 adds a tri-state `reachable` to the MCP `VaraStatusDto` (a non-mutating `try_lock` + bare cmd-port TCP touch, TTL-cached) and a new read-only `vara_probe` tool (connect + `VERSION` handshake, never a setter). Contract 2 makes `modem_get_status` report **both** `selected` (operator target, persisted to `Config`) and `running` (live, sourced from session state — for ARDOP from `snapshot_transport_present()`/`ModemState`, **not** `active_transport_kind`), with `kind` dispatched on what's running instead of a hardcoded literal.

**Tech Stack:** Rust (Tauri backend, two workspace crates: `tuxlink-mcp-core` for the port traits/router, `src-tauri` for the real impls), `std::net` for the socket touch, `std::sync::Mutex`, serde/schemars for DTOs; React + TypeScript + Vitest for the frontend.

## Global Constraints

- **Perception-only.** No task issues `CONNECT`, transmits, or launches VARA. No `MYCALL`/`BW`/`LISTEN` setter is ever sent (those mutate the operator's live VARA). RADIO-1 and the egress arm/taint gate are untouched.
- **Additive.** No existing signal changes meaning. `vara_engine_available` stays the CONFIGURE gate; new signals are added alongside. `ModemStatusDto.connected` keeps its meaning.
- **No session-mutex contention.** Reachability/probe paths never *hold* `VaraSession.inner` (or `ModemSession.inner`) across a socket op. One brief `try_lock` classification is permitted; a contended lock returns `reachable: unknown` (never waits).
- **No hardcoded device/host identity.** `host`/`cmd_port` come from `config_get_vara()`, never a literal.
- **Pi cannot compile Rust.** The local TDD "run the test" steps for Rust tasks are satisfied by **CI on amd64+arm64** (the compile/verify oracle). Write the failing test, write the impl, then push; CI is where red→green is confirmed. Frontend (Vitest) tasks DO run locally. Arm clippy traps where a compile-only mistake is likely.
- **Trait names (correcting the design spec):** the trait carrying `vara_status`/`modem_status` is **`StatusPort`** (there is no `VaraPort`/`ModemPort`). Provisioning is `ProvisionPort`.
- **Naming:** MCP DTOs use the `...Dto` suffix. Do NOT name a new struct `VaraStatus` (collides with the re-exported `commands::VaraStatus`).

**Baseline anchors** (worktree `worktrees/bd-tuxlink-7ppfq-perception`, off `origin/main`, schema v5):
- `src-tauri/tuxlink-mcp-core/src/ports.rs`: `VaraStatusDto` L262-267, `ModemStatusDto` L254-260, `StatusPort` trait L506-525.
- `src-tauri/tuxlink-mcp-core/src/lib.rs`: `MockStatus` L178; `modem_status` mock L189-195; `vara_status` mock L196-202.
- `src-tauri/src/mcp_ports.rs`: `modem_status` impl L194-211 (hardcodes `kind:"ardop"` at L207); `vara_status` impl L213-232.
- `src-tauri/tuxlink-mcp-core/src/router.rs`: `#[tool_router]` at L86; `vara_status` tool L129-137; input-taking tool pattern `vara_install_start` L479-498; `VaraInstallParams` L1224-1229.
- `src-tauri/src/winlink/modem/vara/commands.rs`: `VaraSession` struct L152; `VaraSessionInner` L217 (holds `inner: Mutex<VaraSessionInner>` — **`std::sync::Mutex`**, imported L27); `snapshot()` L332; `VaraState` enum L73-99 (`Closed`/`Connecting`/`Open`/`Error`/`SocketLost`); `VaraStatus` L105-124; `config_get_vara()` L1125-1129; `build_transport_config()` L1144-1156 (`connect_timeout: Duration::from_secs(5)`); Tauri cmd `vara_status` L1878.
- `src-tauri/src/winlink/modem/vara/transport.rs`: `VaraConfig` L24-48 (`cmd_port`/`data_port`/`connect_timeout`/`read_timeout`); `VaraTransport` L51-61; `connect()` L63-116; `send_raw()` ~L128.
- `src-tauri/resources/wine-vara-setup/lib/checkpoints.sh`: `wv_wait_ports()` L58-76 — the read-only handshake to mirror: `printf 'VERSION\r' | nc … | grep -qi VARA`.
- `src-tauri/src/bin/vara_tcp_probe.rs`: the **mutating** probe (sends MYCALL/BW/LISTEN at L85-104) — do NOT copy those setters; its read-only banner drain L70-83 is safe to mirror.
- `src-tauri/src/modem_status.rs`: `ModemState` enum L147-173 (kebab-case; `Stopped`…`SocketLost`); `snapshot_transport_present()` L1170-1178; `active_transport_kind()` reader L869-875; `set_active_session_mode()` L793-803 (ONLY setter of `active_transport_kind`); `install_transport()` L588-590; broadcaster `STATUS_POLL_INTERVAL` L1354 (250 ms / 4 Hz), `STATUS_EVENT="modem:status"` L1374.
- `src-tauri/src/modem_commands.rs`: `modem_ardop_connect` L1508-1600 → `modem_ardop_connect_post_consume_with_factory` installs transport at **L534** and **never** sets `active_transport_kind`; `ardop_open_session_inner` sets it at **L1007** (spawn-only, unused by the live Connect button); `modem_get_status` Tauri cmd L300; `modem_get_status_inner`.
- `src-tauri/src/config.rs`: `CONFIG_SCHEMA_VERSION` L21 (=5); `detect_schema_action` L40-47; `Config` struct L208 (`#[serde(deny_unknown_fields, remote="Self")]`); hand-written `Deserialize` w/ post-migrate L381-392; golden-set test `config_schema_version_tracks_field_set` L1680-1706; additive-load precedent `connect_host_defaults_to_cms_z_when_absent_from_config` L1765+.
- `src-tauri/src/ui_commands.rs`: `config_set_review_inbound` L6910-6921 (the frontend→config persist pattern); `config_read`→`ConfigViewDto` L3463.
- `src-tauri/src/lib.rs`: `invoke_handler` registration (`modem_get_status` at L2029) — new Tauri commands register here.
- `src/shell/AppShell.tsx`: `activeConnection` state L474-481 (`ConnectionKey = {sessionType, protocol}`); `useModemIsActive` L834; `activeModem` hardcode L846-849 (`{kind:'ardop-hf', intent:'cms'}`); status-driven writer L851-863; `onSelectConnection` writer L1443-1446.
- `src/modem/useModemStatus.ts`: `useModemIsActive` L49-83 (the deduped-selector model), `MODEM_STATUS_EVENT`.
- `src/connections/sessionTypes.ts`: `SessionTypeId`, `ProtocolId` (`'ardop-hf'`, `'vara-hf'`, …), `ConnectionKey` L1-3.
- `src/radio/types.ts`: `RadioPanelMode` L22-23 (`{kind:'ardop-hf'|'vara-hf', intent:'cms'|'p2p'|'radio-only'}`).
- `src-tauri/src/winlink/listener/transport.rs`: backend `TransportKind` L22-45 — `Ardop→"ardop"` vs UI `"ardop-hf"` (bridge required).

---

## Task 1: VARA cmd-port reachability (`VaraStatusDto.reachable`)

**Files:**
- Modify: `src-tauri/tuxlink-mcp-core/src/ports.rs` (add `reachable` field to `VaraStatusDto` ~L262-267)
- Modify: `src-tauri/tuxlink-mcp-core/src/lib.rs` (`MockStatus::vara_status` L196-202)
- Modify: `src-tauri/src/winlink/modem/vara/commands.rs` (add reachability method + TTL cache field to `VaraSession`)
- Modify: `src-tauri/src/winlink/modem/vara/transport.rs` (add free fn `cmd_port_reachable`)
- Modify: `src-tauri/src/mcp_ports.rs` (`vara_status` impl L213-232 populates `reachable`)
- Modify: `src-tauri/tuxlink-mcp-core/src/router.rs` (`vara_status` tool description L130 documents `reachable`)
- Test: unit tests colocated in `transport.rs` and `commands.rs`; DTO test in `ports.rs`

**Interfaces:**
- Produces: `VaraStatusDto { connected: bool, bandwidth: u32, state: String, reachable: Option<bool> }` — `reachable`: `Some(true)` cmd port answered / session Open; `Some(false)` no answer; `None` = unknown (probe skipped because the session lock was contended). Task 2/3/4 leave this field as-is.
- Produces: `pub fn cmd_port_reachable(host: &str, cmd_port: u16, timeout: std::time::Duration) -> bool` in `transport.rs`.
- Produces: `impl VaraSession { pub fn probe_reachable(&self, host: &str, cmd_port: u16, timeout: std::time::Duration) -> Option<bool> }`.

- [ ] **Step 1: Write the failing test for the socket touch** (in `transport.rs` `#[cfg(test)] mod tests`)

```rust
#[test]
fn cmd_port_reachable_true_when_listener_bound() {
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let port = listener.local_addr().unwrap().port();
    assert!(super::cmd_port_reachable("127.0.0.1", port, std::time::Duration::from_secs(5)));
}

#[test]
fn cmd_port_reachable_false_when_no_listener() {
    // Bind then drop to obtain a port nothing is listening on.
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    assert!(!super::cmd_port_reachable("127.0.0.1", port, std::time::Duration::from_millis(500)));
}
```

- [ ] **Step 2: (CI oracle) verify it fails** — Push is the oracle (Pi can't compile). Expected CI FAIL: `cannot find function cmd_port_reachable`.

- [ ] **Step 3: Implement `cmd_port_reachable`** (in `transport.rs`, module level)

```rust
/// Read-only TCP reachability touch on VARA's COMMAND port. Opens a socket,
/// then immediately shuts it down — issues NO VARA command, so it never mutates
/// modem state. `cmd`-reachable is NOT "ready to send": 8300 can accept while
/// 8301 (data) still lags on a WINE restart.
pub fn cmd_port_reachable(host: &str, cmd_port: u16, timeout: std::time::Duration) -> bool {
    use std::net::ToSocketAddrs;
    let Ok(mut addrs) = (host, cmd_port).to_socket_addrs() else {
        return false;
    };
    let Some(addr) = addrs.next() else { return false };
    match std::net::TcpStream::connect_timeout(&addr, timeout) {
        Ok(stream) => {
            // Explicit shutdown so we never leave a half-open connection on
            // VARA's single-App acceptor.
            let _ = stream.shutdown(std::net::Shutdown::Both);
            true
        }
        Err(_) => false,
    }
}
```

- [ ] **Step 4: Write the failing test for `probe_reachable` derivation + no-contention** (in `commands.rs` tests). `VaraSession::new()` (L296) starts `Closed`, so the socket tests need no helper. Add two small `#[cfg(test)]` seams next to the existing `set_transport_owner_for_test` (L729):
  ```rust
  #[cfg(test)]
  pub fn set_state_for_test(&self, s: VaraState) {
      if let Ok(mut g) = self.inner.lock() { g.status.state = s; }
  }
  #[cfg(test)]
  pub fn lock_inner_for_test(&self) -> std::sync::MutexGuard<'_, VaraSessionInner> {
      self.inner.lock().unwrap()
  }
  ```
  Then (using `VaraSession::new()` + `set_state_for_test` instead of a `new_in_state_for_test` constructor):

```rust
#[test]
fn probe_reachable_open_session_reports_true_without_socket() {
    // Open session: derive from state == Open, do NOT touch a socket.
    let s = VaraSession::new();
    s.set_state_for_test(VaraState::Open);
    // Port 1 is privileged/unused; a socket attempt would fail, proving we skipped it.
    assert_eq!(s.probe_reachable("127.0.0.1", 1, std::time::Duration::from_millis(50)), Some(true));
}

#[test]
fn probe_reachable_closed_session_touches_socket_true() {
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let s = VaraSession::new(); // new() starts Closed
    assert_eq!(s.probe_reachable("127.0.0.1", port, std::time::Duration::from_secs(5)), Some(true));
}

#[test]
fn probe_reachable_closed_session_no_listener_false() {
    let port = { let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); l.local_addr().unwrap().port() };
    let s = VaraSession::new();
    assert_eq!(s.probe_reachable("127.0.0.1", port, std::time::Duration::from_millis(500)), Some(false));
}

#[test]
fn probe_reachable_returns_unknown_when_lock_contended() {
    // TOCTOU / no-contention invariant: holding inner must NOT make the probe wait.
    let s = VaraSession::new();
    let guard = s.lock_inner_for_test(); // holds inner.lock()
    // With inner held, try_lock fails → unknown, and this call must return promptly.
    assert_eq!(s.probe_reachable("127.0.0.1", 1, std::time::Duration::from_secs(5)), None);
    drop(guard);
}
```
(If `VaraSession::new()` does not default to `Closed`, call `s.set_state_for_test(VaraState::Closed)` first in the socket tests.)

- [ ] **Step 5: Implement `probe_reachable` + TTL cache** (in `commands.rs`). Add a sibling field to `VaraSession` (NOT inside `inner`): `reachable_cache: Mutex<Option<(std::time::Instant, bool)>>`, initialized `Mutex::new(None)` in every constructor. Implement:

```rust
impl VaraSession {
    /// Read-only cmd-port reachability, classified WITHOUT holding `inner`
    /// across a socket op. Returns `None` (unknown) if the session lock is
    /// contended — never waits. TTL-cached (~heartbeat cadence) so routine
    /// polls don't churn VARA's single-App acceptor.
    pub fn probe_reachable(&self, host: &str, cmd_port: u16, timeout: std::time::Duration) -> Option<bool> {
        // One brief try_lock classification. Contended → unknown (never wait).
        let state = match self.inner.try_lock() {
            Ok(g) => g.status.state,          // copy the Copy enum, drop guard immediately
            Err(std::sync::TryLockError::WouldBlock) => return None,
            Err(std::sync::TryLockError::Poisoned(p)) => p.into_inner().status.state,
        };
        // Guard dropped above. If a session is live, lean on the heartbeat — no socket.
        if matches!(state, VaraState::Open | VaraState::Connecting) {
            return Some(matches!(state, VaraState::Open));
        }
        // No live session: bare cmd-port touch, TTL-cached.
        const TTL: std::time::Duration = std::time::Duration::from_secs(3);
        if let Ok(cache) = self.reachable_cache.lock() {
            if let Some((at, val)) = *cache {
                if at.elapsed() < TTL {
                    return Some(val);
                }
            }
        }
        let val = crate::winlink::modem::vara::transport::cmd_port_reachable(host, cmd_port, timeout);
        if let Ok(mut cache) = self.reachable_cache.lock() {
            *cache = Some((std::time::Instant::now(), val));
        }
        Some(val)
    }
}
```

Note: `VaraState` derives `Copy` (L73), so copying it out of the guard and dropping the guard before the socket op satisfies the no-contention invariant.

- [ ] **Step 6: Wire the DTO + real impl + mock + router description.**
  - `ports.rs`: add `pub reachable: Option<bool>,` to `VaraStatusDto`.
  - `mcp_ports.rs` `vara_status` (L213-232): after building `connected`/`bandwidth`/`state`, compute reachability from a **shared** timeout knob so it can't drift from the transport:
    ```rust
    let ui = crate::winlink::modem::vara::commands::config_get_vara();
    let tcfg = crate::winlink::modem::vara::commands::build_transport_config(&ui);
    let reachable = session.probe_reachable(&tcfg.host, tcfg.cmd_port, tcfg.connect_timeout);
    ```
    (`build_transport_config` already sets `connect_timeout` = 5 s — the single source; `host`/`cmd_port` come from config, never hardcoded.) Add `reachable` to the returned `VaraStatusDto`.
  - `lib.rs` `MockStatus::vara_status`: add `reachable: Some(false),`.
  - `router.rs` L130 description → append: `Also reports \`reachable\` (Option<bool>): cmd-port (8300) reachability — true=answering, false=no answer, null=unknown (session busy). cmd-reachable is NOT ready-to-send.`

- [ ] **Step 7: DTO round-trip test** (in `ports.rs` tests): serialize a `VaraStatusDto { reachable: None, .. }` and one with `Some(true)`, assert deserialize round-trips and `null`↔`None`.

- [ ] **Step 8: Commit**

```bash
git add -A && git commit -m "feat(mcp): VARA cmd-port reachability on vara_status (Contract 1a)"
```

---

## Task 2: `vara_probe` read-only deep probe tool

**Files:**
- Modify: `src-tauri/tuxlink-mcp-core/src/ports.rs` (new `VaraProbeDto`; new `StatusPort::vara_probe` method)
- Modify: `src-tauri/tuxlink-mcp-core/src/lib.rs` (`MockStatus::vara_probe`)
- Modify: `src-tauri/src/winlink/modem/vara/transport.rs` (free fn `deep_probe`)
- Modify: `src-tauri/src/mcp_ports.rs` (real `vara_probe`)
- Modify: `src-tauri/tuxlink-mcp-core/src/router.rs` (new `#[tool] vara_probe`)
- Test: `transport.rs` tests (fake listener, assert no setter sent)

**Interfaces:**
- Consumes: `VaraConfig` (`host`/`cmd_port`/`connect_timeout`/`read_timeout`).
- Produces: `VaraProbeDto { classification: String, banner: Option<String> }` — `classification` ∈ `"down"` (no TCP), `"socket-not-vara"` (TCP open but no VARA in the reply), `"vara-ok"` (reply contains `VARA`).
- Produces: `pub fn deep_probe(cfg: &VaraConfig) -> VaraProbeDto` in `transport.rs`.

- [ ] **Step 1: Write the failing test with a fake VARA listener that records bytes** (in `transport.rs` tests). The fake accepts one connection, records everything it receives, and replies with a caller-chosen banner. This lets us assert (a) classification and (b) **no stateful setter was sent**.

```rust
#[cfg(test)]
fn spawn_fake_vara(reply: &'static str) -> (u16, std::sync::mpsc::Receiver<String>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        if let Ok((mut sock, _)) = listener.accept() {
            sock.set_read_timeout(Some(std::time::Duration::from_millis(300))).ok();
            let _ = sock.write_all(reply.as_bytes());   // startup banner
            let mut buf = [0u8; 512];
            let mut seen = String::new();
            while let Ok(n) = sock.read(&mut buf) {
                if n == 0 { break; }
                seen.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
            let _ = tx.send(seen);
        }
    });
    (port, rx)
}

#[test]
fn deep_probe_classifies_vara_ok_and_sends_no_setter() {
    let (port, rx) = spawn_fake_vara("VARA HF v4.8.6 Ready\r");
    let cfg = super::VaraConfig { host: "127.0.0.1".into(), cmd_port: port, data_port: port,
        connect_timeout: std::time::Duration::from_secs(5),
        read_timeout: Some(std::time::Duration::from_millis(300)) };
    let dto = super::deep_probe(&cfg);
    assert_eq!(dto.classification, "vara-ok");
    assert!(dto.banner.unwrap_or_default().to_uppercase().contains("VARA"));
    // Give the fake a moment to flush, then assert NO mutating setter was sent.
    let seen = rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap_or_default();
    let up = seen.to_uppercase();
    assert!(!up.contains("MYCALL"), "probe must not send MYCALL");
    assert!(!up.contains("BW"), "probe must not send BW");
    assert!(!up.contains("LISTEN"), "probe must not send LISTEN");
}

#[test]
fn deep_probe_socket_not_vara() {
    let (port, _rx) = spawn_fake_vara("gibberish\r");
    let cfg = super::VaraConfig { host: "127.0.0.1".into(), cmd_port: port, data_port: port,
        connect_timeout: std::time::Duration::from_secs(5),
        read_timeout: Some(std::time::Duration::from_millis(300)) };
    assert_eq!(super::deep_probe(&cfg).classification, "socket-not-vara");
}

#[test]
fn deep_probe_down_when_no_listener() {
    let port = { let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); l.local_addr().unwrap().port() };
    let cfg = super::VaraConfig { host: "127.0.0.1".into(), cmd_port: port, data_port: port,
        connect_timeout: std::time::Duration::from_millis(500),
        read_timeout: Some(std::time::Duration::from_millis(300)) };
    assert_eq!(super::deep_probe(&cfg).classification, "down");
}
```

- [ ] **Step 2: (CI oracle) verify fail** — Expected: `cannot find function deep_probe` / `VaraProbeDto`.

- [ ] **Step 3: Implement `deep_probe`** (in `transport.rs`). Mirror the setup engine's `wv_wait_ports` read-only handshake: connect the cmd port, read the startup banner, if it lacks `VARA` send a single `VERSION\r` query (a pure read — NOT a setter) and read the reply. Classify. Never send MYCALL/BW/LISTEN.

```rust
/// READ-ONLY deep probe: connect the cmd port, read the startup banner, and if
/// needed send a single `VERSION` query (a pure read — it does not mutate modem
/// state, unlike MYCALL/BW/LISTEN). Mirrors the setup engine's `wv_wait_ports`
/// verify handshake. Never opens the data port, never keys a radio.
pub fn deep_probe(cfg: &VaraConfig) -> VaraProbeDto {
    use std::io::{Read, Write};
    use std::net::ToSocketAddrs;
    let addr = match (cfg.host.as_str(), cfg.cmd_port).to_socket_addrs().ok().and_then(|mut a| a.next()) {
        Some(a) => a,
        None => return VaraProbeDto { classification: "down".into(), banner: None },
    };
    let mut stream = match std::net::TcpStream::connect_timeout(&addr, cfg.connect_timeout) {
        Ok(s) => s,
        Err(_) => return VaraProbeDto { classification: "down".into(), banner: None },
    };
    stream.set_read_timeout(cfg.read_timeout.or(Some(std::time::Duration::from_millis(500)))).ok();
    let mut acc = String::new();
    let mut buf = [0u8; 512];
    // Drain any startup banner first (read-only).
    if let Ok(n) = stream.read(&mut buf) { acc.push_str(&String::from_utf8_lossy(&buf[..n])); }
    if !acc.to_uppercase().contains("VARA") {
        // Single read-only VERSION query (CR terminator matches VARA's wire codec).
        let _ = stream.write_all(b"VERSION\r");
        if let Ok(n) = stream.read(&mut buf) { acc.push_str(&String::from_utf8_lossy(&buf[..n])); }
    }
    let _ = stream.shutdown(std::net::Shutdown::Both);
    let banner = acc.trim().to_string();
    let classification = if banner.to_uppercase().contains("VARA") { "vara-ok" } else { "socket-not-vara" };
    VaraProbeDto { classification: classification.into(), banner: if banner.is_empty() { None } else { Some(banner) } }
}
```

- [ ] **Step 4: Wire the DTO + trait + real impl + mock + tool.**
  - `ports.rs`: add
    ```rust
    /// Read-only VARA deep-probe result: `classification` ∈ {"down","socket-not-vara","vara-ok"}.
    /// (Match the sibling output DTOs `VaraStatusDto`/`ModemStatusDto` — they do NOT derive
    /// `schemars::JsonSchema`; only tool INPUT `Params` structs do.)
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct VaraProbeDto { pub classification: String, pub banner: Option<String> }
    ```
    and add to `StatusPort`: `async fn vara_probe(&self) -> Result<VaraProbeDto, PortError>;`
  - `mcp_ports.rs`: implement `vara_probe` — build `VaraConfig` via `build_transport_config(&config_get_vara())`, call `tokio::task::spawn_blocking(move || deep_probe(&cfg))` (it's blocking I/O) and map its `JoinError` to `PortError`.
  - `lib.rs` `MockStatus`: `async fn vara_probe(&self) -> Result<VaraProbeDto, PortError> { Ok(VaraProbeDto { classification: "down".into(), banner: None }) }`
  - `router.rs`: new tool after `vara_status`:
    ```rust
    #[tool(name = "vara_probe", description = "Deep READ-ONLY probe of the VARA modem: connect the command port and read its startup banner / VERSION reply. Returns classification: \"down\" (no TCP), \"socket-not-vara\" (something is listening but is not VARA), or \"vara-ok\" (a real VARA answered). NEVER sends MYCALL/BW/LISTEN, never opens the data port, never transmits. Use after vara_status.reachable to confirm a real VARA before a send attempt.")]
    pub async fn vara_probe(&self) -> Result<CallToolResult, ErrorData> {
        let dto = self.state.status.vara_probe().await.map_err(port_err)?;
        Ok(CallToolResult::success(vec![ContentBlock::json(dto)?]))
    }
    ```

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(mcp): read-only vara_probe deep probe tool (Contract 1b)"
```

---

## Task 3: Persist the operator's selected connection in `Config` (schema 5→6)

**Files:**
- Modify: `src-tauri/src/config.rs` (new `SelectedConnection` struct + `Config.active_connection` field; bump `CONFIG_SCHEMA_VERSION`; update golden-set test; add additive-load test)
- Modify: `src-tauri/src/ui_commands.rs` (new `config_set_active_connection` command; expose in `config_read`/`ConfigViewDto` if that view is the read path)
- Modify: `src-tauri/src/lib.rs` (register `config_set_active_connection` in `invoke_handler`)
- Test: `config.rs` tests

**Interfaces:**
- Produces: `pub struct SelectedConnection { pub session_type: String, pub protocol: String }` (serde: fields snake_case on disk; the frontend maps its camelCase `ConnectionKey`).
- Produces: `Config.active_connection: Option<SelectedConnection>` — always-serialized (`#[serde(default)]`, serializes to `null` when unset), so it enters the golden set and REQUIRES the version bump.
- Produces: Tauri command `config_set_active_connection(session_type: String, protocol: String)`.

- [ ] **Step 1: Write the failing additive-load test** (in `config.rs` tests, mirroring `connect_host_defaults_to_cms_z_when_absent_from_config` L1765+):

```rust
#[test]
fn active_connection_defaults_to_none_when_absent_from_config() {
    // A config written before this field existed must load with active_connection = None
    // (tuxlink-ulrz additive-load: no data loss, no crash on the missing key).
    let json = config_json(2, ""); // any v>1,<CURRENT triggers MigrateAdditive
    let cfg: Config = serde_json::from_str(&json).expect("older config deserializes additively");
    assert_eq!(cfg.active_connection, None);
}

#[test]
fn active_connection_round_trips() {
    let mut cfg: Config = serde_json::from_str(&config_json(CONFIG_SCHEMA_VERSION, "")).unwrap();
    cfg.active_connection = Some(SelectedConnection { session_type: "cms".into(), protocol: "vara-hf".into() });
    let s = serde_json::to_string(&cfg).unwrap();
    let back: Config = serde_json::from_str(&s).unwrap();
    assert_eq!(back.active_connection, Some(SelectedConnection { session_type: "cms".into(), protocol: "vara-hf".into() }));
}
```

- [ ] **Step 2: (CI oracle) verify fail** — Expected: `no field active_connection` / `cannot find type SelectedConnection`, AND `config_schema_version_tracks_field_set` fails ("field set changed without bumping CONFIG_SCHEMA_VERSION").

- [ ] **Step 3: Implement the field + struct + version bump.**
  - Add near the other UI config structs in `config.rs`:
    ```rust
    /// The operator's currently-selected connection (session type + protocol),
    /// persisted so the MCP layer can report `selected` (React state / localStorage
    /// are invisible to Rust). Perception only — never triggers a connect.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct SelectedConnection {
        pub session_type: String,
        pub protocol: String,
    }
    ```
  - Add to `Config` (with the other `#[serde(default)]` fields): `#[serde(default)] pub active_connection: Option<SelectedConnection>,`
  - Bump `CONFIG_SCHEMA_VERSION` 5 → **6** (L21).
  - Update `config_schema_version_tracks_field_set` (L1680-1706): add `"active_connection"` to the `expected` vec (it is always-serialized).

- [ ] **Step 4: (CI oracle) verify the two new tests + golden-set test pass.**

- [ ] **Step 5: Implement `config_set_active_connection`** (in `ui_commands.rs`, mirroring `config_set_review_inbound` L6910-6921). NOTE: `config::read_config`/`write_config_atomic` resolve a process-global path (no dir-injection helper), so do NOT write a disk-round-trip test — the serde round-trip (Step 1) covers the field, and this command is a thin wrapper over the already-tested `write_config_atomic` (do not re-test the framework). Also expose the field for the frontend hydrate (Task 5): add `active_connection` to the `config_read` → `ConfigViewDto` output (`ui_commands.rs` ~L3463) as a read-through of `cfg.active_connection` (shape `{ session_type, protocol }` or `null`).

```rust
#[tauri::command]
pub async fn config_set_active_connection(
    state: State<'_, BackendState>,
    session_type: String,
    protocol: String,
) -> Result<(), UiError> {
    let mut cfg = config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    cfg.active_connection = Some(config::SelectedConnection { session_type, protocol });
    config::write_config_atomic(&cfg).map_err(|e| UiError::Internal { detail: e.to_string() })?;
    if let Some(backend) = state.current() { backend.set_config(cfg); }
    Ok(())
}
```
Register it in `src-tauri/src/lib.rs` `invoke_handler![…]` alongside the other `config_set_*` commands.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(config): persist selected connection; bump schema 5->6 (Contract 2, ulrz-safe)"
```

---

## Task 4: `modem_get_status` reports `selected` + `running` with SoT `kind`

**Files:**
- Modify: `src-tauri/tuxlink-mcp-core/src/ports.rs` (`ModemStatusDto` widen; `RunningModemDto`, `SelectedConnectionDto`)
- Modify: `src-tauri/tuxlink-mcp-core/src/lib.rs` (`MockStatus::modem_status`)
- Modify: `src-tauri/src/mcp_ports.rs` (`modem_status` impl — extract to `gather_modem_status` + pure `derive_modem_status`; SoT derivation)
- Modify: `src-tauri/tuxlink-mcp-core/src/router.rs` (`modem_status` tool description)
- Test: `mcp_ports.rs` `#[test]` derivation tests (pure) + a gather-level trap-guard test using a real `ModemSession` + the existing `StubTransport`

**Testability decomposition (REQUIRED — `MonolithStatusPort` holds an `AppHandle` and has NO Tauri test harness; do NOT try to test the trait impl directly):**
Split into two functions in `mcp_ports.rs` so the logic is testable without booting Tauri:
```rust
// Pure — all derivation rules. Unit-tested exhaustively.
pub(crate) fn derive_modem_status(
    ardop_state: &crate::modem_status::ModemState,
    ardop_transport_present: bool,
    vara_state: &crate::winlink::modem::vara::commands::VaraState,
    selected: Option<SelectedConnectionDto>,
) -> ModemStatusDto { /* rules below */ }

// Session-taking — the real gathering seam. Trap-guarded with a real ModemSession.
// `selected` is passed in (NOT &Config) because `Config` does NOT impl Default —
// the codebase uses `read_config().ok()`, so the config read lives in the trait impl.
pub(crate) fn gather_modem_status(
    modem: &crate::modem_status::ModemSession,
    vara: &crate::winlink::modem::vara::VaraSession,
    selected: Option<SelectedConnectionDto>,
) -> ModemStatusDto {
    let ms = modem.status_snapshot();
    derive_modem_status(&ms.state, modem.snapshot_transport_present(), &vara.snapshot().state, selected)
}
```
The trait impl becomes:
```rust
let selected = crate::config::read_config().ok()
    .and_then(|c| c.active_connection)
    .map(|s| SelectedConnectionDto { session_type: s.session_type, protocol: s.protocol });
Ok(gather_modem_status(&modem_session, &vara_session, selected))
```
**`gather_modem_status` MUST read ARDOP liveness from `modem.snapshot_transport_present()`, NEVER `modem.active_transport_kind()`** — the trap-guard test proves it. (The config→`selected` `.map()` glue in the trait impl is one line; its inputs are covered by Task 3's serde round-trip and Task 4's `derive_*` selected tests.)

**Interfaces:**
- Consumes: `Config.active_connection` (Task 3); `ModemSession` (`snapshot_transport_present`, `status_snapshot().state`); `VaraSession::snapshot().state`.
- Produces: widened
  ```rust
  pub struct ModemStatusDto {
      pub kind: String,          // SoT: primary running modem kind, "idle" when none
      pub connected: bool,       // unchanged meaning; now pairs with the SoT kind
      pub state: String,         // primary running session's state, "idle" when none
      pub running: Vec<RunningModemDto>,          // NEW: every live session
      pub selected: Option<SelectedConnectionDto>, // NEW: persisted operator target
      pub conflict: bool,        // NEW: true when >1 modem running (convention forbids, code allows)
  }
  pub struct RunningModemDto { pub kind: String, pub state: String }
  pub struct SelectedConnectionDto { pub session_type: String, pub protocol: String }
  ```

**Derivation rules (exact):**
- **ARDOP running** ⇔ `modem_session.snapshot_transport_present() || !matches!(state, ModemState::Stopped | ModemState::Error)`. `state` from `modem_get_status_inner(session.inner())`. Do **NOT** use `active_transport_kind()` — the live `modem_ardop_connect` path leaves it `None` (only the unused `ardop_open_session` sets it), so it returns idle for a live session (coverage trap). `SocketLost` ⇒ ARDOP counts as running (degraded), so `running` lists it (`state:"socketlost"`).
- **VARA running** ⇔ `!matches!(vara_state, VaraState::Closed | VaraState::Error)` (Open/Connecting/SocketLost). `vara_state` from `vara_session.snapshot().state`.
- **`running` list order (fixed tie-break):** push ARDOP first, then VARA. Document that in a genuine conflict the agent consults `running` + `conflict` + `selected`; `kind`/`state` reflect `running[0]`.
- **`kind`** = `running.first().map(|r| r.kind).unwrap_or("idle")`. Never falls back to `selected` (that would re-introduce a false-positive against `connected`).
- **`state`** = `running.first().map(|r| r.state).unwrap_or("idle")`.
- **`connected`** pairs with `kind`: `"ardop"` ⇒ `matches!(ardop_state, ConnectedIrs|ConnectedIss)`; `"vara-hf"` ⇒ `matches!(vara_state, Open)`; else `false`.
- **`conflict`** = `running.len() > 1`.
- **`selected`** = `read_config().ok().and_then(|c| c.active_connection).map(|s| SelectedConnectionDto { session_type: s.session_type, protocol: s.protocol })`.
- **Vocabulary bridge:** the running `kind` string for ARDOP is `"ardop"` (matches the backend `TransportKind::Ardop` wire form + the existing DTO history); VARA is `"vara-hf"`.

- [ ] **Step 1a: Write the failing pure-derivation tests** (in `mcp_ports.rs` `#[cfg(test)] mod tests`, sync `#[test]`). Import `ModemState`, `VaraState`. These exhaustively cover the rules:

```rust
use crate::modem_status::ModemState;
use crate::winlink::modem::vara::commands::VaraState;
use super::{derive_modem_status, SelectedConnectionDto};

#[test]
fn derive_idle_when_nothing_running() {
    let dto = derive_modem_status(&ModemState::Stopped, false, &VaraState::Closed, None);
    assert_eq!(dto.kind, "idle");
    assert_eq!(dto.state, "idle");
    assert!(!dto.connected);
    assert!(dto.running.is_empty());
    assert!(!dto.conflict);
}

#[test]
fn derive_ardop_running_from_transport_present() {
    // The trap in pure form: a live ARDOP session = transport present, state may lag.
    let dto = derive_modem_status(&ModemState::ConnectedIss, true, &VaraState::Closed, None);
    assert_eq!(dto.kind, "ardop");
    assert!(dto.connected); // ConnectedIss pairs with kind=ardop
    assert!(dto.running.iter().any(|r| r.kind == "ardop"));
}

#[test]
fn derive_vara_running_and_connected_pairing() {
    let dto = derive_modem_status(&ModemState::Stopped, false, &VaraState::Open, None);
    assert_eq!(dto.kind, "vara-hf");
    assert!(dto.connected); // VaraState::Open pairs with kind=vara-hf
}

#[test]
fn derive_socketlost_ardop_is_running_but_not_connected() {
    let dto = derive_modem_status(&ModemState::SocketLost, true, &VaraState::Closed, None);
    assert!(dto.running.iter().any(|r| r.kind == "ardop"));
    assert!(!dto.connected); // degraded: running, not connected
}

#[test]
fn derive_conflict_when_both_running() {
    let dto = derive_modem_status(&ModemState::ConnectedIss, true, &VaraState::Open, None);
    assert!(dto.conflict);
    assert_eq!(dto.running.len(), 2);
    assert_eq!(dto.kind, "ardop"); // fixed tie-break: ARDOP first
}

#[test]
fn derive_selected_never_leaks_into_kind_when_idle() {
    let sel = Some(SelectedConnectionDto { session_type: "cms".into(), protocol: "vara-hf".into() });
    let dto = derive_modem_status(&ModemState::Stopped, false, &VaraState::Closed, sel);
    assert_eq!(dto.selected.unwrap().protocol, "vara-hf");
    assert_eq!(dto.kind, "idle"); // NOT "vara-hf" — no false-positive against `connected`
    assert!(!dto.connected);
}
```

- [ ] **Step 1b: Write the failing gather-level trap-guard test** (the k61j composed-seam guard — uses the REAL `ModemSession` + config, catching a wrong `active_transport_kind` source that the pure test cannot):

```rust
#[test]
fn gather_sources_ardop_from_transport_not_active_kind() {
    use crate::modem_status::ModemSession;
    use crate::winlink::modem::vara::VaraSession;
    // Mirror exactly what modem_ardop_connect does (install_transport at L534,
    // and it NEVER calls set_active_session_mode):
    let modem = ModemSession::new();
    modem.install_transport(Box::new(StubTransport)); // StubTransport already exists in modem_status.rs tests
    assert!(modem.snapshot_transport_present());
    assert_eq!(modem.active_transport_kind(), None, "connect path leaves this None — must not be the source");

    let dto = super::gather_modem_status(&modem, &VaraSession::new(), None);
    assert_eq!(dto.kind, "ardop", "a wrong impl reading active_transport_kind would return 'idle' here");
}

#[test]
fn gather_passes_selected_through() {
    use crate::modem_status::ModemSession;
    use crate::winlink::modem::vara::VaraSession;
    let sel = Some(SelectedConnectionDto { session_type: "cms".into(), protocol: "vara-hf".into() });
    let dto = super::gather_modem_status(&ModemSession::new(), &VaraSession::new(), sel);
    assert_eq!(dto.selected.unwrap().protocol, "vara-hf");
}
```
(`StubTransport` is the zero-field fake at `modem_status.rs:1638`; if it is `mod`-private to that test module, add a minimal identical `struct StubTransport; impl ModemTransport for StubTransport {…}` in the `mcp_ports.rs` test module — its `drain_status_events` is a no-op.)

- [ ] **Step 2: (CI oracle) verify fail.**

- [ ] **Step 3: Widen the DTO** in `ports.rs` (add the three fields + two structs above). Derive `schemars::JsonSchema` on the new structs if the DTO carries it.

- [ ] **Step 4: Implement `derive_modem_status` + `gather_modem_status` + thin trait impl** in `mcp_ports.rs` following the exact rules above. The trait impl `modem_status` (replace L194-211 body) becomes: read `Arc<ModemSession>` and `Arc<VaraSession>` from `self.app.state::<>()` (as `vara_status` does at L214-216), then `Ok(gather_modem_status(&modem_session, &vara_session, &crate::config::read_config().unwrap_or_default()))`. Put `derive_modem_status`/`gather_modem_status` at module level (not inside the impl) so tests reach them via `super::`.

- [ ] **Step 5: Update the mock** `MockStatus::modem_status` in `lib.rs` to the new shape (keep it honest — idle):
```rust
async fn modem_status(&self) -> Result<ModemStatusDto, PortError> {
    Ok(ModemStatusDto { kind: "idle".into(), connected: false, state: "idle".into(),
        running: vec![], selected: None, conflict: false })
}
```

- [ ] **Step 6: Update the router description** (`router.rs` `modem_status` tool): document `selected` vs `running`, `kind`=running (honest idle), and `conflict`.

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "feat(mcp): modem_status reports selected + running with SoT kind (Contract 2)"
```

---

## Task 5: Frontend — track & persist the selected connection; kind-aware panel

**Files:**
- Modify: `src/modem/useModemStatus.ts` (new `useActiveModemMode` selector + `connectionToPanelMode` mapper)
- Modify: `src/shell/AppShell.tsx` (replace the `activeModem` hardcode L846-849; add persist effect; hydrate on mount)
- Test: `src/modem/useModemStatus.test.ts` (or colocated) — Vitest, RUNS LOCALLY.

**Interfaces:**
- Consumes: `ConnectionKey = {sessionType, protocol}`; `useModemIsActive` (deduped liveness bool); Tauri `config_set_active_connection` (Task 3), `config_read` (for hydrate).
- Produces: `export function connectionToPanelMode(conn: ConnectionKey): RadioPanelMode | null` — maps `protocol` `'ardop-hf'→{kind:'ardop-hf'}`, `'vara-hf'→{kind:'vara-hf'}`, others → `null`; `intent` from `sessionType` (`'cms'|'p2p'|'radio-only'`, else `'cms'`).
- Produces: `export function useActiveModemMode(active: ConnectionKey): RadioPanelMode | null` — returns `connectionToPanelMode(active)` only while the modem is live (`useModemIsActive`), else `null`; memoized so it changes only when liveness flips or `active` changes (never at the 4 Hz broadcaster cadence).

- [ ] **Step 1: Write the failing Vitest tests** (`src/modem/useModemStatus.test.ts`):

```ts
import { describe, it, expect } from 'vitest';
import { connectionToPanelMode } from './useModemStatus';

describe('connectionToPanelMode', () => {
  it('maps vara-hf selection to a vara-hf panel mode', () => {
    expect(connectionToPanelMode({ sessionType: 'cms', protocol: 'vara-hf' }))
      .toEqual({ kind: 'vara-hf', intent: 'cms' });
  });
  it('maps ardop-hf selection to an ardop-hf panel mode', () => {
    expect(connectionToPanelMode({ sessionType: 'p2p', protocol: 'ardop-hf' }))
      .toEqual({ kind: 'ardop-hf', intent: 'p2p' });
  });
  it('returns null for non-radio protocols', () => {
    expect(connectionToPanelMode({ sessionType: 'cms', protocol: 'telnet' })).toBeNull();
    expect(connectionToPanelMode({ sessionType: 'cms', protocol: 'packet' })).toBeNull();
  });
});
```

- [ ] **Step 2: Run locally, verify fail**

Run: `cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-7ppfq-perception && npx vitest run src/modem/useModemStatus.test.ts`
Expected: FAIL — `connectionToPanelMode is not a function`.

- [ ] **Step 3: Implement `connectionToPanelMode` + `useActiveModemMode`** in `useModemStatus.ts`:

```ts
import type { ConnectionKey } from '../connections/sessionTypes';
import type { RadioPanelMode } from '../radio/types';
import { useMemo } from 'react';

export function connectionToPanelMode(conn: ConnectionKey): RadioPanelMode | null {
  const intent = (conn.sessionType === 'p2p' || conn.sessionType === 'radio-only')
    ? conn.sessionType : 'cms';
  if (conn.protocol === 'vara-hf') return { kind: 'vara-hf', intent };
  if (conn.protocol === 'ardop-hf') return { kind: 'ardop-hf', intent };
  return null;
}

/** Panel mode for the active connection, only while a modem is live. Deduped:
 *  re-derives only when liveness flips or `active` changes — not at 4 Hz. */
export function useActiveModemMode(active: ConnectionKey): RadioPanelMode | null {
  const isActive = useModemIsActive();
  return useMemo(
    () => (isActive ? connectionToPanelMode(active) : null),
    [isActive, active.sessionType, active.protocol],
  );
}
```

- [ ] **Step 4: Run locally, verify pass**

Run: `npx vitest run src/modem/useModemStatus.test.ts`
Expected: PASS.

- [ ] **Step 5: Write the failing persist/hydrate test** (`src/shell/…` or a focused hook test) mocking `@tauri-apps/api/core` `invoke`. Assert: (a) changing `activeConnection` invokes `config_set_active_connection` with `{ sessionType, protocol }`; (b) on mount, a mocked `config_read` returning `{ active_connection: { session_type:'cms', protocol:'vara-hf' } }` hydrates `activeConnection` to `vara-hf`. Follow the existing AppShell test harness / invoke-mock convention (mocks are called no-args at teardown — see the project vitest cleanup pitfall).

- [ ] **Step 6: Replace the `activeModem` hardcode + add persistence** in `AppShell.tsx`:
  - Replace L846-849 with `const activeModem = useActiveModemMode(activeConnection);`
  - Add a persist effect (captures BOTH writers — `onSelectConnection` AND the status-driven effect — per the hoi1 multi-writer lesson; gate on a hydration flag so the mount-hydrate value isn't immediately re-persisted):
    ```tsx
    const hydratedRef = useRef(false);
    useEffect(() => {
      if (!hydratedRef.current) return; // don't persist until initial hydrate completes
      void invoke('config_set_active_connection', {
        sessionType: activeConnection.sessionType,
        protocol: activeConnection.protocol,
      }).catch(() => { /* perception persistence is best-effort */ });
    }, [activeConnection]);
    ```
  - Hydrate on mount:
    ```tsx
    useEffect(() => {
      let cancelled = false;
      invoke<ConfigViewShape>('config_read').then((cfg) => {
        const sel = cfg?.active_connection;
        if (!cancelled && sel) {
          setActiveConnection({ sessionType: sel.session_type as SessionTypeId, protocol: sel.protocol as ProtocolId });
        }
      }).catch(() => {}).finally(() => { hydratedRef.current = true; });
    }, []);
    ```
    (Use the actual `config_read` return type / field name in the codebase; if `config_read`→`ConfigViewDto` does not surface `active_connection`, add it there as a read-through in Task 3 Step 5 instead. **Match `ConfigViewDto`'s serde case convention** — if it is `#[serde(rename_all="camelCase")]`, the wire field is `activeConnection: { sessionType, protocol }` and the hydrate reads `sel.sessionType`; if snake_case, `sel.session_type`. Read the DTO's derive before writing the hydrate accessor.)

- [ ] **Step 7: Run the full frontend gate locally**

Run: `npx vitest run src/modem src/shell && npx tsc --noEmit -p tsconfig.json`
Expected: PASS (tsc parity per TEST-1 — no `node:fs`, no shadow-CI test).

- [ ] **Step 8: Commit**

```bash
git add -A && git commit -m "feat(ui): kind-aware active-modem panel + persist selected connection (Contract 2 frontend)"
```

---

## Task 6: Integration, CI, and wire-walk gate

- [ ] **Step 1:** Push the branch; open the PR against `main` (title `[esker-wren-towhee] tuxlink-7ppfq: VARA reachability + active-modem SoT (perception)`). CI compiles both crates on amd64+arm64 (clippy `--all-targets`, full test suites) — this is the Rust compile/verify oracle.
- [ ] **Step 2:** Drive CI to green; fix-forward on failures (no `--no-verify`, no local `--locked` masking).
- [ ] **Step 3:** Post-implementation adversarial pass on the diff (Codex review, `git diff origin/main..HEAD`) — a code review of the *implementation* (design adrev is already done; this is not a re-adrev). Disposition findings.
- [ ] **Step 4:** **Wire-walk gate** (reachability check, no on-air): confirm the new tools are reachable end-to-end — `vara_status.reachable`/`vara_probe` classify correctly against a real/absent VARA cmd port, and `modem_get_status` reports `selected` tracking a UI selection and `running` tracking a real ARDOP connect. On-air validation stays operator-only (RADIO-1 / ADR 0018). Do NOT claim "done" before this gate.

---

## Self-Review

**Spec coverage:** Contract 1 reachable → Task 1; Contract 1 `vara_probe` read-only → Task 2; Contract 2 `selected` persistence → Task 3; Contract 2 `running`/`kind` SoT (with the `active_transport_kind` coverage trap) → Task 4; Contract 2 frontend selector + two-writer persistence + hydrate → Task 5; CONFIG_SCHEMA_VERSION 5→6 + golden-set + additive-load (ulrz) → Task 3; vocabulary bridge (`ardop` vs `ardop-hf`) → Tasks 4/5. `vara_start` explicitly OUT (u269g). Egress-lock / RADIO-1 untouched (Global Constraints).

**Type consistency:** `VaraStatusDto.reachable: Option<bool>`; `VaraProbeDto {classification, banner}`; `ModemStatusDto {kind, connected, state, running: Vec<RunningModemDto>, selected: Option<SelectedConnectionDto>, conflict}`; `SelectedConnection {session_type, protocol}` (config) vs `SelectedConnectionDto {session_type, protocol}` (MCP) — deliberately distinct types, same field names. `connectionToPanelMode`/`useActiveModemMode` names match across Task 5 steps.

**Pitfalls folded in:** k61j composed-seam → Task 4 Step 1 drives the real `modem_ardop_connect` + config→MCP seam; hoi1 multi-writer clobber → Task 5 persist effect hooks the `activeConnection` transition (both writers), hydration-flag guards the rollback/mount re-persist; testing §5 TOCTOU → Task 1 contended-lock test; testing §6 boundary → Task 1 timeout tests; ulrz schema trap → Task 3; TEST-1 tsc parity → Task 5 Step 7.
