# 2026-06-09 ivy-redwood-alder — inbound selection: shipped (works) → two enhancements for the fresh session

## Arc

The session opened to operator-smoke + merge PR #480 (inbound message selection, `tuxlink-bsiy`). The smoke found the feature **didn't fire** (no prompt; messages auto-downloaded). Root-caused + fixed it, then redesigned its default + surface per operator feedback. Three PRs landed on `main`, all operator-confirmed working. The operator now wants **two enhancements built in a fresh session** — this handoff sets them up.

## What shipped this session (all merged to `origin/main`)

| PR | Merge | What |
|----|-------|------|
| #480 (`tuxlink-bsiy`) | (operator) | The inbound message selection feature itself |
| #482 (`tuxlink-u9z8`) | `9dd1cc07` | **Connect-path staleness fix** — the "no prompt" bug |
| #491 (`tuxlink-pmp5`) | `e90a1cf8` | Default review **ON** + inline ribbon control, out of the Settings modal |

- **#482 root cause:** the selecting decider was gated on the backend's in-memory `live_config().review_inbound_before_download`, which is seeded at app startup and only refreshed by `set_config` — which `config_set_review_inbound` skips when the backend isn't installed yet. Toggling the brand-new preference during startup wrote `true` to disk but left the live copy `false` → accept-all. Fix: `cms_connect` gates the `CmsSelectionContext` on the **fresh disk** preference; `native_connect` selects the decider on the **context's presence**, not the stale flag. (Every unit test missed it — the integration test injected the flag directly, skipping the toggle→live-config seam.)
- **#491:** `review_inbound_before_download` now defaults `true` (serde default fn + both wizard new-install paths). The control is an inline **`On connect: [Review] [Download all]`** segmented control on the dashboard ribbon next to Connect (operator-picked "Variant A" from a mock), wired through AppShell via `config_read` / `config_set_review_inbound`. Removed from `SettingsPanel` (now purely GPS & Privacy). It renders by default, so it's visible immediately on the next converged build.

## State (re-verified at session end)

- **`origin/main`:** has #480 + #482 + #491. Verified the actual fix code is on `main` via the GitHub contents API (ribbon control + default-ON serde fn present).
- **Worktrees:** all three this session (`bsiy`, `u9z8`, `pmp5`) **disposed** per the ADR-0009 ritual (rm -rf + prune); their remote branches deleted on merge. No in-flight worktrees from this session.
- **bd:** `tuxlink-u9z8` + `tuxlink-pmp5` closed; `tuxlink-bsiy` left IN_PROGRESS pending the operator's own Task-9 cms-z gate pass (Hold/Delete semantics, 45s socket-idle, abort, multi-batch) — that smoke is post-merge on warm `main`, operator-owned.
- **Root checkout (`bd-tuxlink-xygm/recover-handoffs`):** untouched operator state (staged `.beads`, peers' untracked handoffs). **This handoff is written but NOT committed** — left for the operator's recover-handoffs collection, consistent with the peer handoffs already sitting there. Other live sessions hold the main-checkout lease.
- **Process note locked into memory this session:** NEVER hold a merge for an operator/feature smoke — CI (build+verify both arches) is the merge gate; smoke is post-merge + fix-forward. Per-feature pre-merge builds don't fit this device's time/compute.

## Next session — the two enhancements (operator's first-thing work)

Both target the **inbound selection panel** (`src/connections/InboundSelectionPanel.tsx` + `useInboundSelection.ts`) and converge on **one expanded, sortable table** with columns (size, attachments?, est. time, and — pending research — sender/date). Filed as bd issues:

### 1. `tuxlink-fzek` — Sortable inbound-selection panel (P2)

Sort pending inbound by sender, date, size, attachments y/n, etc. — like the Inbox `MessageList`. Likely expands the selector into a sortable table.

**⚠️ CRITICAL RESEARCH GATE — do this BEFORE designing.** The raw B2F proposal (`FC` line) carries only **MID + uncompressed/compressed SIZE** — NOT sender/date/subject/attachments, which live in the message body downloaded *after* selection. So **sort-by-size is doable today; sort-by-sender/date/attachments needs metadata that may not exist at proposal time.** Research what the Winlink proposal actually carries and how WLE's "Review Pending Messages" surfaces sender/subject/date — verify against **Pat / wl2k-go / decompiled RMS Express** (authoritative; the prose docs are unreliable — see memory `winlink-re-authoritative-sources`). The research outcome sets feasibility + scope. Check `PendingProposalDto` (Rust, `src-tauri/src/winlink/inbound_selection.rs`) + the proposal parse (`src-tauri/src/winlink/proposal.rs`) for current fields.

### 2. `tuxlink-nwwv` — Download-time estimate, calibrated from the live data rate (P2)

Estimate per-message (and total-selection) download time from the **actual** data rate observed in the current connect's initial exchange — not a static rate. Most valuable on RF (ARDOP/VARA HF: slow + variable); near-instant on telnet. Design questions in the issue: where to measure throughput (modem-reported rate vs. measure-first-message-then-refine vs. bytes/sec over the auth window), display (an "Est. time" column + running total), transport-aware behavior. `compressed` size is already in the proposal.

### First action for the fresh session

These are **feature/UI** work → **brainstorm/design first** (office-hours or `superpowers:brainstorming`), and **launch the visual companion / high-fidelity mock immediately** per the brainstorming convention (don't ask). For `tuxlink-fzek`, **run the Winlink-proposal-metadata research before committing to a sortable-by-sender/date design** — it gates the whole thing. Then plan + build (CI-green is the merge gate; operator smokes post-merge). Worktrees off `main` are required (other sessions hold the main-checkout lease).
