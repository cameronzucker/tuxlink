# find-messages — operator smoke (tuxlink-1hu)

Branch: `bd-tuxlink-1hu/find-messages`. Build + run:

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages
pnpm install
pnpm tauri dev
```

Smoke checks (✓/✗):

1. Shell opens; the SearchBar is visible in the ribbon's leftmost slot; Callsign/Grid/UTC/CMS cluster on the right; ChipStrip below the ribbon shows the "No active filter — click + to add" placeholder.
2. Press ⌘F (Mac) / Ctrl-F (Linux/Windows): focus jumps to the SearchBar input.
3. Type "damage": after ~150 ms the rows pane filters to matches. ChipStrip's far-right meta shows match count + ms.
4. Click a folder in the sidebar: the active search filter remains; results stay cross-folder until the search is cleared.
5. Open the search bar dropdown (click the chevron `▾` or focus the input). The dropdown shows two sections — Saved (empty initially) and Recent (the "damage" query you just ran appears chronologically).
6. Click `Manage… ⚙` in the dropdown footer → SavedSearchesPanel modal opens.
7. In the panel, click `+ New saved search` → enter `name: Storm Net Test`, `free text: damage` → click Save. The new saved search appears in the panel's list.
8. Close the panel. Open the search dropdown again — `Storm Net Test` now appears in the Saved section above the Recent section.
9. Click the `Storm Net Test` row in Saved. The SearchBar's input now shows the saved-search name (`★ Storm Net Test`); the rows pane shows damage matches; ChipStrip meta shows `★ Storm Net Test`.
10. Click the filled ★ next to the saved name in the SearchBar → un-saves; the SearchBar reverts to the raw "damage" free-text input.
11. Type a new query "weather"; close the dropdown; observe results. Reopen the dropdown — the new query appears as a Recent entry above older ones.
12. Click the empty ☆ next to the "weather" Recent row → name prompt appears → name "Weather Quick" → it's promoted to Saved.
13. In the SavedSearchesPanel modal, click `Rebuild search index` under Maintenance. A banner appears: `Indexed <N> messages in <ms>`.
14. Quit the app; relaunch via `pnpm tauri dev`. Saved searches persist; recent searches persist (capped at 20 entries).

Failure mode reporting: if any step fails, capture a screenshot to `dev/scratch/find-messages-smoke-<step>.png`, file a bd issue, and decide whether to merge with a known-issues entry or block the PR. Known v0.1 gaps that are NOT smoke failures (already filed):

- Empty subject on search-result rows — `tuxlink-g4dj` (messages_meta lacks subject column).
- TOCTOU race on `path.exists()` in `Index::open` — `tuxlink-xoom` (v0.5+ hardening).
- No live match-counts in the dropdown — deferred to v0.2 per spec §10.
