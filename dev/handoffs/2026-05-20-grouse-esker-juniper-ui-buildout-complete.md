# Handoff — 2026-05-20 — Autonomous UI Build-Out COMPLETE (functional + foundation; fidelity polish remains)

**From agent:** `grouse-esker-juniper` (continued `oriole-lichen-bayou`'s mid-flight handoff of `dev/plans/2026-05-19-autonomous-ui-buildout.md`).
**Outcome:** Both operator milestones (M1 wizard, M2 main UI) smoked + approved. **The v0.0.1 UI is functionally complete and foundation-styled.** It is NOT yet at full mock fidelity — that's the one substantial remaining slice (`tuxlink-cbz`).
**State:** `feat/v0.0.1` @ `bb1b2cb`. Main checkout on `task-amd-main-ui` (operator branch, untouched except this handoff). **All worktrees disposed; nothing in-flight.**

---

## What landed on `feat/v0.0.1` this session (merge sequence)

| PR | What | SHA |
|---|---|---|
| #81 | Task 13 reading pane (fix: NotFound state, raw-size copy, pre-wrap) | `a9d626b` |
| #83 | **M1 wizard stack collapse** — Tasks 10 + 11.5 + 11 (all commits preserved) + consent P0 fixes | `0fda658` |
| #84 | **Orchestrator integration commit** — AppShell region wiring, IPC registration, `config_read`/`backend_status`, compose capability + routing, `session_log_snapshot` | `3a94790` |
| #85 | **Global CSS foundation** — reset/box-sizing, central `--tux-*` tokens + amber accent, dark base+form theming | `bb1b2cb` |

(#75 Task 10 / #82 Task 11.5 auto-merged via the #83 stack-collapse; their branches are deleted.)

The complete v0.0.1 UI is now on `feat`: first-run wizard (CMS + offline + mocked test-send) → main shell (ribbon, folder sidebar, tabbed list, reading pane, session log, status bar) → separate compose window → system tray (close-to-tray).

## Milestones

- **M1 (wizard):** operator-smoked mocked first-run flow + keyring write. PASSED. Codex Part-97 re-round on the consent fix was CLEAN (no double-transmission path).
- **M2 (main UI):** functional smoke PASSED. Surfaced the visual gap (below); the foundation CSS fix was re-smoked + operator-approved ("much cleaner"). Tray close-to-tray confirmed expected (the Pi has a working `wf-panel-pi` tray; the icon appears there — right-click → Quit, or File→Quit / Ctrl+Q).

## The keyring incident (resolved)

Operator's M1 CMS smoke hit a locked login keyring. **Root cause (forensics):** `~/.bash_history` line 1809 — a `gnome-keyring-daemon --unlock`-with-typed-password recipe run today re-keyed the login keyring with an unrecoverable password (before that it was password-less/auto-unlocking on this auto-login Pi). **Resolution:** operator moved the broken keyring aside (`login.keyring.broken-20260520.bak`) + recreated it password-less via a `secret-tool` probe (blank password). **Systemic fix filed: `tuxlink-cnd` (P1)** — the keyring integration tests write to the operator's REAL system keyring (only `XDG_CONFIG_HOME` is isolated, not `XDG_DATA_HOME`/`HOME`), a cross-project hazard (geographica shares the keyring). Also `tuxlink-qn8` (P2) — the wizard should pre-flight a locked keyring with in-app guidance on auto-login systems.

## ⚠️ THE MAIN REMAINING WORK — `tuxlink-cbz` (visual fidelity, in_progress)

**The autonomous build had NO visual-fidelity gate.** It gated logic (Codex caught P0s in most tasks) + functional structure (unit tests), but the global design layer from the mocks (`docs/design/v0.0.1-ux-mockups.md` + `docs/design/mockups/images/`) was never implemented. M2's smoke caught it (browser-smoke-before-ship vindicated).

- **DONE (foundation slice, PR #85):** global reset + box-sizing (fixed "floating" + cramming), central `--tux-*` tokens, amber accent `#e8923a` from `mock-d-mailapp-minimal`, dark base + form-control theming (fixed light-on-dark wizard/forms), retitle.
- **REMAINING (the polish, `tuxlink-cbz`):** (1) `src/compose/Compose.css` carries a self-contained **Catppuccin** palette that ignores `--tux-*` — reconcile it onto the central tokens (the compose window looks stylistically different right now); (2) apply the amber accent to components (Reply button, unread dots); (3) match the mocks' typography/spacing/aesthetic per `mock-d-mailapp-minimal.png`. This is **design-engaged work** — best done fresh with the visual companion + the mocks open, with operator direction.

## Process lessons (for the transfer goal)

1. **No visual/frontend gate** — the biggest gap. `tuxlink-cbz` (the UI gap) + `tuxlink-b2s` (CI `build-linux` path filter excludes `src/**`, so frontend-only PRs run NO CI). A frontend gate (tsc + vitest in CI on `src/**`, + a visual-fidelity check against the mocks) would have caught this before the smoke.
2. **Cross-provider Codex gate was load-bearing** — caught Task 11's 2 Part-97 P0s (double-transmission, Busy-corrupts-consent) and the integration commit's 2 P1s (compose window had zero Tauri capability → no IPC; missing `session_log_snapshot`) — all behind green unit tests.
3. **Codex stdin hang:** `codex exec "…"` in a backgrounded non-TTY context hangs reading stdin; always append `< /dev/null`. Run rounds one at a time (concurrent codex share `~/.codex/` state).

## bd state

- **49 issues closed.** This session closed: Task 13 (`y5c`), the wizard cluster (`1r5`/`d76`/`e4x`), the integration commit (`8zg`).
- **Open follow-ups:** `cnd` (P1 keyring test isolation), `cbz` (in_progress — visual fidelity polish, **the headline next work**), `b2s` (P2 CI frontend gap), `qn8` (P2 wizard locked-keyring UX), `9w8` (P2 Busy stuck-state), `22l` (P2 live Pat bootstrap / **CMS:8773 verification** — the live round-trip is unverifiable until this lands), `cs7`/`gkn`/`n65` (P2 Tasks 17/18/19 — AppImage/README/CI, non-UI), `nk7` (P2 Task 6 live-CMS), `h2y`/`xx3`/`g3d`/`fzm`/`2a7`/`f1a`/`8zt` (P3 hardening/polish).

## Worktrees + working tree

- **All 11 worktrees disposed** (ADR 0009 ritual: inventory → all clean except a throwaway `Cargo.lock` delta → rm -rf → prune). None in-flight.
- Main-checkout untracked (pre-existing, harmless): `dev/scratch/`, `src-tauri/gstshark_*/`, `src-tauri/sidecars/`.
- Codex transcripts in `dev/adversarial/` (gitignored, local-only).

## Nothing pending operator decision

The Part-97 consent decision was made (click-exception governs §3.8; reconciled). The live CMS:8773 verification is operator-gated future work under `tuxlink-22l` (needs a configured/running Pat — stubbed today).

Agent: grouse-esker-juniper
