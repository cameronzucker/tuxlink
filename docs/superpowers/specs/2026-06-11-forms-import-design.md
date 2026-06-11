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
