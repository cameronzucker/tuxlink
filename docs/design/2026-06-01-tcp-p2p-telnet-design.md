# TCP P2P Telnet (Winlink) — design spec

> **Status:** design, pending operator review → `writing-plans`.
> **Date:** 2026-06-01 · **Agent:** larch-clover-delta · **bd issue:** `tuxlink-0pnb`.
> **WLE behavior ground-truth:** [`dev/scratch/winlink-re/findings/p2p-telnet.md`](../../dev/scratch/winlink-re/findings/p2p-telnet.md)
> (ILSpy 8.2 decompile of `RMS Express.exe`; raw source local under
> `dev/scratch/winlink-re/decompiled/`). This spec is the *design*; that doc is
> the *evidence*.
> **Rides on:** the session-type-selector UI architecture
> ([`docs/plans/2026-05-22-session-type-selector.md`](../plans/2026-05-22-session-type-selector.md)),
> which already models `p2p+telnet` as an unbuilt cell in the connection matrix.
> The B2F protocol layer (`winlink/session.rs`, `winlink/handshake.rs`,
> `winlink/transfer.rs`, `winlink/proposal.rs`) is reused unchanged.

## 1. Goal & scope

Add raw-TCP B2F as a Winlink P2P transport in tuxlink's native client, so an
operator can exchange real Winlink mail (with attachments) against their
Windows-side Winlink Express instance configured as a Telnet-P2P listener (or in
Post Office mode). WLE then acts as the registered-SID intermediary that
forwards to/from CMS — solving the production-CMS unknown-SID rejection
([`[[cms-rejects-unknown-clients]]`](../../.claude/projects/-home-administrator-Code-tuxlink/memory/project_cms_rejects_unknown_clients.md))
*and* providing the first **real end-to-end smoke target** for the native
client (no synthetic harness, no CMS-dev workarounds).

**In scope (v0.1):**
- **Client-dial transport:** tuxlink connects to a peer's listening TCP port,
  runs the WLE telnet-login wrapper (answering optional password prompt), then
  the standard B2F session as dialer-master.
- **Listener transport:** tuxlink binds a configurable port, accepts incoming
  connections, runs the telnet-login wrapper (issuing optional password prompt
  if configured) + allowed-stations gate, then the standard B2F session as
  listener-slave.
- **Allowed-stations enforcement:** callsign + IP allowlist with `*` wildcards.
- **Optional station-password layer:** symmetric — listener may prompt; dialer
  may answer. Keyring-stored, never in plaintext config.
- **Attachments + bidirectional mail:** both directions in one session, full
  B2F surface including binary attachments (the session driver already does
  this for CMS — same code path).
- **UI surface:** populate the `p2p+telnet` cell in the existing
  session-type-selector matrix, mirroring the Telnet-CMS pane's structure.

**Out of scope (deferred):**
- B2F sessions over modems (ARDOP/VARA/Packet) in P2P mode — the protocol
  surface is the same but the transport is different; separate bd issues.
- Auto-poll scheduler (the `AutoConnect Time` WLE concept). Tuxlink ships with
  operator-triggered sessions only; auto-poll is a separate UX surface.
- Cross-shack / internet-exposed listener with cert-based auth. Defaults
  bind-loopback; LAN/WAN exposure is operator-opt-in; cert-based auth (rather
  than the WLE-compat plaintext-password) is a v0.2+ consideration.
- Replicating WLE's Telnet-RMS / Iridium GO / AREDN MESH variants — those are
  CMS routes via specialized transports, not P2P.

## 2. Operational modes (ground-truthed)

Tuxlink can act as **dialer-master** (initiating outbound) or **listener-slave**
(accepting inbound). The B2F session primitives are reused; the orchestration
role differs.

| Mode | tuxlink role (FBB) | Who speaks first | Auth source |
|---|---|---|---|
| **Dial P2P peer** | master (dialer) | peer (listener) sends `CALLSIGN :` prompt; B2F runs after login | peer's RF callsign + optional shared password |
| **Listen for P2P peer** | slave (listener) | **tuxlink** (issues `CALLSIGN :` + optional `Password :` prompt; then peer sends B2F handshake) | dialer's RF callsign + optional configured password |

**Telnet-login wrapper** (WLE-compat, evidence: `TelnetP2PSession.cs:1252-1340`):

The wrapper runs BEFORE the B2F `[NAME-VERSION-CODES]` exchange:

```
listener → dialer:   CALLSIGN :\r
dialer   → listener: <DIALER-CALLSIGN>\r
listener → dialer:   Password :\r       ← optional, only if listener has password configured
dialer   → listener: <PASSWORD>\r        ← optional, plaintext, case-sensitive
listener → dialer:   <B2F handshake — [RMS-EXPRESS-1.7.18.0-B2FHM$] etc.>
dialer   → listener: <B2F handshake — [TUXLINK-0.0.1-B2FHM$] etc.>
...standard B2F follows...
```

Notes:
- The listener decides whether to prompt for password based on its own config
  (per `TelnetP2PSession.cs:1299`: `if (strStationPassword != "")` — non-empty
  triggers the prompt).
- Allowed-stations gate applies AFTER the dialer's callsign is received, BEFORE
  any B2F handshake. Failed match → close connection.
- After the wrapper, the **listener** is in B2F-slave role (server speaks first
  in the existing `run_exchange` model — which is correct: listener emits the
  B2F handshake first).

## 3. Architecture (layering)

```
⑥ UI + config        (NEW — session-type-selector cell: p2p+telnet pane;
                       Settings → P2P-Telnet sub-section: listener port,
                       bind IP, allowlist editor, optional station-password,
                       transport visibility per the established anti-pattern)
⑤ Orchestration      (NEW — TransportConfig::TelnetP2P { mode: Dial | Listen,
                       peer: PeerEndpoint, ... }; lifecycle + abort)
④ B2F session        (REUSE run_exchange (dialer-master path) + listener-slave
                       path that already exists for the CMS-secure-login mock
                       tests in session.rs)
③ Telnet-login       (NEW — winlink/telnet_p2p_login.rs: CALLSIGN/Password
   wrapper             prompt issuer (listener) + answerer (dialer))
② Allowlist gate     (NEW — winlink/p2p_allowlist.rs: parse + match callsigns/IPs
                       with * wildcards)
① TCP transport      (NEW — winlink/telnet_p2p.rs: connect() / bind+accept();
                       sibling to telnet.rs; presents Read + Write to session)
```

The Telnet-login wrapper and Allowlist gate live BETWEEN the raw TCP socket and
`run_exchange`. They consume bytes off the socket until B2F begins, then hand
off the (possibly unwrapped) byte stream to the session driver.

## 4. Component design

### 4.1 `telnet_p2p.rs` (sibling to `telnet.rs`)

Two entry points, one struct.

**Outbound (dialer-master):**
```rust
pub fn connect_and_exchange(
    config: &P2pTelnetConfig,
    creds: &P2pPeerCredentials,
    outbox: Vec<OutboundMessage>,
    sink: impl FnMut(ReceiveOutcome),
) -> Result<ExchangeResult, P2pTelnetError> {
    // 1. Resolve + connect TCP (CONNECT_TIMEOUT + CONNECT_TOTAL_DEADLINE
    //    bounded — reuse the constants from telnet.rs).
    // 2. Run telnet-login wrapper as dialer (answer CALLSIGN: + optional Password:).
    // 3. Hand the (Read, Write) halves to session::run_exchange_with_role(Master, ...).
    // 4. Return ExchangeResult.
}
```

**Inbound (listener-slave):**
```rust
pub fn listen_and_exchange(
    config: &P2pListenerConfig,
    allowlist: &AllowedStations,
    station_password: Option<&str>,
    accept_handler: impl FnMut(IncomingSession) -> SessionDisposition,
) -> Result<(), P2pTelnetError> {
    // 1. Bind TCP on (config.bind_ip, config.port) — refuse if non-loopback bind
    //    is not explicitly opt-in.
    // 2. accept() loop on a worker thread.
    // 3. For each accepted socket:
    //    a. Run telnet-login wrapper as listener:
    //       - Emit "CALLSIGN :\r".
    //       - Read peer callsign line.
    //       - Match against allowlist (callsign list + accepted-IP list).
    //         Fail → log + close.
    //       - If station_password is Some, emit "Password :\r", read response,
    //         constant-time compare. Fail → log + close.
    //    b. Hand off to session::run_exchange_with_role(Slave, ...).
    //    c. Notify accept_handler with outcome.
}
```

Both use the same `BoundedTcpStream` wrapper as `telnet.rs` for per-read/write
timeouts (`TIMEOUT = 60s`) and abort-via-shutdown.

### 4.2 Telnet-login wrapper (`telnet_p2p_login.rs`)

Two pure functions plus an `IoStream` adapter:

```rust
pub fn dialer_login<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    our_callsign: &str,
    password: Option<&str>,
) -> Result<(), LoginError> {
    // Read until line ending in "CALLSIGN :" (case-insensitive, tolerant of
    // surrounding whitespace, with a small read budget).
    // Send "<our_callsign>\r".
    // Peek one line. If it ends in "Password :" → require Some(password), send "<password>\r".
    //   Else: that line is the start of the B2F handshake — push it back so the session driver sees it.
}

pub fn listener_login<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    allowlist: &AllowedStations,
    peer_ip: IpAddr,
    expected_password: Option<&str>,
) -> Result<PeerIdentity, LoginError> {
    // Emit "CALLSIGN :\r".
    // Read peer callsign line (TIMEOUT bounded).
    // allowlist.match_callsign(...) AND allowlist.match_ip(peer_ip) — operator
    // decides AND vs OR via config; default AND for defense-in-depth.
    // If expected_password.is_some():
    //   Emit "Password :\r".
    //   Read response. constant_time_eq against expected_password.
    // Return PeerIdentity { callsign, peer_ip }.
}
```

Key choices:
- Constant-time compare for password matching (`subtle::ConstantTimeEq` or
  equivalent) — defense against timing side-channels. WLE uses
  `string.Compare(..., ignoreCase: false)` which is NOT constant-time; this is
  a quiet improvement.
- "Peek a line and push back" is the awkward bit. Implementation: wrap the
  reader in a `Peekable<BufRead>` adapter that prepends an unread buffer; the
  session driver consumes from the same adapter so no bytes are lost.
- All log lines (callsign, IP) are emitted to the operator-visible session log,
  not to a hidden file. Failed allowlist match logs as `<callsign>@<ip> rejected`.

### 4.3 Allowlist (`winlink/p2p_allowlist.rs`)

```rust
pub struct AllowedStations {
    pub callsign_entries: Vec<CallsignPattern>,    // "N7CPZ", "N7*", "K?ABC", ...
    pub ip_entries: Vec<IpPattern>,                // "192.168.1.50", "192.168.*", "10.*.*.*"
    pub allow_all: bool,                            // OPERATOR-OPT-IN — default false (diverges from WLE)
}

impl AllowedStations {
    pub fn matches(&self, callsign: &str, peer_ip: IpAddr) -> AllowDecision { ... }
    pub fn load_from_file(path: &Path) -> Result<Self, ...> { ... }
}

pub enum AllowDecision {
    Allowed,
    Rejected { reason: &'static str },  // "not on callsign list" / "not on IP list" / "no entries configured + allow_all=false"
}
```

**Wildcard semantics** (verified against WLE decompile, simplified):
- `*` matches any sequence of letters/digits within a token.
- Callsign patterns are case-insensitive and SSID-aware: `N7CPZ` matches
  `N7CPZ`, `N7CPZ-7`, `N7CPZ-15`. To restrict to a specific SSID, use
  `N7CPZ-7` literally.
- IP patterns are octet-wise: `192.168.*` matches `192.168.0.1` through
  `192.168.255.255`. `*` MAY appear in any octet position.
- File format: one entry per line, comments with `#`, blank lines OK. Stored at
  `<config-dir>/p2p-telnet-allowed-stations.txt` (tuxlink config dir, not
  `Globals.strDataDirectory`).

### 4.4 Station password storage

Keyring-backed per [`[[no-disk-creds-default]]`](../../.claude/projects/-home-administrator-Code-tuxlink/memory/feedback_no_disk_creds_default.md):

```rust
pub struct P2pStationCredentials {
    // Listener: the password we challenge incoming peers with. None = no challenge issued.
    pub listener_password: Option<String>,
    // Per-peer: passwords to send when DIALING specific peers. Keyed by callsign.
    pub peer_passwords: HashMap<String, String>,
}
```

Storage (via the existing `winlink/credentials.rs` keyring abstraction PR
#191's predecessors landed):

```
keyring service: "tuxlink"
keys:
  "p2p-listener-password"        → listener-side challenge password
  "p2p-peer:<CALLSIGN>"          → outbound password for that peer
```

UI never displays the stored value (typical password-input pattern: show
`<set>` / `<not set>` indicator, plus "change" / "clear" actions).

**Divergence from WLE:** WLE stores plaintext in `RMS Express.ini` under
`[<CallsignAndQualifier>]` `Telnet P2P Station Password`. Tuxlink uses the
OS keyring. The wire-protocol behavior is identical (same prompt sequence,
same plaintext-on-wire token); only the at-rest storage differs.

### 4.5 UI surface

Slots into the unbuilt `p2p+telnet` cell of the session-type-selector matrix
([`docs/plans/2026-05-22-session-type-selector.md:107-131`](../plans/2026-05-22-session-type-selector.md#L107-L131)).
Mirrors the structure of the Telnet-CMS pane (Task 4 in that plan).

**Pane layout** (`src/connections/TelnetP2pPanel.tsx`):

```
┌─ P2P → Telnet ─────────────────────────────────────────────┐
│                                                             │
│  Mode: ◉ Dial peer  ◯ Listen for peer                       │
│                                                             │
│  ─── if Dial mode ───                                       │
│  Peer host: [127.0.0.1                ]  Port: [8772]       │
│  Peer callsign: [N7CPZ                ]                     │
│  Password: [Set] [Clear]    Status: <set>                   │
│  [Connect]  [Abort]                                         │
│                                                             │
│  ─── if Listen mode ───                                     │
│  Listening on: [127.0.0.1 (loopback)] ▾  Port: [8772]       │
│      ⚠ Loopback only — only same-machine peers can connect. │
│      [ Change to LAN ]  [ Change to all interfaces ⚠ ]      │
│  My station password: [Set] [Clear]    Status: <not set>    │
│  Allowed stations: 3 callsigns, 1 IP  [Edit allowlist]      │
│  [Start listener]  [Stop listener]                          │
│                                                             │
│  ─── Session log ────────────────────────────────────────── │
│  (shared with other connection panes; chronological)        │
└─────────────────────────────────────────────────────────────┘
```

Key UI principles:
- **Per the transport-visibility anti-pattern**
  ([`docs/ux-anti-patterns.md:65-76`](../ux-anti-patterns.md#L65-L76)):
  the listener-IP choice is **always visible**, never auto-selected. Operator
  picks `loopback` / `<LAN-IP>` / `0.0.0.0` explicitly.
- Loopback default is annotated `(loopback)` and the UI explicitly warns when
  the operator changes it.
- Listener can be started/stopped without restarting the app. Listening state
  persists in config so it survives restart (but operator can clear).
- Allowlist editor is a sub-modal with a multi-line textarea + a "test entry"
  helper (paste a callsign+IP, see whether it would be allowed).

### 4.6 Backend wiring

`TransportConfig` (in `winlink/config.rs` or wherever the existing
`Telnet`/`Packet` variants live) gains a new variant:

```rust
pub enum TransportConfig {
    Telnet(TelnetConfig),               // existing CMS-bound
    Packet { ... },                     // existing AX.25
    TelnetP2P {
        mode: TelnetP2pMode,            // Dial | Listen
        peer: Option<PeerEndpoint>,     // dial mode
        listener: Option<ListenerConfig>, // listen mode
    },
    // ... future modes
}
```

Tauri commands:
- `telnet_p2p_dial(peer: PeerEndpoint, outbox: Vec<DraftId>) -> Result<ExchangeResult, _>`
- `telnet_p2p_listener_start(config: ListenerConfig) -> Result<(), _>`
- `telnet_p2p_listener_stop() -> Result<(), _>`
- `telnet_p2p_listener_status() -> Result<ListenerStatus, _>` (running / not running, peer count, last connection)
- `p2p_allowlist_read() -> Result<AllowedStations, _>`
- `p2p_allowlist_write(stations: AllowedStations) -> Result<(), _>`
- `p2p_station_password_set(password: Option<String>) -> Result<(), _>` (None = clear)
- `p2p_peer_password_set(callsign: String, password: Option<String>) -> Result<(), _>`

## 5. Defaults & divergences from WLE

Two divergences from strict WLE parity, both improvements per the operator's
"parity unless there's a reason to improve" rule. Each is documented in this
spec AND surfaced visibly in the UI (transport-visibility anti-pattern), so
operators familiar with WLE see the difference at a glance rather than being
surprised.

| | WLE behavior | Tuxlink behavior | Rationale |
|---|---|---|---|
| **Listener IP default** | `"Default"` (OS-chosen, all interfaces) | `127.0.0.1` (loopback) | Safer default: out-of-the-box, only same-machine peers can connect. Operator explicitly opts into LAN/all. |
| **Allowed-stations default** | `Allow All Connections = TRUE` (allowlist ignored) | `Allow All = FALSE` (allowlist enforced; empty list = no incoming) | Safer default: out-of-the-box, listener accepts no one until operator curates the list. Operator can opt into WLE-style "open" mode with a single toggle. |
| **Station password storage** | Plaintext in `RMS Express.ini` | OS keyring (Keyring on Linux, Keychain on macOS, DPAPI on Windows) | Per `[[no-disk-creds-default]]`; same wire protocol, better at-rest hygiene. |
| **Password compare** | `string.Compare(ignoreCase: false)` (variable-time) | Constant-time (`subtle::ConstantTimeEq`) | Defense against timing side-channels. Same semantic (case-sensitive exact match). |

All other defaults match WLE:

- Port 8772 plaintext (no TLS — WLE does not support TLS for P2P; verified by
  zero `SslStream` references across all P2P-mode source files).
- File format for allowlist (text, one entry per line, callsigns + IPs mixed).
- Wildcard semantics for callsign + IP entries.
- Telnet-login wrapper sequence (`CALLSIGN :` → callsign → optional
  `Password :` → password → B2F).

## 6. Out of scope (deferred to follow-up bd issues)

- **B2F over modem-P2P** (Packet P2P, ARDOP P2P, VARA P2P): Same B2F session
  surface, different transport. Each transport is its own bd issue.
- **AutoConnect scheduler**: per-transport auto-poll interval (WLE's
  `Disabled` / `15min` / ... / `24h`). Tuxlink ships operator-triggered only;
  scheduled polling is a separate UX surface that affects multiple transports
  uniformly.
- **WLE Post Office mode replication**: tuxlink-as-listener serves a single
  station's mail (its own callsign). Multi-station hub-operator mode is WLE
  capability 2.14 / 9.1; not in scope here.
- **Cross-shack / internet-facing listener with cert-based auth**: requires
  PKI / cert distribution. Plaintext-password layer is sufficient for the
  LAN/same-machine target. v0.2+.
- **wildcard-mask semantics for SSIDs explicitly**: v0.1 ships "base callsign
  matches all SSIDs" + "exact SSID-qualified matches only that SSID". More
  granular SSID-range patterns (e.g., `N7CPZ-[1-9]`) deferred.

## 7. Build phasing

Per the operator's "(2) Stacked: client-first, listener-second" choice and
"(c) bidirectional with attachments from v0.1" choice:

**PR 1 — Client-dial (peer-dial-master) + attachments**
- `winlink/telnet_p2p.rs` (connect-only)
- `winlink/telnet_p2p_login.rs` (dialer side only)
- `winlink/credentials.rs` keyring keys for `p2p-peer:<CALLSIGN>`
- `TransportConfig::TelnetP2P { mode: Dial, ... }`
- UI: TelnetP2pPanel.tsx in Dial mode only; populates `p2p+telnet` cell
- Tests:
  - Unit: dialer-login state machine (with mock peer scripts)
  - Integration: in-memory peer that emits `CALLSIGN :` / `Password :` / B2F-slave handshake / message turn / close
  - **No live-network test** — operator smokes against their WLE
- Operator smoke target: tuxlink dial → operator's WLE Telnet-P2P listener →
  round-trip a real ICS-213 + small attachment ← this is the **e2e validation
  that addresses the methodology gap from this morning's component-only smoke
  gate**.

**PR 2 — Listener (P2P-listen)**
- `winlink/telnet_p2p.rs` listen path
- `winlink/telnet_p2p_login.rs` listener side
- `winlink/p2p_allowlist.rs` (read + match)
- Allowlist file persistence
- Keyring key `p2p-listener-password`
- `TransportConfig::TelnetP2P { mode: Listen, ... }` variant
- UI: TelnetP2pPanel.tsx Listen mode, allowlist editor sub-modal
- Tests:
  - Unit: listener-login state machine + allowlist matching
  - Integration: in-memory dialer client connecting through listener
  - **Operator smoke**: tuxlink listener ← WLE dials in

**PR 3 — Polish (optional, may bundle into PR 2)**
- Status indicators in the global UI (which mode active, connection count)
- Session log integration
- AppShell wiring of the dispatched pane

## 8. Tests

Per existing tuxlink conventions:

- **Cargo tests** (`src-tauri/src/winlink/`):
  - `telnet_p2p::tests::dialer_login_handles_callsign_prompt`
  - `telnet_p2p::tests::dialer_login_answers_password_when_present`
  - `telnet_p2p::tests::dialer_login_skips_password_when_listener_omits_prompt`
  - `telnet_p2p::tests::listener_login_rejects_unknown_callsign`
  - `telnet_p2p::tests::listener_login_rejects_wrong_password`
  - `telnet_p2p::tests::listener_login_uses_constant_time_compare`
  - `telnet_p2p::tests::end_to_end_round_trip_with_attachment_against_scripted_peer`
  - `p2p_allowlist::tests::wildcard_matches_callsign_prefix`
  - `p2p_allowlist::tests::wildcard_matches_ip_octet`
  - `p2p_allowlist::tests::ssid_aware_basecall_matching`
  - `p2p_allowlist::tests::allow_all_false_with_empty_list_rejects`
- **Vitest tests** (`src/connections/`):
  - `TelnetP2pPanel.test.tsx` — Dial/Listen mode toggle, controls render,
    allowlist editor opens, password set/clear flow.
- **Operator smoke** (manual, on-air-safe per RADIO-1; no RF involved):
  - PR 1: dial WLE listener, round-trip message + attachment.
  - PR 2: WLE dial tuxlink listener, round-trip message + attachment;
    verify allowlist rejects an off-list callsign.

## 9. Open items (deferred to plan/implementation)

These were identified in the decompile findings doc as questions the
ILSpy pass did not resolve. None block this design; all are
implementation-phase verifications:

- **Exact wildcard-match semantics in WLE's allowlist consumer** — my parser
  read confirmed `*` is captured as a token character, but I did not read the
  matching code itself in WLE. Tuxlink should pick reasonable semantics (above)
  and document them; if WLE-compat surprises emerge during operator smoke, we
  revisit.
- **WLE listener's tolerance for clients skipping the telnet-login wrapper** —
  i.e., can a "raw B2F" client connect directly without `CALLSIGN :` /
  `Password :`? Tuxlink-as-dialer always runs the wrapper (parity); the
  question only matters if we want a fallback path for non-WLE clients dialing
  tuxlink's listener. v0.2 consideration.
- **B2F-level differences between P2P sessions and CMS sessions in WLE's
  emission** — the handshake prefix `[RMS-EXPRESS-1.7.18.0-B2FHM$]` is the
  same; specific `;FW:` / option lines may differ. Verify during PR 1 smoke
  against real WLE.

## 10. Self-review (spec hygiene)

- **Placeholders:** none. Every section commits to a specific behavior.
- **Internal consistency:** the divergences from WLE in §5 are tabulated; the
  same divergences are reflected in the component-design sections (4.3
  `allow_all` default, 4.4 keyring storage). No conflict.
- **Scope check:** one transport, one bd issue, two PRs (client + listener) —
  appropriately scoped for a single implementation plan.
- **Ambiguity:** wildcard semantics explicitly documented in §4.3; the open
  item in §9 acknowledges the v0.1 simplification vs. WLE's possibly-different
  algorithm.

---

**Next:** operator review of this spec → on approval, invoke
`superpowers:writing-plans` to produce the task-by-task implementation plan
for PR 1 (client-dial), then for PR 2 (listener) after PR 1 lands.
