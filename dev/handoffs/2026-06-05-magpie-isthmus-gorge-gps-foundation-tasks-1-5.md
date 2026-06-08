# Handoff — magpie-isthmus-gorge — GPS foundation Tasks 1-5/32 landed; Tasks 6-32 continue next session

> **Date:** 2026-06-05 · **Agent:** `magpie-isthmus-gorge` · **Machine:** pandora
>
> **Arc:** Marathon multi-day session that did the full BRF cycle on the GPS setup UX. Brainstorm → 4-bd architectural design → mockup → adversarial review (3 Claude rounds + 1 Codex round) → plan v1 → Codex review of plan (20 findings) → plan v2 incorporating all findings → execution kicked off via superpowers:subagent-driven-development. Five tasks of the 32-task bd-1 chain shipped with full implementer + spec-reviewer + code-quality-reviewer rounds. Pushed cleanly to origin.
>
> **Status at handoff:** Branch `bd-tuxlink-9xy1/gps-foundation` is at `3d54695` with Tasks 1-5/32 of the v2 plan complete. All 5 commits durable on origin. Remaining work is 23 tasks across Rust probes (Tasks 6-12, 17) + frontend types/fixtures (Task 18) + wizard reducer (Task 19) + React component layer (Tasks 20-28) + Wizard.tsx + SettingsPanel.tsx wiring (Tasks 29-30) + smoke + push (Tasks 31-32). The next session resumes via the subagent-driven-development skill with Task 6.

---

## 0. Critical first action — next session

```
1. Read THIS handoff first.
2. Read docs/superpowers/plans/2026-06-05-gps-setup-bd-1-plan-v2.md — this is the
   APPROVED plan being executed. v1 (sibling file) is superseded; DO NOT execute v1.
3. Read dev/adversarial/2026-06-05-gps-bd-1-plan-review-codex.md (gitignored) for the
   Codex review findings the plan incorporates. The corrections table at the top of
   v2 maps each CODEX-N finding to the task that fixes it.
4. Resume execution at Task 6 (probe/types.rs creation). Use superpowers:subagent-driven-development;
   one fresh subagent per task; spec reviewer then code quality reviewer between.
5. Critical: the cargo invocation pattern in the v2 plan is BACKWARDS (`cargo --manifest-path X subcommand`
   doesn't work in this cargo). Use `cargo subcommand --manifest-path X` in every subagent dispatch.
   The same correction was already applied to all dispatches for Tasks 2-4.
6. The `udev = "0.10"` in plan v2 doesn't exist on crates.io. Task 5 already adjusted to `udev = "0.9"` (0.9.3 latest).
   Subsequent probe/serial.rs work should use the 0.9 API — verify any API differences from the plan's pseudocode.
```

---

## 1. Session arc (compressed)

This was an exceptionally long session covering the entire pre-execution BRF cycle for the GPS setup feature, then the start of execution.

**Phase 1: Brainstorm + design.** Office-hours skill produced the 4-bd architectural plan (bd-1 foundation + bd-2 pkexec helper + bd-3 native NMEA + bd-4 live monitoring) with 4 personas (Bob/Sue/Dave/Mike) and the "failure mode is the product" thesis. Visual mockup at `docs/design/mockups/2026-06-04-gps-setup-mocks.html`. Design doc at `docs/design/2026-06-05-gps-setup-ux-design.md`.

**Phase 2: Adversarial review on the design.** Four rounds:
- Codex round 1 (architectural) hit quota mid-prompt (78-line stub at `dev/adversarial/2026-06-05-gps-setup-r1-architectural-codex.md`)
- Claude rounds 2-4 (security, UX persona, implementation feasibility) — 55 findings synthesized into `docs/design/2026-06-05-gps-setup-ux-design-addendum-r2-r4.md`

**Phase 3: Plan v1** via `superpowers:writing-plans` at `docs/superpowers/plans/2026-06-05-gps-setup-bd-1-plan.md` (32 tasks). Self-review flagged Tasks 11-18 sketched at header level.

**Phase 4: Codex round 5 (deferred to post-quota-reset) reviewed the PLAN** (not the design — higher leverage). 20 findings (1 CRITICAL, 14 HIGH, 4 MEDIUM, 1 INFORMATIONAL). Verdict: "Do not dispatch this plan as written." Transcript at `dev/adversarial/2026-06-05-gps-bd-1-plan-review-codex.md` (gitignored, 11,227 lines). Operator decided: every finding gets fixed.

**Phase 5: Plan v2** at `docs/superpowers/plans/2026-06-05-gps-setup-bd-1-plan-v2.md`. Folds in all 20 Codex findings:
- WizardPhase enum + persistence migration (CODEX-1 CRITICAL)
- Three new backend prerequisite tasks
- probe/ directory split (CODEX-20)
- gpsd probe split into wizard 400ms vs settings 1800ms (CODEX-12)
- Three-bucket derivePickerData (sources/triage/diagnostics — CODEX-6 scope fix)
- Tasks 11-18 expanded to full TDD (CODEX-14)
- All other corrections per the mapping table at the top of v2

Mockup updated: State B (Sue's path) split into "bd-3 vision" (green source) + "bd-1 alpha" (diagnostic info card) per CODEX-6. Design doc gained a scope-correction note matching.

**Phase 6: Execution start.** Operator chose Option 1 (subagent-driven). Invoked `superpowers:subagent-driven-development` skill. Tasks 1-5 landed with full review chains. Tasks 6-32 continue in next session.

---

## 2. Branch state

| Branch | State |
|---|---|
| `main` | Untouched this session (operator's main has had unrelated activity from parallel sessions). |
| `bd-tuxlink-9xy1/gps-foundation` | **NEW branch, pushed to origin at 3d54695.** Holds Tasks 1-5 of the v2 plan. CI will likely fail until Tasks 6-32 land (this is an in-flight feature branch, not a ship-ready PR yet). |
| `bd-tuxlink-xygm/recover-handoffs` | Operator's main-checkout branch from session start. NOT TOUCHED by this session except for this handoff doc. |

## 3. Commits on bd-tuxlink-9xy1/gps-foundation

```
3d54695 build(cargo)(deps): add udev 0.9.3 for GPS detection probes (tuxlink-9xy1)   [Task 5]
839fc12 feat(wizard): get_wizard_phase routing with legacy compat (tuxlink-9xy1)    [Task 4]
d090227 feat(wizard): WizardPhase persistence + wizard_persist_gps command (tuxlink-9xy1)  [Task 3]
d67281b feat(wizard): WizardPhase enum for first-class Location step (tuxlink-9xy1)  [Task 2]
fa367526 (was main HEAD when worktree created)
```

Each commit has `Agent: magpie-isthmus-gorge-tN-impl` trailer (T2 / T3 / T4 are subagent monikers; T5 is parent-level since it was an inline 1-line edit). Tasks 2, 3, and 4 each passed both spec compliance and code quality reviews before commit.

## 4. Plan v2 task progress

| Task | Status | Notes |
|---|---|---|
| 1. Worktree + bd setup | ✅ | Inline |
| 2. WizardPhase enum | ✅ | Subagent dispatch, both reviews APPROVED |
| 3. wizard.rs persistence migration + wizard_persist_gps + get_wizard_phase | ✅ | Subagent dispatch (substantial — 14 files touched, all Config{} literal cascades). Drive-by clippy fix on Task 2's `wizard_phase.rs` (impl Default → derive). 29 wizard-scoped tests + 1202-test lib suite green. CODEX-1 fix landed. |
| 4. App.tsx get_wizard_phase consumer (useWizardPhase hook) | ✅ | Subagent dispatch. AppRouter refactor for QueryClientProvider boundary. queryClient.clear() in beforeEach for test cache hygiene. Pre-9xy1 legacy compat handled. |
| 5. udev = "0.9" dep (plan said 0.10, doesn't exist) | ✅ | Inline (one-line edit) |
| 6-12, 17. Rust probes + grid validation + aggregate | ⏭ Next | Tasks 6-12 use the new `probe/` directory (one file per probe per CODEX-20). Task 11 (gpsd) is the most complex — 8 new tests, wizard vs settings probe split, WrongDevice cross-reference. |
| 18. Frontend types + fixtures (serde round-trip) | ⏭ Next | CODEX-8/9 fix — `ContainerStatus` needs `#[serde(tag="kind")]` in Rust to match TS shape. Round-trip test imports same fixture in Rust + Vitest. |
| 19. Wizard reducer + types | ⏭ Next | Add `'location'` step + `SUBMIT_GPS_SUCCESS` action + `pendingDialoutVerification` field. Reorder: this MUST land before Tasks 20-28 (CODEX-3). |
| 20-25. React presentational layer | ⏭ Next | derivePickerData (3 buckets) → SourceCard → TriageCard (full a11y) → ManualGridEditor (always visible) → DiagnosticCard → GpsSourcePickerPresentational |
| 26. useGpsProbeReport hook | ⏭ Next | TanStack Query + Tauri invoke mock pattern |
| 27. Step4Location | ⏭ Next | Wizard container; reads pendingDialoutVerification for resume banner |
| 28. SettingsGpsPanel | ⏭ Next | Uses config_read + config_set_grid + position_set_source({source: "Gps"}) — NO position_set_source_kind (CODEX-5) |
| 29. Wire Step4Location into Wizard.tsx | ⏭ Next | |
| 30. Mount SettingsGpsPanel in SettingsPanel.tsx | ⏭ Next | CODEX-16 fix — read actual file first; existing panel only has privacy controls. |
| 31. Smoke walk per persona via localStorage override seam | ⏭ Next | CODEX-18 fix — NEVER mutate operator's dialout via `sudo gpasswd -d`. |
| 32. Build verification + push + open PR | ⏭ Next | Branch already pushed; final task wraps with PR open. |

**5/32 tasks complete (16%). 27 tasks remain.**

---

## 5. Critical guidance for next session

1. **Plan v2 is authoritative; v1 is superseded.** Always read v2 first. The Codex-finding → task mapping table at the top of v2 (and the addendum at `2026-06-05-gps-setup-ux-design-addendum-r2-r4.md`) explain WHY each task is shaped the way it is.

2. **Cargo invocation pattern correction.** The v2 plan has `cargo --manifest-path X test` order embedded in many tasks. THIS DOES NOT WORK with this cargo version. Always use `cargo test --manifest-path X --lib ...` (subcommand BEFORE --manifest-path). Apply this correction in every subagent dispatch.

3. **udev = "0.10" → "0.9".** Task 5 already adjusted. Subagent for Task 10 (probe/serial.rs) needs to use the 0.9.3 API — verify against `https://docs.rs/udev/0.9.3/udev/` if the plan's pseudocode references features only in 0.10+ (it shouldn't; the design uses fundamental enumerate/filter/property_value APIs).

4. **Subagent dispatch pattern.** Use the templates from `~/.claude/plugins/cache/claude-plugins-official/superpowers/5.1.0/skills/subagent-driven-development/`. For each task: implementer (with task moniker like `magpie-isthmus-gorge-t6-impl`) → spec reviewer → code quality reviewer → commit landed → next task. Don't skip reviews even for small tasks; the discipline catches bugs (e.g., Task 4's queryClient cache-leak was self-discovered in implementation, but a missed review could have shipped it).

5. **Tasks 11-18 (the React layer) are NOT header-only in v2.** They're full TDD bodies. But Tasks 3, 8, 12, 18, 24 in v2 still have "(Full TDD steps continued — execute via subagent…)" markers where the pattern from earlier tasks gives a subagent enough context to expand in-flight. This is per the ORCH-1 callout — the writing-plans skill's "skilled developer assumption" + the established Task 2 / Task 7 patterns.

6. **The branch is pushed.** CI on bd-tuxlink-9xy1/gps-foundation is likely red (the branch refers to Tauri commands and React components that don't exist yet). This is expected for an in-flight feature branch. Don't waste effort on CI green until Task 32.

7. **Operator decision points the next session may need to surface:**
   - If a subagent finds a v2 plan instruction that conflicts with the actual codebase reality, follow the reality (the spec reviewer for Task 3 verified this with the Config literal cascade — sometimes the plan can't anticipate the full file-touch surface).
   - If Codex quota is available, run a parent-level Codex round after Tasks 6-12 (the Rust probe layer) land — that's the natural batch checkpoint. Otherwise defer to next-session round of post-task adrev.

8. **Pitfalls in active force on this work:**
   - [[browser-smoke-before-ship]] — Task 31's `pnpm tauri dev` walk is mandatory before declaring UI work done.
   - [[no-incomplete-or-internal-refs-in-shipped-features]] — Sue's bd-1 path is a diagnostic info card, NOT a placeholder stub. Direct NMEA reading lands in bd-3.
   - [[gps-precision-reduction]] — 4-char Maidenhead default. ManualGridEditor (Task 23) defaults to 4-char.
   - [[no-disk-creds-default]] — bd-1 doesn't ship pkexec. "Fix it for me" buttons render disabled.
   - DRIFT-1 at every command-registration boundary (Task 17 adds position_validate_grid; register in lib.rs in the same commit).
   - ORCH-1 at every subagent dispatch (worktree paths, log to dev/scratch if helpful).
   - RADIO-1 at Task 31's smoke section (no on-air transmission during testing).

---

## 6. Untouched state (operator owns)

- Operator's main checkout on `bd-tuxlink-xygm/recover-handoffs` with whatever staged/untracked state was there at session start.
- `task-amd-main-ui` — 5 stashes preserved unchanged.
- All other worktrees from prior sessions — operator's to dispose at their cadence.

---

## 7. Next-session prompt (paste into a fresh session)

```
Resume tuxlink bd-1 GPS foundation execution from the magpie-isthmus-gorge 2026-06-05 handoff.

Handoff doc: dev/handoffs/2026-06-05-magpie-isthmus-gorge-gps-foundation-tasks-1-5.md
READ IT FIRST.

State: 5/32 tasks complete on branch bd-tuxlink-9xy1/gps-foundation (pushed to origin).
WizardPhase + persistence migration + App.tsx routing + udev dep all landed and reviewed.

Next: Task 6 (probe/types.rs) per docs/superpowers/plans/2026-06-05-gps-setup-bd-1-plan-v2.md.

CRITICAL CORRECTIONS to apply in every subagent dispatch:
1. cargo invocation order: `cargo SUBCOMMAND --manifest-path X` (NOT `cargo --manifest-path X SUBCOMMAND`)
2. udev = "0.9" (plan v2 says 0.10 — doesn't exist on crates.io)

Resume via superpowers:subagent-driven-development. One fresh subagent per task,
spec reviewer THEN code quality reviewer between. Don't skip reviews even for small tasks.

The Codex review of plan v2 produced 20 findings (1 CRITICAL, 14 HIGH, 4 MEDIUM, 1 INFO).
All are folded into the v2 plan; the mapping table at the top of v2 maps each finding
to the task that fixes it. Honor every finding; operator was explicit: alpha candidate,
no skipping.
```

---

Agent: magpie-isthmus-gorge
