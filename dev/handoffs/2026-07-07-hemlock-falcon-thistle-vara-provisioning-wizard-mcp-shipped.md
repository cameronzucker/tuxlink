# Handoff — 2026-07-07 — hemlock-falcon-thistle

**Session: stood up a new public repo for VARA-under-WINE provisioning, then wired it into
Tuxlink end-to-end (first-run wizard + VARA panel + Elmer MCP), wire-walk-gated.**

## What shipped

### 1. New public repo — `wine-vara-setup`
- **github.com/cameronzucker/wine-vara-setup** (public, MIT) @ `5c43d51`. CI green.
- Dependency-light Bash tool: provisions VARA HF under native WINE on **x86_64** via an
  idempotent checkpoint pipeline (`deps→prefix→vara→vb6→ocx→verify→autostart`). Three faces
  over one pipeline: whiptail TUI, headless flags, `--json` JSONL contract. 40 bats, shellcheck-clean.
- Hard constraints (operator-set, brainstormed): x86_64 only; never bundle/download VARA (user
  supplies the `.exe`); never contact winlink.org server-side (IP-blocks probes); provisioning is
  prep-time/online (apt + winetricks download), never in the field.
- Adversarial rounds (Codex + self): JSONL newline escaping, do_ocx fail-on-any-regsvr32, both-port
  verify, pidfile-kill guard, non-Debian `do_deps` guard, systemd space-safe quoting.

### 2. Tuxlink integration — **PR #1035 (READY)**, branch `bd-tuxlink-w7212/vara-setup-wizard`
Engine vendored to `src-tauri/resources/wine-vara-setup/` + `bundle.resources`; rosmodem added to
the shell-open capability allowlist. **All 3 wire-walk flows traced ✅ (see the PR comment):**
- **Flow 1 (fresh install / first launch):** wizard `location → vara_provision → complete`;
  `StepVaraProvision` (self-skips on non-x86_64 / unbundled engine) → shared `src/radio/VaraProvision.tsx`.
- **Flow 2 (upgrade from VARA-less config):** "Set up VARA HF…" entry point in `VaraRadioPanel`
  (gated on `!platformBlocked && engineAvailable`) → same shared component. (Wire-walk originally
  found the one-time wizard was the *only* entry point — this fixed it.)
- **Flow 3 (Elmer):** 3 MCP tools on `TuxlinkMcp` (`vara_engine_available`, `vara_install_status`
  read/ungated; `vara_install_start` ungated — NON-TRANSMIT, pkexec is the operator-presence gate)
  → `ProvisionPort` → `MonolithProvisionPort` → shared `install.rs::run_*`. Wired into BOTH
  `McpState` constructors. Playbook `tuxlink://playbook/vara-wine-setup` in the knowledge catalog.
- Backend: `winlink/modem/vara/install.rs` (`vara_install_start` spawns engine, streams
  `vara_install:progress`, returns summary). **CI verify green both arches @ `cdb57091`** (clippy
  `-D warnings` + cargo test + typecheck + vitest + build).

**Flow 2b DROPPED by operator:** a `vara_setup_offered` config flag (→ schema v6) to *proactively*
prompt on upgrade. The panel entry already makes it reachable at moment-of-need; no schema
migration rides 0.82.0.

## Critical context correction (this session)
- A mid-session scare — "VARA under WINE doesn't work on R2" — was **external config, NOT VARA**.
  SSH diagnosis on R2 (`wpctl`): VARA's audio streams register + run fine on the real USB C-Media
  cards. The wall was PTT: `VARA.ini PTTPort/CATPort=COM1 → /dev/ttyS0` (dead legacy port) while
  the radio's CP2105 is `/dev/ttyUSB0/1` = com33/com34, PLUS `administrator` not in `dialout`.
  Operator confirmed "issues external to VARA"; another agent owns the R2 config fix. The
  VARA-under-WINE premise is VALID. (Memory corrected: `project_vara_under_wine_nonfunctional` deleted.)

## State
- **Branch `bd-tuxlink-w7212/vara-setup-wizard`:** PR #1035 ready, CI-green. Working tree: this
  handoff. Untracked (gitignored): `dev/adversarial/*-codex.md`.
- **Worktree** `worktrees/bd-tuxlink-w7212-vara-setup-wizard` (has `node_modules` from `pnpm install`).
- **bd tuxlink-w7212:** built; PR #1035 ready (in_progress until merged).
- Codex was quota-limited on the Flow 3 round (resets ~05:31); CI verify (authoritative) covered it.

## Pending (next session / operator)
1. **Operator: review + merge PR #1035** (no auto-merge; operator merges). ECT/Release packaging
   builds were finishing at handoff — confirm green before merge.
2. **Real on-air VARA exchange is STILL unproven** — provisioning installs a working VARA, but a
   completed CONNECT to a live gateway on R2 has never happened (operator-only, on-air). Separate
   from this PR.
3. Optional: Flow 2b (proactive upgrade prompt, schema v6) if the operator later wants it.

Agent: hemlock-falcon-thistle
