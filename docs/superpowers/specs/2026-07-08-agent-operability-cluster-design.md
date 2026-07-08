# Agent-operability cluster — design

**Date:** 2026-07-08
**Agent:** hawk-kingfisher-spruce
**Issues (build order):** tuxlink-pf6re (P1, **lead**), tuxlink-7ppfq (P1),
tuxlink-z2nwx (P2), tuxlink-77seh (P2). Split-out follow-ups: tuxlink-u269g
(`vara_start`), tuxlink-etjp9 (predict_path runaway), tuxlink-iicsh (listen +
cooldown).
**Origin:** two live 0.85.0 VARA HF agent-mode alpha tests (Elmer + Opus-4.8,
operator N7CPZ, armed + supervised under Part 97) exposed a cluster of gaps in
the agent's `perceive → configure → operate → report` loop. Root causes are
diagnosed in the bd `--notes`; this spec designs the fixes. It has been hardened
against a 5-round adversarial review (1 Codex + 4 Claude; raw at
`dev/adversarial/2026-07-08-7ppfq-design-codex.md`).

## North star

A stressed, tired, cold operator fighting Linux + WINE needs the agent to get
VARA working **no matter what**. The goal is **actionable capability**, not
minimal-surface purity. Every capability pairs a *perception* primitive with an
*action* primitive and degrades gracefully. The agent's surface tracks **what a
competent operator could do at a shell**, gated only by a real safety boundary.
RADIO-1 (keying a transmitter under the callsign) is real and preserved; the
egress arm/taint gate is real and preserved. "Open a local socket," "start a
local process," "list printers," "read `/proc/asound`" are not boundaries.

## Invariants (all contracts)

- **Additive.** No existing signal changes meaning. `vara_engine_available` stays
  the **CONFIGURE gate** ("is the installer bundled / can the agent provision?",
  `install.rs:127`), never redefined to liveness. New signals are added alongside.
- **Egress lock is absolute.** Nothing here relaxes "tainted **or** unarmed ⇒
  cannot transmit" (the 2ouqf injection-defense invariant; the red-team proved the
  local model is fully injectable, so the gate is load-bearing). Denied egress is
  never retried and never succeeds while unarmed/tainted.
- **No session-mutex contention.** Reachability/probe paths never *hold*
  `VaraSession.inner` across a socket op or an exchange. One brief `try_lock`
  classification is permitted; a blocked lock returns `reachable: unknown` rather
  than waiting (the open path holds the lock across a 5 s connect).
- **RADIO-1-free.** Nothing here issues `CONNECT` or transmits.
- **No hardcoded device identity.** The audio surface equips the agent to pick
  *with* the user; it never picks.

---

## Contract 0 — graceful egress denial + arm/taint perception (tuxlink-pf6re) — LEAD

### Problem

At the finish line of the armed test, an egress call (`rig_tune`) was denied
because send authority had expired ~1195 s earlier. The runner returned
`RunOutcome::ToolDenied` and surfaced a raw `-32600` to the operator,
**overwriting the agent's in-progress message.** The agent could neither *see*
the arm/taint state up front nor *narrate* the denial.

Verified path: gate `Denied` → `router.rs:46` `ErrorData::invalid_request`
(-32600) → executor `ToolOutcome::Denied` → `runner.rs:163-165`
`return RunOutcome::ToolDenied` (terminal). `mcp_client.rs:22` documents the
terminality as deliberate injection defense ("never retried").

### Design

The "never retry egress" property is injection defense and **stays**. What
changes is that a denial no longer *kills the turn and clobbers output*.

1. **Egress lock absolute** — a denied send is never retried, never succeeds
   while unarmed/tainted. Unchanged.
2. **Graceful narration for BOTH reasons** (operator decision 2026-07-08). On a
   `Denied` outcome, feed the denial back to the model as a **tool result** so it
   composes a helpful reply ("authority lapsed 20 min ago — re-arm and I'll resume
   from station 3") and the turn ends with the **agent's** message, not a raw
   error. This holds for expiry/not-armed **and** taint denials; taint stays
   egress-locked, just no longer turn-killing. (Bound continued reasoning: the
   model gets a turn to *narrate*, but any further egress call is still gated —
   the injection invariant is unaffected because egress cannot succeed.)
3. **Perception surface** — an agent-readable arm/taint status
   `{armed, seconds_remaining, tainted, taint_reason}` (the runner already carries
   an informational `EgressStatus` snapshot, `runner.rs:186`, and there is an
   `egress_arm/disarm/status` command set) so the agent checks *before* burning a
   long station sequence.
4. **Fix the miscoding** — a policy denial is not `-32600 InvalidRequest` (the
   request was well-formed). Classify it as a non-fatal, agent-visible denial.

### Security posture (2ouqf alignment)

Does **not** touch taint-clear semantics (2ouqf's decided model: re-arm clears
taint only if it also drops tainted turns; restart-only baseline holds). This
contract is purely about *how a denial is surfaced*, not *when the gate opens*.
Because it borders the injection-defense boundary, it gets its own brainstorm +
full adrev before code.

### Testing

Runner test: a `Denied` outcome appends a tool result and yields a final model
turn (agent message preserved), rather than a bare terminal `ToolDenied`. Assert
no subsequent egress op runs (gate still closed). Arm/taint perception surface:
DTO round-trip; armed/expired/tainted variants. Injection regression: a tainted
session still cannot transmit.

---

## Contract 1 — VARA reachability + read-only probe (tuxlink-7ppfq, part A)

*(The `vara_start` launch path is SPLIT OUT to tuxlink-u269g — see below. 7ppfq
is perception-only.)*

### Design (adrev-hardened)

- **`vara_status.reachable: bool`** (new field on the **MCP** `ports::VaraStatusDto`
  — *not* the command-side `commands::VaraStatus`; they collide by name). Populate:
  `try_lock` the session once; if state ∈ {Open, Connecting} → `reachable` derived
  from `state == Open` (lean on the existing ~3 s heartbeat; **no socket**); if the
  lock is contended → `reachable: unknown`; else a bare
  `std::net::TcpStream::connect_timeout(host:cmd_port, ≥5s)` — **cmd port only**,
  not `VaraTransport` (which opens the cmd+data pair), explicit `shutdown` on drop,
  result **TTL-cached** (~heartbeat cadence) so routine polls don't churn VARA's
  single-App acceptor. `host`/`cmd_port` from `config_get_vara()`, never hardcoded.
  Timeout is a **shared config knob** with the transport so they can't drift; never
  a bare `connect`.
- **`cmd`-reachable ≠ ready-to-send** — 8300 can accept while 8301 (data) lags on a
  WINE restart. The field is named/described as *cmd-port reachability*, not
  "usable session."
- **`vara_probe` (new tool, deep, READ-ONLY)** — connect + read VARA's startup
  banner (and/or a read-only `VERSION`-style query, matching the setup engine's
  verify check). **Must not** send `MYCALL`/`BW`/`LISTEN` setters — the existing
  `vara_tcp_probe` bin does, and those *mutate the operator's live VARA*. Returns a
  structured classification: down / socket-but-not-VARA / VARA-ok.
- Update the hand-written tool description strings (`router.rs:131`) so the agent
  learns `reachable` exists; add the field to the mock impl (`lib.rs:197`,
  compiler-caught).

### Testing

Probe against a fake TCP listener (banner / no-banner / no-listener). `reachable`
derivation under open/closed/contended-lock. Assert the deep probe sends no
stateful setter.

---

## Contract 2 — active-modem source of truth (tuxlink-7ppfq, part B)

### Design (adrev-hardened)

`modem_get_status` reports **both** `selected` (operator's target, persisted) and
`running` (live). `kind` dispatches on the SoT, not the `"ardop"` literal.

- **`running`** — for VARA, from the `VaraSession` snapshot; **for ARDOP, from
  `ModemState`/`snapshot_transport_present()`, NOT `active_transport_kind`** — the
  real `modem_ardop_connect` path never sets `active_transport_kind` (only the
  unused `ardop_open_session` does), so sourcing it there returns idle for a live
  ARDOP session (a coverage trap). The DTO test MUST exercise `modem_ardop_connect`.
  ARDOP and VARA are **two independent session objects**: make `running` a **list**
  (honest about the "both non-idle" state that convention forbids but code doesn't
  enforce), or define explicit precedence + a `conflict` flag. `SocketLost` ⇒
  `running = Ardop` (degraded), so the agent knows to close+reopen, not "idle."
- **`selected`** — **reuse the existing `activeConnection`** (`AppShell.tsx:478`,
  `(intent, transport_kind)`); it already *is* the selection and only lacks
  persistence. Do not stand up a parallel field. Persist at the **`activeConnection`
  state transition** (a `useEffect([activeConnection])`), because there are **two**
  writers — `onSelectConnection` (`:1443`) and the status-driven effect
  (`:851-863`) — and hooking only the first guarantees React↔config drift. Hydrate
  `activeConnection`'s initial state from the persisted value on mount.
- **Config schema** — persisting to `Config` (which is `deny_unknown_fields`) trips
  the version guard: bump `CONFIG_SCHEMA_VERSION` 5→6, field is
  `#[serde(default)]`, update `config_schema_version_tracks_field_set`
  (`config.rs:1680`) and add an additive-load test. This is the tuxlink-ulrz
  data-loss trap; the plan must call it out. (The MCP surface can only read Rust
  state — `localStorage` is invisible to it — so a Rust-side store is required;
  `Config` is the natural home.)
- **`kind = running`** (honest idle), **not** `running ?? selected` — `kind` pairs
  with `connected` (`mcp_ports.rs:206`), so a `selected` fallback re-introduces a
  false-positive. The agent reads `selected` separately.
- **Both hardcode sites** — `mcp_ports.rs:207` (dispatch on SoT) and
  `AppShell.tsx:846-849`. The frontend `activeModem` derives from `activeConnection`
  (selection), **not** liveness; add a deduped `useActiveModemMode` selector so the
  shell doesn't re-render at the modem-broadcaster cadence (the 4 Hz storm
  `useModemIsActive` exists to avoid).
- **Vocabulary** — backend `TransportKind::Ardop` serializes `"ardop"` but the UI
  uses `"ardop-hf"`. Define the wire shape explicitly (transport-kind separate from
  UI protocol/intent) so AppShell can map it.

### Testing

`modem_get_status` DTO test: `selected` follows a config change; `running` follows
a session open/close **via `modem_ardop_connect`** and via VARA. Config round-trip
+ additive-load (schema-guard). Frontend vitest: `activeModem` tracks
`activeConnection` across all protocols.

---

## Contract 3 — print + report export (tuxlink-z2nwx)

Two shell-equivalent capabilities; neither is a RADIO-1 act.

1. **Literal print (CUPS)** — `printer_list` (`lpstat -p -d`) + `print_document`
   (`lp -d <printer>`). CUPS auto-filters text/markdown; no PDF dependency. Empty
   list ⇒ agent falls back to export.
2. **Report export (file)** — write markdown/`.txt` to a **sandboxed**
   `~/Documents/Tuxlink/reports/`; agent picks filename, not directory; traversal
   rejected (reuse the tuxlink-5lbm guard); absolute path returned. PDF deferred.

They compose: generate → export → optionally print. **Testing:** sandbox test
(traversal rejected, path returned); printer tools against mocked `lpstat`/`lp`.

---

## Contract 4 — audio-device surface (tuxlink-77seh)

**An audio-device inspection tool** returning, per device (read-only, no root):
ALSA card name + index, USB `VID:PID` (`0d8c:013a` vs `0d8c:0013`), bus/port path
(`-3` vs `-7.2`), capture/playback in-use (`/proc/asound` + `fuser`). **The
disambiguation *method* ships as agent-readable guidance**, never a code-side
ranking (a ranking is a device-identity heuristic in disguise). The agent applies
the method (radio interface = full-duplex USB card distinct from a headset;
confirm capture+playback on the *same* card; DRA-100 class = `0d8c:xxxx`; use
in-use + port path to split two identical-name cards) and advises VARA **Input AND
Output = the same full-duplex card**. Grounded on the operator's bench: FT-710 uses
the **DRA-100 for audio** and a **separate FT-710 USB for CAT/RTS PTT** (digital
preset; CAT readable for state). Do not ship that identity — guide a new user to
find theirs. **Testing:** fixture `/proc/asound` trees + mocked `fuser`
(two-identical-name-cards is the key case).

---

## Build order

1. **tuxlink-pf6re** (P1, lead) — graceful denial + arm/taint perception. Most
   acute; independent of the VARA plumbing; security-sensitive (own brainstorm +
   full adrev).
2. **tuxlink-7ppfq** (P1) — perception-only: `reachable` + `vara_probe` + SoT.
   Unblocks the send test (agent finally sees the live VARA).
3. **tuxlink-z2nwx** + **tuxlink-77seh** (P2).

Split-out follow-ups (own brainstorms): **tuxlink-u269g** (`vara_start` — the
local-vs-remote launch question), **tuxlink-etjp9** (predict_path runaway),
**tuxlink-iicsh** (listen + cooldown).

Each ships via `build-robust-features` (adrev incl. a Codex round + CI on
amd64/arm64), each preserving the CONFIGURE + egress-lock invariants, each ending
at the **wire-walk** reachability gate before any "done" claim. On-air validation
is operator-only (RADIO-1 / ADR 0018).
