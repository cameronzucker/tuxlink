# Handoff — 2026-07-19 (fir-towhee-maple): VARA.ini agent config (in-flight) + session loose ends

Long multi-thread session. The **in-flight work to continue is `tuxlink-iww9r` — agent-drivable
VARA setup via `VARA.ini` edit + relaunch** (draft PR #1156). Increment 1 (the pure INI core) is
built, locally tested, clippy-clean, and pushed. The rest is app-crate glue + a coordinated MCP seam.

## ⭐ Continue here: `tuxlink-iww9r` / branch `bd-tuxlink-iww9r/vara-ini-config` / draft PR #1156

**Feasibility is settled with running code, not a claim.** Confirmed against the live VARA under
WINE on R2 (`ssh r2-poe`, `~/.wine-vara/drive_c/VARA/VARA.ini`): VARA persists ALL config to a
plaintext INI in its install dir — `[Soundcard] Input/Output Device Name` + `ALC Drive Level`,
`[PTT] Rig/PTTPort/CATPort/Baud`, `[Setup] TCP Command Port/Callsign Licence/Registration Code`,
`[Position]`, etc. Editing the file + relaunching VARA configures it deterministically, no GUI
automation. **This overturns the prior "agents can't touch VARA setup" belief** (which conflated
"can't drive VARA's GUI" with "can't configure VARA"). `Output Device Name` is exactly the
TX-audio knob behind the winlink-programs-group FT-991a thread.

### Done this session (increment 1 — commit on the branch)
- New leaf crate `src-tauri/tuxlink-vara-ini/` (added to `src-tauri/Cargo.toml` members). Leaf crate
  ON PURPOSE: the monolith can't cold-build on this Pi, but a leaf crate red-green tests locally
  (2.9s). 7 KATs, `clippy --all-targets -D warnings` clean, MSRV 1.75, zero deps.
- Capabilities: round-trip **byte-exact** INI (preserves unknown sections/keys/blank lines + CRLF vs
  LF — never destroys settings it doesn't understand); section-scoped `get`; `set`
  updates-in-place / inserts-in-section / appends-section; redaction of `Registration Code*` (paid
  license key) + `Password encryption` in `redacted()`/`Debug`. The unredacted file bytes are only
  reachable via explicit `render()` (deliberately NOT `Display`, so `{}` can't leak the key).

### Next (in priority order)
1. **App-crate glue** (the monolith — CI-paced, can't cold-build locally; write + let CI validate,
   arm clippy traps):
   - Resolve the `VARA.ini` path per active WINE prefix (VARA vs VARA2; prefix is operator-config).
   - **Stop → edit → start lifecycle** — VARA rewrites the INI on exit (it saves `[Position]`), so
     edit-while-running is clobbered. The glue must own the VARA bounce (Tuxlink already manages the
     VARA process; wire into `src/winlink/modem/vara/`). Atomic write + timestamped backup.
2. **Device-name bridge** — the INI stores devices by NAME as VARA sees them (WINE audio layer),
   which may differ from raw ALSA names. Enumeration feeding a `set` must reflect VARA's namespace.
   Ties directly to `tuxlink-hq9g0`.
3. **MCP tool wiring** in `tuxlink-mcp-core/src/router.rs` so Elmer can call it — **DEFERRED to
   coordinate with the Routines epic** (router.rs is the collision surface; see below).

## Related open issue: `tuxlink-hq9g0` (P2, UNCLAIMED) — audio-device MCP tooling is mode-tied
Audio enumeration is bolted onto ARDOP + FT8 (`ardop_list_audio_devices`, `ft8_list_audio_devices`)
but VARA — the most common mode — has neither a list nor a device-set tool (`config_set_vara` sets
only bandwidth). Fix = one station-level `list_audio_devices` + VARA device selection via the
existing `StableAudioId`. **This is the set-side that `iww9r` implements the mechanism for.**
`hq9g0` carries a COLLISION WARNING: it and `iww9r`'s step-3 both edit `router.rs` (tool_router) +
`config.rs`, which the **Routines epic actively modifies**. Cameron intends to hand `hq9g0` (and the
`iww9r` router seam) to the Routines-aware agent so the seam is built once, coherently. Consider a
`bd dep` edge from `hq9g0` → the Routines issue that owns the current router.rs changes.

## Other session outcomes (already landed / actionable — NOT in-flight)
- **FT-8 Station-Intelligence M3** shipped earlier: PR #1043 (merged).
- **MCP shim packaging defect** `tuxlink-5l49z`: fixed + merged (PR #1078). `tuxlink-mcp` is now
  staged into release + ECT `externalBin` with a `.deb`-content guard. CLOSED.
- **Releases un-frozen** (PR #1130, merged) — `.github/RELEASE_FREEZE` removed after verifying all 6
  Routines parts landed incl. the Part 97 consent-closure guard (regression test
  `attended_transmit_run_parks_on_the_configured_consent_port`). Nightly release automation is live.
- **0.92.0 is cuttable on demand:** release-please draft PR #1131 targets exactly 0.92.0.
  OPERATOR runs `gh workflow run release-merge.yml` off-cadence (agents never cut releases). This
  first cut is the artifact for the deferred Routines end-to-end wire-walk before any stable promote.
- **Elmer system prompt** is `ELMER_SYSTEM_PROMPT` at
  `src-tauri/tuxlink-agent-frontend/src/provider.rs` (~L829-901). Public-linkable; no docs mirror.

## Worktree / branch / bd state
- Worktree `worktrees/bd-tuxlink-iww9r-vara-ini-config` — KEEP (active, PR #1156 open). Tracked tree
  clean. Gitignored-on-disk: `node_modules/` (installed for pre-push `lint:docs`), `src-tauri/target/`.
  Nothing at-risk beyond committed+pushed.
- `tuxlink-iww9r`: in_progress (claims this worktree). `tuxlink-hq9g0`: open, unclaimed.
  `tuxlink-5l49z`: closed. `tuxlink-k37zm` (un-freeze): closed.
- Main checkout is on `bd-tuxlink-ant8s/ardop-connect-fixes` (operator state; 91 uncommitted files —
  do NOT touch). Ground against `origin/main` (currently ~`da20c3dd`+), not the main checkout.

## Moniker
fir-towhee-maple.
