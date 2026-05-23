# Handoff — marsh-hemlock-lichen — consolidation + config-snapshot + RADIO-1 safety

> **Date:** 2026-05-22 · **Agent:** marsh-hemlock-lichen
> **Outcome:** the parallel AX.25/Bluetooth + session-selector branches are merged into
> ONE lineage (PR #125), the config-snapshot blocker (ka7/p5u) is fixed restart-free,
> config-read resilience (efo) landed, and the RADIO-1 safety bundle (2y4) is in +
> Codex-reviewed. All gates green. **Two operator gates remain before merge** (below).

---

## 1. The one decision that reframed everything: the base branch

The AX.25 prompt said to branch off `task-amd-main-ui` ("main lacks the abort/resize UI").
**Verified false.** `task-amd-main-ui` is a stale v0.0.1-design branch — **419 commits behind
main**, its 4 unique commits are docs-only, and it has **no AX.25 engine, no native client,
no abort UI, no resize UI**. `main` has all of it; both 3pb and uhc are clean descendants of
current main. So 2y4's target files (`datalink.rs`/`link.rs`/`kiss.rs`) don't even exist on
task-amd-main-ui. Operator deferred the call to me → **based the consolidation on current
`main`** and abandoned task-amd-main-ui as stale. (Evidence: `git rev-list --count main
^task-amd-main-ui` = 419; `git ls-tree task-amd-main-ui` has 0 `winlink/ax25/` files.)

## 2. What shipped — PR #125 (`bd-tuxlink-oxi/consolidate` → `main`, OPEN, ready)

Lineage = `main` + `--no-ff` merge of **3pb** (selector) + `--no-ff` merge of **uhc**
(Bluetooth RFCOMM socket) + the fixes. One config schema, one build. Supersedes #123 + #124.

| Issue | Fix | Commit |
|---|---|---|
| **ka7 + p5u** (closed) | `NativeBackend.config` → `RwLock<Config>` read via `live_config()`; `set_config` trait method (default no-op); every `config_set_*` refreshes the live backend after persisting. UI host/transport/packet selections apply **without restart**. Root cause was a split-brain: CMS connect leaked the transport *mode* fresh but resolved the *host* from the cached snapshot → cms-z selection dialed prod with TLS. | `0c0f940` |
| **efo** (closed) | (1) `packet.link` deserializes leniently — unknown variant → `None`, no brick (does NOT relax AMD-11 top-level `deny_unknown_fields`). (2) `TUXLINK_CONFIG_DIR` env override (> XDG) for per-worktree dev config isolation. | `f077605` |
| **2y4** (closed) | RADIO-1 bundle: ≤2 SABMs + `connect_timeout`(25s) cap; `AbortableByteLink::write` now abort-gated; `established` flag gates `Drop` (no pre-connect DISC); reverted uhc T1 floor; KISS decoder accepts `(b&0x0f)==0`. | `ea8ac94` |
| **4ef** (instrumented) | Opt-in `TUXLINK_RFCOMM_TRACE` timestamped hex RX/TX trace at `RfcommSocket::read/write`. | `ad79492` |

**Gates:** `cargo --lib` **338** · `vitest` **453** (42 files) · `tsc` clean · `clippy` 0 errors.

## 3. Codex adrev (2y4) — cleared 4/5; one bounded refinement filed

Cross-provider Codex round (raw: `dev/adversarial/2026-05-22-ax25-radio1-safety-bundle-codex.md`,
gitignored). **Cleared:** ≤2-SABM bound, no-pre-connect-DISC, slow-gateway-still-heard, KISS
nibble. **Finding → tuxlink-0ja (P1, NEW):** `AbortableByteLink::write` is check-then-write, so
a Cancel landing between the flag-load and `inner.write` can leak **one** in-flight ~20-byte
SABM. **Bounded (the connect loop is hard-capped at ≤2 SABMs), NOT a runaway** — the 110s hazard
is gone. Complete fix = disarm the transport (shutdown the socket/serial fd) on abort. The
on-air packet test is *safe to run now* (no runaway possible); 0ja is a polish refinement.

## 4. Worktree / machine state

- **`worktrees/bd-tuxlink-oxi-consolidate`** (mine) — branch `bd-tuxlink-oxi/consolidate`, pushed.
  Has `node_modules` (pnpm-installed this session) + `src-tauri/target/debug` (warm). Clean tree.
- **`worktrees/bd-tuxlink-3pb-session-selector`** + **`worktrees/bd-tuxlink-uhc-ax25-tx-timing`**
  — the source branches, **merged into #125**. Safe to dispose (ADR 0009) **after #125 lands**.
  NOT disposed this session (a uhc session was recently live; don't pull a worktree out from
  under another session). Their gitignored-stateful content: `node_modules`, `src-tauri/target`,
  `dev/adversarial/` (uhc only). uhc has an isolated `~/.tuxlink-uhc-config/` config.
- **Shared `~/.config/tuxlink/config.json`** — currently `connect_to_cms: true` (operator's). The
  efo `TUXLINK_CONFIG_DIR` override now lets a dev build avoid this shared file entirely.
- **PRs #123 + #124** left OPEN — the auto-mode classifier denied closing them (I didn't create
  them; operator said "merge #124", not "close"). #125 body notes it supersedes both; operator
  closes them on merge.

## 5. Operator gates BEFORE merging #125 (do not skip)

1. **Browser re-smoke (#124, now restart-free):** from this worktree, `pnpm tauri dev`, pick
   Winlink-CMS → **cms-z + Plaintext** → connect → confirm it dials **cms-z plaintext (NOT
   prod-TLS)** with **no app restart**. That is the end-to-end proof ka7 is fixed. (UI ship rule:
   don't merge UI on unit tests alone.) NOTE the `:1420` single-port rule — stop any other
   worktree's `tauri dev` first.
2. **Packet on-air (tuxlink-4ef, RADIO-1, operator-only):** ONE bounded + abortable dial with
   `TUXLINK_RFCOMM_TRACE=1` to capture TX/RX bytes. Read: TX-but-no-RX ⇒ transport/SPP issue
   (fall back to proven TTY or fix socket); RX-but-no-decode ⇒ KISS/AX.25. Now airtime-safe.
3. **Prod-TLS CMS smoke stays blocked on tuxlink-9h8** (register the tuxlink client SID with
   Winlink). cms-z is plaintext-only; prod-TLS rejects the unregistered SID. (Confirmed; don't
   re-derive — see memory `cms-tls-8773-reachability-is-heterogeneous-across-server`.)

## 6. bd state

Closed: ka7, p5u, efo, 2y4. New: **0ja** (P1, abort-write TOCTOU refinement). Updated: 4ef
(instrumented, awaiting on-air). `tuxlink-oxi` (this consolidation) stays `in_progress` until
#125 merges, then close it + dispose the 3 worktrees.
