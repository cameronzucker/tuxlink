# Session handoff — slate-arroyo-marsh — 2026-06-13

Finished **APRS tactical chat Phase 1a** (the original task) AND designed + built the **frontend redesign** (placement + look) on PR #642. The **native UV-Pro control backend shipped in parallel** (tuxlink-nx95 / PR #647). The next session's job is the **Phase 2 frontend control surface** — but it needs a **single-host mode-switch UX brainstorm first**.

## ⚠️ Read first — state

- **PR #642** (`bd-tuxlink-2f2n/aprs-tactical-chat`, worktree `worktrees/bd-tuxlink-2f2n-aprs-tactical-chat`): **draft, MERGEABLE, CI-GREEN 4/4** at HEAD `a79f6219`. Working tree clean, 0 ahead / 0 behind origin.
- The APRS chat **frontend redesign is code-complete**: Plan 1 (surface) + Plan 2 (dock re-home) both implemented, tested, CI-green.
- **#642 is NOT marked ready** (correct). Remaining gate to ready+merge: **operator on-air smoke**, which is **blocked by `tuxlink-9ky`** (P1, open: the Pi can't Bluetooth-connect to the UV-Pro — BR/EDR Page Timeout / EHOSTDOWN). Neither the KISS chat nor native control can be on-air-smoked on the Pi until that's resolved (or smoked from another host).
- **No cold cargo on this Pi** — all Rust validated via GitHub CI. Keep that posture.

## What this session did

1. **Original task — finished.** The handoff's "CI queue stall" was actually a **merge conflict with `main`** (silently skips `pull_request` CI). Merged `origin/main`, resolved, CI went green. Phase 1a complete.
2. **Operator reviewed the built UI** → decided to redesign it (the shipped UI was an `'aprs'` sidebar pseudo-folder placeholder, and the styling needed work).
3. **Scope finding (evidence-checked):** the **native UV-Pro protocol was never built** — only the KISS layer. Operator launched a **parallel agent** for the native backend → it **shipped** (see below).
4. **Frontend design brainstorm → spec** (`docs/superpowers/specs/2026-06-12-aprs-tactical-chat-frontend-ia-design.md`, on the #642 branch): **Placement C** — chat lives in the **shared switchable right dock** (APRS chat ⇄ Modem console; connection driven from the status-bar Connect button so no capability lost). **Entry ① status-strip control + ② dock tabs** (③ View-menu deferred). **Office register**, not tactical-toy. (Design mocks rendered to `worktrees/bd-tuxlink-2f2n-…/dev/scratch/*.png` — gitignored.)
5. **Plan 1 + Plan 2** written (`docs/superpowers/plans/2026-06-12-aprs-chat-{surface-redesign,dock-rehome}.md`) and **executed via subagent-driven-development** (3 subagents; parent committed each per the worktree-commit rule).
6. **Native backend assessed** (see below) — the "both modes" wiring is the next phase.

### Frontend commits on `bd-tuxlink-2f2n/aprs-tactical-chat` (this session)
- `251a4efa` feat(aprs): surface redesign — timestamps, ACK time, 67-char counter, open-channel cue (Plan 1)
- `922a851e` feat(aprs): re-home tactical chat into the shared dock + entry points (Plan 2)
- `1fa830eb` merge: origin/main (32 commits) — resolved additive shell conflicts (my `aprs` prop vs main's identity-switcher props)
- `a79f6219` fix(aprs): force 24-hour `formatTime` (locale-deterministic — CI's en-US locale rendered "02:08 PM" and broke the anchored `Acked HH:MM` test)
- (+ the spec & two plan doc commits `b707e094`, `bc9dbb3f`, `7fe7705a`)

## Native UV-Pro control backend — SHIPPED (tuxlink-nx95 / PR #647)

Built by `thistle-willow-chasm`. **Already in our branch** (came via the `origin/main` merge). RE'd from **BenLink** (Python protocol-doc) + **HTCommander**, cross-validated, golden byte-vectors committed. Non-transmitting by construction (RADIO-1-clean). CI-green; PR #647 marked ready.

- **Module:** `src-tauri/src/winlink/ax25/uvpro/` (9 files).
- **Frontend contract:** [`docs/design/uvpro-control-api.md`](../../docs/design/uvpro-control-api.md) — 7 commands (`uvpro_connect/disconnect/get_status/get_channels/set_channel/set_frequency/set_mode`) + `uvpro:status` event (channel/freq/mode/battery/RSSI/TX-RX). DTOs `UvproStatus`/`UvproChannel`. Error kinds incl. `LinkBusy`.

## The next phase (tuxlink-ve3j) — brainstorm BEFORE wiring

**`tuxlink-ve3j`** (filed; deps: nx95 + 2f2n): wire native control into the dock, filling the `controlStrip` seam (`AprsChatPanel.controlStrip` prop).

**CRUX — single-Bluetooth-host.** The UV-Pro takes ONE BT connection: **KISS (chat messaging) XOR native (device control), never both.** The native backend does *control only*; messaging stays KISS. So the APRS listener and native control **cannot hold the radio at once** (`uvpro_connect` rejects `LinkBusy{holder}`). Wiring "both modes" therefore needs a **UX decision first** — e.g. does control live in the chat's `controlStrip` (read-only while listening?) or a dock device-view you switch to that pauses the listener? How does `LinkBusy` surface? **Brainstorm this visually in the real dock shell** (same pattern as the placement study), then Plan 3 → build.

## Pending decisions / deferred (non-blocking)
- **Entry ③ (View-menu `menu:view:aprs_chat`)** — deferred: crosses the Rust `menu_event_ids` parity contract (`menuModel.ts`); a small cross-language follow-up.
- **Native backend Codex adrev** (`tuxlink-bv0b`) — the RE'd protocol shipped without the cross-provider adrev (operator de-gated on quota). Quality gap worth closing for a correctness-critical RF protocol.
- **`tuxlink-9tq3`** — flaky `GpsSourcePicker 'rescans on demand'` test (unrelated to APRS; intermittently reds CI; needs a `waitFor` on the rescan count).
- **Dock-state polish** — clicking Connect while on the APRS tab won't auto-flip to the Modem console (one tab away; status strip shows the connection). Decide during the smoke.

## Worktree state
`worktrees/bd-tuxlink-2f2n-aprs-tactical-chat` — tracked clean, all pushed (HEAD `a79f6219`). Untracked/gitignored on disk: `node_modules/` (pnpm-installed for the pre-push lint:docs hook), `dev/scratch/*.{png,html}` (design mocks — placement/entry-point study). No at-risk content. **Do NOT dispose** — active until PR #642 merges (then ADR-0009 ritual).

Also live: the native backend's worktree `worktrees/bd-tuxlink-nx95-uvpro-benshi-control` (its handoff documents its untracked `dev/scratch/benshi-re/` RE clones).

Agent: slate-arroyo-marsh
