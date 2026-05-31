# Handoff — crag-hemlock-kestrel — radio-panel brainstorm → spec → plan complete; P1 ready to dispatch

> **Date:** 2026-05-31 · **Agent:** crag-hemlock-kestrel · **Machine:** pandora (Pi 5)
>
> **Session arc:** started with a tactical PR #164 Codex review, ran into operator-flagged ARDOP UI brokenness, escalated to a full radio-mode UX brainstorm, converged on a design, wrote the spec, then wrote the implementation plan. Both spec and plan are committed; spec is merged, plan is in PR #171 awaiting review. No code work executed beyond the dock-visibility fix in PR #166 (which is now superseded by the redesign).
>
> **Status:** Brainstorm + spec + plan **done**. P1 implementation **not started** — that's the next session's job, and it should start immediately with no rebriefing.

---

## 0. Critical first action — next session

**Dispatch the P1 subagent immediately.** Don't re-read the spec end-to-end. Don't re-discuss decisions. The brainstorm took this entire session; the plan captures every decision. Just execute.

```
1. Invoke superpowers:subagent-driven-development.
2. Plan file: docs/superpowers/plans/2026-05-31-radio-mode-right-panel-implementation.md
3. Phase 1 (the only one to dispatch now) — tasks 1.1 through 1.8.
4. File the P1 bd-issue at start: "feat: RadioPanel shell scaffold +
   bottom-strip removal (radio-panel P1 of 5)" — slug
   `radio-panel-shell`.
5. Create the worktree via new_tuxlink_worktree.py and run the subagent
   from there.
6. After P1 lands (Codex clean + merged + operator smoke), pause for
   operator approval before starting P2.
```

Paste-ready next-session prompt is at the bottom of this handoff.

---

## 1. What's locked in source / git

| Artifact | Location | Status |
|---|---|---|
| **Spec** (5-section design doc) | `docs/superpowers/specs/2026-05-31-radio-mode-right-panel-design.md` | **Merged to main** via PR #169 (v1) + PR #170 (v2 brainstorm-revisions) |
| **Plan** (5-phase TDD implementation plan) | `docs/superpowers/plans/2026-05-31-radio-mode-right-panel-implementation.md` | Committed on `bd-tuxlink-nr21/radio-panel-impl-plan` branch; PR [#171](https://github.com/cameronzucker/tuxlink/pull/171) open awaiting review |
| **README banner** (pre-alpha + de-overselling) | `README.md` top-of-file | **Merged to main** via PR #167 |
| **ARDOP dock-visibility fix** (`tuxlink-mnk4`) | `bd-tuxlink-mnk4/ardop-dock-cold-start` branch | PR [#166](https://github.com/cameronzucker/tuxlink/pull/166) open; **WILL BE CLOSED WITHOUT MERGE in P4.8** — entire ArdopDock goes away in the redesign |
| **PR #164 ARDOP pre-smoke** (o3f2 + j0ij + 60wh) | `feda9de` | Merged earlier this session; Codex clean |
| **release-please v0.7.0** | `595b25f` | Auto-merged by release-please during the session |

---

## 2. bd issue lifecycle

| Issue | State | Disposition |
|---|---|---|
| `tuxlink-o3f2` ARDOP abort during connect | **closed** in PR #164 | done |
| `tuxlink-j0ij` ARDOP ARQBW selector | **closed** in PR #164 | done |
| `tuxlink-60wh` ARDOP WebGUI link | **closed** in PR #164 | done |
| `tuxlink-mnk4` ARDOP dock dead-end | **closed by absorption** target in P4.8 — close the bd issue + close PR #166 without merge | will close in P4 |
| `tuxlink-o264` README construction banner | **closed** in PR #167 | done |
| `tuxlink-74mx` spec | in_progress | closes in P5.5 (after the full implementation lands) |
| `tuxlink-nr21` plan | in_progress | closes in P5.5 |
| `tuxlink-ed51` iframe-embed WebGUI | will close in P4 — resolved by existing Open WebGUI ↗ button (`tuxlink-60wh`), not by absorption into a Full overlay we dropped | will close in P4 |
| `tuxlink-mzr7` TWOTONETEST | same — close as resolved by Open WebGUI ↗ | will close in P4 |
| `tuxlink-1637` PINGACK structured S/N | absorbed into P4.3 (Signal section Quality score) | closes in P4 |

P1 (next) creates its own bd-issue at dispatch time per the plan.

---

## 3. Worktree inventory (live + stale)

All in `worktrees/` at repo root, all gitignored:

| Worktree | Branch | Disposition |
|---|---|---|
| `bd-tuxlink-o3f2-ardop-abort-connect/` | `bd-tuxlink-mnk4/ardop-dock-cold-start` | Operator's running tauri-dev — leave alone until P4 supersedes PR #166 |
| `bd-tuxlink-o264-readme-construction-banner/` | `bd-tuxlink-o264/readme-construction-banner` (merged + remote-deleted) | Stale; dispose per ADR 0009 ritual at convenience |
| `bd-tuxlink-74mx-radio-panel-design-spec/` | `bd-tuxlink-74mx/radio-panel-design-spec` (merged + remote-deleted) | Stale; dispose per ADR 0009 ritual at convenience |
| `bd-tuxlink-nr21-radio-panel-impl-plan/` | `bd-tuxlink-nr21/radio-panel-impl-plan` | **Active** — this handoff lives here; PR #171 pending; dispose after operator review/merge |
| Other stale worktrees from this session arc (n2uz, ecth, qvl, ytg, 926y, 4ek, i8i, 9phd, 1hu, etc.) | various | Stale; dispose at convenience |

Worktree disposal is a separate hygiene task — not blocking the implementation start. Per memory `feedback_shared_cargo_target_dir`, disk isn't tight (442 GB free); the stale worktrees aren't urgent.

No worktree-internal untracked content of concern; the brainstorm screens at `.superpowers/brainstorm/610438-1780202736/content/` are gitignored design artifacts and don't need to ship.

---

## 4. Brainstorm arc — for context if anything in the spec reads strange

The operator pushed back at five points during the brainstorm, each producing a substantive revision:

1. **"This is the wrong approach — we need to define the overall radio UX which will suit each mode"** — rejected piecemeal ARDOP-vs-AX.25 alignment; forced the holistic-paradigm framing.
2. **"Why can we not just do everything in a compact and space-efficient right-hand panel?"** — landed the compact-right-panel paradigm + dropped the reading-pane-panel-for-connection-config pattern entirely.
3. **"Do we fit everything we need in the 360 px or not?"** — dropped the Full overlay (Mode-2 troubleshooting expanded view) + the iframe ambition; collapsed migration from 7 to 5 phases.
4. **"We should just show the main panel size all the time if it's selected"** + **"The session log can't be expanded large at the bottom"** — dropped "compact" terminology and the bottom session-log strip; log moved into the panel.
5. **"Tuxlink is not a useless Windows troubleshooting Wizard"** — dropped the Troubleshoot section's action buttons (Change band / Lower bandwidth / etc.) as patronizing; replaced with Signal section showing dense operator-meaningful data (Quality / S/N trend / frame ribbon).

Plus a sixth pushback on menu congruence: **"Do those same controls apply now that we're incorporating the log into the radio panel?"** — produced the §6.2 complete menu audit (full before/after for every top-level menu, not just deltas).

Every revision is captured in the spec's header revision-history table. If a section reads strange, check the v1→v2 deltas inline.

---

## 5. Per-phase summary for orientation

Once P1 dispatches, the subagent works from the plan. For the operator's overview:

| Phase | Scope | Key outputs |
|---|---|---|
| **P1** shell scaffold + bottom-strip removal | 8 tasks | `src/radio/RadioPanel.tsx`, `useRadioPanelVisibility.ts`, `PlaceholderRadioPanel`. Bottom session-log strip deleted. View → Toggle Radio Panel renamed. AppShell integrates. No mode migration. |
| **P2** Telnet panel | 4 tasks | `TelnetRadioPanel`, `SessionLogSection` (shared). Delete `TelnetCmsPanel`. |
| **P3** Packet panel | 4 tasks | `PacketRadioPanel`, `ModemLinkSection`. Delete `PacketConnectionPanel`. |
| **P4** ARDOP panel + Signal section | 8 tasks (includes Rust) | `ArdopRadioPanel`, `SignalSection`, `Sparkline`, `FrameRibbon`. Rust: PINGACK / PING parsing → `quality: Option<u8>` in ModemStatus. Delete `ArdopDock` + `ArdopHfStub`. Close PR #166 + cascade-close `tuxlink-mnk4`, `tuxlink-ed51`, `tuxlink-mzr7`, `tuxlink-1637`. |
| **P5** Vocabulary cleanup | 5 tasks | `Connect/Disconnect → Start/Stop/Abort`, ribbon Connect button removed, F5 contextual binding, `Show transport` retired. Close `tuxlink-74mx` + `tuxlink-nr21`. |

Each phase is its own PR, own bd-issue, own worktree, own Codex round, own operator smoke. The plan documents the workflow once at the top and repeats the per-phase steps verbatim in each phase's final task (per the skill rule that the implementing engineer may read tasks out of order).

---

## 6. Operator gates between phases

Each phase ends with operator smoke before close. Recommended pause points:
- After **P1** — verify panel mounts/unmounts correctly per visibility rule; bottom strip gone; Ctrl+Shift+M works.
- After **P2** — verify Telnet panel renders, Start fires a real CMS connect.
- After **P3** — verify Packet panel densifies correctly at 360 px, all the AX.25 controls still work.
- After **P4** — full ARDOP flow including the new Signal section; this is the biggest visible change.
- After **P5** — vocabulary is consistent everywhere; F5 fires the right contextual Start.

---

## 7. Quality gates per phase

Same set every phase (Rust-touched phases include the cargo lines):

```bash
pnpm vitest run                                          # frontend tests
pnpm exec tsc --noEmit                                   # frontend typecheck
cargo test --manifest-path src-tauri/Cargo.toml --lib    # backend tests (P4 only)
cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings  # backend lint (P4 only)
```

Plus one Codex adversarial round per PR before merge. Output to `dev/adversarial/2026-05-31-<phase-slug>-codex.md` (gitignored).

---

## 8. What's out of scope through P5

Per spec §8:
- VARA HF / VARA FM panel **implementation** (design-only in spec §5.4; build when the backends ship)
- Ardop P2P intent (v0.1+ ProtocolEntry; panel will inherit Ardop Winlink's chrome)
- Conditions-degraded auto-detection on the Signal section (heuristic deferred; manual ⤢ stays the way in if added later)
- Multi-modem concurrent operation (one active modem at a time)
- Rig control (already `disabled: true`)
- Mobile / narrow-window layout (<1024 px window)
- Persistent dock content (session-timer / outbox / last-sessions) from the original `v0.0.1-ux-mockups.md` §3.5 — that was for the dropped "Off" state of the dock
- Last-sessions / closed-session log browser (sidebar selection + Stopped state shows most-recent only)

---

## 9. Next-session paste-ready prompt

```
The radio-panel brainstorm + spec + plan are complete and committed.
Spec is merged (PRs #169, #170); plan is in PR #171 awaiting review.
Handoff at dev/handoffs/2026-05-31-crag-hemlock-kestrel-radio-panel-
brainstorm-spec-plan-complete.md.

START IMMEDIATELY with superpowers:subagent-driven-development against
docs/superpowers/plans/2026-05-31-radio-mode-right-panel-
implementation.md.

Do NOT re-read the spec end-to-end. Do NOT re-discuss decisions. The
brainstorm took an entire session; the plan captures every decision.
Just execute.

Phase 1 only on this run — file the bd-issue ("feat: RadioPanel shell
scaffold + bottom-strip removal (radio-panel P1 of 5)", slug
`radio-panel-shell`), create the worktree via
`.claude/scripts/new_tuxlink_worktree.py --slug radio-panel-shell
--issue <new-id> --base main --moniker <your-moniker>`, dispatch the
subagent for tasks 1.1 through 1.8. After P1 lands (Codex clean +
merged + operator smoke), PAUSE for operator approval before starting
P2.
```
