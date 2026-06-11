# Spec: In-app form import (Forms-push G5 + G6)

**Status:** APPROVED (operator approved the design + Approach A in the 2026-06-11 brainstorm).
**Issues:** `tuxlink-z0le` (G5 single-file) + `tuxlink-fwob` (G6 org bundle). Epic `tuxlink-zkuk`.
**Mock (approved UX reference):** `dev/scratch/2026-06-11-forms-import-ux-mock.html`.
**Vision (locked, office-hours 2026-06-11):** tuxlink forms = interop + import done beautifully for the one evidenced user — the member stuck installing their org's custom Winlink forms (N0DAJ; AAMRON Discord). The aggregation/COP layer (G1–G4) is DEFERRED behind ground-truth `tuxlink-sobm`. This spec does NOT touch aggregation.

## 1. Problem

Tuxlink can render and send Winlink forms (webview host + bundled WLE catalog + `CatalogBrowser`, all shipped under `tuxlink-ytya`), and it enumerates an operator custom-forms directory (`forms::wle_templates::list`, custom overrides bundled by `id`). But there is no in-app way to GET a third-party or organization form INTO that directory. Today import is a manual file-drop into a hidden XDG path (`~/.local/share/tuxlink/forms/custom/`) with no UI and no docs — the exact wall new org members hit. This spec adds the import flow so the enumeration that already exists can surface the imported forms.

## 2. Scope

**In:** a unified **Import…** action (single `.html` file, a folder, or a `.zip`); validate-before-write; collision handling with confirm-before-overwrite; per-file result report; imported forms surfaced in the catalog by category; a "reveal custom-forms folder" affordance; a short in-app help entry (G11, `tuxlink-48uc`) covering finding + importing + using org forms.

**Out (explicit non-goals):** aggregation / common-operating-picture (G1–G4, deferred); a form **authoring/editor**; any change to the render/enumeration engine beyond a sort tweak; standard-forms updater changes; drag-and-drop OS integration polish (a stretch, not required).

## 3. UX flow (approved)

The Forms picker is the existing `CatalogBrowser` card (inline, no pop-up windows).

1. **Entry points (two, for discoverability):** a persistent `Import…` control in the card header, AND a prominent CTA in the empty-custom state ("No custom forms yet — bring in your group's forms…"). A `Open forms folder` ghost button in the actions footer is the power-user escape hatch.
2. **Import sheet (inline accordion over the results, not a window):** one sheet offering *Choose file… / Choose folder… / Choose ZIP…* (Tauri dialog).
3. **Preview report:** after the pick, tuxlink validates every candidate and shows a per-file report classifying each as: `added` · `updated` · `overrides standard` · `skipped` (intra-batch duplicate) · `rejected` (with reason) · and an amber `no viewer` warning (form sends fine but a received copy can't render — the WLE version-skew pain, surfaced not hidden). NO writes have happened yet.
4. **Confirm overwrites:** if any candidate would replace an EXISTING CUSTOM form (`update`), the report shows those rows with a checkbox, default UNCHECKED (keep existing). The operator opts in per-form. A candidate that merely overrides a BUNDLED form is the intended behavior (no confirm; reported `overrides standard`).
5. **Commit:** apply the validated set (+ only the confirmed overwrites). Catalog refreshes; new/updated forms appear under their category, `custom`-badged, briefly highlighted. The member is now operational — definition of done.

## 4. Architecture — Approach A: two-phase, validate-before-write

New module `src-tauri/src/forms/import.rs`. Three Tauri commands (registered in `lib.rs`, surfaced to `CatalogBrowser`):

- `forms_import_preview(sources: Vec<String>) -> ImportPlan`
  Stage each source (file / folder / archive) into a fresh per-invocation temp dir under the OS temp root; validate every candidate; classify; detect collisions against the live custom dir + bundled catalog + intra-batch. **Writes nothing to `custom_root`.** Returns `ImportPlan { staging_token, entries: Vec<ImportEntry>, summary }` where `ImportEntry { rel_path, id, folder, kind: Added|Update|OverridesStandard|Skip|Reject, reason, has_viewer }`.
- `forms_import_commit(staging_token: String, approved_overwrite_ids: Vec<String>) -> ImportResult`
  Copy the staged, already-validated set into `custom_root_for_app()` preserving subfolders (→ categories), applying ONLY the approved overwrites; skip `Update` entries not in the approved set; never apply `Reject`/`Skip`. Returns the realized per-file outcome. Idempotent for a given plan. Clean up the staging dir.
- `open_forms_folder() -> ()` — reveal `custom_root_for_app()` via the OS opener (create the dir first if absent).

Frontend: a new `src/compose/ImportSheet.tsx` (the sheet + report + confirm UI), mounted inline in `CatalogBrowser`; `CatalogBrowser` gains the header `Import…` control, the empty-state CTA, the footer `Open forms folder`, and a post-commit catalog refresh + highlight. Reuse the existing catalog-refresh path (`CatalogBrowser` already has a refresh sub-flow).

**Why two phases:** the confirm-before-overwrite decision REQUIRES classification before any write; validating in `preview` against a temp staging dir is also the security boundary (a malicious archive never lands in `custom_root` unless it passed validation and the operator committed).

## 5. Validation = the security boundary

`preview` is where all rejection happens. Rules:

- **Is-a-Winlink-form:** an imported `.html` must contain a `<form>` whose submit target is the form-server path (consistent with how the webview host serves forms). Non-form HTML (`readme.html`, stray pages) → `Reject{reason:"not a Winlink form"}`. Exact heuristic is refined in the adversarial round (§8); err toward rejecting ambiguous files with a clear reason rather than importing junk.
- **Archive hardening (zip-slip / path traversal):** reject any entry with `..`, an absolute path, a symlink, or that resolves outside the staging dir. Enforce caps: max entry count, max single-file size, max total uncompressed size (guard zip bombs). Only ever write under the temp staging dir, then under `custom_root`.
- **Filename / id:** reuse `forms::validation::is_valid_form_id` constraints (ASCII, length bounds) on each imported stem; reject names that would not round-trip as a `form_id`.
- **Companion detection:** for each authoring form, pull its recognized companions from the SAME source folder — the display/viewer `.html` (referenced by the input form) and the `.txt` template. A form with no detectable viewer imports with the `no viewer` warning, not a rejection (input-only forms send fine).
- **Webview note (for the adversarial round):** imported forms render in the EXISTING form-host webview and execute JS exactly as bundled forms do — import does not widen privilege. But user-sourced forms are less trusted than bundled. §8 must probe what IPC/host surface the form webview exposes to form JS (the form-server token path is believed to scope it) and whether import changes that exposure. If the webview exposes anything dangerous, that is a pre-existing finding to file, not a blocker for import.

## 6. Collision semantics (precise)

For each candidate `id` (derived from filename stem; `folder` from its relative path):
- Matches an existing **custom** form → `Update` (needs operator confirm; default keep).
- Matches only a **bundled** form → `OverridesStandard` (intended; no confirm; the engine already does custom-wins-by-id).
- Matches another candidate in the SAME batch → `Skip{reason:"duplicate in this import"}` (first wins).
- No match → `Added`.

## 7. Surfacing

Imported forms enumerate through the unchanged `forms::wle_templates::list` into their `folder` categories. Change: `CatalogBrowser`'s `buildFolderTree` sorts **custom categories first** (currently custom sorts last) — for an org member, their forms are the point. Custom items keep the `custom` badge. Post-commit: refresh + briefly highlight the new/updated ids.

## 8. For the adversarial review (open, to converge)

- The exact is-a-form heuristic (how strict; what about `.htm`; forms that build their own `action` via JS).
- Archive library choice + hardened-extraction specifics (the `zip` crate if not already a dep; symlink + zip-bomb handling).
- Temp staging location + cleanup on crash/abort; staging-token lifecycle (TOCTOU between preview and commit — re-validate on commit? the staged copy is the source of truth, so commit copies from staging, not from the original source, closing the TOCTOU).
- The webview IPC exposure to imported form JS (§5 note) — pre-existing surface to characterize.
- Whether `OverridesStandard` should warn (silently shadowing a standard ICS-213 could confuse) — surface it in the report.

## 9. Testing (TDD)

- **Backend unit (over a temp fixture tree):** classify add / update / override-standard / intra-batch-skip / reject; companion-pull (input+viewer+txt); `no viewer` warning; is-a-form validator accept/reject; filename/id rejection.
- **Backend security:** zip-slip entry rejected; absolute-path entry rejected; symlink rejected; oversize / too-many-entries rejected; nothing written to `custom_root` on a rejected/aborted preview.
- **Backend commit:** only approved overwrites applied; `Update` not in approved set is left untouched; idempotent re-commit; staging cleaned up.
- **Frontend:** `ImportSheet` renders the report from a plan; commit is gated behind the confirm step when `Update` rows exist; post-commit refresh requested; `Import…` + empty-CTA + `Open forms folder` present and wired.

## 10. Definition of done

A stuck onboarding member can: open Forms → `Import…` → pick their org's ZIP → see a clear report → confirm → see their org's forms in the catalog and open one — **in a real build**. Plus a short in-app help entry (G11) explaining it. Gates: cargo + clippy `--all-targets`, full vitest, typecheck.

## 11. Adversarial-review dispositions (4 Claude rounds 2026-06-11; Codex round DEFERRED to the diff after Jun 13, quota). These REVISE the design above; writing-plans implements the revised version.

### 11.1 Detection model — FLIP to `.txt`-directive-driven (was HTML-heuristic)
The HTML `is-a-form` heuristic in §5 is WRONG against the real bundle (`src-tauri/resources/wle-forms/Standard_Forms/`): real forms carry the literal unsubstituted `action="http://{FormServer}:{FormPort}"` (or `localhost:8001`), are **Windows-1252** (not UTF-8), and some authoring forms (e.g. `ARC 213 Message Initial.html`) contain **zero `<form>`**. The `.txt` `Form: input.html[,display.html]` directive is the only reliable input↔viewer binding.
- The **import unit is the `.txt` template**: parse `Form:` (input + optional viewer) + `Attach:` + `ReplyTemplate:`; the named input is the authoring form (trust the directive even if the `<form>` probe is inconclusive); the named display is imported as a **companion** (copied, not catalog-surfaced). Orphan HTML with no governing `.txt` falls back to the HTML probe: any `<form method=post enctype=multipart/form-data>` whose action contains `{FormServer}`/`{FormPort}`/`localhost`/`127.0.0.1` (case-insensitive). Read files as **bytes** (`from_utf8_lossy`), never `read_to_string().unwrap()`. Match directive filenames to disk **case-insensitively** (reuse `resolve_viewer_for`'s approach).
- **Import detection MUST equal enumeration surfacing.** Run the SAME `is_authoring_template_stem` filter the catalog applies during preview; classify viewer/sendreply files as `companion`, never `Added` (else they import as bogus compose options — they DO contain `<form>`). Accept `.htm` AND `.html` case-insensitively, and make `walk_html`'s extension filter the literal same predicate so import and enumeration never diverge (forms that import but never appear is the worst outcome).

### 11.2 Identity / collision — folder-aware + unwrap real bundle layout
- The catalog is keyed by **stem only** (`wle_templates.rs` `by_id`), and the real bundle has cross-folder stem dupes. Import MUST detect cross-folder stem collisions (intra-batch AND vs existing custom/bundled) and surface them in the report (`Skip{reason:"duplicate stem in <folder>"}`), never silently collapse. The deeper engine fix (catalog identity → `(folder, id)`) is a PRE-EXISTING limitation affecting bundled forms too — file as a separate issue under the epic; do NOT fold the engine-identity change into this import PR (scope).
- Real org sets ship as a top-level `Standard_Forms/` wrapper + `Standard_Forms_Version.dat` + `Changelog.txt`. **Strip a leading `Standard_Forms/` wrapper** on import (reuse the updater's `needs_wrap`/unwrap), and **silently ignore** `*.dat` / `Changelog.txt` / `*_Version.dat` (expected metadata, not `rejected` rows).

### 11.3 Scope additions (required for "done beautifully")
- **Uninstall:** add a confirm-gated `forms_custom_delete(ids)` + per-custom-form Remove in the catalog (and a "Remove these" on the import result). Precedent: `form_draft_library_delete` / `contact_delete`. Without it, first-run mistakes force the exact hidden-folder hand-deletion this feature kills.
- **`OverridesStandard` warns:** no confirm, but an amber report row ("Replaces the standard <name>") AND a persistent shadow badge in the catalog (a custom ICS-213 silently shadowing the standard one is a wrong-form-sent risk).
- **`no viewer` reworded** to be actionable: "Sends fine. Receiving stations see raw data, not a formatted view. Import your group's viewer file alongside to fix." Importing the full org folder (which includes the viewer) clears it automatically.
- **Empty-state leads with "Choose ZIP…"** (how orgs distribute); single-file demoted. Label entry points precisely to disambiguate from the existing standard-forms `Refresh…`: "Import group forms…" vs "Update standard forms…".

### 11.4 Architecture — REUSE `forms::updater::install`, do not reinvent
Model commit on `updater::install` (`updater.rs:252-372`): staging → **atomic `rename` swap with `.prev` backup + rollback-on-failure**; retain `.prev` on `Update` overwrites (data-loss guard). Specific decisions the plan MUST pin:
- **Token:** opaque server-minted key (16-hex `rand`, like `mint_session_token`) into an in-memory `State<ImportStagingRegistry>` (`token → (staging_path, created_at)`). `commit` resolves token→path via the registry ONLY (never string-builds a path). Tokens die with the process (closes cross-session replay). `commit` is **single-shot** (consumes the token); re-commit → typed `TokenExpired`. Rewrite the §9 "idempotent re-commit" test to "re-commit returns TokenExpired, no double-write".
- **Staging cleanup:** the dominant path is **preview-without-commit** (cancel). Add `forms_import_cancel(token)` (frontend fires it on ImportSheet unmount/Escape), a TTL reaper, and a boot-time sweep of stale staging dirs. Stage in an owner-only (`0700`) dir via `tempfile::Builder` (NOT bare `/tmp`) — closes the staging-tamper TOCTOU.
- **Concurrency:** share one mutex with `updater::install` over the forms data dir (promote/sibling `INSTALL_LOCK`). On commit, **re-classify under the lock** against the now-live custom dir; abort with `CommitConflict` ("catalog changed, re-preview") if an `Added` became an `Update` or an approved `Update` vanished (classification TOCTOU, distinct from content TOCTOU).
- **Bind overwrites to the plan:** `commit` intersects `approved_overwrite_ids` with the plan's actual `Update` entries; ignore the rest (prevents forcing an unconfirmed overwrite).
- **Opener:** no `tauri-plugin-opener` in deps — implement `open_forms_folder` via `tauri-plugin-shell` + `xdg-open` (Linux/Pi target), surface a typed error when no file-manager handler is registered (labwc/Wayland). Refuse if `data_dir()` is unavailable (don't fall back to a CWD-relative path).
- **Typed `ImportError`** enum (serialized tag/content like `UiError`): `TokenExpired`, `StagingFailed`, `CommitConflict`, `Io`. Per-entry outcomes stay inside `ImportResult`; command-level `Err` is whole-operation failure only.
- **Frontend Escape state machine:** extend `CatalogBrowser`'s single existing Escape handler with import unwind levels (confirming → sheet-open → idle); `committing` is uninterruptible (like `refreshing`); refresh and import are mutually exclusive sub-flows; cancel/unmount fires `forms_import_cancel`.

### 11.5 Security (the boundary)
- **Archive caps:** the existing `updater.rs` extractor has the total-uncompressed-byte cap but NO entry-count cap and NO per-entry compression-ratio guard. Import MUST add both (reject `archive.len() > MAX_ENTRIES`; reject ratio > ~200) + a per-single-file cap. Apply the SAME caps uniformly to file/folder/zip sources, stat-first before reading content.
- **Path safety:** validate EVERY path component of the staged relative path (not just the stem) — reject `..`, `.`, empty/leading-dot components, NUL/control chars, reserved Windows names. Reject **symlinks at stage time** via `symlink_metadata` (folder sources are fully attacker-controlled); on commit re-check `symlink_metadata` of each dest component (defend a pre-seeded symlink in the operator's own custom dir). `is_valid_form_id`'s WLE-roundtrip relaxation (allows space/dot/`&`) is a receive-path concession — import is a write path and validates path components strictly.
- **Webview blast radius (characterized):** the form child-webview capability `src-tauri/capabilities/forms-webview.json` is **empty** (`"permissions": []`), no `window.__TAURI__`, served from an ephemeral `127.0.0.1:0` listener — so imported-form JS has **no IPC/keyring/mailbox reach**. The residual exfil channel is network: `folder_handler` (`http_server.rs:860`) serves `/folder/*.html|.htm` with **no CSP**. Close it — apply `FORM_CSP` (+`X-Content-Type-Options: nosniff`) to all `/folder/*` text/html responses, and refuse to serve `text/html`/`.htm`/`.svg` from `/folder/*` (assets should be css/js/images; SVG is a scriptable sink). This is in-scope BEFORE untrusted forms can be imported.

### 11.6 New issue to file
Cross-folder catalog-identity (`(folder, id)`) engine change — pre-existing limitation surfaced by this review; separate issue under epic `tuxlink-zkuk`, not in the import PR.
