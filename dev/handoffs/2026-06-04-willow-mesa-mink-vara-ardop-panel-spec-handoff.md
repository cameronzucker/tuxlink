# Handoff — willow-mesa-mink — VARA + ARDOP panel alpha-polish spec (ready for plan + adrev + impl)

> **Date:** 2026-06-04 · **Agent:** `willow-mesa-mink` · **Machine:** pandora
>
> **Arc:** Continuation from the same `willow-mesa-mink` session that landed PR #348 (listener feature E2E) + opened PR #350 (tuxlink-tccc arming label fix). Operator hit the converged build, found that the VARA panel's outbound side was missing entirely and the listener UX was fragile, framed the broader need as "alpha-bound polish." This session brainstormed the design for ARDOP + VARA HF + VARA FM panels together; spec is committed and ready for plan-writing + adrev + implementation in a fresh session.
>
> **Status at handoff:** Spec on disk at [docs/superpowers/specs/2026-06-04-vara-ardop-panel-alpha-design.md](docs/superpowers/specs/2026-06-04-vara-ardop-panel-alpha-design.md) (commit `73333e2`). Operator approved the shape. Pipeline next: `writing-plans` → `build-robust-features` (includes the cross-provider Codex adrev round) → implementation.

---

## 0. Critical first action — next session

```
1. Read THIS handoff first.
2. Read the spec at docs/superpowers/specs/2026-06-04-vara-ardop-panel-alpha-design.md — it is the source of truth for the redesign.
3. Read the brainstorm companion mocks at docs/design/mockups/2026-06-04-vara-panel-mocks.html
   (v2 mocks show the intent matrix + state progression; they predate the
   session-lifecycle correction so they still show explicit Arm buttons.
   The spec supersedes them on that point.)
4. Invoke the writing-plans skill against the spec. Per the brainstorming-skill terminal state, writing-plans is the canonical next step. After the plan lands, invoke build-robust-features for the cross-provider adrev round and the subagent-driven implementation.
5. bd issue tuxlink-0ye6 is the umbrella. Claim it at session start. It declares depends-on tuxlink-fzl7 (VARA Phase 3 outbound) and tuxlink-12sc (VARA disarm ABORT). Both subsume into the impl.
6. Decide PR #350 (tuxlink-tccc) disposition: the disabled-prop addition to ListenArmButton is still useful, but the VARA-specific use of !isOpen goes away with auto-arming. Either close + supersede, OR merge as a defensive fix that lasts until the bigger redesign lands. Operator's call.
```

---

## 1. Session arc (this turn only)

This was a single brainstorming arc inside an already-long `willow-mesa-mink` session.

1. Operator hit converged build, saw VARA outbound was missing + listener UX was fragile (perpetual "Arming…" label from PR #348). Filed tuxlink-tccc as a quick fix; opened PR #350.
2. Pushed back on the proposed quick-fix: "Arm Listener is now just disabled" — they wanted auto-open transport on Arm. I started to implement; operator interrupted with a bigger framing: the outbound side doesn't exist at all, panel is half-built, alpha-bound work needs to address both halves.
3. Switched into brainstorming mode. Initially invoked `office-hours` (wrong tool per project convention — operator correctly pointed out brainstorming is the brf-pipeline starter). Pivoted to `superpowers:brainstorming`.
4. Built v1 mocks (mocks A/B/C with in-panel Dial-as toggle). Operator rejected: intent toggle is a category error because sidebar already drives intent; redesign for both VARA AND ARDOP consistency.
5. Built v2 mocks (intent-aware, sidebar-driven). Confirmed intent matrix: cms + p2p + radio-only, with listener for p2p AND radio-only (tuxlink divergence from WLE).
6. Proposed lifecycle model with Open Session / Close Session lifecycle button. Operator approved the shape but flagged WLE actually auto-arms listener when session window opens — tuxlink needs to address the disparity.
7. Multiple operator corrections during brainstorming:
   - **RADIO-1 is agent-internal**, not user-facing architecture. Drop from UI vocabulary + identifier surface.
   - **Alpha = vettedness, not built-ness.** No minimal first slices. Saved as memory `alpha-is-vettedness-not-built-ness`.
   - **No tuxlink-added safeguards.** Drop CONNECT_DEADLINE = 120s (added reactively to a 2026-05-22 incident, not grounded in WLE or Part 97). Drop ConsentModal. Mirror legacy WLE behavior. Saved as memory `no-tuxlink-added-safeguards`.
   - **Legacy WLE session lifecycle is load-bearing.** Start opens the SESSION (a longer-lived state); for P2P, listener auto-arms on session open; Connect is within-session.
8. Synthesized the final shape into a design spec; spec self-review caught one ambiguity (shared vs separate panel components) which was resolved inline.
9. Operator: "good, but we'll have to hand this off for plan writing, adrev, and implementation." Session ends here.

---

## 2. What's on disk

| Artifact | Path | Commit |
|---|---|---|
| **Design spec** (source of truth) | `docs/superpowers/specs/2026-06-04-vara-ardop-panel-alpha-design.md` | `73333e2` |
| Brainstorm companion mocks | `docs/design/mockups/2026-06-04-vara-panel-mocks.html` | `73333e2` |
| Memory: alpha-vettedness | `~/.claude/projects/.../memory/feedback_alpha_is_vettedness_not_built_ness.md` | not git-tracked (auto-memory) |
| Memory: no-tuxlink-added-safeguards | `~/.claude/projects/.../memory/feedback_no_tuxlink_added_safeguards.md` | not git-tracked (auto-memory) |
| This handoff | `dev/handoffs/2026-06-04-willow-mesa-mink-vara-ardop-panel-spec-handoff.md` | next commit |

**No code changes this turn.** The Arm-label fix PR (#350) is on its own branch (`bd-tuxlink-tccc/vara-arming-label-fix`) and is operator-decision pending — the redesign supersedes its specific use of `!isOpen`.

---

## 3. Branch state

| Branch | State |
|---|---|
| `main` | Latest merged PR is #348 (commit `281074935...`). Untouched this turn. |
| `bd-tuxlink-xygm/recover-handoffs` | Operator's parked recovery branch. Spec committed here (`73333e2`) per the docs-handoffs-go-on-current-branch convention. **PUSHED.** This branch now carries the spec + companion mocks + this handoff. |
| `bd-tuxlink-tccc/vara-arming-label-fix` | **OPEN PR #350.** Operator decides whether to merge (defensive fix) or close + supersede (redesign covers it). |
| `bd-tuxlink-9ls2/listener-vara-ardop-p2p` | Merged (PR #348). |
| `task-amd-main-ui` | OPERATOR STATE — interactive rebase still mid-flight, 5 stashes. UNTOUCHED. |

---

## 4. bd issues this session

| Issue | Status | Role |
|---|---|---|
| **tuxlink-0ye6** | open (NEW, P1) | Umbrella for the redesign. Subsumes tuxlink-fzl7 + tuxlink-12sc per dep edges. Claim this at session start. |
| tuxlink-fzl7 | open (blocked → in scope) | VARA Phase 3 outbound RF dial. Spec covers this. |
| tuxlink-12sc | open → must-land-for-alpha | VARA disarm ABORT side-channel. Spec § "Watched failure modes" makes this alpha-required. |
| tuxlink-tccc | open · PR #350 pending | Arming label fix. Disposition TBD. |
| tuxlink-9ls2 | closed (PR #348 merged) | — |

`bd ready` ordering after session: tuxlink-0ye6 is the next P1 to chip.

---

## 5. Memories saved this session

Two load-bearing operator framings captured to auto-memory (will auto-load in future sessions):

1. **`alpha-is-vettedness-not-built-ness`**: tuxlink alpha = quality bar on shipped features, NOT partial completion. No minimal first slices. Reject "MVP" alternatives during alpha scoping. Diverges from industry-standard "alpha = early build."

2. **`no-tuxlink-added-safeguards`**: VARA + ARDOP must mirror legacy WLE behavior. No tuxlink-added bounded-airtime caps, TOT timers, extra confirmation modals. ADRs are agent-internal — they must not inform app behavior. Audit existing safeguards (CONNECT_DEADLINE, ConsentModal, "RADIO-1 SAFETY" comments) for removal.

Both saved with full rationale + watched failure modes per the memory protocol.

---

## 6. Spec self-review

Performed inline. One ambiguity caught + resolved (shared vs separate panel components — chose shared `RadioSessionPanel` with per-protocol adapters, per-protocol-adapter alternative considered + rejected because the inconsistency we're fixing comes from divergent panel shapes). Spec § 10 documents the resolved concern.

No unresolved concerns at spec write-time. The build-walk-revise loop in §8 is the planned mechanism for catching divergences from WLE-parity that only surface when the operator actually walks the panel.

---

## 7. Out-of-repo state / cleanup

- HTTP server for companion mocks (`python3 -m http.server 8765`): killed at session end.
- No background processes left running.
- No stashes created.
- No worktrees created or modified this turn (all work happened in the main checkout on the recover-handoffs branch).

---

## 8. Untouched state (operator owns)

- `task-amd-main-ui` interactive rebase still mid-flight, 5 stashes — entirely untouched
- PR #350 (tuxlink-tccc) — operator-decision pending (close + supersede vs merge + defensive-overlap)

---

## 9. Why this matters

Operator framing throughout: tuxlink is close to alpha, this needs to land polished, and the previous half-built state was a real liability. The spec captures the WLE-grounded shape that should make ARDOP + VARA + VARA FM panels feel like one consistent product. The corrections the operator made during brainstorming (especially "alpha = vettedness" and "no tuxlink-added safeguards") are durable principles that will outlive this redesign — they're saved to memory so future sessions don't re-litigate them.

The build-walk-revise loop is explicitly called out so the post-WLE-parity refinement doesn't get skipped. Once the impl lands and operator walks it, divergences from the radio-dock surface (vs WLE's separate-window pattern) will get cleaned up.

---

## 10. Next-session prompt

```
Resume tuxlink from the willow-mesa-mink 2026-06-04 VARA + ARDOP panel spec handoff.

Handoff doc: dev/handoffs/2026-06-04-willow-mesa-mink-vara-ardop-panel-spec-handoff.md
READ IT FIRST.

State: design spec at docs/superpowers/specs/2026-06-04-vara-ardop-panel-alpha-design.md (commit 73333e2) is committed + pushed and operator-approved at the shape level. bd umbrella is tuxlink-0ye6 (P1). The brainstorming arc terminated at spec-approval per the brainstorming-skill workflow.

Critical first actions:
1. Read the spec. It captures the WLE-grounded redesign (sidebar-driven intent; shared RadioSessionPanel; Open/Close Session lifecycle that arms listener for p2p/radio-only and unlocks outbound; drops tuxlink-added safeguards including CONNECT_DEADLINE + ConsentModal + RADIO-1 identifier surface; radio-only listener is tuxlink-divergence).
2. Claim tuxlink-0ye6. Invoke the writing-plans skill against the spec. After the plan lands, invoke build-robust-features for the cross-provider Codex adrev + subagent-driven implementation.
3. Decide PR #350 (tuxlink-tccc) disposition before impl starts: close + supersede, or merge as defensive overlap. Operator's call.

Two new memories load automatically: alpha-is-vettedness-not-built-ness + no-tuxlink-added-safeguards. Both are load-bearing for the redesign.

Untouched operator state: task-amd-main-ui rebase still mid-flight + 5 stashes.
```

---

Agent: willow-mesa-mink
