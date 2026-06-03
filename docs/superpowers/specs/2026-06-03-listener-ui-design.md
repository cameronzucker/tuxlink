# Multi-transport listener UI + ARDOP live-modem wiring + foundation default flip

> **bd:** tuxlink-7vea · **Date:** 2026-06-03 · **Agent:** `plover-magnolia-salamander`
>
> **Amends:** `docs/design/2026-06-03-multi-transport-listener-architecture.md` §5 (default flip), §4.1 ARDOP block (deferred wiring now done).
>
> **Mock:** `docs/design/mockups/2026-06-03-listener-ui-mocks.html`

## 0. Problem

PRs #318/#319/#320 shipped the multi-transport listener backend (allowed_stations + arms_record + listener_decide + per-transport adapters + 24 Tauri commands). The frontend wiring shipped zero React UI — operators can only exercise these features via devtools `invoke()` calls. Operator review surfaced three further corrections:

1. **No UI** — backend-only on a feature whose value depends on operator interaction.
2. **`AllowedStations` default-FALSE was a footgun** — diverges from WLE without earning the security it claimed. Station-password isn't TCP-layer security; TCP-level access is. Default-FALSE forces operators into an "arms my listener → nobody can connect → why?" loop on first run.
3. **RADIO-1 references leaked into UI strings** — RADIO-1 is internal ADR jargon; production strings should describe what the listener does, not the regulatory frame.

Also: the ARDOP listener was deferred at the "gate-only" layer (tuxlink-95g8 filed for the live-modem wiring). The deferral was procedural, not architectural; we ship the wiring as part of this work.

## 1. Architecture overview

Three layers change. Each is small individually; bundled here because they're entangled at the boundary.

### 1.1 Foundation (one file, one default flip)

`src-tauri/src/winlink/listener/allowed_stations.rs`:

- `AllowedStations::default()` flips `allow_all` from `false` to `true`. Fresh-installed tuxlink now ACCEPTS every inbound peer until the operator curates a callsign restriction or sets a station password.
- `AllowedStations::new()` follows. (They share the same default.)
- Tests that exercised the reject-by-default path migrate to explicit `with_allow_all(false)` construction. The injection mechanism (`with_packet_allowlist` builder on `NativeBackend`) stays — still useful for tests that want explicit-deny behavior.

Architecture doc §5 row "AllowedStations default" updates from `FALSE per no-disk-creds-default-class defensive posture` to `TRUE per WLE-parity + first-run-UX reasoning`. New project memory captures the design judgment so future agents don't re-suggest restrict-by-default on security grounds.

### 1.2 ARDOP backend (closes tuxlink-95g8)

Three concrete pieces:

**(a)** `winlink/modem/ardop/transport.rs` exposes a `cmd_socket_handle() -> CmdSocketHandle` accessor on the live `Transport` that lets callers (specifically `ardop_listen`) send commands to a running modem without owning the full session. The current abstraction owns the cmd socket inside `init_tnc` + the async event loop; we add an `Arc<Mutex<TcpStream>>` (or equivalent — match the existing pattern in `ardop/session.rs`) that the UI side can take to drive `set_listen(true/false)`.

**(b)** `ui_commands.rs::ardop_listen()` becomes load-bearing:

- Load the allowlist (already validates at arm-time per the Codex round)
- Mint the `ListenerArmsRecord` (already does)
- Reach the live modem's cmd socket via the new accessor; call `set_listen(handle, true)` (the existing function in `winlink/modem/ardop/listener.rs:580`)
- Install a CONNECTED-event router on the async loop. When the event consumer receives `Command::Connected { peer_call, bandwidth_hz }`, it calls `gate_inbound_peer_now` (mirror of the Packet path), routes Accept → `run_exchange_with_role(ExchangeRole::Answer, ..)`, routes Reject → `arq_disconnect` + forensics log
- `ardop_set_listen(false)` sends `LISTEN FALSE` + clears arms

**(c)** `ui_commands.rs::ardop_set_listen(enabled)` stops returning Err (which the Codex round had it doing because it couldn't actually arm). With wiring it becomes a real toggle.

The B2F handoff on Accept is the same shape as Packet's. Mailbox Inbox-persist + Outbox-drain on inbound exchange is DEFERRED for ARDOP in this round (matching the same deferral for Telnet at tuxlink-k3ru). A new bd issue will be filed during execution covering "ARDOP listener inbound-mail symmetry" mirroring tuxlink-k3ru's scope. The listener gate + LISTEN-flip + CONNECTED routing + B2F handshake ship complete; the operator sees inbound peers connect + exchange protocol-level data, but received messages won't land in the Inbox until the symmetry follow-up ships. Packet listener already has full mailbox symmetry via `native_packet_exchange` (shipped in PR #318).

### 1.3 Frontend (three React panels)

Per the mock, the listener affordances live inside each transport's existing `RadioPanel` — no new navigation. Layout: Option A (collapsible details inside one "Listen" sub-section). Per-transport:

**TelnetP2pRadioPanel** (NEW Listen section after the existing Peer Password section):

```
Listen (Accept Inbound)         [ARMED · 47 min] | [disarmed]
  [Arm listener · TTL 1 h]       (primary button, becomes "Disarm" red when armed)
  Help text describing what happens

  ▶ Listener setup        (collapsed by default; 8774 · loopback chip)
      Bind, Port, TTL controls

  ▶ Allowed stations      (collapsed; "2 callsigns · 1 IP" count chip)
      Allow-any-peer toggle (DEFAULT ON post-flip)
      Callsign chip-row + add chip
      IP-pattern chip-row + add chip
      Help text: "Match logic: callsign-allow OR IP-allow (WLE-parity)"

  ▶ Station password       (collapsed; "set in keyring" chip OR "not set")
      Set/Change/Clear chips
      Help text: keyring-stored, sent as challenge before B2F
```

**PacketRadioPanel** (EXTEND existing Listen section):

```
Listen                          (existing header)
  [Listen for an incoming call]  (existing button; "Stop" when armed)
  ☐ Auto-arm Listen at startup  (existing checkbox)

  ▶ Allowed stations      (NEW; "1 callsign" count chip)
      Allow-any-peer toggle (DEFAULT ON post-flip)
      Callsign chip-row + add chip
      (no IP-pattern row — AX.25 has no IP layer)
      Help text on the new gate
```

No "Listener setup" expander (KISS-TCP host/port stays in the existing Modem Link section above). No "Station password" expander (Packet has no in-band password challenge per packet-p2p.md §"Auth").

**ArdopRadioPanel** (NEW Listen section, alongside the existing Connect form):

```
Listen (Accept Inbound)         [ARMED · 47 min] | [disarmed]
  [Arm listener · TTL 1 h]       (real button — wiring landed in §1.2)
  Help text on what happens at the modem layer

  ▶ Allowed stations      (collapsed)
      Allow-any-peer toggle (DEFAULT ON post-flip)
      Callsign chip-row + add chip
      Help text on no-password-layer rationale
```

No "Listener setup" expander (ARDOP modem TCP host/port already lives elsewhere). No "Station password" expander (ARDOP has no password layer per ardop-p2p.md divergence 2). With the §1.2 wiring landed, there's no "partial wiring" banner.

### 1.4 UI text scrub

Existing listener-related strings reviewed; RADIO-1 references stripped. Replacement language describes what the listener does in operator-facing terms, not regulatory ADR terms.

Examples:
- BEFORE: `"Arms inbound consent under RADIO-1."`
- AFTER: `"Accepts inbound Telnet P2P sessions on 127.0.0.1:8774 until disarmed or the TTL expires."`

Search target: any UI-bound string containing `"RADIO-1"`, `"consent token"`, `"Part 97"`, or related ADR jargon. Backend forensics log + bd issue bodies + ADRs keep their RADIO-1 references — that's where they belong.

## 2. Data flow

### 2.1 Telnet listener (unchanged backend, new UI driving it)

```
User clicks "Arm listener"
  → invoke('telnet_listen')
  → backend binds TcpListener on 127.0.0.1:8774
  → spawns accept-loop in tokio::spawn_blocking
  → listener_arms.jsonl appends arm event
  → UI flips to ARMED state (count-down timer from TTL)

Inbound peer connects
  → accept_loop reads CALLSIGN prompt response
  → listener_decide checks allowlist + arms TTL
  → on Accept: prompts Password (unconditional per WLE wire-parity)
                checks via StationPassword::verify
                hands stream to run_exchange_with_role(Answer)
  → on Reject: sends WLE-compat error message + closes TCP

User clicks "Disarm"
  → invoke('telnet_set_listen', { enabled: false })
  → backend sets shutdown flag + closes the bound listener to wake accept()
```

### 2.2 Packet listener (mostly-existing flow + new allowlist gate UI)

```
User toggles "Listen for incoming call"
  → invoke('packet_listen')
  → existing flow: backend binds KISS link, calls answer()
  → answer() returns connected stream + peer AX.25 callsign
  → NEW: listener_decide called via packet_gate (already shipped in #318)
  → on Accept: hand to native_packet_exchange (existing path, includes mailbox-persist)
  → on Reject: drop stream → Ax25Stream::drop fires DISC + appends to forensics log

User edits allowed-stations list
  → invoke('packet_allowed_stations_add', { callsign }) (or _remove, _set_allow_all)
  → backend writes to <config-dir>/listener/packet/allowed_stations.json
  → next inbound uses the new list (gate loads from disk per connect)
```

### 2.3 ARDOP listener (NEW backend wiring + UI)

```
User clicks "Arm listener"
  → invoke('ardop_listen')
  → backend validates allowlist file loads
  → backend mints ListenerArmsRecord + appends to listener_arms.jsonl
  → backend grabs live modem cmd socket via new accessor
  → sends "LISTEN TRUE\r" to modem
  → installs CONNECTED-event router on async loop

Inbound peer connects (ARQ handshake completes on modem)
  → modem fires Command::Connected { peer_call, bandwidth_hz }
  → CONNECTED router calls gate_inbound_peer_now
  → on Accept: routes the modem stream to run_exchange_with_role(Answer)
  → on Reject: arq_disconnect + appends reject event to forensics log

User clicks "Disarm"
  → invoke('ardop_set_listen', { enabled: false })
  → backend sends "LISTEN FALSE\r"
  → clears arms record + uninstalls CONNECTED router
```

## 3. Tests

### 3.1 Foundation default-flip tests

- `AllowedStations::default()` and `AllowedStations::new()` both return a value where `allow_all() == true`.
- Existing tests in `decide.rs`, `allowed_stations.rs`, `packet_gate.rs`, `ardop/listener.rs` that exercised reject-by-default are updated to use `with_allow_all(false)` explicitly.
- `winlink_backend::packet_two_real_peers_complete_a_connect_and_b2f_over_tcp_kiss` — the `with_packet_allowlist(allow_all=TRUE)` injection becomes redundant (the default IS now TRUE); keep the injection for clarity (it documents intent) but verify the test passes without it.

### 3.2 ARDOP backend wiring tests

- Unit tests on the new cmd-socket accessor (lock semantics, none-when-uninitialised).
- Mocked CONNECTED event → gate_inbound_peer_now → Accept path verified ends at run_exchange_with_role(Answer) entrypoint.
- Mocked CONNECTED event → reject path verified ends at arq_disconnect being called.
- LISTEN TRUE / LISTEN FALSE send-and-ack round-trip against the existing modem mock.
- `ardop_set_listen(true)` and `ardop_set_listen(false)` against a mocked modem session.

### 3.3 Frontend tests

- For each panel, a `*.test.tsx` that exercises:
  - Arm button click → invokes the right Tauri command
  - Disarm button click → invokes the right Tauri command
  - Allowed-stations add/remove → invokes the right Tauri command
  - Allow-any-peer toggle → invokes the right Tauri command
  - For Telnet: station-password Set/Change/Clear → invokes the right Tauri commands
  - Collapse/expand of the detail accordions persists across re-renders
  - Armed state shows the count-down indicator + correct button label/color

### 3.4 Browser smoke (per `feedback_browser_smoke_before_ship`)

Before declaring complete:
- `pnpm tauri dev` from the worktree
- Walk each transport's listener flow: arm, edit allowlist, disarm
- For Telnet: `nc 127.0.0.1 8774` to verify the wire protocol actually works end-to-end through the UI-driven flow
- For Packet: requires a Dire Wolf peer — operator runs (RADIO-1 gates real-radio test)
- For ARDOP: requires ardopcf + a peer — operator runs (RADIO-1 gates real-radio test)

## 4. Error handling

The backend already handles the load-failure / arm-validation / DISCONNECT-on-reject error paths (Codex round of PRs #318/#319/#320). Frontend wiring surfaces those errors:

- `invoke()` rejections → SessionLog warning line (existing pattern in `useSessionLog`)
- Allowlist file load failure on arm → the existing `UiError::Internal { detail: "..." }` flows through `.catch()` to the SessionLog
- Bind-port-busy on Telnet arm → same path
- ARDOP modem-not-running on arm → new error case: `ardop_listen` errors with `"ARDOP modem not running; start the modem before arming the listener"` (clear operator-facing message)

## 5. Out of scope (explicit)

- **Mailbox Inbox-persist + Outbox-drain symmetry on Telnet inbound exchange** (tuxlink-k3ru remains open).
- **Mailbox Inbox-persist + Outbox-drain symmetry on ARDOP inbound exchange** (new bd issue filed during execution; mirrors tuxlink-k3ru for ARDOP).
- Both above: operator gets listener + gate + B2F handshake working; received messages do not land in Inbox until the symmetry work ships. Packet already has full mailbox symmetry via `native_packet_exchange` (shipped in PR #318).
- **VARA listener** (tuxlink-xnoy remains blocked) — ADR 0014 modem-replacement-direction boundary.
- **Global "incoming session" notification surface** — the architecture doc §2.3 calls for a global indicator when a peer connects. Filed as a follow-up — separate concern from the per-transport panel work.
- **Listener-status indicator in the StatusBar** — one-line addition, separate from the panel UI; can ship alongside the global notification surface.
- **Listener forensics log viewer** — operators can `tail -f ~/.config/tuxlink/listener/listener_arms.jsonl` for now; a UI viewer is a separate concern.

## 6. Memory + documentation propagation

Per CLAUDE.md propagation contract (max 3 sites per policy claim):

1. **Architecture doc** at `docs/design/2026-06-03-multi-transport-listener-architecture.md` §5 — the canonical change of the default.
2. **CLAUDE.md** — no update needed; the architecture doc carries the substance.
3. **New project memory** at `~/.claude/projects/-home-administrator-Code-tuxlink/memory/project_allowed_stations_default_true.md` — captures the design judgment so future agents understand why the default is what it is.

## 7. References

- `docs/design/2026-06-03-multi-transport-listener-architecture.md` — original architecture (§5 default + §4.1 per-transport block)
- `dev/scratch/winlink-re/findings/telnet-p2p.md` §5 (station password wire), §8 (wire protocol), §9 (divergences)
- `dev/scratch/winlink-re/findings/ardop-p2p.md` §"Allowed-stations", §"Auth", §"Divergences"
- `dev/scratch/winlink-re/findings/packet-p2p.md` §"Allowed-stations model"
- `src-tauri/src/winlink/listener/*` — foundation API (already shipped in PR #299)
- `src-tauri/src/winlink/modem/ardop/listener.rs` — gate module (already shipped in PR #319)
- `src-tauri/src/winlink/modem/ardop/session.rs` — modem session (where the cmd-socket accessor lands)
- `src/radio/modes/{Telnet,Packet,Ardop,TelnetP2p}RadioPanel.tsx` — frontend pattern
- `docs/design/mockups/2026-06-03-listener-ui-mocks.html` — visual mock with both layout options

---

Agent: plover-magnolia-salamander
