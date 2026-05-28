# Handoff — 2026-05-22 — raven-salamander-tamarack — AX.25 packet: RADIO-1 incident + safety redesign needed

## 🚨 HEADLINE — read before any packet work

A packet dial over the new RFCOMM-socket transport **ran ~110 s of repeated SABM
keying with no reachable abort; the operator had to power OFF the radio to stop it.**
That is a **RADIO-1 violation**. **Do NOT dial packet (any transport) until
[`tuxlink-2y4`](#the-fix-bundle-tuxlink-2y4--radio-1-must-fix) lands.** The code on
`bd-tuxlink-uhc/ax25-tx-timing` (PR #123) is **not air-safe** and PR #123 is marked
**do-not-merge**.

## What this session set out to do vs. what happened

**Goal:** fix `tuxlink-uhc` (radio "double-keys on connect"; T1/TXdelay "not
respected"), TDD hardware-free, then operator retests on-air.

**What actually unfolded** (each step taught something; see findings):
1. Diagnosed `uhc` as a too-short connect T1 (default 3 s ≪ ~7 s RF round-trip → premature SABM retransmit). Floored runtime T1 to 10 s in `config.rs into_params` (`MIN_RF_T1_MS`). 314 lib tests green. **This was the wrong lever — see incident.**
2. On-air retest hit **"Broken pipe"** (transport open). Chased it through: stale `rfcomm bind` slot, the **dynamic SPP channel** (rotated 4→5→1→4), and a read-only socket probe.
3. Concluded the `serialport`-TTY open's termios was tearing down the radio's SPP session, and built the **in-app RFCOMM-socket transport** (`tuxlink-nx2`): `KissLinkConfig::Bluetooth { mac }` → `AF_BLUETOOTH`/`BTPROTO_RFCOMM` socket, SPP channel resolved from SDP at connect. 321 lib + 40 FE tests green. Socket **connect+hold proven via Python** (no RF).
4. On-air retest of the socket transport → **RADIO-1 incident**: radio keyed repeatedly for ~110 s, never received the relay's reply, operator powered off the radio. Also: no abort button, output window not resizable.
5. **Operator (correctly) halted me** twice: (a) build/config provenance was wrong — I'd been debugging a build that wasn't even mine; (b) "be more thorough" on the *core* issue (why we TX too long and never receive the relay's packets).

## Provenance discoveries (the operator was right)

- **The operator was running a *different worktree's* build** (`bd-tuxlink-3pb-session-selector`, a parallel agent session) on Vite `:1420` — **not** my `uhc` build. So my fixes were never exercised; the Broken pipes were the *old* serialport-TTY code + a stale bind, not my socket.
- **The shared config thrashes.** All worktree builds read one `~/.config/tuxlink/config.json` and contend for `:1420`. A parallel session's app (with a *divergent schema*) reverted my `Bluetooth` link edit to `Serial` and at one point wrote a `host` field my build's `deny_unknown_fields` rejected (`unknown field host`).
- **Resolution adopted:** gave the `uhc` build its **own** config via `XDG_CONFIG_HOME=$HOME/.tuxlink-uhc-config` (isolated; parallel sessions can't touch it). That config has `link: Bluetooth { 38:D2:00:01:55:5C }`.

## Codex adversarial findings (cross-provider, this session)

Transcript: `dev/adversarial/2026-05-22-ax25-socket-rx-runaway-codex.md` (gitignored). Agent `sparrow-magnolia-taiga`. Five findings, **two of which the main loop missed**:

1. **No-RX over the socket ≠ TTY.** TX works (radio keys) but `connect()` never receives the relay's UA/DM, so it loops the full N2. `recv_frame` handles WouldBlock/TimedOut equally; the break is most likely *below* AX.25 — the raw RFCOMM socket not reproducing the TTY's bidirectional SPP behavior (modem-control/MSC, or service/channel). **Can't be diagnosed from source — needs raw byte logging + one bounded on-air dial.** → `tuxlink-4ef`.
2. **Unsafe airtime envelope.** `connect()` sends `(n2_retries+1)` SABMs, one per T1. With my floored T1=10 s and default n2=10 ⇒ ~110 s of re-keying. → `tuxlink-2y4`.
3. **(MISSED) Pre-connect DISC on drop.** `connect()` builds `Ax25Stream{closed:false}` *before* the link is up; a failed dial (DM/timeout/abort) drops it → `Drop` → `disconnect()` → **sends a DISC + waits another T1** ([datalink.rs:714](../../src-tauri/src/winlink/ax25/datalink.rs#L714)). Wrong before a UA. → `tuxlink-2y4`.
4. **(MISSED) Abort can't halt TX in time.** Flag is checked only in `AbortableByteLink::read`, never `write` ([link.rs:80](../../src-tauri/src/winlink/ax25/link.rs#L80)); `connect()` transmits at the top of each attempt before any abort-aware read. → the RADIO-1 hole. → `tuxlink-2y4`.
5. **KISS decoder accepts only command byte `== 0x00`** ([kiss.rs:66](../../src-tauri/src/winlink/ax25/kiss.rs#L66)); should be `(buf[0] & 0x0f) == 0` (data = low-nibble-0, any port). RX fragility (not the socket-vs-TTY cause; the TTY used the same decoder). → fold into `tuxlink-2y4`.

## Honest reframe (mine to own)

The **serialport-TTY path was proven to receive** (the W7MOT-6 **DM** in the
`chasm-sorrel-glade` session came back over it). My pivot to the socket rested on a
**termios-teardown theory I never proved**; the Broken-pipe episodes are better
explained by stale bind / wrong channel / wrong-worktree build. So I likely replaced
a working-RX transport with one that has a no-RX bug, on a shaky diagnosis. The
transport choice (socket vs. proven TTY) is **an open operator decision** —
`tuxlink-nx2` (socket, no bind, RX unproven) vs. the TTY (proven RX, but the
bind/channel jank the operator wants gone).

## The fix bundle (`tuxlink-2y4`) — RADIO-1 MUST-FIX (blocks uhc, nx2, 4ef)

Before ANY packet dial, on a branch off **`task-amd-main-ui`** (has abort + resize UI; `main` lacks them):
1. **Bounded airtime** — split the connect SABM-retry policy from data N2: 1 SABM + ≤1 retry, **hard total-elapsed cap (~20–30 s)** regardless of `n2_retries`.
2. **Abort before every transmit** — check the flag in `AbortableByteLink::write`, and in `connect()` before `push_kiss_params` and before each SABM; never key after Cancel.
3. **No pre-connect DISC** — only arm `Ax25Stream::Drop` teardown after a UA is received (`established` flag, or a handshake-local struct with no `Drop` DISC).
4. **Revert the T1 floor** (`config.rs` `MIN_RF_T1_MS` / `into_params`) — it tripled worst-case airtime and was the wrong lever.
5. **Raw byte logging** at `RfcommSocket::read`/`write` (timestamped hex) so the next on-air test yields evidence, not guesses (`tuxlink-4ef`).
6. Decoder: accept `(buf[0] & 0x0f)==0`.

Then ONE **bounded + abortable + instrumented** on-air dial (operator) to capture
TX/RX bytes and settle the no-RX cause + the transport choice.

## State

- **Branch:** `bd-tuxlink-uhc/ax25-tx-timing` (off `origin/main`), **PR #123 OPEN, marked DO-NOT-MERGE.** 3 commits: T1 floor (uhc), RFCOMM socket transport (nx2), TS DTO types. **361 tests green but the connect is air-unsafe.**
- **Worktree:** `worktrees/bd-tuxlink-uhc-ax25-tx-timing`. Untracked/gitignored: `dev/adversarial/` (Codex transcript), `node_modules`, `src-tauri/target` (warm). Bin compiles (28 s).
- **bd:** `tuxlink-2y4` (P0, the safety bundle), `tuxlink-4ef` (P1, socket no-RX), both block `tuxlink-uhc` + `tuxlink-nx2` (in_progress). `tuxlink-p5u` (P3, stale-config-snapshot) is **likely moot for packet** — `packet_connect` reads config fresh from disk (ui_commands.rs:1293).
- **Config:** shared `~/.config/tuxlink/config.json` is currently `Serial` (parallel session). Backups: `config.json.bak-*`, `bak2-*`. Isolated `uhc` config at `~/.tuxlink-uhc-config/tuxlink/config.json` (`Bluetooth` link). The operator's old serial config is recoverable from the backups.
- **Parallel sessions:** the operator runs multiple worktree sessions sharing one config + one Vite port. Verify build provenance (`/proc/<pid>/cwd` of the running `tauri dev`) and use an isolated `XDG_CONFIG_HOME` for any on-air test build.

## Radio facts (durable)

- UV-Pro `38:D2:00:01:55:5C`: paired/bonded/trusted. SPP channel **rotates** (saw 4→5→1→4) — always resolve from SDP at connect, never hardcode.
- RFCOMM **socket** connect to the SPP channel succeeds + holds (Python, no RF). `AF_BLUETOOTH/BTPROTO_RFCOMM` socket creatable as uid 1000 (no root).
- TX over the socket works (radio keys). **RX over the socket is unproven/broken** — the open question.
