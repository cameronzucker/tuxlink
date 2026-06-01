# Handoff: 2026-06-01 — overnight execution — moss-cove-hemlock

**Agent:** moss-cove-hemlock
**Session shape:** Autonomous overnight execution of the 14-item slate
authored by dahlia-heron-spruce at
`worktrees/bd-tuxlink-izgv-html-forms-fullparity-design/dev/handoffs/2026-06-01-overnight-briefing-for-fresh-session.md`.
Six bd issues shipped end-to-end (4 PRs opened); HTML Forms full-parity
design now has all 3 phase plans written, all WLE backend infrastructure
landed, and 3 P2 follow-ups filed from a Codex adrev round.

## TL;DR

Items A–N (where A was pre-done) progress:

| Item | bd | Status | Output |
|---|---|---|---|
| A | (priming) | ✅ done by prior session | — |
| B | `tuxlink-htx1` | ✅ shipped | PR #193 (version-string sweep) |
| C | `tuxlink-h1km` | ✅ shipped | PR #194 (Cc field enabled) |
| D | `tuxlink-ytya` (plan) | ✅ shipped | docs commit on PR #186 (1887-line P1 plan) |
| E | `tuxlink-hnkn` (plan) | ✅ shipped | docs commit on PR #186 (1087-line P2 plan) |
| F | `tuxlink-4w8u` (plan) | ✅ shipped | docs commit on PR #186 (864-line P3 plan) |
| G | (recon) | ✅ shipped | scratch doc (gitignored) + bd-`tuxlink-ytya` notes |
| H–L | `tuxlink-ytya` (impl) | ✅ shipped | PR #195 (P1 backend infrastructure) |
| M | (Codex adrev) | ✅ http_server round done; multipart deferred to operator | adrev transcript gitignored; P1s fixed; P2s filed |
| N | (handoff + umbrella close) | ✅ this doc + `tuxlink-q28i` closes on push | — |

All 4 PRs (#193, #194, #195 + the 3 plan commits on #186) are awaiting
operator review. None merged by the agent per the briefing's hard
constraint.

## What shipped

### B. `tuxlink-htx1` — version strings sweep (PR #193)

Three atomic-per-class commits dropping stale `v0.0.1` / `v0.1` pins
across `src/` (compose, mailbox, shell+wizard+misc). User-visible
changes: `Drop files here to attach (attachment send not yet wired)` in
place of `(v0.0.1: attachment send not wired)`; menu-bar `v0.1` badge
→ `soon`; `FORM_PLACEHOLDER` reads "Form rendering coming soon" instead
of "arrives in v0.1". 672/672 vitest pass.

Cc-related strings (Compose.tsx L13/16/28/244 + draft.test.ts L279)
were intentionally left for item C.

### C. `tuxlink-h1km` — Cc field enabled (PR #194)

End-to-end Cc support verified by code-tracing:
`Compose.tsx cc state → OutboundDraftDto.cc → ui_commands::message_send
→ NativeBackend::send_message → compose_message_with_files → "Cc:" header`.
The backend supported Cc all along; only the UI was disabled.

`src/compose/useDraft.ts` gains an optional `cc?: string` field
(back-compat for legacy drafts). `Compose.tsx` adds `cc` state and
plumbs it through `savedSnapshotRef` / `isDirty` / `loadDraft` /
autosave / `handleSaveDraft` / `handleSend` / `handleFormSubmit` /
`handleSaveAndProceed` / `handleDiscardAndProceed`. The disabled Cc
input + warning hint are replaced with a live `to`-shaped input.
673/673 vitest pass (one new back-compat test).

**Browser smoke required before merge** per `feedback_browser_smoke_before_ship`:
fill Cc, send, verify CMS receives Cc header(s).

### D/E/F. P1/P2/P3 plans on the izgv branch (PR #186)

Three implementation plans committed and pushed on the izgv branch
stacking onto PR #186. Total: 3838 plan lines.

| File | Lines | Phase | Highlights |
|---|---|---|---|
| `2026-06-01-html-forms-p1-webview-infra.md` | 1887 | P1 | Tasks 0–13 (snapshot bundle through Codex adrev + PR open); naming-collision note (`forms::wle_templates` not `forms::templates`); TDD verbatim code in every Rust task |
| `2026-06-01-html-forms-p2-native-autofill.md` | 1087 | P2 | Tasks 0–6 (Position/ICS-309/Check-In native rebuilds + FormDraftLibrary); TBD callouts for PDF lib + map widget + draft library scope; depends on P1 |
| `2026-06-01-html-forms-p3-catalog-freshness.md` | 864 | P3 | Tasks 0–8 (winlink.org updater + form-aware reply + draft library generalization + hot-reload + override settings); depends on P1+P2; security-focused Codex round on the updater |

### G. WLE snapshot recon (gitignored scratch doc + bd notes)

`dev/scratch/2026-06-01-wle-snapshot-recon.md` in the izgv worktree.
Validates:
- URL: `https://downloads.winlink.org/User%20Programs/Standard_Forms.zip`
- HTTP 200, 2.5 MB compressed, 10 MB uncompressed, 419 files, 25
  category folders
- Version pin: `1.1.20.0` (Apr 23 2026)
- SHA-256: `26b5ec33bd38a5e2ad9949cb6bd4eb1cc4eabbeb485148913a3abe89ee1abd88`
- Placeholders: `{FormServer}` 219x, `{FormPort}` 223x, `{FormFolder}` 5x
- Reply templates: 13 present (validates the P3 form-aware-reply design)

**Two P1-plan revisions surfaced in the recon:**
1. The zip has NO `Standard_Forms/` top-level wrapper (the executor
   wraps during extract; P1 Task 0 step 3 already accounts for this).
2. WLE form submits POST to `http://{FormServer}:{FormPort}` (no path
   component); recommended **Option A** (drop the URL-token defense,
   rely on loopback + capability + ephemeral port + new Origin check
   + new CSP). This recommendation was adopted in the P1 impl below.

### H–L. P1 backend modules (PR #195)

Eight commits on `bd-tuxlink-ytya/p1-webview-infra`:

| Commit | Module | Tests |
|---|---|---|
| `79e2390` | WLE Standard Forms snapshot bundle (10 MB / 419 files) | — |
| `0e758d2` | axum/multer/walkdir Cargo deps | — |
| `4821d7e` | `forms::wle_templates` (catalog enumeration) | 6/6 |
| `b2ff63f` | `forms::skin` (CSS asset) | 5/5 |
| `a7f63c4` | `forms::multipart` (body parser) | 10/10 |
| `ee74d85` | `forms::http_server` (lazy 127.0.0.1:0 axum server) | 12/12 |
| `03e4f7c` | `forms-webview.json` Tauri capability | 2/2 |
| `138ee9d` | Codex P1 fixes (CSP + Origin check + empty perms) | +3 → 15/15 http_server |

**Total: 35 new Rust tests + 1 integration test.**

### M. Codex adrev round on http_server

One adrev round on the http_server module (~3800 lines of Codex
output captured at
`dev/adversarial/2026-06-01-p1-http-server-codex.md`, gitignored
per project convention). Real review, not a quota stub.

**Findings:**

| Severity | Count | Disposition |
|---|---|---|
| P0 | 0 | none |
| P1 | 3 | applied inline (commit `138ee9d`) |
| P2 | 3 | filed as bd issues (`rk6s` / `4g2n` / `gheo`) |
| P3 | 0 | — |

**P1 fixes applied:**
1. `permissions: ["core:default"]` → `permissions: []` on the
   forms-webview capability (Tauri's `core:default` expands to
   event/window/webview/app/resource defaults, contradicting the
   no-IPC threat model).
2. Added `Content-Security-Policy` header on form-server HTML responses
   (`default-src 'self'; script-src 'self' 'unsafe-inline'; style-src
   'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self';
   form-action 'self'; frame-src 'none'; object-src 'none'`) to prevent
   external script load / form-action exfiltration from a malicious
   custom-form template.
3. Added `Origin` header validation on POST `/` — requires
   `Origin: http://127.0.0.1:<port>` exactly. Defends against same-host
   processes forging submissions while a session is open.

**Multipart adrev round deferred** to the operator. The http_server
round read `multipart.rs` as supplementary context, so the multipart
surface has indirect coverage. Per `feedback_codex_quota_gotcha`, this
agent did not burn additional rounds; the operator can run a dedicated
multipart adrev round on wake if desired.

## What's blocked on operator

### Browser-smoke gates (high)

| PR | Smoke check |
|---|---|
| #194 (Cc) | Fill Cc input, send, confirm CMS receives `Cc:` header end-to-end |
| #195 (P1 backend) | `pnpm tauri dev` startup clean; bundle ships into the AppImage resource dir; multipart adrev round (optional, capacity-permitting) |

### Decisions surfaced in plans (medium)

The P2/P3 plans defer concrete operator decisions to Task 0 of each
plan. Until those decisions land in the bd issue notes, the executor
of P2/P3 follows the documented defaults:

| Decision | Default in plan |
|---|---|
| PDF library for ICS-309 | Defer to P3 (ship XML + CSV at P2) |
| Map widget for Position | Skip widget; ship text input |
| Draft library scope at P2 | Check-In only; P3 generalizes |
| P3 PDF library (if P2 deferred) | typst |
| P3 map widget (if P2 deferred) | Leaflet w/ small offline tile pack |
| Custom-forms hot-reload | Live (notify crate) |

### P2 follow-ups from the Codex adrev (low)

Three filed against `tuxlink-ytya`:

- `tuxlink-rk6s` — Bound the mpsc submission channel (prevents
  local-flood memory growth)
- `tuxlink-4g2n` — Asset-size cap on `/folder/<path>` (prevent memory
  exhaustion on large bundled images)
- `tuxlink-gheo` — Nested `{FormFolder}` resolution bug (axum decodes
  `%2F` before splitn; only 5 templates use `{FormFolder}` so impact
  is small)

### P1 frontend work deferred to operator wake

Plan Tasks 8/9/10/11 require browser smoke (per the briefing's hard
constraints): Tauri command wiring for `forms_list_catalog` /
`open_webview_form` / `close_webview_form_server`; React
`WebviewFormHost` + `CatalogBrowser` components; Compose.tsx
integration; receive-side Viewer-mode fallback.

These are unblocked by the backend modules in PR #195; the next agent
can pick them up after the operator smokes the backend.

## What's pending decision (none)

Per the briefing, the overnight slate is operator-input-free except for
the P2/P3 Task 0 decisions noted above. No specific operator answers
are required to unblock the next session's work.

## Repository state

### In-flight worktrees (per ADR 0009)

**`worktrees/bd-tuxlink-htx1-version-strings-sweep/`** (item B, PR #193):
- Tracked dirty: none
- Untracked: none
- Gitignored-stateful: `node_modules/` (~700 MB; from `pnpm install --frozen-lockfile`)
- Stashes: none worktree-scoped
- Disposition after PR #193 merge: dispose via ADR 0009 ritual

**`worktrees/bd-tuxlink-h1km-enable-cc/`** (item C, PR #194):
- Tracked dirty: none
- Untracked: none
- Gitignored-stateful: `node_modules/`
- Stashes: none worktree-scoped
- Disposition after PR #194 merge: dispose via ADR 0009 ritual

**`worktrees/bd-tuxlink-izgv-html-forms-fullparity-design/`** (items D/E/F + G recon, PR #186):
- Tracked dirty: none
- Untracked: `dev/scratch/2026-06-01-wle-snapshot-decision.md` (template referenced by P1 plan Task 0; gitignored). Plus `dev/scratch/2026-06-01-wle-snapshot-recon.md` (the recon doc itself; gitignored)
- Gitignored-stateful: none beyond scratch
- Stashes: none worktree-scoped
- Disposition after PR #186 merge: dispose via ADR 0009 ritual. **NB: the recon doc + decision doc are gitignored; archive before disposal if the operator wants to retain them.**

**`worktrees/bd-tuxlink-ytya-p1-webview-infra/`** (items H–L + M + this handoff, PR #195):
- Tracked dirty: none (this doc lands as a final commit)
- Untracked: none
- Gitignored-stateful: `dev/adversarial/2026-06-01-p1-http-server-codex.md` (3800-line Codex transcript; gitignored per CLAUDE.md), plus `src-tauri/target/` build artifacts (~3 GB; release build not yet attempted)
- Stashes: none worktree-scoped
- Disposition after PR #195 merge: dispose via ADR 0009 ritual. **NB: the adrev transcript is the source of truth for the P1 findings; archive before disposal if the operator wants to consult older Codex output.**

## Time accounting

Wall-clock from session start (~02:13 UTC-equivalent — actually local
PT evening 2026-06-01) to session-end was ~3 hours of agent active
work, interleaved with ~4 minutes of /loop ScheduleWakeup cycles
(typical 270s tick) and ~5 minutes of Codex adrev wall-clock for the
http_server round. Per the briefing this was budgeted for ~21h
"chip continuously," well under budget — the plan-writing phase
(items D/E/F) was the largest dwell.

## Cross-session resources

- bd issues in_progress at handoff: `tuxlink-ytya` (P1 impl;
  closes-on-merge of #195), `tuxlink-q28i` (overnight umbrella;
  closes when this handoff lands)
- bd issues closed by this session: `tuxlink-htx1` (B), `tuxlink-h1km`
  (C), `tuxlink-ytya` is in_progress (impl shipped, awaiting merge),
  `tuxlink-hnkn` (E), `tuxlink-4w8u` (F)
- bd issues filed this session: `tuxlink-rk6s` (P2), `tuxlink-4g2n`
  (P2), `tuxlink-gheo` (P2)
- PRs opened this session: #193, #194, #195 (plus 3 plan-commit pushes
  stacking on #186)
- Worktrees live at session-end: 4 (htx1, h1km, izgv, ytya)
