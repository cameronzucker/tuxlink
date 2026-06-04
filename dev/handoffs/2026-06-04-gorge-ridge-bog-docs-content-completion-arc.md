# Handoff — gorge-ridge-bog — docs content-completion arc

> **Date:** 2026-06-04 · **Agent:** `gorge-ridge-bog` · **Machine:** pandora
>
> **Arc:** Multi-PR docs effort. Started by resuming a willow-yew-esker handoff
> for PR #347 (docs renderer + IA), through content fill, verification,
> Mermaid CSS regression + recovery, screenshot markers, Tauri app_id
> bandaid cleanup, and a Winlink coverage check.

---

## 0. Critical first action — next session

```
1. Read THIS handoff first.
2. If PR #361 (diagrams + markers, branch bd-tuxlink-yt2g/docs-diagrams-markers)
   is still open: smoke walk the help window with topics 02, 03, 04, 05, 06,
   07, 09, 10, 11, 14, 15, 16, 17, 18, 23, 24, 25, 29, 32. Every diagram
   should now render legibly (PR #357's mermaid CSS revert is merged).
   Merge when satisfied.
3. Pick from `bd ready`. Two issues filed for next session, both P2:
   - tuxlink-drbi: Tauri 2.x app_id mismatch — register
     GApplication.application_id matching tauri.conf.json bundle identifier.
     Retires the install-desktop-entry.sh + uninstall-desktop-entry.sh
     bandaid pair.
   - tuxlink-yzn6: docs Wikipedia gaps + Winlink Programs Group user-pain
     research synthesis. READ THE BD ISSUE — it carries the access-method
     finding for the Gmail group (Takeout is dead; curated paste is the
     kickoff workflow). The 4 Wikipedia-derived gap topics are: VARA FM,
     Hybrid Network / Radio-only Winlink, Open Message Viewer + amateur
     no-encryption rule, PACTOR not-supported in topic 17.
4. Dispose merged-dead worktrees per ADR 0009 ritual when convenient
   (see §6).
```

---

## 1. Session arc (compressed)

This was a long, multi-thread session. The order:

1. **Resumed willow-yew-esker's PR #347 handoff.** PR #347 had been merged
   to main carrying the renderer + IA + 23 placeholder stubs. Resumed
   work intending to merge after smoke walk.
2. **PR #347 reality check.** Operator noticed the 23 stubs contained
   `*Content coming in a future PR — tracking issue: tuxlink-ymiv.*`
   placeholders in shipped user-facing docs — violation of his "no
   internal references in shipped features" rule. Rejected the
   admin-spiral plan; directed to "finish the work, not ship more
   stubs." See memory `feedback_no_incomplete_or_internal_refs_in_shipped_features`
   (saved this session).
3. **PR #351 — content fill (merged).** All 23 stub topics replaced with
   substantive content. 6 Mermaid diagrams across 04, 05, 07, 14, 17.
   ~14,000 lines of authored markdown. Branched from main as
   bd-tuxlink-obxz/docs-content-fill.
4. **Verification pass.** Per the AI-amateur-radio-reliability memory,
   training data was suspect. Verified against tuxlink source + Hamlib
   master + Hamexandria. 10+ corrections landed:
   - 5 of 6 Hamlib model IDs were wrong (G90 3081→3088, IC-7300
     3061→3073, FT-991A 1027→1035, TS-590S 2014→2031, TS-590SG
     2032→2037).
   - AX.25 paclen 256→128 per tuxlink's actual params.rs default.
   - VARA HF tier names corrected per command.rs (Narrow=BW500,
     Standard=BW2300, Tactical/Wide=BW2750).
   - Catalog request protocol completely rewritten per tuxlink's
     composer.rs (To=INQUIRY@winlink.org, Subject=REQUEST, Body=newline-
     separated filenames; weather is Saildocs not Winlink).
   - Mailbox path corrected to native-mbox in 02, 07, 22, 32.
   - B2F FS-answer codes expanded with letter/symbol/offset variants
     per proposal.rs.
   - Maidenhead precision table corrected (grids aren't square).
   - ARES check-in form correction (Winlink Check-in, not ICS-213)
     via Hamexandria OH8STN finding.
5. **PR #352 — Mermaid contrast + search slug-drift (merged).** Two
   bugs surfaced by operator smoke:
   - Mermaid diagrams rendered black-on-dark-blue (CSS gap on SVG text
     fill).
   - Search results all opened `01-what-is-tuxlink` (FTS5 index
     populated only on empty; old slugs from pre-#347 build never
     replaced). Fix: Index::docs_slugs() + slug-set drift check.
6. **PR #354 — uninstall-desktop-entry.sh (merged).** Operator noticed
   "still in Hamradio menu after apt remove." Diagnosed:
   `scripts/install-desktop-entry.sh` is a bandaid for Tauri 2.x app_id
   mismatch that drops user-local files surviving `apt remove`. No
   companion uninstaller existed. Shipped symmetric script + ran it
   (10 files removed). Filed tuxlink-drbi for the deeper Tauri-side
   fix.
7. **PR #357 — surgical mermaid CSS revert (merged).** PR #352's
   broader Mermaid CSS made diagrams ENTIRELY invisible (operator-
   confirmed via grim screenshot). Root cause: `.label-container
   { fill: var(--surface) }` — SVG `fill` is inherited; propagated
   into every descendant; everything painted as body background.
   Reverted to surgical minimum: theme shapes, theme edges, color
   SVG text with `!important`, color foreignObject HTML labels. No
   more selectors. The lesson is in the commit body.
8. **PR #361 — 14 diagrams + 23 markers (OPEN at session end).**
   Closed the spec §7.3 gap from PR #351:
   - 14 new Mermaid diagrams added across topics 02, 03, 06 (x2), 09,
     10, 11, 15, 16, 18, 23, 24, 25, 29, 32.
   - 23 new screenshot-needed markers across 02, 03, 10, 13, 18, 19,
     20, 21, 22, 23, 25, 26, 27.
   - Mermaid coverage: 20 / ~30 spec target = ~67%.
   - Marker coverage: 24 / ~63 spec target = ~38%.
9. **Winlink coverage check.** Operator asked whether tuxlink docs
   covered everything the Winlink "bible" covers. Did the comparison:
   - winlink.org/content/winlink_book_knowledge: 72 words, basically
     empty.
   - Wikipedia article: 2530 words, real reference.
   - Found 4 real gaps: VARA FM, Hybrid Network / Radio-only Winlink,
     Open Message Viewer + amateur no-encryption rule, PACTOR-not-
     supported in topic 17's mode comparison.
10. **Winlink Programs Group access investigation.** Operator wants
    user-pain research from the group at
    https://groups.google.com/g/winlink-programs-group. Findings:
    - Anonymous fetch returns "you don't have permission."
    - Google Takeout only exports groups operator OWNS (not member
      groups).
    - No third-party mirror exists.
    - Recommended path: curated paste (kickoff) + Playwright +
      exported cookies (scale-up if needed).
    All filed as part of tuxlink-yzn6.

---

## 2. Branch state at session end

| Branch | State |
|---|---|
| `main` | Has the merged content from PRs #347, #351, #352, #354, #357 (plus parallel-session PRs #356, #358, #359, #360 from other agents) |
| `bd-tuxlink-xygm/recover-handoffs` | OPERATOR STATE — main-checkout branch. Pre-session untouched. 3 untracked handoff docs + 1 untracked mockup HTML still uncommitted from prior sessions. |
| `bd-tuxlink-yt2g/docs-diagrams-markers` | **OPEN PR #361** — 2 commits (8 + 6 mermaid, 18 + 5 markers); pre-push lint:docs passed; awaiting operator smoke + merge |
| `bd-tuxlink-ymiv/docs-knowledge-base-spec` | Merged (PR #347) — branch is dead |
| `bd-tuxlink-obxz/docs-content-fill` | Merged (PR #351) — branch is dead |
| `bd-tuxlink-b5oa/docs-mermaid-search-fixes` | Merged (PR #352) — branch is dead |
| `bd-tuxlink-md17/uninstall-desktop-entry` | Merged (PR #354) — branch is dead |
| `bd-tuxlink-btf3/mermaid-surgical-revert` | Merged (PR #357) — branch is dead |

---

## 3. PRs from this session

| # | Title | State | Branch |
|---|---|---|---|
| #351 | content fill for all 23 stubs | MERGED | bd-tuxlink-obxz/docs-content-fill |
| #352 | mermaid contrast + search slug-drift | MERGED | bd-tuxlink-b5oa/docs-mermaid-search-fixes |
| #354 | uninstall-desktop-entry.sh | MERGED | bd-tuxlink-md17/uninstall-desktop-entry |
| #357 | surgical mermaid CSS revert | MERGED | bd-tuxlink-btf3/mermaid-surgical-revert |
| #361 | 14 diagrams + 23 markers | OPEN | bd-tuxlink-yt2g/docs-diagrams-markers |

---

## 4. bd issues filed this session for next session

| Issue | Pri | Title |
|---|---|---|
| `tuxlink-drbi` | P2 | Tauri 2.x app_id mismatch — register GApplication.application_id matching bundle identifier (retires the install/uninstall-desktop-entry bandaid pair) |
| `tuxlink-yzn6` | P2 | docs: Wikipedia gaps + Winlink Programs Group user-pain research synthesis (carries access-method findings — start there before any docs work) |

bd issues closed by PRs in this session: `tuxlink-f95k` (in PR #352),
`tuxlink-b5oa` (in PR #352), `tuxlink-md17` (in PR #354), `tuxlink-btf3`
(in PR #357), `tuxlink-obxz` (closed when PR #351 merged), `tuxlink-yt2g`
(will close when PR #361 merges).

---

## 5. Memories saved this session

| Memory | Type | Use |
|---|---|---|
| `feedback_no_incomplete_or_internal_refs_in_shipped_features` | feedback | The "no admin spiral, no internal refs" rule operator stated explicitly when rejecting PR #347's stub-shipping plan |
| `feedback_no_users_calibration` | feedback | "Only data loss is a real consequence; everything else recoverable." Recalibrated risk model |

---

## 6. Worktree disposal — ready when convenient

Per ADR 0009 ritual, all 5 merged-dead worktrees can be disposed:

```bash
# For each merged-dead worktree (run from inside it):
cd /home/administrator/Code/tuxlink/worktrees/<worktree-name>
git status --short && git ls-files --others --exclude-standard && git stash list
cd /home/administrator/Code/tuxlink
rm -rf /home/administrator/Code/tuxlink/worktrees/<worktree-name>
git worktree prune
```

Worktrees to dispose (all merged-dead):
- `bd-tuxlink-ymiv-docs-knowledge-base-spec` (PR #347)
- `bd-tuxlink-obxz-docs-content-fill` (PR #351)
- `bd-tuxlink-b5oa-docs-mermaid-search-fixes` (PR #352)
- `bd-tuxlink-md17-uninstall-desktop-entry` (PR #354)
- `bd-tuxlink-btf3-mermaid-surgical-revert` (PR #357)

Worktrees to KEEP for now (PR open):
- `bd-tuxlink-yt2g-docs-diagrams-markers` (PR #361 — dispose after merge)

`dev/adversarial/` directories in these worktrees are gitignored and
will be lost on disposal — no Codex adrev was run this session, so this
is fine.

---

## 7. Out-of-repo state changes

System-level cleanup performed this session via `bash scripts/uninstall-desktop-entry.sh`:
- Removed 2 `.desktop` files in `~/.local/share/applications/` (tuxlink,
  com.tuxlink.app).
- Removed 8 icon files in `~/.local/share/icons/hicolor/<sizes>/apps/`.
- Refreshed `update-desktop-database` + `gtk-update-icon-cache`.

Operator confirmed menu entry gone.

Side effect: Tauri dev windows now render with generic icon (no
.desktop to look up against). Tracked by tuxlink-drbi for proper fix
via GApplication.application_id registration.

---

## 8. Critical guidance for next session

1. **PR #361 still open at session end.** Operator should smoke
   diagrams + merge.
2. **tuxlink-yzn6's body carries the Gmail-group access-method
   finding.** READ THE BD ISSUE before any docs work. Don't try
   Takeout, don't try anonymous fetch — both confirmed not viable.
   Start with curated paste workflow.
3. **The "no incomplete or internal refs" rule is now in memory.**
   For ANY user-facing surface (docs strings, UI copy, etc.) — no
   bd IDs, no "future PR", no phase numbers, no stub placeholders.
4. **The "no users calibration" memory** unblocks decisive autonomous
   execution for non-data-loss situations. Tighten only on data loss,
   destructive ops, RF transmission.
5. **PR #347 + #351 merge mistake context.** Operator merged PR #347
   then realized the stubs violated his principle. This session
   recovered by writing all the content. The pattern to avoid: don't
   ship "PR #1 of N" infrastructure with placeholder user-visible
   content; collapse the plan.

---

## 9. Session totals

- **5 PRs merged** (#351, #352, #354, #357 + parallel-session #356/#358/#359/#360)
- **1 PR open** (#361 — diagrams + markers, 2 commits)
- **2 bd issues filed for next session** (tuxlink-drbi, tuxlink-yzn6)
- **6 bd issues closed** via merges
- **2 memories saved** (`feedback_no_incomplete_or_internal_refs_in_shipped_features`, `feedback_no_users_calibration`)
- **23 docs topics** went from placeholder stubs to substantive content
- **14 Mermaid diagrams** added (across PR #361)
- **23 screenshot-needed markers** added (across PR #361)
- **Verification corrections:** Hamlib IDs (5), AX.25 paclen, VARA tier, catalog protocol (substantial rewrite), mailbox paths (3 files), B2F answer codes, Maidenhead precision, ARES form, DigiRig USB IDs
- **One scripts/ tool shipped** (uninstall-desktop-entry.sh)
- **One CSS regression survived** (PR #352 → invisible diagrams → PR #357 surgical revert)

---

## 10. Next-session prompt (paste into a fresh session)

```
Resume tuxlink from gorge-ridge-bog's 2026-06-04 docs content-completion handoff.

Handoff doc: dev/handoffs/2026-06-04-gorge-ridge-bog-docs-content-completion-arc.md
READ IT FIRST.

State: PR #361 (14 mermaid + 23 markers, branch bd-tuxlink-yt2g/docs-diagrams-markers)
may still be OPEN — check `gh pr view 361`. If open, smoke walk the help
window with topics 02, 03, 04, 05, 06, 07, 09, 10, 11, 14, 15, 16, 17,
18, 23, 24, 25, 29, 32 — diagrams should render legibly (PR #357's
mermaid CSS revert is merged). Merge when satisfied.

Then pick from `bd ready`:
- tuxlink-yzn6 (P2) — docs: Wikipedia gaps + Winlink Programs Group
  user-pain research. READ THE BD ISSUE before starting — it carries
  the Gmail-group access-method finding (Takeout dead; curated paste
  is the kickoff workflow). Confirm with operator which access path
  they prefer before doing any docs work.
- tuxlink-drbi (P2) — Tauri 2.x app_id mismatch fix.

Worktree cleanup (low priority): 5 merged-dead worktrees to dispose
per ADR 0009 ritual. See handoff §6.
```

---

Agent: gorge-ridge-bog
