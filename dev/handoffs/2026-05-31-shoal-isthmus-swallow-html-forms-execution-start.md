# Handoff — 2026-05-31 — shoal-isthmus-swallow — HTML Forms v0.1 execution started; T1.1 shipped; 25 tasks + Codex round + PR open remain

> Date: 2026-05-31 · Agent: shoal-isthmus-swallow · bd: tuxlink-v1p · Machine: pandora · Worktree: `worktrees/bd-tuxlink-v1p-html-forms-execution/`

## 0. TL;DR

This session shipped PR #175 (Pat strip + native B2F + attachments), then revised the HTML Forms plan to rev-4 to align with the new native backend, then began HTML Forms v0.1 execution. **T1.1 (forms module + types scaffolding) is the only HTML Forms commit landed; the autonomous loop fired once before the operator chose to hand off for a fresh-context start.**

The branch is `bd-tuxlink-v1p/html-forms-execution`, branched off current main (which now includes the Pat strip). 25 tasks remain (T1.2 → T11.1) plus the final Codex cross-provider review and PR open. Plan rev-4 + spec rev-3 are both on the branch and consistent.

**The next session should resume HTML Forms execution via `superpowers:subagent-driven-development` starting at T1.2.**

## 1. Branch + bd state

- **Branch:** `bd-tuxlink-v1p/html-forms-execution` (off `origin/main` HEAD `da66b15`)
- **Latest commit:** `ba34575` (T1.1 — forms module + types scaffolding)
- **Working tree:** clean
- **Pushed:** yes (3 commits ahead of `origin/main` — plan rev-4, ADR ref fix, T1.1)
- **bd `tuxlink-v1p`:** `in_progress`, blocker cleared after PR #175 merged. Notes updated with worktree path + plan baseline.
- **bd `tuxlink-9phd`:** **CLOSED** this session (the Pat strip PR landed).

## 2. Commits on the branch (3 ahead of main)

```
ba34575 feat(forms): create module + types per spec §6.1 (tuxlink-v1p)
749dce3 docs(plan): fix ADR 0016 filename reference in rev-4 header
c99003b docs(plan): HTML Forms v0.1 plan rev-3 → rev-4 (align with native backend)
```

All three commits authored by `shoal-isthmus-swallow`. Carry the `Agent:` trailer.

## 3. What's done

### Pre-execution setup (this session)

1. **PR #175 (Pat strip) merged.** All 9phd phases are on main.
2. **Plan revised rev-3 → rev-4** to align with the now-shipped native backend:
   - T0.1 + T0.2 marked **DONE** (landed via PR #151 — `OutboundAttachment` + `OutboundMessage.attachments`).
   - T0.3 marked **OBSOLETE** (`pat_client.rs` deleted; native `compose_message_with_files` is the equivalent).
   - T3.1 rewritten to use the native compose pipeline (no Pat REST routing).
   - Architecture paragraph, File Structure table, Self-Review table all updated to drop Path A.
   - Pat-prose audit: all load-bearing references swapped; non-load-bearing interop mentions (WLE+Pat wire-format compatibility) preserved.
   - ADR 0016 path corrected to `docs/adr/0016-native-b2f-outbound-with-attachments.md`.
3. **Stale v1p worktree disposed** per ADR 0009 disposal ritual; yew-cypress-oak's adrev transcripts preserved at `.claude/worktree-archives/v1p-adrev-transcripts-20260531T*Z/`.
4. **Fresh worktree** created off main: `worktrees/bd-tuxlink-v1p-html-forms-execution/`.
5. **Zombie remote branch** (`bd-tuxlink-9phd/strip-pat-add-native-attachments`) deleted after my post-merge push accidentally recreated it.
6. **bd cleanup**: `tuxlink-9phd` closed; `tuxlink-v1p` dependency on it removed.

### T1.1 — forms module + types scaffolding (committed)

`src-tauri/src/forms/{mod,types,catalog,parse,serialize,validation}.rs` + `templates/mod.rs` created. `pub mod forms;` added to `lib.rs`. Public types: `FormDef`, `FormField`, `FormPayload`, `FieldKind`, `FormParameters` (all per spec §6.1). Submodule files are stubs to keep `mod.rs`'s `pub mod` declarations resolving; populated in T1.2+.

`cargo build --workspace`: clean (no new warnings). No tests yet — T1.8 covers integration tests at the Phase 1 close.

## 4. What's pending (the next session's full backlog)

### Phase 1 — forms backend module (7 tasks remaining)
- **T1.2:** validation module — `form_id` regex (`^[A-Za-z0-9_-]{1,32}$`) + `MAX_FORM_XML_BYTES = 256 * 1024` constant.
- **T1.3:** Add `quick-xml = "0.39"` (spec §10 mandates `0.39.x`, not 0.36).
- **T1.4:** `parse.rs::detect_form_attachment` — scan an `&[OutboundAttachment]`-like input for filenames matching `RMS_Express_Form_*.xml`.
- **T1.5:** `parse.rs::parse_form_xml` — hardened parser (quick-xml + size cap + entity-expansion rejection).
- **T1.6:** `serialize.rs::serialize_form_xml` + `render_body_template`.
- **T1.7:** `catalog.rs` + `templates/ics213.rs` — first `FormDef`.
- **T1.8:** Integration test `forms_test.rs` — round-trip serialize→parse.

### Phase 2 — detection bug fix + DTO extension (3 tasks)
- **T2.1:** Fix `is_form` detection in `ui_commands.rs:325-327` — attachment-name match instead of body prefix. (The existing detection looks at `body.starts_with('<?xml')` which is WRONG for WLE compatibility — XML lives in the attachment.)
- **T2.2:** Add `form_id: Option<String>` + `form_payload: Option<FormPayload>` to `ParsedMessageDto` at `ui_commands.rs:244-247`.
- **T2.3:** Update dev fixtures (`src/mailbox/devFixture.ts:256-265`) + existing tests for the new attachment-based format.

### Phase 3 — send_form Tauri command (1 task — rev-4 REWROTE this)
- **T3.1:** `send_form` constructs `OutboundMessage` with `OutboundAttachment { filename, bytes }` (**NO `content_type`** — that field was dropped in 9phd T1.1), calls `backend.send_message(msg)` directly (same path as `message_send`). Return: `Result<String, UiError>` (the MID string). Register in `lib.rs` `invoke_handler`. **Important:** the rev-3 version used Pat REST routing; rev-4 strips that entirely. Read the rev-4 task text carefully.

### Phase 4 — React forms infrastructure (4 tasks)
- **T4.1:** TS types mirror in `src/forms/types.ts`.
- **T4.2:** Form component registry (`src/forms/forms.ts`).
- **T4.3:** `KeyValueView` fallback for unknown forms.
- **T4.4:** `FormPicker` modal.

### Phase 5 — ICS-213 React form (3 tasks)
- **T5.1:** `Ics213Form` compose-side.
- **T5.2:** `Ics213View` read-side.
- **T5.3:** Register ICS-213 in the form registry.

### Phase 6 — Compose integration (4 tasks)
- **T6.1:** Compose form button + region replacement.
- **T6.2:** Pre-form-switch unsaved-changes dialog.
- **T6.3:** Extend `DraftData` for form fields.
- **T6.4:** Form-draft round-trip test.

### Phase 7 — MessageView integration (1 task)
- **T7.1:** Form-render dispatch in `src/mailbox/MessageView.tsx:141-232`.

### Phase 8 — replyActions update (1 task)
- **T8.1:** Body-vs-XML logic for new attachment-based detection at `src/mailbox/replyActions.ts:75-95`; update tests at `replyActions.test.ts`.

### Phase 9 — additional bundled forms (4 tasks, SERIAL execution per rev-2 fix)
- **T9.1:** ICS-309 Communications Log.
- **T9.2:** GPS Position Report.
- **T9.3:** Bulletin.
- **T9.4:** Damage Assessment.

(These touch `catalog.rs` + `forms.ts` registry, hence the serial requirement — parallel claim from rev-1 was a defect, corrected in rev-2 + carried forward to rev-4.)

### Phase 10 — hardening cross-cuts (2 tasks)
- **T10.1:** `dangerouslySetInnerHTML` ban (Vitest assertion that scans `src/forms/**/*.tsx` for the string).
- **T10.2:** Attachment filename sanitization. **Note:** my 9phd Codex P2.2 fix already rejects `\r\n\0` in attachment filenames at the `compose.rs` level; T10.2 should verify the spec's full requirement (path-traversal, control chars beyond `\r\n\0`, length) and align — possibly mostly DONE already.

### Phase 11 — Codex review + live smokes
- **T11.1:** Codex cross-provider review on the full implementation. Use CLAUDE.md custom-prompt pattern (stdin-piped to `npx --yes @openai/codex review -`). Save to `dev/adversarial/<date>-html-forms-post-impl-codex.md` (gitignored). Apply P0/P1 findings; document P2 in PR body.
- **T11.2-T11.5 (live smokes):** explicitly **OPERATOR-DRIVEN** per yew-cypress-oak's BRF — out of subagent scope. Document them in PR body as operator-pending.

### Final: PR open
- `gh pr create --base main --head bd-tuxlink-v1p/html-forms-execution --title "[shoal-isthmus-swallow] HTML Forms v0.1 (tuxlink-v1p)" --body-file <body>`.
- Update `bd update tuxlink-v1p --notes` with PR link.

## 5. Worktree state at session end

| Worktree | bd issue | Branch | State |
|---|---|---|---|
| `worktrees/bd-tuxlink-v1p-html-forms-execution/` | tuxlink-v1p | `bd-tuxlink-v1p/html-forms-execution` | **LIVE — T1.1 shipped + pushed; 25 tasks remaining; working tree clean.** |

**Disposed this session:** `worktrees/bd-tuxlink-v1p-html-forms-design/` (the old paused-mid-Phase-0 worktree from heron-tanager-bog).

**The bd-tuxlink-9phd worktree** (`worktrees/bd-tuxlink-9phd-strip-pat-add-native-attachments/`) is STILL LIVE on disk even though PR #175 merged. The next session may dispose of it per ADR 0009 (its branch is gone; the cargo target dir is ~several GB). Leaving disposal to the next session's discretion since it has no pending work attached.

**Untracked state in the v1p worktree at end-of-session:** none.

**Gitignored-stateful content in the v1p worktree** (per ADR 0009):
- `dev/adversarial/` — empty (this session hasn't run Codex yet; T11.1 will populate)
- `src-tauri/target/` — cargo build artifacts from T1.1's build verification
- `node_modules/` — NOT installed yet (the operator's smoke may have installed it elsewhere; this worktree is fresh)

**Archived adrev reference material** (gitignored, per-machine):
- `.claude/worktree-archives/v1p-adrev-transcripts-20260531T*Z/` — 9 transcripts from yew-cypress-oak's BRF rounds. Useful as historical context for plan rev-4's design choices. **Don't re-run those adrev rounds — they're done.**

## 6. Memory + bd state

- **bd `tuxlink-v1p`:** `in_progress`, owned by Cameron Zucker. Notes contain worktree path + plan baseline. **Update at session end:** the next session should append "T1.1 shipped; resuming at T1.2" or similar.
- **bd `tuxlink-9phd`:** **CLOSED** this session.
- **Memory updates this session:** none new — this handoff is the structured record.

## 7. Critical gotchas for the next session

### Cwd revert pattern (hit this 6+ times this session)
Bash tool's `cd <worktree> && command` keeps the payload's cwd at the SHELL'S starting cwd. The lease hook reads payload.cwd, not the inline `cd`. **Fix:** run `cd <worktree>` in a standalone Bash call FIRST, then run the git op in a separate Bash call. After any subagent dispatch, **re-cd** because subagent dispatches can disturb shell cwd.

Per memory `feedback_pin_paths_in_worktree_sessions` + `feedback_worktree_git_hook_cwd_and_mergebase`.

### Lease hook may deny git ops
If you see `Main-checkout HEAD/branch/history operation BLOCKED`, you're not in the worktree's cwd. Re-cd standalone and retry. **Never** try to take the lease per memory `feedback_main_checkout_is_operator_state` + `feedback_stale_lease_means_worktree`.

### Port :1420 collision (browser smokes)
Per memory `project_worktree_dev_port_collision`: only ONE `pnpm tauri dev` can run machine-wide because Vite binds `:1420 strictPort`. The operator may have another worktree's dev running. **DO NOT launch tauri dev autonomously** — browser smoke is operator-deferred per `feedback_browser_smoke_before_ship`.

### Codex quota
Per memory `feedback_codex_quota_gotcha`: if the final Codex round returns `ERROR: You've hit your usage limit ... try again at HH:MM`, that's capacity-defer, NOT skip. Either wait + retry, or write a handoff documenting the deferral. **Don't substitute Claude** — `feedback_no_carveout_on_cross_provider_adrev` is absolute.

### bd dep removal
The flag I instinctively typed (`--remove-blocker`) doesn't exist. The correct command is `bd dep remove <blocked-id> <blocker-id>`. Same for adding: `bd dep add <blocked-id> <blocker-id>`.

### Plan rev-3 fictional API risk
Plan rev-3 had ~6 fictional API citations the 9phd subagents caught via verify-before-coding. Plan rev-4 fixed the most load-bearing ones (T0.3, T3.1) but residual drift in tasks I didn't deeply audit (Phases 1, 2, 4-10) is possible. **Subagents MUST verify-before-coding** every task; surface any divergence in commit bodies.

### Phase 9 must be SERIAL
Per rev-2 fix carried into rev-4: T9.1-T9.4 each touch `catalog.rs` + `forms.ts` registry. Subagent-driven-development cannot parallelize these. Dispatch them one at a time.

## 8. Approach notes for the next session

1. **Use `superpowers:subagent-driven-development`** — invoke the Skill at start.
2. **Per task workflow:** implementer → spec reviewer (or controller-side spec review if mechanical) → code-quality reviewer → controller commits + pushes. The skill's own prompt-templates (`./implementer-prompt.md`, `./spec-reviewer-prompt.md`, `./code-quality-reviewer-prompt.md`) are the canonical shape.
3. **Subagents do code + tests but stop before commit.** Controller commits from the worktree.
4. **Push every commit immediately** per `feedback_never_hold_a_push`.
5. **Phase-end review loops:** after each phase, run `cargo build --workspace + cargo test --workspace + cargo clippy + npx tsc --noEmit + npx vitest run <changed-fe-area>`.
6. **`feedback_no_ceremony_spiral_on_small_fixes`:** trivial mechanical changes (1 file, <30 LOC, follows exact plan text) can use controller-side spec review + subagent code-quality only. Magpie-grouse-shoal's batched pattern.
7. **`feedback_subagent_ldc_scoping`:** subagent prompts must explicitly authorize updating the plan's checkbox state if you want them to mark `- [x]`. Default subagent posture is "don't modify plan text."

## 9. Final Codex round (T11.1) — full attack-angle prompt

For T11.1, when ready, dispatch Codex against the branch diff vs `main`. Custom-prompt pattern (stdin):

```bash
cat > /tmp/codex-prompt.txt <<'EOF'
You are doing adversarial code review of the diff against origin/main in
this worktree. Run `git diff origin/main..HEAD` to see all changes.

Context: this is bd-tuxlink-v1p — HTML Forms v0.1 implementation per
docs/superpowers/specs/2026-05-30-html-forms-design.md (rev-3) and
docs/superpowers/plans/2026-05-30-html-forms-v0.1-plan.md (rev-4).

Focus attack angles:

1. XML parser hardening: src-tauri/src/forms/parse.rs uses quick-xml.
   Verify: size cap enforced BEFORE parsing (not after), entity expansion
   disabled, DTD references rejected, billion-laughs immune, deeply
   nested elements bounded.

2. Form payload integrity: the round-trip serialize → wire → parse must
   be byte-identical for known forms. Check WLE wire-format compatibility
   per spec §3.

3. send_form IPC bridge: src-tauri/src/ui_commands.rs::send_form must
   construct OutboundAttachment with NO content_type field (dropped in
   9phd T1.1). Verify no stale references.

4. XSS surface: src/forms/**/*.tsx must NOT use dangerouslySetInnerHTML.
   T10.1 enforces this via Vitest. Verify the assertion catches the right
   files.

5. Filename injection: attachment filenames must reject \r \n \0 (already
   enforced at compose.rs in 9phd Codex P2.2). T10.2 may extend to path-
   traversal characters; verify spec alignment.

6. Form registry completeness: every FormDef in catalog.rs must have a
   matching React component pair (Form + View) in src/forms/<id>/ AND be
   registered in src/forms/forms.ts. KeyValueView fallback only fires for
   UNREGISTERED form_ids.

7. Subagent self-review bias: many commits authored by subagents with
   controller-side review only. Look for things the subagents would NOT
   have flagged about their own work — dropped error branches, off-by-one
   in size-cap math, missed import edges.

Output findings as markdown at the end, prioritized P0/P1/P2/P3 with
file:line citations.
EOF
cat /tmp/codex-prompt.txt | npx --yes @openai/codex review - 2>&1 \
  | tee dev/adversarial/$(date +%Y-%m-%d)-html-forms-post-impl-codex.md
```

The above attack-angle prompt is informed by the actual implementation surfaces this PR ships.

## 10. PR body template (for when ready)

Mirror the structure of PR #175. Sections:
- Summary (1 paragraph)
- What ships (bullet list of capabilities)
- Spec + plan references (links)
- Cross-provider review (Codex findings + resolutions)
- Operator post-merge actions (browser smokes T11.2-T11.5; bd close + memory updates)
- BREAKING CHANGES (any)
- Verification (cargo test count, tsc, vitest, Codex applied)
- Test plan checkbox list

---

Agent: shoal-isthmus-swallow
