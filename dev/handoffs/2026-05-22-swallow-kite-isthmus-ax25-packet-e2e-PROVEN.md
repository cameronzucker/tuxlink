# Handoff — 2026-05-22 — swallow-kite-isthmus — AX.25 packet: end-to-end PROVEN over TCP/KISS

**Supersedes** the `*-NOT-done.md` handoff. The headline gap is closed: the packet
feature now **completes a real connect + listen + B2F message transfer end-to-end**,
proven by running it (no mocks). It is hardened for real-RF conditions via a Codex
adversarial round. What remains is operator app-smoke + the independent-decoder /
hardware / on-air rungs — none of which a prior "done" claim should skip.

## Branch / PR / state
`bd-tuxlink-7fr/ax25-packet` @ `1ccde4c`, **pushed**. PR **#115** → main is **no longer
conflicting** (this session merged `origin/main` 420a16e in — tuxlink-686's position
subsystem — resolving the conflict via merge, not rebase). Worktree
`worktrees/bd-tuxlink-7fr-ax25-packet`. Epic `tuxlink-7fr` open. `tuxlink-3wh`
left **in_progress** (see "why" below). 304 lib tests green.

## What this session did
1. **Merged origin/main** (commit 3b43054). Conflicts were additive (both branches
   appended Tauri commands / tests); kept both. Then `08c57d1` fixed two 686 test
   fixtures missing the new `packet` field (auto-merge couldn't catch it — only
   compiling did).
2. **Built the REAL end-to-end test** (`8cf6811`): two production `NativeBackend`s,
   one `Listen` (Answer/master), one `DialTo` (Dial/slave), connected over a TCP
   relay that stands in for the TNC+RF wire. Every layer is shipping code; only the
   byte-relay is non-tuxlink (and the TNC is transparent to AX.25 above KISS, which
   is the layer RADIO-1 bars anyway). Test:
   `winlink_backend::…::packet_two_real_peers_complete_a_connect_and_b2f_over_tcp_kiss`.
3. **Fixed the headline bug it surfaced**: AX.25 connect (SABM/UA) worked, but the
   B2F handshake **deadlocked** — the `Answer` (master) role reused `build_handshake`
   (slave format, no `>` prompt) and read via `read_remote_handshake` (which only
   ends on `>`). Grounded the fix in **wl2k-go `fbb/handshake.go`**: added
   `build_master_handshake` (client handshake + trailing `>`) and
   `read_slave_handshake` (master ends the slave handshake on a peeked `F` turn line).
   Dial/slave path unchanged (still proven vs real CMS).
4. **Codex adversarial round** (`1ccde4c`) found 3 RF-real defects the lossless
   localhost relay masked, all fixed:
   - **BLOCKER**: `Ax25Stream::read` returns `Ok(0)`=no-data, but the B2F `BufReader`
     treats `Ok(0)` as EOF → premature `ConnectionClosed` on RF latency. Fix:
     `BlockingB2fStream` adapter at the B2F boundary (+ `Ax25Stream::is_closed()`,
     + a deterministic unit test). Lower-layer contract left intact.
   - `answer()` never pushed KISS TXDELAY/persistence/slot params (`connect()` did) →
     factored `push_kiss_params()`, both call it.
   - Defensive LF-skip in the master peek (CRLF residue). Raw transcript:
     `dev/adversarial/2026-05-22-…-codex.md` (gitignored).

## Why tuxlink-3wh is still in_progress (not closed)
The backend chain is **built + proven by running** (automated, Codex-hardened) — its
deliverable is met. It's left open deliberately so the operator confirms via
app-smoke before it closes (two prior sessions declared "done" prematurely). What is
NOT yet covered, by design:
- **Operator app-smoke**: does the UI's Connect/Listen actually invoke the backend
  commands? (The test drives `NativeBackend::connect` directly — same entry the
  `packet_connect`/`packet_listen` commands call — so the backend chain is proven,
  but the frontend→command wiring is not smoke-tested here.)
- **Independent-decoder validation** (`tuxlink-mbw`, filed): the e2e test wires two
  tuxlink peers, which **share the codec** — a symmetric framing/FCS bug is invisible.
  Validate vs Dire Wolf + kernel AX.25 (`kissattach`), no RF. Needs your OK for
  `sudo apt install direwolf ax25-tools`.
- **Hardware USB/BT** (`tuxlink-jvp`), **live status feed** (`tuxlink-orj`),
  **serial Stop** (`tuxlink-nj1`) — the P2 follow-ons, unbuilt.
- **On-air** — RADIO-1, operator-only.

## Operator: how to verify (no RF)
Re-run the end-to-end proof yourself (warm shared cache; ~20s):
```bash
CARGO_TARGET_DIR=/home/administrator/.cache/tuxlink-cargo-target \
  cargo test --manifest-path \
  /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-7fr-ax25-packet/src-tauri/Cargo.toml \
  --lib packet_two_real_peers -- --nocapture
```
App-smoke the Packet UI wiring (only ONE `tauri dev` at a time — binds :1420):
```bash
CARGO_TARGET_DIR=/home/administrator/.cache/tuxlink-cargo-target \
  pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-7fr-ax25-packet tauri dev
```

## Disk note (operator-corrected this session)
Per-worktree builds are fine; the rule is **clean up your `target/` when done**, not
tiptoe. This session reclaimed a stray **7.8 GB** orphaned target from the merged
`bd-tuxlink-686` worktree, and used the warm shared `CARGO_TARGET_DIR` (no new
per-worktree target created). 17 fully-merged worktrees are pristine + disposable
(~400 MB total) if you want them swept — offered, not done.

## Working tree
Clean, pushed. Local-only scratch: `dev/scratch/winlink-re/` (decompiled RMS Express +
findings, gitignored), `dev/adversarial/*-codex.md` (gitignored). No worktrees were
disposed.
