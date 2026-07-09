# Design — VARA connect/transmit flow (the deferred Phase 3)

Date: 2026-07-09
Author: opossum-badger-gulch
bd: tuxlink-yrrjq
ADR: 0018 (this is the "build it whole" remediation of the incident 0018 documents)
Status: DRAFT — pending Codex adversarial review (build-robust-features step 2) + operator sign-off

## Problem

`vara_start_session` opens the VARA cmd/data sockets, sends `MYCALL` (+ optional `BW`), and stops at state `Open`. It never sends `CONNECT`. `OutboundCommand::Connect` is dead code; no `vara_connect` command exists for the UI or the agent surface. VARA therefore cannot raise a station or move a message — the flagship HF path is a socket-open stub (ADR 0018).

## Goal

Build the whole VARA connect/transmit flow, **mirroring the established, RADIO-1-safe `modem_ardop_connect` pattern** — reuse its architecture verbatim where possible, do not reinvent it. Definition of done (per ADR 0018): the flow is wire-walked end-to-end to a real VARA connection that moves a B2F message. The final on-air wire-walk is the operator's (RADIO-1); everything up to it is code-complete + unit-tested + Codex-reviewed + CI-green.

## The ARDOP pattern being mirrored (source of truth: `modem_commands.rs`)

1. **Server-minted consent token** — `ModemSession::mint_consent_token()` exposed via `modem_mint_consent`; the frontend's Connect-modal mints server-side so a compromised renderer cannot self-mint.
2. **Consume-first gate** — `consume_consent_token()` is the FIRST action in the gated connect: atomic equality-check-and-clear under one lock; wrong/missing/replayed token returns `Err` before ANY I/O, spawn, or status mutation. One mint authorizes exactly one connect (Part 97 per-invocation consent).
3. **Bounded airtime** — a connect deadline (ARDOP: `CONNECT_DEADLINE` 120s), NO retry loop; a failure flips status to `Error` and returns. Retry = a fresh user-initiated Connect with a fresh token.
4. **Abort-before-block** — an abort writer is installed BEFORE the blocking connect begins, so Disconnect can break a runaway mid-connect (memory `radio1-bounded-airtime-abort`).

## Architecture

### Session + consent (`VaraSession`)

`VaraSession` currently holds `{ transport: Option<VaraTransport>, status: VaraStatus }` behind a `Mutex`. Extend it — mirroring `ModemSession` — with:
- `mint_consent_token()` / `consume_consent_token(&str)` (same atomic semantics as ARDOP).
- An abort hook: a cloned handle to the cmd-socket writer so Disconnect can send `DISCONNECT`/`ABORT` while the connect recv-loop holds the transport.

**Decision to confirm in adrev:** keep VARA on its own `VaraSession` (per-modem session, consistent with the existing split and ADR 0015's per-transport posture) rather than folding into ARDOP's `ModemSession`. Leaning: separate session, shared *consent helper* factored into one place so the Part-97 semantics can't drift between the two modems.

### Commands (Tauri + MCP)

- `vara_mint_consent` → mirrors `modem_mint_consent`.
- `vara_connect { target, consent_token }` (gated) → mirrors `modem_ardop_connect`: consume token FIRST, then run the connect flow. Registered in `lib.rs` AND exposed on the agent/MCP surface (`mcp_ports.rs`) so an agent can drive it (the gap that made "agents can't use it" true).
- `vara_disconnect` → send `DISCONNECT`, invalidate the consent token, reset status, close.

### Connect flow (post-consume), driven off `VaraTransport`

```
consume_consent_token            # RADIO-1 gate, first — no I/O before this passes
ensure session open              # reuse vara_start_session_inner (MYCALL + BW); require callsign
install abort writer             # cloned cmd_writer; Disconnect sends DISCONNECT/ABORT
status = Connecting
send(OutboundCommand::Connect { mycall, target })
loop until deadline:             # bounded airtime, mirrors CONNECT_DEADLINE
    match recv()? :
        Ptt(true)   -> rig.key()      # RADIO-1: VARA asks host to key; drive configured ptt_method
        Ptt(false)  -> rig.unkey()
        Pending     -> status = Connecting (negotiating)
        Connected{peer,bw} -> break Ok    # install transport, publish Connected snapshot
        Disconnected -> break Err("remote/again")
        Unknown(l)  -> log + continue
        None        -> (timeout tick) check deadline / abort flag
on Ok(Connected): run B2F over data_stream()   # move the message
always on exit: ensure rig.unkey() + DISCONNECT   # never leave the radio keyed
```

### PTT integration (RADIO-1-critical)

VARA does host-side PTT: it emits `PTT ON`/`PTT OFF` and the host keys the radio. The connect loop routes those to the rig via the operator's configured `ptt_method` (`cat_command` → `TX1;`/`TX0;`, or rigctld PTT). **This reuses the rig-control path; it does not add a second keying mechanism** (memory `never-vox`). The unkey-on-every-exit invariant (including error/abort/panic-unwind via a guard) is mandatory: a lost path mid-TX must fail toward unkey (memory `radio1-bounded-airtime-abort`, and the pine-poplar-raven note that close-serial CAT fails UNSAFE — this must be handled: prefer a keying path that deasserts on drop).

**Open design point for adrev/escalation:** the R2 rig/PTT path itself is unproven this session (the `os error 11` / dialout / baud thread). The VARA connect code must be *correct given a working rig*; whether the operator's specific rig config keys is a separate, on-air question (his wire-walk). The code must not silently no-op PTT — if keying fails, the connect aborts with an actionable error, never a silent dead-air TX.

### B2F over VARA

On `CONNECTED`, hand `transport.data_stream()` to the existing B2F session layer. **To verify in adrev:** how ARDOP/CMS B2F is wired (`winlink_backend` / `b2f.rs`) and whether that layer is transport-generic over a `Read+Write` stream (it should be — `data_stream()` returns `&mut TcpStream`). If B2F is currently ARDOP/CMS-specific, factor the shared driver so VARA reuses it (no fork of B2F).

### Status + UI

Extend `VaraStatus`/`VaraState` with the connect states (`Connecting`/`Pending`/`Connected{peer,bw}`/`Error`), mirroring `ModemState`. UI: a Connect modal with the RADIO-1 consent acknowledgement (mirror the ARDOP modal), a target-station field, and live status. No new keying UI — consent = the Send/Connect click (memory `radio1-governs-tx-not-ui`).

## Testing (TDD; no on-air)

- Consent gate: wrong/missing/replayed token → `Err` before any send; one mint = one connect (unit, mirror ARDOP's gate tests).
- Connect state machine over a **mock VARA server** (scriptable inbound sequences): `PENDING→CONNECTED` → Connected + B2F handed the stream; `PENDING→DISCONNECTED` → Error; `PTT ON`/`PTT OFF` → mock rig key/unkey calls in order; deadline exceeded → Error + unkey; abort mid-connect → DISCONNECT sent + unkey.
- Unkey-on-exit invariant across every exit path (success, error, deadline, abort, panic) — the single most safety-critical test.
- Never a silent PTT no-op: keying failure → connect aborts with an actionable error.

## Wire-walk (operator, on-air — closes "done")

With a working rig config + access restored: mint consent → `vara_connect <gateway>` → observe PTT keys, `PENDING`→`CONNECTED`, a B2F message exchanged, clean `DISCONNECT`, radio returns to RX. Bounded + abortable throughout.

## Explicitly out of scope (NOT deferred silently — flagged per ADR 0018)

- Fixing the operator's specific R2 rig/PTT/baud config so it keys — that is the separate on-air/rig thread, and it is the operator's wire-walk, not this code change.
- VARA FM / P2P intents beyond the single connect-to-gateway flow (memory `vara-intents-grounded-wle`): if the operator wants those too, that is an operator-authorized additional scope, raised as its own bd issue — not something this change quietly includes or quietly omits.

## Review gates before implementation

1. Codex adversarial review (build-robust-features step 2, ≥1 Codex round) — attack: the consent gate ordering, the unkey-on-exit invariant across panics, B2F stream reuse, deadline/abort correctness.
2. Operator sign-off on this design + the two "out of scope" boundaries (ADR 0018: the operator decides scope, not the agent).
