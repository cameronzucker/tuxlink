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

### Connect + session flow (post-consume), driven off `VaraTransport` — HARDENED per Codex 2026-07-09

Three invariants the review forced (see "Codex review" section for the failure sequences):
- **Lock discipline (P1):** take the transport OUT of the `VaraSession` mutex and release the lock before ANY blocking I/O; install the abort writer under the lock first. All recv/PTT/B2F work runs on a local `transport`, so `vara_disconnect` can acquire the mutex and fire the abort at any instant. (Mirrors ARDOP's `take_transport` + run-outside-lock discipline.)
- **One airtime budget (P1):** a single deadline covers connect AND B2F. Bound BOTH sockets — add a write timeout on the data socket (the transport sets only read timeouts today).
- **PTT pump runs through B2F (P1):** VARA emits `PTT ON/OFF`/`DISCONNECTED` on the CMD socket for the whole ARQ session, including during B2F on the DATA socket. A concurrent pump owns keying for the entire session.

```
# ── under session lock, ONE critical section (P1 single-flight) ──
if status in {Connecting, Connected}: return Err(busy)     # reject concurrent connect
if !consume_consent_token(tok):       return Err(no-consent) # RADIO-1 gate, before ANY I/O
status = Connecting
transport = take_transport()                                # move out of the mutex
install_abort_writer(cmd_writer.clone())
# ── lock RELEASED; all blocking I/O below runs on local `transport` ──

ptt = resolve_ptt_backend()?          # P2/#6: FAIL CLOSED — if unconfigured, or a
                                      # key+unkey probe fails, return Err BEFORE any CONNECT.
                                      # Never emit CONNECT unless we can reliably key AND unkey.
let _guard = UnkeyOnDrop(&ptt)        # Drop guard: unkey on EVERY exit, incl. panic-unwind

send(Connect { mycall, target })
loop bounded by deadline:
    match recv():                     # recv() must distinguish EOF from read-timeout (P2)
        Ptt(true)  -> ptt.key()   (bounded write; on Err -> abort)
        Ptt(false) -> ptt.unkey() (bounded write; on Err -> abort)
        Pending    -> status = Connecting
        Connected{peer,bw} -> break Ok
        Disconnected                              -> break Err(remote)
        WrongCallsign|MissingSoundcard|Offline|CancelPending -> break Err(fatal)   # P2
        EOF                                        -> break Err(eof)                # P2
        Unknown(l) -> log + continue
        timeout    -> if past deadline or abort-flag: break Err(timeout/abort)

on Ok(Connected):
    # P1: the cmd/PTT pump keeps running CONCURRENTLY with B2F.
    spawn pump(transport.cmd_reader):
        loop: match recv(): Ptt(true/false)->ptt.key/unkey;
              Disconnected|fatal|EOF -> set shared abort flag + return
    run_b2f_exchange(transport.data_stream(), deadline, abort_flag)   # bounded + abort-aware
    signal pump stop; join pump
# ── always (guard drop + explicit) ──
ptt.unkey()                           # belt-and-suspenders atop the Drop guard
send_best_effort(DISCONNECT) (bounded)
status = Idle | Error
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

## Scope (per ADR 0018 — built whole, nothing carved)

The VARA connect feature is ONE target-agnostic flow: `CONNECT <mycall> <target>` works for a gateway OR a peer callsign, so gateway and P2P are the *same* code, both covered — there is no slice to defer. VARA FM vs VARA HF is a config choice (which engine/ports the operator points at); the connect flow is identical, so it is covered too. Nothing about the VARA-connect capability is deferred.

The one thing this change does NOT do — because it is a *different concern*, not a slice of this feature — is fix the operator's specific R2 machine rig/PTT/serial config so his particular radio keys. That is host/hardware configuration and belongs to his on-air wire-walk, not to this app-code change. The code fails CLOSED if the rig can't key (Codex #6), surfacing an actionable error rather than dead-air.

## Codex adversarial review — 2026-07-09 (gpt-5.5, xhigh)

Transcript: `dev/adversarial/2026-07-09-vara-connect-design-codex.md` (gitignored). Six findings, all incorporated into the hardened flow above:

| # | Sev | Finding | Disposition |
|---|-----|---------|-------------|
| 1 | P1 | cmd/PTT pump stops at CONNECTED; VARA can request keying during B2F with no reader → stuck TX | Concurrent pump owns keying for the whole session, runs alongside B2F |
| 2 | P1 | Deadline only wraps connect; B2F unbounded; data socket has no write timeout | One airtime budget over connect+B2F; write timeout added on data socket |
| 3 | P1 | Borrowing transport under the mutex deadlocks `vara_disconnect` → abort wedge | Take transport out of mutex; all blocking I/O outside the lock |
| 4 | P1 | Mint stays open during in-flight connect → two connects race unkey paths | Atomic single-flight: consume token + mark Connecting in one critical section; reject while Connecting/Connected |
| 5 | P2 | Only Connected/Disconnected handled; fatal events + EOF ignored until deadline → PTT held | Abort immediately on WrongCallsign/MissingSoundcard/Offline/CancelPending/EOF; recv distinguishes EOF from timeout |
| 6 | P2 | No VARA PTT/rig surface exists; impl could no-op or unbounded-write | Resolve+probe a concrete keyer after consent, fail CLOSED before CONNECT; Drop-guard unkey |

## Remaining gate before implementation

Operator review of this hardened design. Per ADR 0018 there is no scope to authorize — the feature is built whole; this is a correctness/approach review, not a scope negotiation.
