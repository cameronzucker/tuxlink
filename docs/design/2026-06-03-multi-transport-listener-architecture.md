# Multi-transport P2P listener architecture + closure-plan amendment

> Date: 2026-06-03 · Agent: thistle-swallow-cedar · bd: tuxlink-qe5q (umbrella)
>
> **Amends:** `docs/design/2026-06-02-wle-client-parity-closure-plan.md` (§2 priority table, §3.1 cross-cutting finding, §5 sequencing).
>
> **Premise correction.** The 2026-06-02 closure plan concluded "inbound P2P parity = Telnet-P2P only; Packet/ARDOP have no WLE listener to match" and dispositioned `tuxlink-inde` (Packet-P2P) + `tuxlink-dhbl` (ARDOP-P2P) as "🟠 defer/skip-listener." That conclusion conflated two layers. WLE doesn't bind an application-layer socket for Packet/ARDOP, but the **TNC and modem do listen** at the link/physical layer — Packet-P2P inbound sessions arrive through the TNC (KISS/AGW connect-indication); ARDOP-P2P inbound sessions arrive through the modem control channel (`CONNECTED <peer>` event). The receive side is a real WLE feature; the 2026-06-02 deep dive flagged "how does the operator receive an inbound Packet-P2P session in WLE" as an open item which the synthesis pass should have surfaced as "audit incomplete" rather than "feature absent." Tuxlink needs the receive side across every P2P-capable transport.

## 0. Purpose

Three things, in order:

1. **Survey what tuxlink already has** for inbound P2P handling per transport (much more than the closure plan implied).
2. **Architect** the missing pieces — shared listener-arms-consent model, allowlist + auth overlay, per-transport adapters, RADIO-1 framing for listener mode.
3. **Re-sequence** the affected bd issues + amend the closure plan.

This doc is the spec input for the implementation work tracked under `tuxlink-qe5q` and its children.

## 1. Survey — what tuxlink has today

Sourced from a focused subagent pass over `src-tauri/src/winlink/` on 2026-06-03.

### 1.1 AX.25 (Packet) — listener path SHIPPED end-to-end

- **Datalink answerer:** `winlink/ax25/datalink.rs:430` `pub fn answer(link, mycall, params)` — waits for SABM, extracts peer callsign, replies UA, returns connected stream. Production-ready.
- **B2F answerer:** `winlink/handshake.rs:65` `build_master_handshake()` + `winlink/handshake.rs:93` `read_slave_handshake()` + `winlink/session.rs:156` `run_exchange_with_role(ExchangeRole::Answer)`. Fully wired.
- **Accept-decision plumbing:** `winlink/session.rs:133` — `run_exchange_with_role(.., decide: F)` accepts a `Fn(&[Proposal]) -> Vec<Answer>` callback. Per-proposal accept/reject decisions are first-class.
- **UI surface:** `ui_commands.rs:1618` `packet_listen()` + `:1676` `packet_set_listen(enabled)` + `:1529` `packet_listen_transport_from_config()`. Tauri commands wired.
- **Backend status:** `winlink_backend.rs:146` `PacketRole::Listen` enum + `:192` resolution to `ExchangeRole::Answer` + `:800` `packet_connect_inner()` listener spawn + `BackendStatus::Listening { transport }`.

**Gap on Packet-P2P:** the tuxlink-divergence overlay (allowlist FALSE-by-default + station-password keyring) is NOT yet on top of this. Allowlist + auth is the **tuxlink-divergence-from-WLE work**, not the listener work itself.

### 1.2 ARDOP — `LISTEN TRUE` never sent

- **Init:** `winlink/modem/ardop/session.rs:264` `init_tnc()` explicitly sends `LISTEN FALSE`. Hardcoded off.
- **Async event loop:** `set_and_ack()` consumes events; no inbound `CONNECTED` handler when WE didn't dial.
- **UI:** zero. No `ardop_listen` Tauri command.

**Implementation gap:** send `LISTEN TRUE`, route inbound `CONNECTED <peer>` event to the answerer-side session, add UI command. The hard part is already done — the answerer-side B2F (`run_exchange_with_role::Answer`) and accept-decision callback are transport-agnostic.

### 1.3 VARA — `Listen(bool)` enum defined, never invoked

- **Command codec:** `winlink/modem/vara/command.rs:112-113` — `Listen(bool) => "LISTEN ON" / "LISTEN OFF"` codec entries exist.
- **Transport layer:** never calls `command::Listen(true)`.
- **UI:** zero.

**Implementation gap:** same shape as ARDOP — invoke `Listen(true)`, route inbound `CONNECTED` event, add UI command. VARA-HF + VARA-FM share this control codec; both get the listener for free once the VARA layer arms.

### 1.4 Telnet — no listener at all

- `winlink/telnet.rs` is outbound-only: `connect_and_exchange()`, `connect_stream()`, `connect_with_deadline()`, `telnet_login()`. Zero `TcpListener::bind` calls.

**Implementation gap (the only genuine NEW listener code):**
- `TcpListener::bind` per the spec in `dev/scratch/winlink-re/findings/telnet-p2p.md`: port 8774 default (NOT 8772 — that's the RMS Relay client-direction port), `"Default"` IP-bind semantics (loopback for tuxlink per the deep-dive divergence proposal), allowed-stations enforcement, station-password challenge AT the telnet-login layer (BEFORE B2F).
- Accept loop, callsign+password challenge, run `run_exchange_with_role(ExchangeRole::Answer)`.
- UI commands: `telnet_listen()`, `telnet_set_listen(enabled)`, allowed-stations editor.

### 1.5 Pactor — out of scope (commercial hardware operator-decision-gated)

Per the closure plan §4 operator-decision gates. Listener architecture should NOT block on Pactor; if Pactor ever ships, its listener path follows the same per-transport-adapter pattern.

## 2. Architecture — what we're building

The shape that falls out of the survey:

```
┌─────────────────────────────────────────────────────────────────┐
│  UI: per-transport listener toggle + allowed-stations editor    │
│      + station-password keyring entry (operator-arms-consent)   │
└──────────────────┬──────────────────────────────────────────────┘
                   │ Tauri commands
                   ▼
┌─────────────────────────────────────────────────────────────────┐
│  Shared listener-arms layer (NEW):                              │
│    - Allowlist gate (callsign + IP — applies to Telnet)         │
│    - Station-password challenge (transport-specific layer)      │
│    - Listener-arms-consent record (RADIO-1 framing)             │
└──────────────────┬──────────────────────────────────────────────┘
                   │ peer-accepted: yes/no
                   ▼
┌─────────────────────────────────────────────────────────────────┐
│  Transport adapters (per-transport listener path):              │
│    Telnet (NEW) | Packet (SHIPPED) | ARDOP (PARTIAL→COMPLETE)   │
│    VARA-HF (PARTIAL→COMPLETE) | VARA-FM (PARTIAL→COMPLETE)      │
└──────────────────┬──────────────────────────────────────────────┘
                   │ connected stream + peer identity
                   ▼
┌─────────────────────────────────────────────────────────────────┐
│  B2F answerer state machine (SHIPPED):                          │
│    handshake.rs::build_master_handshake / read_slave_handshake  │
│    session.rs::run_exchange_with_role(ExchangeRole::Answer)     │
└─────────────────────────────────────────────────────────────────┘
```

### 2.1 Shared listener-arms layer

A small library crate (or module within `src-tauri/src/winlink/`) holding:

- **`AllowedStations`** — operator-curated list. Storage: keyring or config-file (operator-decided per existing memory `no-disk-creds-default` — file is OK for non-secrets like callsigns + IPs, keyring for the per-callsign passwords). Default: empty list with `Allow All Connections: FALSE` (DIVERGES from WLE's permissive default).
- **`StationPassword`** — optional per-listener password. Storage: OS keyring (DIVERGES from WLE's plaintext INI / `.dat` file). Challenge protocol is transport-specific (Telnet does it at the telnet-login layer; Packet/ARDOP/VARA challenge before handing the stream to the B2F answerer, OR inline at B2F start).
- **`ListenerArmsRecord`** — per-arm-event metadata recording operator consent. The RADIO-1 framing: arming the listener for transport X authorizes incoming sessions on that transport until disarmed (or until a configured TTL elapses). Per `radio1-governs-tx-not-ui`, this is UX wrapping operator-consent semantics, not a Part 97 escalation.

### 2.2 Per-transport adapters

For each transport, the adapter does:

1. Place the transport in listen mode (`bind()` for Telnet; `LISTEN TRUE` for ARDOP; `Listen(true)` for VARA; nothing for Packet — AX.25 already accepts whenever the TNC is up).
2. Receive inbound-connection notification (TCP `accept()` for Telnet; modem `CONNECTED <peer>` event for ARDOP/VARA; KISS connect-indication for Packet which is already wired).
3. Run the allowlist + auth gate against the inbound peer identity.
4. If accepted, hand the connected stream to `run_exchange_with_role(ExchangeRole::Answer)`.
5. If rejected, close the connection with the appropriate transport-specific signal.

The adapter trait sketch:

```rust
trait InboundListener {
    type PeerId;
    type Stream;
    fn arm(&mut self) -> Result<()>;
    fn disarm(&mut self) -> Result<()>;
    fn next_inbound(&mut self) -> impl Stream<Item = (Self::PeerId, Self::Stream)>;
}
```

Each transport implements; the shared arms layer (allowlist + auth + RADIO-1-consent) wraps the trait.

### 2.3 UI surface

Per transport, three UI affordances:

- **Listener toggle** — on/off, with TTL ("listen for next 1 hour / until I disarm").
- **Allowed-stations editor** — separate from the toggle. Operator-curated list of callsigns (+ IPs for Telnet).
- **Station-password setter** — keyring-stored; per-listener (not per-station — same password challenges all incoming peers, matching WLE's model).

Plus a global "incoming session" indicator/notification when an inbound peer connects.

### 2.4 RADIO-1 framing

Per memory `radio1-governs-tx-not-ui`: RADIO-1 governs TX consent on click of Send/Receive, not UI. For listener mode, the operator's act of arming the listener IS the per-invocation consent for any inbound session received during the armed window. This needs to be **explicit in the UI** — not "🟢 LISTENING," but "🟢 LISTENING — arming this authorizes inbound connections to transmit (your radio will key) for the next 1 hour." The TTL is the consent boundary.

This is a UX framing, not a hook-layer enforcement. Operator memory `radio1-bounded-airtime-abort` may apply: bounded airtime + abort-before-on-air. For listener-mode the analogue is "bounded armed-window + abort-on-disarm." Worth a follow-up bd issue on the UX framing.

## 3. Closure plan amendment

The 2026-06-02 closure plan §2 priority table updates as follows. The amendment also extends Tier 1 to include the listener foundation work.

### 3.1 Re-disposition

| § | bd | Was | Is now | Rationale |
|---|---|---|---|---|
| 2.4 | tuxlink-inde | "🟠 defer/skip-listener" + P2 | **🟢 ship divergence overlay** + **P1** | Packet listener already shipped end-to-end; remaining work is the WLE-divergence overlay (allowlist + keyring password) — small scope, big operational value, ships in Tier 1. |
| 2.7 | tuxlink-dhbl | "🟠 defer/skip-listener" + P2 | **🟢 ship LISTEN TRUE flip + event routing + UI** + **P1** | ARDOP `LISTEN FALSE` is hardcoded at `session.rs:264`; flip + route inbound `CONNECTED` + add UI command. Tier 1. |
| 2.8 | tuxlink-qpqh | "🟢 ship client surface" + P2 | **🟢 ship client surface + listener (LISTEN ON)** + **P2 (unchanged)** | VARA HF listener implementation falls out of the listener foundation; ships when client-surface ships. |
| 2.9 | tuxlink-do6j | "🟢 ship client surface" + P2 | **🟢 ship client surface + listener (LISTEN ON)** + **P2 (unchanged)** | Same as 2.8 for FM. |

### 3.2 New rows added to closure plan §2

| § | Capability | bd | Priority | Disposition |
|---|---|---|---|---|
| L.1 | Shared listener-arms layer (allowlist + keyring password + RADIO-1 consent framing) | tuxlink-NEW1 | P1 | 🟢 ship — unblocks Tier 1 listener work for all transports |
| L.2 | Telnet-P2P listener (the missing piece — `TcpListener::bind` + accept loop + telnet-login layer auth) | (folded into tuxlink-xehu) | P1 unchanged | 🟢 ship — corrects the xehu scope from "completeness pass" to "completeness pass + ship the actual listener" |
| L.3 | RADIO-1 framing for listener-mode TTL + abort-on-disarm | tuxlink-NEW2 | P2 | 🟢 ship — UX guard rail per `radio1-bounded-airtime-abort` analogue |

### 3.3 Cross-cutting finding §3.1 amendment

The 2026-06-02 closure plan §3.1 said: *"WLE-parity for accepting inbound P2P maps cleanly to Telnet-P2P. For Packet-P2P and ARDOP-P2P, parity with WLE means not adding a listener."*

**Corrected:** "WLE-parity for accepting inbound P2P maps to every P2P-capable transport. WLE doesn't bind an application-layer socket for Packet/ARDOP/VARA-P2P because the TNC/modem owns the listen at the link/physical layer — but inbound sessions arrive via TNC connect-indication (KISS/AGW for Packet) or modem `CONNECTED` event (control channel for ARDOP/VARA). Tuxlink needs the receive side for each. Tuxlink also adds an allowlist + station-password overlay that WLE lacks at the app layer — defensible security posture for an operator running an open listener."

### 3.4 Sequencing §5 amendment

Re-shuffle Tier 1 to put listener foundation first:

**Tier 1 (P1, ships first to unblock operator's stated pain):**
1. **tuxlink-NEW1** — shared listener-arms layer (allowlist + keyring + RADIO-1 consent framing). Foundation everyone uses.
2. **tuxlink-xehu** — Telnet-P2P listener (the missing piece — `TcpListener::bind` + accept + telnet-login auth). Largest NEW listener code; spec at `dev/scratch/winlink-re/findings/telnet-p2p.md` (552 lines).
3. **tuxlink-inde** — Packet-P2P divergence overlay (allowlist + keyring on top of shipped Packet listener). Smallest scope; might land before xehu since the listener itself works today.
4. **tuxlink-dhbl** — ARDOP-P2P listener completion (`LISTEN TRUE` + event routing + UI). Modest scope.
5. **tuxlink-bajc** — HF best-channel selector (unchanged).
6. **tuxlink-hfft** — AutoConnect Family A (unchanged).

**Tier 2 (P2):** unchanged ordering, except VARA HF + VARA FM each gain the listener-completion sub-task.

**Tier 3 (P3):** unchanged.

## 4. Implementation plan

The work decomposes into 5 child bd issues (3 new + 1 scope-correction + 1 already-correct):

| New bd issue | Title | Priority | Depends on |
|---|---|---|---|
| tuxlink-NEW1 | Shared listener-arms layer: AllowedStations + StationPassword (keyring) + ListenerArmsRecord | P1 | — |
| tuxlink-NEW2 | RADIO-1 framing for listener mode: bounded armed-window + abort-on-disarm UX | P2 | tuxlink-NEW1 |
| tuxlink-NEW3 | VARA-HF + VARA-FM listener completion: invoke `command::Listen(true)` + inbound `CONNECTED` routing + UI | P2 | tuxlink-NEW1, ADR 0014 boundary |

Plus scope corrections to existing issues:

| Existing bd issue | Scope correction |
|---|---|
| tuxlink-xehu (Telnet-P2P) | Add: TcpListener bind + accept loop + telnet-login layer auth (the NEW listener code that was missed). Existing completeness-pass scope unchanged. |
| tuxlink-inde (Packet-P2P) | Re-scope from "🟠 defer/skip-listener" to "🟢 ship allowlist + keyring divergence overlay on top of shipped Packet listener." |
| tuxlink-dhbl (ARDOP-P2P) | Re-scope from "🟠 defer/skip-listener" to "🟢 flip LISTEN TRUE + inbound CONNECTED routing + UI." |

The 4 transport-listener issues (xehu, inde, dhbl, NEW3) can ship in parallel once tuxlink-NEW1 (foundation) lands.

### 4.1 Implementation sequencing per transport

**Packet (tuxlink-inde) — smallest scope, ships first probably:**
1. Wire `AllowedStations` gate into `winlink_backend.rs:800` `packet_connect_inner` for the `PacketRole::Listen` path
2. Wire `StationPassword` challenge — but per `packet-p2p.md` open item, there's no in-band place to challenge before B2F (AX.25 has no analog of telnet-login). Likely shed this divergence for Packet OR challenge at B2F start (which would require a B2F protocol extension and is probably not worth it).
3. UI: extend the existing `packet_set_listen` toggle with an Allowed-stations editor + a "log inbound rejections" view.

**ARDOP (tuxlink-dhbl):**
1. Change `session.rs:264` from `LISTEN FALSE` to operator-controlled. Default `false`; arm via UI command.
2. Add inbound `CONNECTED <peer>` event handler that calls `AllowedStations::accept(peer)` and either runs the answerer or drops the session.
3. UI: `ardop_listen()` + `ardop_set_listen(enabled)` Tauri commands mirroring the Packet pattern.

**Telnet (tuxlink-xehu):**
1. NEW `telnet_listen.rs` module: `TcpListener::bind` (port from config, default 8774 per the deep-dive correction), accept loop.
2. Per-connection: prompt `CALLSIGN :\r`, read response, optional `Password :\r` prompt (gated on `StationPassword::is_set()`), run `AllowedStations::accept` against callsign + peer IP.
3. If accepted, hand the stream to `run_exchange_with_role(ExchangeRole::Answer)`.
4. UI: `telnet_listen()`, `telnet_set_listen(enabled)`, plus the shared allowed-stations + password editors.

**VARA (tuxlink-NEW3):**
1. Invoke `command::Listen(true)` from the VARA transport layer.
2. Add inbound `CONNECTED` event handler — same shape as ARDOP.
3. UI: `vara_listen()` + variants.
4. Modem-replacement direction (ADR 0014) — the listener works against any VARA-protocol-compatible modem (reference VARA modem for now; clean-sheet tuxlink-modem when v0.5 ships).

### 4.2 Allowlist + keyring shared layer (tuxlink-NEW1) — concrete deliverables

- `src-tauri/src/winlink/listener/allowed_stations.rs` — `AllowedStations` struct, methods `accept(peer: PeerId) -> Decision`, `load_from(path)`, `save_to(path)`. Storage as plain config-file (callsigns + IPs are not secrets); default `Allow All Connections: FALSE` (DIVERGES from WLE TRUE default).
- `src-tauri/src/winlink/listener/station_password.rs` — `StationPassword` struct, keyring-backed (`keyring` crate already in tuxlink for CMS passwords). Methods `is_set() -> bool`, `verify(input: &str) -> bool`. Per-listener (not per-station).
- `src-tauri/src/winlink/listener/arms_record.rs` — `ListenerArmsRecord` struct capturing operator-consent arming events with TTL. Persists for forensics; expires per operator-configured TTL.
- Wire all three into a `listener_decide(peer, password_input) -> ListenerDecision` function that each transport adapter calls before running the B2F answerer.

### 4.3 Test plan

- **Unit tests:** `AllowedStations::accept` (callsign exact match, IP wildcard, allow-all toggle); `StationPassword::verify` (keyring round-trip); each transport's listener-arming code without a real network.
- **Integration tests:** Telnet listener loopback (bind to 127.0.0.1:8774, dial in, complete handshake, exchange a message). Already-shipped Packet listener integration coverage extends to the allowlist gate.
- **No on-air tests in this work.** RADIO-1 governs the on-air smoke; operator runs once each transport is unit+integration green.
- **WLE-as-truth comparison:** for the protocol-on-the-wire surfaces (Telnet-login challenge, ARDOP `LISTEN TRUE` semantics, VARA `LISTEN ON` semantics), record the deep-dive's documented WLE behavior + verify tuxlink matches via observed byte stream.

## 5. Operator decisions

| Decision | Resolution |
|---|---|
| AllowedStations default: FALSE (restrict by default) vs TRUE (WLE-compatible permissive)? | **TRUE per WLE-parity** (operator review 2026-06-03 / tuxlink-7vea). Earlier defensive-posture framing was reversed: TCP-layer access is the actual security boundary, not application-layer allowlist presence. Default-FALSE footgunned operators into "armed listener rejects everyone" first-run loops without earning the security it claimed. Security-conscious operators explicitly configure a station password and/or callsign restrictions. See `docs/superpowers/specs/2026-06-03-listener-ui-design.md` §1.1 + project memory `allowed-stations-default-true`. |
| StationPassword challenge for Packet — skip entirely (no in-band place to ask), challenge at B2F start (protocol extension), or sidecar (separate first-packet protocol)? | Skip for Packet — defer to allowlist as the only Packet gate |
| Listener-armed TTL — default 1 hour / 8 hours / no expiry? | 1 hour default; operator-configurable; "no expiry" disabled by default |
| Confirmation modal when arming vs implicit "arming is the consent"? | Implicit; UI affordance makes the consent semantic clear without a modal |

## 6. References

- 2026-06-02 closure plan: `docs/design/2026-06-02-wle-client-parity-closure-plan.md`
- Phase 1 verification: `docs/design/2026-06-02-winlink-express-feature-inventory-verification.md`
- Telnet-P2P deep dive: `dev/scratch/winlink-re/findings/telnet-p2p.md` (552 lines; authoritative listener spec)
- Packet-P2P deep dive: `dev/scratch/winlink-re/findings/packet-p2p.md` (open item: "how does the operator receive an inbound Packet-P2P session in WLE" — answered above: via the TNC, no app-layer listener needed in WLE because the AX.25 link layer accepts)
- ARDOP-P2P deep dive: `dev/scratch/winlink-re/findings/ardop-p2p.md`
- VARA HF deep dive: `dev/scratch/winlink-re/findings/vara-hf-cms.md` (client surface only — modem details ADR 0014)
- VARA FM deep dive: `dev/scratch/winlink-re/findings/vara-fm-cms.md`
- Memory: `radio1-governs-tx-not-ui`, `radio1-bounded-airtime-abort`, `no-disk-creds-default`, `clean-sheet-means-concepts-only`

---

Agent: thistle-swallow-cedar
