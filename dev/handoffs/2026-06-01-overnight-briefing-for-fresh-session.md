# Overnight autonomous execution brief — 2026-06-01

> **This is a START briefing for a fresh session, NOT a session-end handoff.**
> A prior session (`dahlia-heron-spruce`) finished HTML Forms P0 trim work
> and went to sleep. You are picking up an overnight backlog from a
> known-good state. Read this brief top-to-bottom BEFORE any action.

## 0. First actions (in order, do not skip)

1. **Pick a session moniker.** `python3 .claude/scripts/get_agent_moniker.py`
   from the main checkout. Use it in every commit trailer, branch name, and
   subagent prompt for this entire session.

2. **Read this brief top-to-bottom.** No skimming. Memory and discipline
   matter for autonomous execution.

3. **Read the design spec on this branch.**
   `docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md` —
   17 sections covering the full-parity architecture you're building toward.

4. **Read the P0 plan on this branch.**
   `docs/superpowers/plans/2026-05-31-html-forms-p0-pr177-trim.md` — already
   executed by the prior session; pattern reference for your P1/P2/P3 plan
   writing.

5. **Read the prior session-end handoff** (latest dated file under
   `dev/handoffs/` excluding this one) — establishes what the prior session
   shipped and what's open.

6. **Refresh `bd ready`** to see the live backlog state. The 9 bd issues
   filed pre-sleep are listed in §3 below; verify they're present and
   unclaimed.

## 1. Project state at hand-off

**Main checkout HEAD:** `task-amd-main-ui` (operator's preferred working
branch, currently equal to `main` for read purposes).

**This worktree:** `worktrees/bd-tuxlink-izgv-html-forms-fullparity-design`
on branch `bd-tuxlink-izgv/html-forms-fullparity-design`.

**Open PRs at hand-off:**

| PR | Branch | Status |
|---|---|---|
| #177 | `bd-tuxlink-v1p/html-forms-execution` | HTML Forms v0.1 trimmed (P0). 12 commits ahead of main. Operator tentatively approved; Task 8 browser smoke partially-blocked-by-operator-availability. **Do NOT merge — operator must do this on wake.** |
| #178 | `bd-tuxlink-g4dj/search-subject` | Search subject fix. Awaiting operator review. **Do NOT touch.** |
| #179 | `bd-tuxlink-gyu6/pitfalls-html-forms-session` | Pitfalls additions. Awaiting operator review. **Do NOT touch.** |
| #186 | `bd-tuxlink-izgv/html-forms-fullparity-design` | Full-parity DESIGN spec + P0 plan. **You may STACK new plans (P1, P2, P3) onto this PR.** |

**bd issues open as of brief authoring** — see §3.

**Last 5 commits on this branch** (sanity check):

```
ceb33de docs(plan): HTML Forms P0 — trim PR #177 to ship valid v0.1 (tuxlink-izgv)
98b9146 docs(spec): HTML Forms full WLE-parity design (tuxlink-izgv)
```

## 2. The backlog (priority order, ~21h wall-clock)

Items A–N below. Execute in order; chip continuously per
`feedback_decisive_autonomous_execution`. Each item has the bd issue id,
expected duration, output (which PR / branch / artifact), and risk class.

### Setup phase (~30m)

- **A. Backlog priming** ✅ ALREADY DONE BY PRIOR SESSION — bd issues
  `tuxlink-xtwc`, `tuxlink-poai`, `tuxlink-m2o6`, `tuxlink-su2h`,
  `tuxlink-h1km`, `tuxlink-htx1`, `tuxlink-i185`, `tuxlink-ytya`,
  `tuxlink-hnkn`, `tuxlink-4w8u`, `tuxlink-q28i` are filed. Skip A.

### Anachronism cleanup (~1.5h)

- **B. Anachronistic version strings sweep** — bd `tuxlink-htx1`.
  Grep src/ + docs/ for `v0.0.1` / `v0.1` / `v0.5` strings that are
  user-facing UI copy referencing deferred behavior (NOT historical
  forensic references like "first shipped in v0.0.1"). Update inaccurate
  ones to current version (the project is at 0.9.0 → 0.10.0). Atomic
  per-string-class commits. Verify tests don't assert against the updated
  strings.
  - Output: new PR off `main`. Branch: `bd-tuxlink-htx1/<slug>`.
  - Risk: low (text-only changes; CSS-blind vitest fine).
  - Browser smoke gate: not required (no visual or interaction changes).

### Cc functionality re-eval (~2h)

- **C. Cc field re-investigation** — bd `tuxlink-h1km`. Per memory
  `project_pat_complete_strip_directive_2026_05_30`, Pat is being stripped.
  The original Cc-disabled rationale was "Pat drops cc silently." Check
  whether native B2F path (`compose_message_with_files` + `OutboundMessage`)
  properly carries Cc end-to-end. If yes: enable Cc input in
  `src/compose/Compose.tsx` + remove disabled tooltip + remove warning hint.
  If no: file deeper bd issues with root-cause specifics; do NOT enable Cc.
  - Output: new PR off `main` (if simple), or detailed bd notes (if complex).
  - Branch: `bd-tuxlink-h1km/<slug>`.
  - Risk: medium (involves wire-format-affecting backend; verify with
    Rust unit tests for OutboundMessage.cc plumbing).
  - Browser smoke gate: required for the UI toggle — DO NOT MERGE without
    operator browser smoke. Push PR but leave open for operator wake.

### Plans (unblock future execution; ~5.5h)

- **D. P1 implementation plan** — bd `tuxlink-ytya`. Write
  `docs/superpowers/plans/2026-06-01-html-forms-p1-webview-infra.md` on
  THIS branch (izgv); stacks onto PR #186. Follow `superpowers:writing-plans`
  skill discipline: bite-sized tasks (2–5 min each), exact file paths, full
  code in every step, exact commands with expected output, frequent commits.
  Reference the design doc §5–§10 (architecture, components, data flow,
  error handling, security model).
  - Output: doc commit on izgv branch.
  - Risk: low (writing, not executing).

- **E. P2 implementation plan** — bd `tuxlink-hnkn`. Write
  `docs/superpowers/plans/2026-06-01-html-forms-p2-native-autofill.md`
  on izgv. Covers 3 native rebuilds (Position with PositionArbiter, ICS-309
  with messages_meta, Winlink Check-In). Operator decisions §13 open Qs in
  design spec are NOT yet resolved (map widget, PDF library) — write plan
  with TBD callouts in plan §"Operator decisions deferred" so the plan is
  executable as soon as operator answers.
  - Output: doc commit on izgv branch.
  - Risk: low.

- **F. P3 implementation plan** — bd `tuxlink-4w8u`. Write
  `docs/superpowers/plans/2026-06-01-html-forms-p3-catalog-freshness.md`
  on izgv. Covers winlink.org auto-update + form-aware reply + draft library
  generalization.
  - Output: doc commit on izgv branch.
  - Risk: low.

### WLE templates acquisition (~1.5h)

- **G. WLE Standard Forms snapshot pre-flight** — investigation-class for
  bd `tuxlink-ytya` P1 prereq. Pull the Standard Forms zip from winlink.org
  (Pat's source `internal/forms/...` has the canonical URL — read Pat's
  source via `git -C` if Pat is still in repo, else web-fetch the WLE
  forms page). Verify: (a) URL responsive without auth, (b) sensible file
  size (~10-20MB), (c) zip structure consistent with WLE template
  conventions (HTML files with `{FormServer}`, `{FormPort}`, `{FormFolder}`
  placeholders). Write findings to `dev/scratch/2026-06-01-wle-snapshot-recon.md`
  (gitignored). Decision artifact: bundle-now vs auto-update-on-first-launch.
  Do NOT commit the zip yet; that's P1 work.
  - Output: scratch findings doc.
  - Risk: medium — network dependent. If URL fails, document and skip;
    P1 work has fallback path of "operator drops zip manually."

### P1 backend implementation (~9h deep work)

ALL P1 backend work happens in a fresh worktree owned by bd `tuxlink-ytya`.
Create that worktree FIRST via `.claude/scripts/new_tuxlink_worktree.py
--slug p1-webview-infra --issue tuxlink-ytya --base main`. Branch will be
`bd-tuxlink-ytya/p1-webview-infra`. All subsequent H–L commits go there.

The P1 plan written in item D should be your task source for H–L. Do NOT
freelance Rust modules; follow the plan you just wrote.

- **H. `forms/templates.rs`** — bundled + custom template enumeration;
  `{FormFolder}` resolution; tests against fixture templates.
  - Risk: medium (filesystem enumeration; thorough tests required).

- **I. `forms/skin.rs`** — tuxlink CSS skin generator per design §5.5;
  tests assert key selectors present (e.g., body bg overridden to
  `--tux-bg`).
  - Risk: low (static asset generation).

- **J. `forms/multipart.rs`** — parse urlencoded + multipart preserving
  repeated names + submitter per Codex adrev §5.3. Use a battle-tested
  Rust crate (`multer` or `axum::extract::Multipart`) rather than rolling
  your own.
  - Risk: medium (multiple parsing edge cases; reference Codex's failure
    cases as test inputs).

- **K. `forms/http_server.rs`** — lazy axum server with bind `127.0.0.1:0`
  + per-open token + `{FormServer}` / `{FormPort}` substitution + Tauri
  lifecycle (start on `open_webview_form`, teardown on close). Hardening
  per design §5.3 (no path traversal, allowlisted assets, no IPC).
  - Risk: high (concurrent lifecycle + security-critical; require Codex
    adrev round before commit).

- **L. `forms-webview.json` Tauri capability** — loopback HTTP only, no
  IPC. Rust integration test on capability scope.
  - Risk: low (config file).

### Quality gates (~1.5h)

- **M. Codex adrev per backend module commit** per
  `feedback_codex_post_subagent_review`. Each of H/I/J/K/L commit hits a
  Codex review (custom-prompt mode per CLAUDE.md "Adversarial-review
  pattern"). Apply P0/P1 findings inline as follow-up commits.
  - Risk: low (review-class).
  - Capacity gate: per `feedback_codex_quota_gotcha`, if Codex returns
    "ERROR: You've hit your usage limit" defer the round to next cycle;
    DO NOT substitute Claude review.

### Handoff (~30m)

- **N. End-of-overnight handoff doc** — write to
  `dev/handoffs/2026-06-02-<your-moniker>-overnight-execution-handoff.md`
  on whatever worktree is your last touch (probably the P1 backend
  worktree). Cover:
  - What shipped (each bd issue with its closed/open state + PR URL)
  - What's blocked on operator (each item with blocking question)
  - What to smoke-test on wake (browser smoke checklist per PR)
  - Codex adrev findings disposition (applied vs deferred)
  - Time accounting (item by item)
  - bd issue `tuxlink-q28i` (this overnight session umbrella) gets
    closed with a pointer to this handoff doc.

## 3. bd issue map at hand-off

```
HIGH-PRIORITY (P1) — overnight scope or referenced
  tuxlink-ytya  HTML Forms P1 implementation        — work in item H–L
  tuxlink-hnkn  HTML Forms P2 implementation        — plan in item E
  tuxlink-q28i  Overnight session umbrella          — close in item N

MEDIUM-PRIORITY (P2) — overnight scope or surfaced
  tuxlink-m2o6  Compose/form label collisions       — DEFER; needs brainstorm
  tuxlink-su2h  Outbox folder enable                — DEFER; needs UI work
  tuxlink-h1km  Cc field re-eval                    — work in item C
  tuxlink-4w8u  HTML Forms P3 implementation        — plan in item F

LOW-PRIORITY (P3) — overnight scope or polish
  tuxlink-xtwc  forms.test.ts uncomment assertions   — quick win during item B sweep
  tuxlink-poai  Draft-restore corner case            — DEFER; needs design call
  tuxlink-i185  setFormMode-during-render refactor   — DEFER; design call
  tuxlink-htx1  Anachronistic version strings sweep  — work in item B
```

## 4. Discipline ("how each task runs")

This is the same discipline pattern dahlia-heron-spruce used for P0;
adopt it verbatim.

### Per-task pattern

1. **`bd update <id> --claim`** before starting.
2. **`bd update <id> --status=in_progress`**.
3. **Create worktree** per ADR 0008 via
   `python3 .claude/scripts/new_tuxlink_worktree.py --slug <slug>
   --issue <id> --base main --moniker <your-moniker>`. The script may
   complain about default base; use `--base main`.
4. **Dispatch implementer subagent** per `superpowers:subagent-driven-development`.
   Use `subagent_type=general-purpose` unless a specialized agent fits.
   Pass: full task text (verbatim from your plan), context (where this
   fits), working directory (absolute path to the new worktree), your
   session moniker (subagent uses this in commit trailers).
5. **Two-stage review** of subagent's commit:
   - Spec compliance reviewer (separate subagent dispatch)
   - Code quality reviewer (separate subagent dispatch, after spec passes)
6. **Codex adrev** on the commit per
   `feedback_codex_post_subagent_review`. Custom-prompt mode (CLAUDE.md
   pattern). Capture to `dev/adversarial/2026-06-01-<task-slug>-codex.md`.
7. **Apply findings** inline as follow-up commits.
8. **Push** the worktree's branch immediately per `feedback_never_hold_a_push`.
9. **Open PR** via `gh pr create --base main --title "[<moniker>] <type>(<scope>): <subject>" --body "..."`.
10. **`bd close <id>`** (or update notes if not closing — e.g., investigation
    that ends in deeper bd issue).
11. **Update TodoWrite** for in-session progress.

### Commit discipline (from CLAUDE.md)

- Conventional commits: `feat(scope):`, `fix(scope):`, `refactor(scope):`,
  `docs(scope):`, `test(scope):`, `chore(scope):`.
- Subject ≤72 chars.
- Body explains the why; references the bd issue id and design spec section.
- Trailers: `Agent: <your-moniker>` + `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`.

### Path pinning (from memory `feedback_pin_paths_in_worktree_sessions`)

Bash cwd can silently revert from a worktree to the main checkout
mid-session. Pin EVERY git / cargo / pnpm command with absolute paths or
`git -C` / `pnpm -C` / `cargo --manifest-path <abs>`. The
main-checkout-race hook will deny mutations attempted in the wrong cwd.

### Cross-provider adrev discipline

Per `feedback_no_carveout_on_cross_provider_adrev`: hard-to-undo
architectural decisions get a Codex adrev round before implementation.
For overnight work, that means:
- Items D/E/F (plans): NO Codex adrev required (writing, not committing
  architecture).
- Item C (Cc re-investigation): Codex adrev required ONLY if your finding
  changes wire format. UI-string-only change skips adrev.
- Items H/I/J/K/L (P1 backend modules): Codex adrev per commit per item M.

## 5. Hard constraints — what you must NOT do

- **No PR merges.** All work lands as PRs awaiting operator review.
  `gh pr merge` is banned for this session.
- **No frontend UI implementation beyond test-only scaffolding.**
  WebviewFormHost and CatalogBrowser components are P1 backend's downstream
  consumers; their React implementation requires browser smoke and that
  needs the operator. If you must write any frontend code, mark it
  `(* operator browser smoke required before merge *)` in the commit body.
- **No RF / transmission / live-CMS testing.** RADIO-1 is unconditional;
  the operator is asleep and cannot give per-invocation consent. Telnet
  CMS testing IS authorized per memory `feedback_cms_telnet_testing_authorized`
  but you have no reason to do it for this slate.
- **No destructive git** — the destructive-git hook will deny it. Never
  `--force` push, `git reset --hard`, `git rebase -i`, `git branch -D`,
  `git commit --amend` on pushed commits, `git worktree remove`,
  `--no-verify`. If a denial surprises you, find a non-destructive
  alternative.
- **No main-checkout writes.** All git mutations happen in worktrees per
  ADR 0008. The main-checkout-race hook will deny mutations there.
- **No edits to PRs #178 or #179.** Other-author work; not yours to touch.
- **No edits to PR #177 beyond P0 trim cleanup.** PR #177 is operator-
  pending; if you find a P0 polish item, file a bd issue rather than
  stacking commits.
- **Do not amend dahlia-heron-spruce's commits.** Amending pushed commits
  is hook-banned and would lose attribution.

## 6. Stop conditions

Stop and write the handoff (item N) if any of these fire:
- All slate items A–N are complete.
- Item K (forms/http_server.rs) is blocked AND your other items A–J,
  L–M are complete or blocked. Don't ceremony-loop on a blocked critical
  item; document and move on.
- ~24h elapsed since session start (the operator's "safe buffer"). Stop
  gracefully — write the handoff with whatever's done.
- The operator returns and gives you direction.

## 7. References

### Specs + plans on this branch
- `docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md` — 530-line full-parity architecture.
- `docs/superpowers/plans/2026-05-31-html-forms-p0-pr177-trim.md` — P0 trim plan (executed by dahlia-heron-spruce).
- `dev/handoffs/2026-05-31-peregrine-maple-thistle-html-forms-and-followups.md` — handoff from the session before dahlia-heron-spruce's.

### CLAUDE.md (root of main checkout)
- §"Agent identity" — moniker discipline, must include in trailers.
- §"Git workflow — worktrees mandatory" — ADR 0008.
- §"Git workflow — destructive commands BANNED" — what NOT to do.
- §"Commit and release discipline" — conventional commits format.
- §"Tool referee" — bd for cross-session tasks, TodoWrite for in-turn.
- §"Session Completion" — handoff doc + push are mandatory.
- §"OpenAI Codex CLI" — adrev invocation pattern.

### Memory entries to remember
- `feedback_decisive_autonomous_execution` — chip continuously; no option menus.
- `feedback_never_hold_a_push` — push as you go.
- `feedback_pin_paths_in_worktree_sessions` — absolute paths everywhere.
- `feedback_codex_post_subagent_review` — Codex adrev per architectural commit.
- `feedback_codex_quota_gotcha` — capacity-defer if quota hit; don't substitute.
- `feedback_browser_smoke_before_ship` — UI work needs eyes-on-screen; defer to operator.
- `feedback_no_carveout_on_cross_provider_adrev` — don't skip cross-provider review for hard-to-undo work.
- `feedback_main_checkout_is_operator_state` — never `git checkout` in main checkout.
- `feedback_stale_lease_means_worktree` — if main-checkout-race hook denies you, create worktree.
- `project_pat_complete_strip_directive_2026_05_30` — Pat is legacy; native client canonical.

### Codex CLI invocation (CLAUDE.md verbatim)
```bash
cat > /tmp/codex-prompt.txt <<EOF
[directed attack-angle prompt]
EOF
cat /tmp/codex-prompt.txt | npx --yes @openai/codex review - 2>&1 \
  | tee dev/adversarial/2026-06-01-<topic>-codex.md
```
Validate via `wc -l dev/adversarial/*.md` — a real review is 1500–4000+
lines; a stub is ~5 lines (means the prompt got rejected; re-run with
stdin pattern).

## 8. Final check before you start

- [ ] You picked a session moniker via the script.
- [ ] You read §0–§7 of this brief.
- [ ] You read the design spec (`docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md`).
- [ ] You read the P0 plan (`docs/superpowers/plans/2026-05-31-html-forms-p0-pr177-trim.md`).
- [ ] You read the prior session-end handoff under `dev/handoffs/`.
- [ ] You ran `bd ready` and confirmed the 11 issues from §3 are visible.
- [ ] You understand that browser-smoke-required UI work is deferred to operator.
- [ ] You understand that the umbrella issue is `tuxlink-q28i`.

Now: **claim bd `tuxlink-htx1`** (the first work item, item B in the slate)
and begin. Chip continuously. Push every commit. Write the handoff at session end.

---

Agent: dahlia-heron-spruce (brief author)
