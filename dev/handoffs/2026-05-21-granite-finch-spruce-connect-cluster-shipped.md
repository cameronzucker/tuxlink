# Handoff — 2026-05-21 — granite-finch-spruce — connect cluster shipped to main

## TL;DR
The native-client **connect cluster is done and shipped to `main`**. `tuxlink-gqo`
(P1 connect stall) and `tuxlink-9z2` (P1 abort) are fixed, Codex-adrev'd (2 High
findings fixed), and merged: **ng3→feat (PR #104) → main (PR #105)**, both MERGED.
`main` now carries the full v0.0.1 work (native client + ng3 chrome + cluster) and
**still has its 2 dependabot commits** (merge, no force-push). `tuxlink-2y5` (P2
manual grid) was **deferred**; Codex Medium/Low edge-cases filed as `tuxlink-lbg`.

## 🚨 CRITICAL FIRST ACTIONS (next session)
1. **Read this handoff.** The connect cluster is shipped — do NOT redo it.
2. **release-please** will (or already did) open a **release PR on `main`** from the
   conventional commits. Cutting the tagged **v0.0.1 release is the operator's call** —
   review that release PR (CHANGELOG/version) and merge it when ready. This session
   integrated feat→main but did NOT tag a release.
3. **Two operator GUI smokes are owed** (backend is validated; the GUI paths are not):
   - **gqo progress lines:** `pnpm -C <worktree> tauri dev` (or from main), click
     Connect against cms-z, confirm "TCP connection established / CMS login complete /
     Negotiating messages" appear in the session-log pane. (Dev needs
     `TUXLINK_CMS_PLAINTEXT=1` since cms-z has no TLS on 8773.)
   - **9z2 abort:** start a stalling TCP listener, point the app at it, click Connect
     (blocks in login), click **Abort** → expect immediate Disconnected + "CMS
     connection aborted." in the log. Deterministic recipe:
     ```bash
     # terminal 1 — accept + stall (sends no login prompt):
     python3 -c "import socket,time; s=socket.socket(); s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1); s.bind(('127.0.0.1',8899)); s.listen(); print('stalling on 8899'); c,_=s.accept(); print('accepted'); time.sleep(120)"
     # terminal 2 — launch the app pointed at it:
     TUXLINK_CMS_HOST=127.0.0.1 TUXLINK_CMS_PORT=8899 TUXLINK_CMS_PLAINTEXT=1 pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-0ic-native-winlink-client tauri dev
     ```

## What shipped this session (granite-finch-spruce)
- **`tuxlink-gqo`** (commit `0de773a`, closed) — root-caused live: cms-z exposes **no
  TLS on 8773** (production `server.winlink.org` does); commit 7c50359 set the dev host
  to cms-z but left transport CmsSsl/8773, and `TcpStream::connect` had no timeout → the
  ~75-130s silent stall. Fixes: `connect_with_timeout` (15s/addr — cms-z:8773 now fails
  in **16s**, live-validated), per-step progress logging (injected `ProgressSink`,
  renders via existing session log), dev transport coherence
  (`TUXLINK_CMS_PLAINTEXT`/`TUXLINK_CMS_PORT` in `native_connect`, mirroring the probe).
  CmsSsl/8773 stays the production default.
- **`tuxlink-9z2`** (commit `1163f04`, closed) — ribbon **Abort** button + `cms_abort`
  command + `WinlinkBackend::abort()` shutting down a `try_clone`'d connect socket to
  unblock a slow TLS/login/exchange phase; result maps to `Cancelled` (Disconnected,
  "CMS connection aborted."). Abort during the ≤15s TCP-connect window is bounded by the
  timeout (no socket to shut down yet); a race-guard shuts down on-register if abort
  already fired.
- **Codex adrev hardening** (commit `d38b92c`) — fixed the 2 High + 1 Low from the
  cross-provider review: **#1 single-flight** (backend `connect_in_progress` CAS +
  frontend onConnect guard — the F5/Ctrl+Shift+O accelerator could otherwise start a
  second connect and **re-send the outbox**); **#2 abort TOCTOU** (check `aborting`
  inside the `abort_handle` lock); **#7 RAII cleanup** (`ConnectGuard` clears handle +
  flag on every exit). Med/Low (#3 unbounded DNS, #4 per-addr vs total deadline, #5
  late-abort label, #6 last-error) → `tuxlink-lbg` (single-A hosts unaffected today).

## Gates (all green, run against the worktree)
- Rust: **161 lib** + all integration/bin suites; `cargo test --manifest-path <wt>/src-tauri/Cargo.toml`.
- Frontend: **354** vitest; `tsc --noEmit` clean (`pnpm -C <wt> exec ...`).
- ⚠️ **cwd gotcha:** the bash cwd silently reverted to the main checkout mid-session; a
  relative-path gate run tested the wrong tree. **Pin paths** (`--manifest-path` abs,
  `pnpm -C`, `git -C`). Saved to auto-memory `feedback_pin_paths_in_worktree_sessions`.

## Branch / worktree state
- Branch `bd-tuxlink-0ic/native-winlink-client`, worktree
  `worktrees/bd-tuxlink-0ic-native-winlink-client/`. **Pushed**; HEAD `d38b92c`
  (+ this handoff commit). **Merged to feat (PR #104) and main (PR #105).**
- Branch NOT deleted (this worktree depends on it). **Disposal owed** via the ADR 0009
  ritual (or leave for the operator): `git worktree remove` is banned.
- Tracked tree clean. Gitignored on-disk (local-only, NOT pushed):
  - `dev/scratch/` — probe traces (`gqo-probe*`, `gqo-probeA/B*`), cargo/vitest logs,
    `pr-body-feat.md`.
  - `dev/adversarial/2026-05-21-connect-cluster-codex.md` — the raw Codex transcript.
  - `node_modules/`, `src-tauri/target/`.

## Open / deferred
| Issue | P | State |
|---|---|---|
| `tuxlink-2y5` | P2 | OPEN — manual grid entry. No Settings UI exists yet; needs a config_set_grid command (Maidenhead 4/6-char validate + write_config_atomic) + an edit-grid UI whose **shape is undecided** (inline ribbon field vs Settings panel) → wants a brief brainstorm. Independent; clean as its own bd branch. |
| `tuxlink-lbg` | P2 | OPEN — Codex Med/Low connect-path hardening (bounded DNS, total connect deadline, abort-label accuracy, all-address errors). |
| v0.0.1 release | — | release-please PR on `main` → operator reviews + merges to tag. |
| worktree disposal | — | bd-tuxlink-0ic worktree + branch (merged) — dispose via ADR 0009 ritual when convenient. |

## Decisions made this session
- **2y5 deferred** to focus on shipping the P1 cluster + the gate/merge; it's P2 and
  independent.
- **Codex Med/Low triaged to tuxlink-lbg** (not blockers): current dev/prod hosts are
  single-A, so the DNS/multi-address timeouts aren't hit; #5/#6 are label/diagnostic.
- **Merged via PRs** (not local merges) for the audit trail + the multi-dev pattern;
  `--merge` (no squash, ADR 0010); never `--delete-branch` (worktree) / never force-push.
