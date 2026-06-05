# Handoff — atoll-basin-crag — HTML Forms P1 frontend shipped (PR #388 open) + P2 staged

**Agent:** atoll-basin-crag · **Date:** 2026-06-04 · **Machine:** pandora

## Critical first action — next session

```
1. SMOKE PR #388 — operator browser-smoke walkthrough in the PR body (9 steps).
   - CatalogBrowser → native ICS-213 compose
   - Catalog ARC213 → in-window webview compose → submit
   - Custom HTML file in ~/.local/share/tuxlink/forms/custom/ → restart → appears
   - Receive-side viewer + KeyValueView fallback for unknown form_ids
   - Per-message viewer state reset
2. If green → gh pr merge 388 --merge --delete-branch (PR title format
   per ADR 0010, no squash)
3. After P1 merges → resume P2 in the existing worktree at
   /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-hnkn-p2-native-autofill
   (operator decisions for P2 already locked in bd tuxlink-hnkn notes:
   Leaflet+tiles / printpdf in P2 / FormDraftLibrary across all native forms)
4. P2 Task 1 is PositionFormV2 — read
   docs/superpowers/plans/2026-06-01-html-forms-p2-native-autofill.md
   starting at "## Task 1: Position Report"
```

## What shipped this session

### PR #388 — HTML Forms P1 frontend (OPEN, awaiting operator smoke)

Branch: `bd-tuxlink-tzr5/forms-alpha-p1-frontend` (stacked off main; 11 commits).

| Commit | Subject |
|---|---|
| `228073e` | feat(forms): Tauri command surface for webview forms (P1 task 8) |
| `1233041` | docs(forms): code-review polish — Mutex/take_submit_rx docs + camelCase OpenFormResult |
| `40a5a2a` | feat(compose): WebviewFormHost — child webview embed + fallback chrome (P1 task 9) |
| `a2b34a8` | fix(compose): WebviewFormHost mounts in-window <Webview>, not a separate WebviewWindow |
| `9c0d04e` | feat(compose): CatalogBrowser + WebviewFormHost wiring in Compose (P1 task 10) |
| `1fa943b` | fix(forms): catalog-form submit + stale-closure + a11y polish (P1 task 10 critical-fix) |
| `223c93c` | feat(forms): receive-side Viewer-mode webview fallback (P1 task 11) |
| `1917cc0` | fix(forms): viewer-failed state must reset per message (P1 task 11 critical-fix) |
| `c8cf9c0` | docs(forms): README + user-guide rewrite for P1 webview infrastructure (P1 task 12) |
| `4e77e4c` | fix(compose): WebviewFormHost pre-PR polish — tauri://error + RAF-coalesced repos (rqrn) |
| `305c70b` | fix(forms): apply Codex P1+P2 findings on P1 frontend full-diff adrev |

**Headline outcomes:**
- All 251 WLE Standard Forms (v1.1.20.0) compose+view via the new CatalogBrowser
- Custom HTML form upload via `~/.local/share/tuxlink/forms/custom/`
- Receive-side viewer for unknown form ids with KeyValueView fallback
- Native compose preserved for ICS-213 + Bulletin
- 5 new Tauri commands: forms_list_catalog, open_webview_form, close_webview_form_server, open_webview_viewer, send_webview_form
- Codex full-diff adrev → 3 P1 + 2 P2 findings, all applied in `305c70b`

**Test gates (final on `305c70b`):**
- vitest run: **1179 passed** (117 files)
- cargo test --lib: **1000 passed**
- cargo clippy --all-targets -D warnings: clean
- tsc --noEmit: clean

**Codex adrev disposition** (transcript at `dev/adversarial/2026-06-04-p1-frontend-full-diff-codex.md`, gitignored):

| # | Sev | Finding | Fix landed |
|---|---|---|---|
| 1 | P1 | Save Draft on webview-form silently lost contents | Hide Save Draft + close-dialog message |
| 2 | P1 | Form IDs with spaces failed `is_valid_form_id` on receive | Relaxed validator to permit space/dot/ampersand |
| 3 | P1 | Bundled viewer filenames don't follow `<id>_Viewer.html` | `resolve_viewer_for` helper with 4-tier fallback |
| 4 | P2 | wle_templates returned non-authoring templates (Viewer, SendReply) | `is_authoring_template_stem` filter |
| 5 | P2 | `{var X}` substitution corrupted `<script>` blocks | Skip substitution inside `<script>` |

## P2 staged — tuxlink-hnkn / worktree exists / decisions locked

**Worktree:** `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-hnkn-p2-native-autofill`

**Branch:** `bd-tuxlink-hnkn/p2-native-autofill` (stacked off `bd-tuxlink-tzr5/forms-alpha-p1-frontend` per ADR 0010 since P1 PR #388 isn't merged yet; pushed to origin).

**Operator decisions locked (bd tuxlink-hnkn notes, 2026-06-04):**
1. **Map widget for Position Report: Leaflet + offline tiles.** Adds leaflet (~50KB) + offline-tiles strategy. Map shows GPS-derived location by default; operator clicks to override.
2. **PDF export for ICS-309: ship in P2 via printpdf Rust crate.** Adds printpdf (~500KB). Operator gets attachable printable PDF.
3. **Form draft library scope: all native forms in P2.** FormDraftLibrary generalizes immediately to ICS-213 + Bulletin + Position + ICS-309 + Winlink Check-In.

**P2 task list** (`docs/superpowers/plans/2026-06-01-html-forms-p2-native-autofill.md`):
- Task 0 — Lock operator decisions ✅ (this handoff)
- Task 1 — PositionFormV2 (PositionArbiter pull + Leaflet override)
- Task 2 — Ics309FormV2 (time-range + auto-aggregate from messages_meta + printpdf export)
- Task 3 — CheckInForm (new native — GPS auto-fill + save-slot library)
- Task 4 — FormDraftLibrary (all native forms)
- Task 5 — CatalogBrowser entries
- Task 6 — E2E smoke + Codex adrev + open PR

## P3 queued — tuxlink-4w8u

**Scope** (`docs/superpowers/plans/2026-06-01-html-forms-p3-catalog-freshness.md`):
- In-app catalog refresh from winlink.org with atomic swap
- Custom-forms hot-reload via `notify` crate (inotify)
- Operator-overridable custom-forms-dir via Settings UI
- Form-aware Reply via WLE `_SendReply.0` templates
- Draft library generalization (now done in P2 per operator decision; this becomes a no-op)

## Worktrees in flight at handoff

Beyond the in-flight Beads-claimed ones documented in [bluff-birch-cove handoff](2026-06-03-bluff-birch-cove-perf-helpwindow-cleanup.md), atoll-basin-crag added:

| Worktree | Branch | Status |
|---|---|---|
| `bd-tuxlink-tzr5-forms-alpha-p1-frontend` | `bd-tuxlink-tzr5/forms-alpha-p1-frontend` | OPEN — PR #388 awaiting operator smoke. **DO NOT DISPOSE.** |
| `bd-tuxlink-hnkn-p2-native-autofill` | `bd-tuxlink-hnkn/p2-native-autofill` | Empty (stacked on tzr5; no P2 commits yet). Don't dispose; P2 work continues here. |

Disposed during atoll-basin-crag's triage at session start: `bd-tuxlink-2x0l-message-list-sort-ui` (archived patch), `bd-tuxlink-7vea-listener-ui-ardop-wiring` (superseded by PR #344's plover work). 5vx worktree was created + unclaimed during the pre-pivot churn — cleanly disposed.

## Bds filed this session

| ID | P | Purpose |
|---|---|---|
| tuxlink-tzr5 | P0 | Alpha-forms umbrella (P1 + P2 + P3) — IN_PROGRESS |
| tuxlink-rqrn | P2 | WebviewFormHost pre-PR polish (3 Important items) — CLOSED ✅ |
| tuxlink-7cn1 | P3 | Extract shared 16-hex token-mint utility (dedupe modem_status + http_server) — OPEN |
| Task 11 follow-ups bd | P3 | viewer substitution-in-script + dedupe token mint + OpenViewerResult shared type — OPEN |

## Recent failure modes worth carrying

1. **Speculation about send_form's catalog coverage** — early in Task 10's review cycle, the spec reviewer approved a CatalogBrowser routing that DIDN'T verify the end-to-end submit path actually worked for non-native forms. The code-quality reviewer caught it (`send_form` was hardcoded to BUNDLED_FORMS — would have failed for ~245/250 catalog forms). **Lesson:** when the spec reviewer says "matches the task," ALSO trace the runtime path for the feature being delivered.

2. **`viewerFailed` state stickiness across MessageView selections** — comment claimed remount-per-message but no `key={message.id}` actually fired the remount. Both reviewers caught it independently. **Lesson:** docstring claims about React remount semantics are unverified; always probe the actual key/remount behavior or add a regression test.

3. **bash cwd silently reverts mid-session** — confirmed during the PR-creation step. The `gh pr create --body-file dev/scratch/...` failed because cwd had drifted from the worktree to the main checkout. **Lesson:** the `feedback_pin_paths_in_worktree_sessions` rule earned its rent today — always pin absolute paths or `cd` explicitly before path-relative commands.

4. **Codex adrev's load-bearing value** — the 2026-06-04 Codex round found a true alpha-blocker (send_form catalog gap) that ALL three claude-side reviewers missed. **Lesson:** the cross-provider adrev round is non-negotiable per `feedback_no_carveout_on_cross_provider_adrev` — and the value of running it on the FULL diff (not just per-module) is real.

5. **Plan version-pinning is stale** — the spec/plans were authored at v0.10.0 framing; current release line is approaching v0.26.0. Operator clarified: don't propagate stale versions into new work + fix in-place when touched. Handled inline in Task 12's README + user-guide rewrites.

## Operator's next-session starting prompt

(Already at the top of this file under "Critical first action.")

## Agent: atoll-basin-crag
