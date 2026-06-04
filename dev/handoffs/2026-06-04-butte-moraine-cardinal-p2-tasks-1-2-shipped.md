# Handoff — butte-moraine-cardinal — HTML Forms P2 Tasks 1+2 shipped (PR not open yet)

**Agent:** butte-moraine-cardinal · **Date:** 2026-06-04 · **Machine:** pandora

## Critical first action — next session

```
1. READ this handoff doc first (the session-start hook surfaces only its filename).
   Don't skip to `bd ready` — the P2 work is mid-arc and the chip-order matters.
2. The branch bd-tuxlink-hnkn/p2-native-autofill is pushed at tip 6487ec2 with
   Tasks 1 (Position) + 2 (ICS-309) done. Tasks 3 (CheckInForm) + 4
   (FormDraftLibrary) + 5 (CatalogBrowser verify) + 6 (smoke + Codex + open PR)
   remain.
3. Recommended next chip: dispatch Task 4 (FormDraftLibrary backend) BEFORE
   Task 3 (CheckInForm) — Task 3's form calls form_draft_library_list/_upsert,
   so Task 4 should land first to enable real (non-mock) save-slot smoke
   later. Plan section: docs/superpowers/plans/2026-06-01-html-forms-p2-
   native-autofill.md lines 827-931.
4. Codex full-diff adrev (Task 6 step) is non-negotiable per
   feedback_no_carveout_on_cross_provider_adrev. Run on the full hnkn diff
   vs main once Tasks 3-5 land.
5. Operator browser-smoke before PR open per feedback_browser_smoke_before_ship.
   The 1420 port collision with operator's converge-build is a recurring
   blocker — coordinate or stop converge-build before smoke.
```

## What shipped this session

### PR #388 — HTML Forms P1 frontend (MERGED)

Session-start state: PR #388 was OPEN + `CONFLICTING` against main, NOT shipped
as the prior handoff's atoll-basin-crag framing suggested. Did the
non-destructive merge resolution (single conflict in
`docs/user-guide/20-html-forms.md`), found the CI Rust-integration-test gap
that local `cargo test --lib` had missed (`forms_capability_scope` test was
stale on Task 11's `viewer-form-*` extension), shipped the test fix (afc10bb),
verified CI green, and operator merged via `gh pr merge 388`.

Main is now at `40eb58f` (after PR #388 + PR #389 release-please cut to
0.32.1).

**Why this matters for next session:** `--lib` is the source-of-truth gap.
Every cargo invocation in P2 work used `cargo test` (full), not `cargo test
--lib`. Preserve that discipline.

### Task 1 — PositionFormV2 (DONE — 4 commits)

Branch: `bd-tuxlink-hnkn/p2-native-autofill`. The tzr5 merge into hnkn happened at
`5a90fae`, then:

| Commit | Subject |
|---|---|
| `583f90e` | feat(forms): position_current_fix Tauri command for PositionFormV2 |
| `452cd53` | feat(forms): PositionFormV2 — native Position Report with PositionArbiter pull |
| `bd35559` | fix(forms): PositionFormV2 — wire-format payload + draft restore + no-fix UX |
| `c1b122f` | fix(forms): PositionFormV2 — onChange in event handlers + inline grid error |

**Two code-review-caught bugs already remediated**, both worth carrying as
process notes:

1. **Wire-format mismatch (silent data loss).** Initial submit shape `{ formId,
   grid, remark }` didn't match `POSITION_REPORT`'s template field IDs
   (`thetime, lat, lon, message`). Every position report would have gone
   on-air with empty fields. Fix: transform UI state → wire format inside the
   Send handler via `src/forms/position/maidenhead.ts::gridToLatLon` (ported
   from `src-tauri/src/position/maidenhead.rs`).

2. **`onChange` in `useEffect` dep array → infinite re-render in production.**
   Compose.tsx:830 passes an inline arrow; `useEffect([..., onChange])`
   re-fires every render → setFormMode → re-render → loop. Tests passed
   because `vi.fn()` is a stable reference. Fix: fire `onChange` inside
   input event handlers (ICS-213 codebase convention).

**Lesson for future native form work in this branch:** the ICS-213 + the
fixed PositionFormV2 are the canonical patterns. Don't put `onChange` in any
useEffect dep array. Always verify the wire-format payload's field IDs match
`src-tauri/src/forms/templates/<form_id>.rs::FIELDS`.

### Task 1b — Leaflet map widget for PositionFormV2 (DONE — 2 commits + 1 fix)

| Commit | Subject |
|---|---|
| `359f54c` | build(deps): add leaflet + react-leaflet for PositionFormV2 map widget |
| `717b76f` | feat(forms): PositionMapWidget — Leaflet map for PositionFormV2 grid override |
| `29cfe5b` | test(shell): complete AppShell test mocks for position_status + search IPCs |

Operator decision (2026-06-04): Leaflet + offline-tiles strategy. Tile
source = OSM online + graceful blank-canvas + Maidenhead grid-square overlay
fallback when offline (detection via `navigator.onLine` + Leaflet
`tileerror`). NO bundled tile pack in this PR — filed as P3 follow-up if
operator wants richer offline UX.

Bundle impact: Leaflet + react-leaflet land in the main `index` chunk:
~453 kB minified / **143 kB gzip**. The operator's "~50 kB" framing was
Leaflet-alone; the actual transitive cost (including react-leaflet wrapper
+ Leaflet's runtime CSS) is closer to 140 kB gzip. **File a P3 bd for
React.lazy on the form-registry entries** — Leaflet doesn't need to load
until the operator opens a Position Report form.

The `29cfe5b` commit fixed AppShell test failures caused by the new
`position_status` and `tauri_search_list_*` IPCs not being mocked — adding
PositionFormV2 to the registry surfaced those queries via React Query's
`useStatusData` polling.

### Task 2 — Ics309FormV2 (DONE — 3 commits)

| Commit | Subject |
|---|---|
| `561c342` | feat(forms): messages_meta_query_for_log + render_ics309_pdf Tauri commands |
| `e38caad` | feat(forms): Ics309FormV2 — native ICS-309 comms log with messages_meta aggregation + PDF |
| `6487ec2` | test(config): raise vitest testTimeout to 15 s for React.lazy AppShell tests |

Backend additions:

- `Index::query_log_rows` (NOT `Mailbox::query_log_rows` as the plan sketched)
  — the implementer adapted to the actual schema (epoch storage; `epoch_to_rfc3339`
  via Hinnant algorithm; CASE-direction folder discriminator)
- `messages_meta_query_for_log` Tauri command
- `render_ics309_pdf` Tauri command via `printpdf = "0.9"` (operator's
  locked PDF library choice). Renders portrait page with header + station +
  date-range + datetime/dir/from/to/subject table; auto page-break.

Frontend: `Ics309FormV2.tsx` with 4 time-range presets (last-hour / today /
op-period / custom), preview table, Send (wire-format-aligned per Task 1's
lesson), Download CSV (in-process Blob), Download PDF (calls
`render_ics309_pdf`).

**vitest testTimeout bumped to 15 s** in `vitest.config.ts` (commit `6487ec2`)
— necessary because the cumulative form-registry growth (PositionFormV2 +
Leaflet's module-init + Ics309FormV2 + printpdf-related serialization)
pushed AppShell's "selecting a row" test over the 5 s default on this Pi
(arm64). Acceptable band-aid; the real fix is React.lazy on the form
registry (filed as P3 follow-up).

**Gates at session end** (run from `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-hnkn-p2-native-autofill/src-tauri`):
- `cargo test` (full): 1104 passed, 0 failed
- `cargo clippy --all-targets --locked -D warnings`: clean
- `tsc --noEmit`: clean
- `vitest run`: 1222/1222 passed (the 1 AppShell timeout flake is now passing
  with the 15 s budget)

## Tasks 3-6 remain — chip order + open questions

### Task 3 — CheckInForm (NEW native form, 7 fields)

Plan section: lines 713-825. Calls into Task 4's FormDraftLibrary commands —
**dispatch Task 4 FIRST** so the slot library is real, not just mocked, when
Task 3's tests stub the IPC.

Open question for Task 3: the WLE `Winlink_Check-In` template's actual field
IDs. The plan sketches Tactical / Operator / Group/Net / Status / Comments /
Position / Initials but says "verify against actual WLE Winlink_Check-In
template". The Task 1 wire-format-mismatch lesson applies here strongly —
verify field IDs in `src-tauri/src/forms/templates/checkin.rs` (which the
implementer creates per plan Step 1) against the WLE source.

### Task 4 — FormDraftLibrary (CRUD backend + small TS wrapper)

Plan section: lines 827-931. Per-form-id slot library backed by a SQLite
table (`form_draft_slots`). Per operator decision (2026-06-04): generalize
to ALL native forms in P2, NOT just Check-In as the plan's original P2 scope
said. Practical implication: the dispatch should wire `FormDraftLibrary`
into Ics213Form, PositionFormV2, Ics309FormV2, AND CheckInForm in the same
commit cluster.

### Task 5 — CatalogBrowser entries (likely no-op)

Plan section: lines 935-952. P1's `CatalogBrowser.onPick` already routes
via `lookupForm(id)?.Form` presence, so Tasks 1-4's registry additions get
picked up automatically. Verify by running CatalogBrowser tests — should be
no code change unless a test asserts on the form-set membership.

### Task 6 — E2E smoke + Codex full-diff adrev + open PR

Plan section: lines 956-1056. The Codex round is non-negotiable per
`feedback_no_carveout_on_cross_provider_adrev`. Run via the custom-prompt
pattern (see CLAUDE.md §"Codex CLI" — the stdin form):

```bash
cat > /tmp/codex-prompt.txt <<'EOF'
You are doing adversarial review of the diff against origin/main on this
worktree. Run `git diff origin/main..HEAD` to see the changes.

Audit for:
1. Wire-format field-ID mismatches (the round-1 PositionFormV2 review
   caught this for Position; verify Ics309FormV2 + CheckInForm don't have
   the same class of bug — read src-tauri/src/forms/templates/*.rs for
   each form_id and confirm the React onSubmit payload matches.)
2. onChange-in-useEffect-deps loops (round-2 PositionFormV2 caught this;
   verify Ics309FormV2 + CheckInForm follow the ICS-213 event-handler
   pattern.)
3. SQL injection / panic-on-malformed-input in the new
   messages_meta_query_for_log + form_draft_library_* commands.
4. printpdf's PDF rendering edge cases (UTF-8 in subjects, very long
   strings, zero rows, page-break edge cases).
5. RADIO-1: any path where a button click could initiate TX without
   operator confirmation? (Spec §10 says no; verify.)
EOF
cat /tmp/codex-prompt.txt | npx --yes @openai/codex review - 2>&1 \
  | tee dev/adversarial/2026-06-XX-p2-full-diff-codex.md
```

Once Codex round green (apply findings P0/P1 inline; file P2/P3 as bd
issues), open the PR per ADR 0010 (no squash; --merge with delete-branch).
PR title format: `[<moniker>] feat(forms): HTML Forms P2 — native auto-fill
for Position + ICS-309 + Check-In + draft library (tuxlink-hnkn)`.

PR body needs the operator browser-smoke walkthrough (per
`feedback_browser_smoke_before_ship`). 9-step walkthrough analog to PR #388:

1. Launch `pnpm tauri dev`. Coordinate with operator's `pnpm dev:converged`
   on `:1420` first — kill or repoint to hnkn before starting.
2. Compose → Position Report (native). Verify GPS pre-fills grid.
   Map widget renders. Click on map → grid updates.
3. Compose → ICS-309 Comms Log. Pick "today". Preview table populates from
   `messages_meta`. Send. Download CSV. Download PDF — opens valid PDF.
4. Compose → Winlink Check-In. Pre-fill tactical call. Save-as-slot.
   Re-open form, pick saved slot, verify rehydration.
5. Draft autosave: type half a form, close compose, reopen — draft restores.

## Worktrees in flight at handoff

| Worktree | Branch | Status |
|---|---|---|
| `bd-tuxlink-hnkn-p2-native-autofill` | `bd-tuxlink-hnkn/p2-native-autofill` | **Active — Tasks 1+2 shipped, Tasks 3-6 pending.** Don't dispose. |
| `bd-tuxlink-tzr5-forms-alpha-p1-frontend` | `bd-tuxlink-tzr5/forms-alpha-p1-frontend` | Merged + deleted on origin (PR #388 landed). Local branch is `merged-dead` per ADR 0017. **Safe to dispose via the 4-step ritual** (ADR 0009) at next-session leisure. Not urgent. |

Other worktrees (~40 of them) listed by `git worktree list` are pre-existing
and unrelated; don't touch them.

## Recent failure modes worth carrying

1. **The handoff-doc-vs-PR-state premise gap.** The session-start prompt
   said "Last session shipped PR #388" — but the PR was open + conflicting.
   The atoll-basin-crag handoff (`d8e7682` on hnkn, pushed but not on main)
   wasn't visible from the session-start hook's most-recent-handoff scan
   because main hadn't absorbed it. **Lesson:** when a session-start prompt
   asserts state, verify the state directly before acting (especially
   merge-state, branch-tip, and CI checks). The `git diff --stat
   origin/main...HEAD` + `gh pr view <N> --json mergeStateStatus` checks
   are 30-second probes.

2. **`cargo test --lib` lies.** Local `--lib` skips integration tests in
   `src-tauri/tests/`. The Task 11 `forms_capability_scope` test had been
   stale for 3 days because every recent session used `--lib`. CI caught
   it on the merge rerun. **Lesson:** always run `cargo test` (full) for
   gates, not `--lib`. The vitest equivalent is fine (`vitest run` runs
   all tests by default).

3. **Bash cwd silently reverts.** Bit me twice this session — once on the
   `merge-base` hook false-positive, once during cargo test. The fix:
   `cd /abs/path && pwd && <cmd>` chained in a single Bash call, or
   `--manifest-path /abs/path/Cargo.toml` for cargo, or `pnpm -C /abs/path`
   for pnpm. **Lesson:** if you're about to run a build/test command and
   you `cd`'d in a previous Bash call, re-`cd` in the current one — bash
   sessions don't persist cwd across the Bash tool's invocations.

4. **The `--lib`/wire-format-mismatch combo is a known PositionForm trap.**
   The form's `onSubmit` payload field IDs MUST match the template's
   `FIELDS` array exactly. The PositionFormV2 first impl sent `{ grid,
   remark, formId }` to a template expecting `{ thetime, lat, lon, message }`
   — and BOTH local `--lib` cargo test AND vitest test were green because
   the test asserted on the React component's onSubmit-call shape, not on
   the wire format. **The Codex full-diff round at Task 6 must verify
   wire-format alignment for Ics309FormV2 and CheckInForm specifically.**

5. **Inline-arrow `onChange` → useEffect dep loop.** Tests pass because
   `vi.fn()` is stable; production loops because Compose.tsx:830 passes an
   inline arrow. **Adopt the ICS-213 pattern** (fire onChange in input
   event handlers, never in a useEffect dep array). The Codex round at
   Task 6 should verify Ics309FormV2 + CheckInForm follow it.

6. **AppShell tests are the silent witness for form-registry growth.**
   When new form modules add IPCs (PositionFormV2 added `position_status`
   via the React Query polling), AppShell's catch-all mock returns
   `undefined` → React Query warns + tests timeout. Pattern: when adding
   ANY new IPC consumed by a form, also extend the mocks in
   `src/shell/AppShell.test.tsx` AND `src/shell/AppShell.radioPanel.test.tsx`.
   The 15 s testTimeout bump is a band-aid for the cumulative growth; the
   real fix is React.lazy on the form registry (filed P3).

## Bd issues filed this session

(None new this session — work was against pre-existing tuxlink-hnkn.)

P3 follow-ups to file (controller didn't get to them):

- React.lazy code-splitting on the form registry to keep Leaflet + printpdf
  + future heavy form deps out of the main bundle
- Tile-pack bundling for Leaflet (offline-first UX richer than the current
  blank-canvas fallback)
- AppShell test timeout band-aid → proper fix via lazy form registry
  (supersedes the 15 s default)
- Retry affordance on PositionFormV2's GPS IPC failure state

## Session-end operator-pasteable starting prompt

```
Continuing tuxlink alpha-forms P2. Last session shipped Tasks 1 (PositionFormV2 +
Leaflet) + 2 (Ics309FormV2 with printpdf) on bd-tuxlink-hnkn/p2-native-autofill
(tip 6487ec2). Tasks 3 (CheckInForm) + 4 (FormDraftLibrary) + 5 (CatalogBrowser
verify) + 6 (e2e smoke + Codex full-diff adrev + open PR) remain.

Read dev/handoffs/2026-06-04-butte-moraine-cardinal-p2-tasks-1-2-shipped.md
for the full chip-order rationale + 6 carry-forward failure modes (the
wire-format-mismatch and onChange-in-useEffect-deps traps caught twice on
Task 1 will reappear on Tasks 3 if not actively guarded).

Critical first action:
1. Dispatch Task 4 (FormDraftLibrary backend) BEFORE Task 3 (CheckInForm)
   so the save-slot smoke is real not mocked.
2. Operator decision context: ALL P2 native forms (Ics213 + Bulletin +
   Position + ICS-309 + Check-In) get FormDraftLibrary wiring in this PR.
3. Codex full-diff adrev (Task 6) is mandatory — DO NOT skip per
   feedback_no_carveout_on_cross_provider_adrev.
4. Coordinate the converge-build on :1420 BEFORE browser-smoke (or kill it).
```

## Agent: butte-moraine-cardinal
