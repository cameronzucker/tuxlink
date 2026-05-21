# Handoff — 2026-05-20 — Mock B rebuild accepted; drive to v0.0.1 operational (hemlock-raven-wren)

**From:** `hemlock-raven-wren`. **Branch:** `bd-tuxlink-cbz/fidelity-polish` (worktree `worktrees/bd-tuxlink-cbz-fidelity-polish/`), off `feat/v0.0.1`. **Pushed.** Latest: `1afee84`. **PR #86** open against `feat/v0.0.1` (title/body now Mock B).

---

## 0. TL;DR

The v0.0.1 main UI is rebuilt to **Mock B (principles-faithful)** — the design the operator approved — and grim-validated in the real app. The operator accepted it ("not perfect but not completely divergent now"). The mandate now: **continue to v0.0.1 *operational***. The single biggest gap is that the **Pat backend is still stubbed** (`AppBackend::None`), so the app shows the dev fixture, not real Winlink mail. Making it operational = landing **`tuxlink-22l`** (live backend bootstrap), then the wizard end-to-end + live-CMS smoke, then packaging/docs/CI.

## 1. ⚠️ READ-FIRST — the spec, and the lesson

- **The approved design is Mock B**, full stop: `docs/design/mockups/images/mock-b-principles-faithful.png` + the `MOCK B` block (`data-choice="mock-b"`, lines ~1136-1323) of `docs/design/mockups/2026-05-17-mocks-v1-four-directions.html`. Shared CSS classes lines 7-931.
- **[ADR 0013](../adr/0013-v001-main-ui-is-mock-b-not-mock-d.md)** is the canonical record and the root-cause fix. Two prior sessions built the wrong thing: a synthesis knockoff, then a faithful build of **Mock D** — a target a prior session *misidentified* as approved and recorded in a handoff + bd issue + ADR 0012. **The rule: the operator's approved mock is the SOLE spec; agent-authored handoffs/ADRs/bd issues are derivative and never the spec. When in doubt about the target, verify the artifact with the operator — do not trust a prior session's claim.**
- **Validate UI in the REAL Tauri/WebKitGTK app via `grim`**, never a Chromium gallery. See [[reference_grim_realapp_validation_pandora]] (memory) for the mechanics on this Pi (labwc + wayvnc; a fresh `pnpm tauri dev` launch focuses the window; screenshots → main-checkout `dev/scratch/` so the operator's editor can open them).

## 2. What's DONE this session

- **Mock B rebuild** (`1afee84`), all components faithful + grim-validated:
  - AppShell `layout-b`: dashboard ribbon / panes[ sidebar 200 | list 380 | reader 1fr ] / human session-log / status bar.
  - DashboardRibbon (callsign·grid·GPS·UTC/local·connection), FolderSidebar (Mailbox + Connections), MessageList (3-line rows), MessageView (Reply(⌘R)/Reply All/Forward, From/To/Date/Form + form-attached box), SessionLog (human, Raw/Human toggle), StatusBar (Telnet ready · N unread · v0.0.1 · Pat 1.0.0).
  - Compose stays a separate floating Tauri window (compose_window.rs).
  - Removed: TabStrip + DashboardRibbon.css (Mock-D orphans).
- **Records corrected:** ADR 0013 supersedes ADR 0012; design-doc §3 banner → "approved design: Mock B"; ADR README updated; `tuxlink-yd4` closed as superseded.
- **Window/tray fix** (earlier this branch, `tuxlink-9zd`): Linux minimize-not-hide on close; window 1200×820; tray "Show Window". **Still needs operator verification on the real labwc compositor + X11** (agent can't drive the WM). 9zd remains open.
- **Dev fixture** (`src/mailbox/devFixture.ts`): reproduces Mock B's content (7 inbox rows, N5VSU/ICS-213 selected, dashboard values, session-log lines, Sent total 87). Gated on `import.meta.env.MODE==='development'` → vite dev server only; OFF in tests + release. **This is why the app looks populated despite the stubbed backend.**
- Gates green: `tsc`, `vitest` 296, `vite build`. Screenshots: main-checkout `dev/scratch/mockb-*.png`.

## 3. Path to v0.0.1 OPERATIONAL (the actual remaining build)

Recommended first step: **merge PR #86** to `feat/v0.0.1` (Mock B accepted) so the next task branches off a clean base. Then, roughly in priority order:

1. **`tuxlink-22l` — PatBackend::spawn + app-start bootstrap (THE operational blocker).** Wire the live path (spec §1.1/§3.3): if config exists AND connect_to_cms, locate the Pat sidecar, spawn via PatProcess (renders Pat config + reads keyring cred), construct PatBackend over the announced HTTP port, store Arc in AppBackend, drain `stream_log()` → emit `session_log:line`. Then `mailbox_list`/`message_read`/`backend_status` return REAL data and the dev fixture goes dormant.
   - **⚠️ Part 97 / live-radio rule:** this path can initiate a CMS session (a transmission under the operator's callsign). The agent **writes + commits** the code; the **licensee runs it** manually. Do NOT run a live-CMS binary to "verify completion" (CLAUDE.md live-radio rule). Use `build-robust-features` + a Codex adversarial round.
2. **Wizard end-to-end** must produce a config the backend can consume (config → keyring cred → connect). Related bugs/polish: `tuxlink-9w8` (test-send Busy lock), `tuxlink-qn8` (locked-keyring UX), `tuxlink-fzm` (MOCKED banner race), `tuxlink-2a7` (failure serialization).
3. **`tuxlink-nk7` (Task 6) — live-CMS smoke binary (operator-only)** to prove real send/receive against a CMS gateway. Operator-run only.
4. **`tuxlink-cnd` (P1 bug)** — keyring integration tests write to the operator's REAL keyring (XDG_DATA_HOME/HOME not isolated). Fix test isolation early; it's a P1.
5. **Ship surface:** `tuxlink-cs7` (Task 17 AppImage), `tuxlink-gkn` (Task 18 README + install docs), `tuxlink-n65` (Task 19 CI + release), `tuxlink-b2s` (CI path filter excludes `src/**` — frontend PRs run NO CI; fix so the Mock B work is actually CI-gated).
6. **P3 hardening** (after operational): `f1a` (read-side byte cap), `g3d`/`h2y` (compose-window capability), `xx3` (session-log ring buffer), `8zt` (keyring use_native_store stale).

`bd ready` is the live list. The v0.0.1 plan: `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md`.

## 4. Working-tree / runtime state

- **Tree clean**, branch pushed (`1afee84`), 0 unpushed. PR #86 updated.
- **A `pnpm tauri dev` is RUNNING** (I relaunched for validation; vite `:1420`, log `dev/scratch/tauri-dev.log`). Left up so the operator can review Mock B live. Stop: `kill -- -$(ps -o pgid= -p $(pgrep -f 'pnpm tauri dev') | tr -d ' ')`.
- bd: `tuxlink-yd4` closed (superseded). `tuxlink-cbz` (visual fidelity, in_progress) — the Mock B rebuild fulfills its intent; close it when the operator signs off, or keep as the fidelity umbrella. `tuxlink-9zd` open (tray, operator-verify). No Dolt remote → bd state in local Dolt + committed `.beads/issues.jsonl`.
- Code comments in a few rebuilt files may still say "Mock D" in passing — cosmetic; the code is Mock B.

## 5. Next session — paste-ready starting prompt

```
Resume tuxlink. Last session (hemlock-raven-wren) rebuilt the v0.0.1 main UI to
Mock B (the APPROVED design) — accepted by the operator, pushed, PR #86 open.
READ FIRST: dev/handoffs/2026-05-20-hemlock-raven-wren-mockb.md and ADR 0013.

The approved design is Mock B (mock-b-principles-faithful.png) — the SOLE spec.
Do NOT reintroduce Mock D. Validate any UI change in the REAL app via grim
(labwc+wayvnc; fresh `pnpm tauri dev` focuses the window), never a Chromium proxy.

Goal: drive to v0.0.1 OPERATIONAL. Step 1: merge PR #86 to feat/v0.0.1. Step 2:
tuxlink-22l (PatBackend::spawn + bootstrap) so real Winlink mail flows (the dev
fixture is only filling the stubbed backend). ⚠️ Part 97: WRITE the live-CMS/Pat
code, COMMIT it, let the licensee RUN it — never run a live-CMS binary yourself.
Use build-robust-features + a Codex adversarial round for the backend. Also early:
tuxlink-cnd (P1: keyring tests hit the real keyring). bd ready = live worklist.
```
