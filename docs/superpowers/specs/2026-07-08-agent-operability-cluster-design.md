# Agent-operability cluster — design

**Date:** 2026-07-08
**Agent:** hawk-kingfisher-spruce
**Issues:** tuxlink-7ppfq (P1), tuxlink-z2nwx (P2), tuxlink-77seh (P2)
**Origin:** the 0.85.0 VARA HF agent-mode alpha test (Elmer + Opus-4.8, operator
N7CPZ) exposed a cluster of gaps in the agent's `perceive → configure → operate →
report` loop. Root cause is fully diagnosed in the 7ppfq bd `--notes` and the
2026-07-08 salamander-sage-magnolia handoff; this spec does **not** re-diagnose,
it designs the fix.

## North star

A stressed, tired, cold operator fighting Linux + WINE needs the agent to get
VARA working **no matter what**. The design goal is therefore **actionable
capability**, not minimal-surface purity. Every capability here pairs a
*perception* primitive with an *action* primitive, and degrades gracefully: the
perception result tells the agent which action is possible.

The agent's capability surface should track **what a competent operator could do
at a shell**, not a curated subset. Capability gates are justified only by a real
safety boundary. RADIO-1 (keying a transmitter under the station callsign) is a
real boundary and is preserved untouched. "Open a local TCP socket," "start a
local process," "list printers," "read `/proc/asound`" are not boundaries — the
agent gets them.

## Invariants (all contracts)

- **Additive.** No existing signal changes meaning. In particular
  `vara_engine_available` stays the **CONFIGURE gate** — "is the vendored VARA
  installer bundled in this build / can the agent provision VARA?"
  (`resolve_engine().is_ok()`, `install.rs`). It is **not** redefined to
  liveness. New liveness/identity signals are added *alongside*.
- **No session-mutex contention.** Reachability/probe paths never acquire
  `VaraSession.inner` (`vara/commands.rs`). They open their own short-lived
  socket, or read a live session snapshot. When a session is already open, the
  reachable signal is derived from that live session — no second cmd-port client.
- **RADIO-1-free.** Nothing in this cluster issues `CONNECT` or transmits. The
  reachability probe is equivalent to opening a TCP connection to
  `localhost:8300` (the `vara_tcp_probe` bin already establishes this posture).
- **No hardcoded device identity.** The audio surface never picks "the radio" for
  the user; it equips the agent to pick *with* the user.

---

## Contract 1 — VARA reachability + start (tuxlink-7ppfq, part A)

### Problem

`vara_status.connected` is `Closed` at rest (normal between exchanges) and was
read as "no VARA." `vara_engine_available` reports installer-present, not
liveness. Neither probes the live TNC on `host:cmd_port`, which SSH confirmed was
listening. All three outputs were false-negatives against the operator's ground
truth ("VARA configured, connected, reading audio, ready to send").

### Design

**Graduated probe — the agent's choice, mirroring shell (`nc -z` vs actually
talking to the port):**

1. **Lightweight — `vara_status.reachable: bool` (new field, alongside
   `connected`).** TCP connect to the configured `host:cmd_port` (from
   `config_get_vara()` — never hardcoded 8300), classify listening/not, drop.
   Cheap and side-effect-free, so the routine perception poll stays cheap. When a
   session is already open, `reachable` is taken from the live session, not a new
   socket.
2. **Deep — a new explicit probe tool (`vara_probe`).**
   Connect, read VARA's startup banner (VARA emits one on connect — a read-only
   confirmation may suffice) and/or send one benign setter and confirm the echo.
   Returns a structured classification the agent acts on:

   | Result | Meaning | Agent's next move |
   |---|---|---|
   | nothing on `host:cmd_port` | VARA.exe not running / WINE down | **start VARA** |
   | socket answers, no VARA banner/echo | wrong process / stale / zombie | diagnose |
   | connects **and** speaks VARA | live and healthy | proceed to configure/send |

**Start/attach path — a new agent-invokable tool (`vara_start`).** Launch VARA
under WINE when the probe says down. In scope for 7ppfq:
the agent can already *install* VARA (the provisioning engine / wizard MCP tool),
so gating it from *starting* VARA is incoherent. Reuses/extends the existing VARA
provisioning + launch infrastructure (`install.rs`, the provisioning-wizard MCP
tool). Starting a local process is not a RADIO-1 act.

### Testing

Probe testable against a fake TCP listener (banner / no-banner / no-listener
fixtures) — no radio. Start path testable via a mocked launcher. `reachable`
field covered by a DTO-shape test.

---

## Contract 2 — active-modem source of truth (tuxlink-7ppfq, part B)

### Problem

`modem_get_status.kind` is the literal `"ardop"` (`mcp_ports.rs:207`). The
frontend replicates the same stale assumption: `AppShell.tsx:846-849` hardcodes
`{ kind: 'ardop-hf', intent: 'cms' }` with a comment "In v1, only the ARDOP modem
exists." Two independent literals, both written when ARDOP was the only modem.

### Design

"Active modem" has **two honest meanings**; the agent needs both:

- **Selected** — the mode the operator has chosen as the *target* (VARA HF, in
  the failing test). Persists even when no session is open — the operator's
  *intent*.
- **Running** — which modem session is *open right now*. Backend already tracks
  this (`ModemSession.active_transport_kind`; `VaraSession` snapshot). At rest,
  nothing is open → the honest answer is "none/idle," not "ardop."

**`modem_get_status` reports both** (additive DTO fields): `selected` +
`running`. `kind` dispatches on the SoT instead of the literal.

**`selected` is backed by persisted config**, written at the UI mode-switch
point (`AppShell.tsx` — the one place a regression hides if missed). Persisting
(not just runtime-managed) means the agent knows the target the moment the
operator picks it, and it survives an app restart mid-troubleshooting (cold
operator relaunches; agent still knows the goal). Running-only was rejected: it
re-creates the exact blind spot that failed the test ("operator picked VARA but
hasn't opened a session yet").

**Both hardcode sites fixed:** `mcp_ports.rs:207` (dispatch on SoT) and
`AppShell.tsx:847` (derive from selected/running, not a literal).

### Testing

Config round-trip test for the persisted selection. `modem_get_status` DTO test
asserting `selected` follows a config change and `running` follows a session
open/close. Frontend: vitest on the `activeModem` derivation.

---

## Contract 3 — print + report export (tuxlink-z2nwx)

Two distinct shell-equivalent capabilities (the operator's mental model — two
tools). Neither is a RADIO-1 act.

1. **Literal print (CUPS).** `printer_list` (`lpstat -p -d`) → attached printers
   + default. `print_document` (`lp -d <printer>`) → send text/a file to a
   printer. CUPS auto-filters text/markdown, so no PDF dependency. If no printer
   is attached the list is empty and the agent falls back to export.
2. **Report export (file).** Write markdown / `.txt` to a **sandboxed**
   `~/Documents/Tuxlink/reports/`. The agent picks the *filename*, not the
   *directory*; path traversal / absolute paths rejected (reuse the
   mid-path-traversal guard, tuxlink-5lbm). Return the **absolute path** to agent
   + operator on success. Markdown/text now; **PDF deferred** (a renderer is a
   whole dependency + failure surface for a formatting nicety — additive
   follow-up if formal PDFs are later needed).

They compose: generate → export to file → optionally print that file.

### Testing

`report_export` sandbox test (traversal rejected, path returned, file written to
the reports dir). Printer tools tested against a mocked `lpstat`/`lp` (CI has no
printer); `printer_list` empty-result path exercised.

---

## Contract 4 — audio-device surface (tuxlink-77seh)

### Problem

The agent's audio surface collapsed everything to "USB PnP Sound Device" and fell
back to proposing a manual unplug test — unable to distinguish the radio
interface. The disambiguating detail exists and is cheap (SSH-verified).

### Design

**An audio-device inspection tool** returning, per device (all read-only,
world-readable on Linux, no root):

- friendly / ALSA card name + index
- USB `VID:PID` (`0d8c:013a` vs `0d8c:0013`)
- bus / port path (usb `-3` vs `-7.2`)
- capture / playback in-use state (`/proc/asound` + `fuser`)

**The disambiguation *method* ships as agent-readable guidance**, not a code-side
ranking: a hardcoded "likely radio interface" score is a device-identity
heuristic in disguise — exactly what the operator said not to do. The tool hands
over rich data; the agent applies the method (radio interface is typically a
full-duplex USB card distinct from a headset; confirm capture+playback on the
*same* card; DRA-100 class enumerates as `0d8c:xxxx`; use in-use state + port
path to distinguish two identical-name cards) and advises VARA **Input AND
Output = the same full-duplex card**. This generalizes to hardware neither the
operator nor the agent has seen — it guides a *new* user, not just this bench.

### Testing

Inspection tool tested against fixture `/proc/asound` trees + mocked `fuser`
(two-identical-name-cards fixture is the key case). No hardware dependency.

---

## Build order

1. **tuxlink-7ppfq** (P1) first — contracts 1 + 2. Unblocks the send test: the
   agent finally perceives the live VARA and can start it.
2. **tuxlink-z2nwx** + **tuxlink-77seh** (P2) — contracts 3 + 4.

Each ships via `build-robust-features` (adversarial review incl. a Codex round +
CI on amd64/arm64), each preserving the CONFIGURE path, each ending at the
**wire-walk** reachability gate before any "done" claim. On-air validation is
operator-only (RADIO-1 / ADR 0018); agents validate transmit-adjacent code via
mocks / loopback / CI.
