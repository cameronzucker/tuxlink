# 2026-06-09 delta-bog-cypress — smoke-walk fixes (5 merged) + Catalog Request Center design + ka3z handoff

## Summary

Filed and shipped five converged-build operator smoke-walk fixes — **all merged to `main`** — then designed the full-screen **Catalog / Request Center** (shape approved, build deferred). The continuation for next session is **`tuxlink-ka3z` (nested folders)**, which I investigated and paused at the design stage because it's a full feature, not a knock-out.

## Shipped this session — all MERGED, issues closed

| PR | Issue | Fix |
|---|---|---|
| #484 | `29zx` | Find a Gateway dismiss — backdrop click + Escape (panel had only the × before) |
| #485 | `r4je` | Contacts grid collapse — `.layout-b .panes > .contacts-panel { grid-column: 2 / 4 }` so ContactsPanel spans the list+reader tracks instead of being crushed into the 380px column (radio dock then auto-places into its own track) |
| #487 | `fr0d` | Favorites/Recent removed from Telnet (now Manual-only like VARA via `isManualOnly`); active-tab restyled from global `--accent` (orange) to radio-panel `--modem-accent` (green) |
| #490 | `6jpf` | "Find a Gateway" relocated Message → **Tools** (`menu:tools:find_gateway`) + an in-panel **"🛰 Find a gateway…"** button on ARDOP/Packet/VARA `RadioPanel` chrome; the by-message info-category requests were **removed** from the panel (scope split) |
| #493 | `tsl5` | Find a Gateway z-index 50 → **1000** (was below the app chrome — menubar 90 / dropdown 100 / resize 200 — so a tall panel's header + × were painted over and unreachable). Regression test asserts overlay z-index > max chrome z-index. |

**Transferable lessons captured:**
- **z-index stacking vs layout** (`tsl5`): a control can be perfectly laid out and still unclickable because higher-z app chrome paints over it. When a "fixed" control is still unreachable, diff it against a sibling that works (the two catalog overlays differed only in z-index: working sibling = 1000, broken = 50) and check the *whole* stacking order. The app's modal z-index convention is **inconsistent** (50 / 100 / 1000) — a standardize-on-one sweep would prevent recurrence.
- **CSS-source assertion tests**: jsdom can't compute layout, so several fixes pin the rule by reading the `.css` as a raw string and regex-asserting (mirrors the existing `AppShell.css` chrome-width tests). Visual correctness remains the operator browser smoke.

## Designed this session — DEFERRED (`tuxlink-eymu`, P2)

Full-screen **Catalog / Request Center** — replaces the cramped vertical `CatalogRequestPanel` modal and gives the removed info-requests a real home. **Shape approved by operator 2026-06-09** ("go with this shape for now, iterate later").

- **Request-first** landing: location-aware common-request **cards** by section — Weather (area forecast, nearby METARs, marine, hazards), Propagation & space (propagation, aurora, solar), Nearby stations (gateways near you, Winlink info). NOT GRIB-first — GRIB is a niche Saildocs integration, demoted to a "More:" link.
- Header **location chip** ("Near CN87") prefills/filters relevant requests.
- **Search + "Browse full catalog by category"** reveal the full 1,477-item / 41-category 3-pane browse for the long tail.
- Parameterized requests (GRIB) open a **form** in the center pane (lat/lon box + "Center on my area" + grid/hours/params).
- **Request basket** (right rail) + Send; tag items by rail (CMS inquiry vs Saildocs) when mixed.
- Full details + open decisions in **`tuxlink-eymu`**. **Mockups (local-only, gitignored):** `.superpowers/brainstorm/1136216-1780984992/content/` — `catalog-request-first-v4.html` is the **approved** shape (v1 = browse, v3 = GRIB form). Re-render via the brainstorming visual-companion server.
- **Open decisions to resolve when building:** (1) "Nearby stations" appears here AND Find a Gateway is now in the radio panels + Tools (#490) — canonical home vs intentional dual-access; (2) dual-rail basket (unified Send-all vs per-form Send); (3) exact common-request set + catalog filename mappings; (4) full-screen route vs large modal; absorb the separate GRIB File Request panel + the Message → Catalog Request menu entry.

## Follow-ups filed (not started)

- **`tuxlink-76zy`** (P2, bug) — `InboundSelectionPanel.css` has the **identical** z-index-50 defect fixed in #493 (fullscreen `place-items:center` modal below the chrome). One-line fix (→ 1000) + the same stacking assertion. Quick.
- **`tuxlink-wynv`** (P2, flake) — `PositionFormV2.test.tsx` C9 offline-map test flakes on amd64 CI (`--active` class race; passes on arm64 + in isolation). Fix = `await waitFor` the class. Blocked #487's CI once; a re-run cleared it.

## THE CONTINUATION — `tuxlink-ka3z` nested folders (P1, in_progress, claimed by delta-bog-cypress)

**Why I handed it off instead of starting:** it's a full feature with a **persisted-schema migration** (hard-to-undo, data-adjacent), not a knock-out, and ~50% context would likely leave a half-done migration dangling.

**Investigation verdict — folders are COMPLETELY FLAT today** (not partial; the operator's "thought it shipped" was Phase 2 = create/rename/delete, which did ship). Spec `docs/superpowers/specs/2026-06-02-user-folders-design.md` **D3: "Flat structure (no nesting in v1)"**; Phase 3 explicitly defers "Nested folders".

**Full-feature scope (cross-stack):**
- **Data model:** `UserFolder` = `{slug, display_name, created_at}` in `src-tauri/src/user_folders.rs` + mirror in `src/mailbox/types.ts`. Needs `parent_slug?` + `.folders.json` **schema v2 migration** + cycle prevention + slug-uniqueness decision (global vs per-parent).
- **Backend/IPC:** `src-tauri/src/ui_commands.rs` — `folder_create(display_name)` needs a parent arg; `mailbox_move(from,to,id)` is slug-keyed; `native_mailbox.rs` `folder_dir()` = `root.join(slug)` (flat — needs path logic); move/delete must handle nested paths + delete-with-children behavior.
- **Frontend:** `src/mailbox/FolderSidebar.tsx:~225` flat `.map()` → recursive tree (indent + expand/collapse); `FolderContextMenu.tsx` (only Rename/Delete today) needs "Create subfolder here"; `NewFolderDialog.tsx` needs parent context; `useUserFolders.ts` create mutation.
- **Tests:** `FolderSidebar.test.tsx` (one-row-per-folder today) + new tree/nesting/cascade tests.

**Where I paused:** the brainstorm's first clarifying question — **nesting depth** (arbitrary / file-manager parity vs capped-2 folder→subfolder vs capped-3). Operator use case = "net traffic by net, weather by region" (≈2 levels), but the ~200px sidebar makes deep nesting visually cramped. Other un-asked design questions: delete-with-children behavior, create-subfolder UX, move-into-nested, schema-migration shape.

**Next session should:** resume `superpowers:brainstorming` (visual companion per CLAUDE.md, launch don't ask) → lock depth + delete behavior + migration shape → spec → because it's a hard-to-undo persisted-schema change, run it through `build-robust-features` (incl. the Codex cross-provider adrev per `discipline_triage_rule`) → cross-stack TDD → **browser-smoke the tree UI** (jsdom can't verify it).

## State at handoff (re-verified directly)

- **Main checkout:** on `bd-tuxlink-xygm/recover-handoffs` (operator branch, untouched by me). `.beads/issues.jsonl` shows modified — that's bd's Dolt-backed state from this session's creates/closes; **don't commit the worktree JSONL** (bd owns Dolt).
- **No in-flight worktrees of mine** — all five fix worktrees (29zx/r4je/fr0d/6jpf/tsl5) disposed + pruned per ADR 0009 (~300 MB reclaimed; no untracked source lost; the repo-wide shared stashes were left untouched — not mine).
- **`ka3z` has no worktree yet** — only the bd claim. Repo-wide worktree sprawl (~90 stale worktrees from prior sessions) is a separate hygiene problem, untouched.
- **Verification gap to close:** #493 (z-index) was merged, but the **live tall-panel visual smoke** (controls clear the chrome when results fill the panel) is still the operator's to confirm in the converged build — the fix is z-index-deterministic + sibling-corroborated + regression-tested but not yet visually confirmed in a running build.

Agent: delta-bog-cypress
