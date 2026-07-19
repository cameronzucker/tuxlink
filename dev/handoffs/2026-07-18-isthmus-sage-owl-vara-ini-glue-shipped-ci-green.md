# Handoff — 2026-07-18 (isthmus-sage-owl): VARA.ini increment 2 (app glue) shipped, CI green

Continuation of `tuxlink-iww9r` from the fir-towhee-maple handoff. **Increment 2 — the
app-crate stop-edit-start glue — is built, adversarially reviewed, and CI-green on draft
PR #1156** (branch `bd-tuxlink-iww9r/vara-ini-config`, head `c63669de`). The PR stays
DRAFT on purpose: the remaining increments are coordination-gated (see below).

## What shipped (4 commits)

- `3057247d` **feat(vara): the glue module** `src-tauri/src/winlink/modem/vara/ini_config.rs`
  - Prefix + instance resolution: `drive_c/"VARA HF"` (engine layout) then `drive_c/VARA`
    for `Primary`; `drive_c/VARA2` for `Vara2`. Grounded against the live R2 install
    (`~/.wine-vara`: VARA on cmd port 8300, VARA2 on 8400, both `VARA.exe`+`VARA.ini`).
    Prefix is a parameter; default = engine's `~/.local/share/wine-vara/prefix`.
  - Stop → settle → read → backup → edit → atomic write → relaunch. Settle = INI mtime
    stability wait (VARA rewrites the file on exit). Relaunch = `ManagedModem::spawn_configured`
    (NEW: env overlay + cwd — `WINEPREFIX`/`WINEDEBUG=-all`/`WINEARCH=win64`, cwd = install
    dir) verified against the POST-edit cmd port.
  - Tauri commands `vara_ini_read` (redacted content only) / `vara_ini_apply`; child parks
    in managed `Arc<VaraProcessSlot>`; leaf crate's `is_sensitive_key` made pub so edit
    logging masks the registration code.
- `276da18c` **fix(vara): all 5 Codex adrev findings** (gpt-5.5 xhigh, transcript local-only
  in `dev/adversarial/2026-07-18-iww9r-ini-glue-codex.md` on the Pi):
  1. P1 — pidfile kills now require ATTRIBUTION to this prefix+instance (full unix exe
     path in argv, or instance-dir form + `WINEPREFIX` in `/proc/<pid>/environ`);
     unattributable wine/VARA processes AND their pidfile are left alone, fail-closed.
  2. P1 — stop+edit runs inside NEW `VaraSession::with_session_excluded` (holds the same
     inner mutex `vara_open_session` holds across connect → opens serialize vs the bounce).
     Relaunch port-wait deliberately OUTSIDE the mutex (WINE cold start ~45 s budget;
     `snapshot()` must not block).
  3. P2 — target cmd port must be free before launch (a foreign listener could fake the
     verification); refusal is pre-mutation.
  4. P2 — 8300 factory default is Primary-only; `Vara2` requires an explicit port (INI or
     edits), refused pre-mutation otherwise; `VaraIniApplyReport.cmd_port` is now `Option`.
  5. P2 — backups are collision-safe (`create_new` + `-N` suffix on same-second stamps).
- `5df5a394` fix: stop-edit closure is `FnMut` → `mut` binding (caught by R2 before CI).
- `c63669de` **test(vara): fork→exec cmdline race** — CI failed
  `unattributable_vara_process_is_left_alone` because `/proc/<pid>/cmdline` read during the
  child's fork→exec window shows the PARENT argv. Both pidfile tests now wait for the
  child's own argv. Test-only.

## Verification provenance

- **CI (authoritative): ALL GREEN on `c63669de`** — CI (verify amd64+arm64), ECT
  (low-floor) build, Release build.
- **R2** (`r2-poe:~/tuxlink-iww9r-build`, rustup stable 1.96, clean checkout `c63669de`):
  `cargo clippy --workspace --all-targets --locked -- -D warnings` green; all 21
  `ini_config` module tests green. This is compile+run validation of the monolith code —
  the Pi never compiled it (as designed).
- NOT validated: a real WINE/VARA bounce end-to-end (no VARA on the Pi; R2 has the install
  but running the binary bounce is an operator call). The engine parity (`wv_wineenv`,
  `wv_stop` semantics) is by construction, worth an operator smoke when convenient:
  `vara_ini_apply` against `~/.wine-vara` on R2 with a trivial `ALC Drive Level` edit.

## ⚠️ Remaining increments — BOTH coordination-gated, do NOT free-run

1. **Device-name bridge** (`tuxlink-hq9g0`, P2, open/unclaimed): INI stores devices by
   VARA's WINE-audio names; enumeration feeding `vara_ini_apply` must live in VARA's
   namespace. hq9g0 carries the COLLISION WARNING with the Routines epic (router.rs +
   config.rs). Cameron intends to hand it to the Routines-aware agent.
2. **MCP tool wiring** in `tuxlink-mcp-core/src/router.rs` so Elmer can call this —
   explicitly deferred pending Routines coordination (same collision surface).

PR #1156 stays **DRAFT** until the feature is whole (ADR 0022); do not mark ready or merge.

## Worktree / state inventory

- Pi worktree `worktrees/bd-tuxlink-iww9r-vara-ini-config` — KEEP (claims tuxlink-iww9r,
  PR open). Tracked tree clean, everything pushed. Gitignored-on-disk: `node_modules/`,
  `src-tauri/target/`, `dev/adversarial/2026-07-18-iww9r-ini-glue-codex.md` (raw Codex
  transcript, local-only per policy).
- R2 build worktree `r2-poe:~/tuxlink-iww9r-build` — KEEP as the increment-3 compile cache
  (detached at `c63669de`, clean; `validate*.log` scratch files on disk). Disposal, when
  the PR lands, is the ADR 0009 ritual run ON R2 from `~/Code/tuxlink`.
- Main Pi checkout: untouched all session (operator state, `bd-tuxlink-ant8s/...`).
- bd: `tuxlink-iww9r` in_progress (notes current); `tuxlink-hq9g0` open/unclaimed.

## Gotchas rediscovered this session (for the next agent)

- R2 non-interactive ssh gets distro cargo 1.75 (edition2024 failures) — `export
  PATH=$HOME/.cargo/bin:$PATH` first.
- `ssh host 'a && b && nohup c & echo done'` backgrounds the WHOLE `&&` list: if `b`
  fails, `c` silently never launches and the echo still prints. Launch nohup jobs in a
  separate ssh after the setup steps succeed.
- The vara-named worktree path makes any naive "vara-in-cmdline" process test flaky on
  local runs — the fork→exec wait in the tests is load-bearing.

## Moniker

isthmus-sage-owl.
