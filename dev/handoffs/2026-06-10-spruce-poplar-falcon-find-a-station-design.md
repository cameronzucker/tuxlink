# 2026-06-10 spruce-poplar-falcon — map P1 shipped, then Find-a-Station redesigned (approved spec)

## One-sentence frame

Shipped the three map-feature P1 items + fixed the operator-reported Map-tiles
render bug (all merged, all in **v0.46.0**), then the operator redirected the
Find-a-Gateway work entirely: a full visual brainstorm produced an **approved
design spec** to reframe it into a **propagation-aware station map** with offline
HF prediction. **Next session starts with the `voacapl` grounding spike** (gated
on the operator approving `gfortran`).

## Part 1 — what merged (all in v0.46.0)

| PR | Merge | Released | What |
|---|---|---|---|
| #550 | `6e27724` | v0.44.0 | Find-a-Gateway pin-on-map (tuxlink-3iav) — **see note: being reverted** |
| #551 | `df6c54e` | v0.45.0 | Position expand-to-overlay + 4/6-char precision (tuxlink-sdbd) |
| #554 | `d8ec2cd` | v0.46.0 | **Fix**: Map-tiles settings rendered inline / compressed app (tuxlink-jgom) |

All three are in a **v0.46.0** build (cumulative). The operator was testing
**0.44.0**, which is missing #551 and #554 — they should rebuild/install **0.46.0**
to see the position overlay and confirm the map-tiles fix.

- **Map-tiles fix (jgom):** root cause was a CSS code-split bug — `MapTileSettingsPanel`
  (lazy chunk) reused the `.tux-settings-*` overlay chrome but didn't import the
  `SettingsPanel.css` chunk where `position:fixed` lives, so opening Map tiles
  before GPS Settings left the backdrop unpositioned (inline block under the bottom
  bar). Fix imports the chrome CSS into the panel's own chunk. **Build-verified** at
  the bundle level (PR #554 + tuxlink-89tr): the panel's `__vitePreload` deps now
  include the chrome CSS. **Visual WebKitGTK grim still pending** (Pi was contended
  all session) — but it's the operator's 0.46.0 rebuild that confirms it visually.
- **CHANGELOG gap (FYI, cosmetic):** v0.46.0's notes dropped the two `fix:` entries
  (jgom + a winlink fix) — release-please hiccup during 3 rapid back-to-back releases.
  Code shipped; notes under-report. Not chased (no-users-calibration).

## Part 2 — the redesign (this session's main outcome)

The operator rejected #550's framing: setting *operator location* in Find a Gateway
is wrong (that's the status bar's job). A map there is only justified if it shows
**stations**. The approved design (PR #565, `docs/design/2026-06-10-find-a-station-propagation-map-design.md`):

- **Find a Station = a station map.** Operator location (status bar) is the reference
  point only. Pin = station (location); **channel = mode × frequency × SSID** = the
  unit handed to the modem. Grounded in real **N0DAJ** (DM34OA): shared HF dials carry
  both VARA + ARDOP; packet connects to an SSID (N0DAJ-10/-11/-12).
- **HF ranked by predicted reachability, not proximity.** Map recolours by selected
  band + time (WLE's proximity sort is wrong for HF; this is the WLE-parity-and-better
  feature).
- **Offline HF prediction via a `voacapl` sidecar** (compute-only; nothing like Pat).
  Only time-varying input is the smoothed **SSN**, which is forecastable → bundled/
  cached, **never a per-session download** → solves the go-bag Catch-22. (Confirmed
  against The Tech Prepper's EmComm Tools approach; we use the open `voacapl` engine
  directly, not vendor his framework.)
- **No SPLAT / Geographica** — VHF/UHF packet listed factually, no terrain prediction.
- Right rail = antenna **bearing** + path **propagation forecast** (replaces the
  redundant nearby-station list).
- **Three units, foundation-first:** U1 prediction service (`voacapl` + SSN cache) →
  `build-robust-features`; U2 persistent station-list cache (offline last-known-good —
  today's cache is in-memory only); U3 the map UI (Mock D), which **supersedes
  `CatalogBuilderPanel` and reverts the #550 operator-location pin**.

**Mocks** (local, served on :8473 during the session — server now stopped):
`dev/scratch/2026-06-10-find-a-station-map-mock{A,B,C,D}*.html`. Re-serve with
`python3 -m http.server 8473 --directory dev/scratch`. Mock D is the approved shape.

## CRITICAL — the next session's FIRST action (operator chose "B now, A next")

**Do NOT fabricate `voacapl`'s I/O format.** The U1 plan can't be written until the
real input-deck + `voacapx.out` contract is captured by running it. The opening move:

1. **Ask the operator to approve `sudo apt install gfortran`** (not installed; sudo
   needs explicit approval). `make`/`git`/`curl` are present; arch is `aarch64`.
2. Build `voacapl` from source (github.com/jawatson/voacapl), run `makeitshfbc`
   (pulls coefficient data), run a **DM43 → DM34 point-to-point circuit** for a few
   HF freqs, and **capture the real input deck + output format**. This also proves
   arm64 field-feasibility.
3. THEN write the complete U1 plan (`writing-plans`) against the real format →
   `build-robust-features` (cross-provider Codex adrev; RF-correctness-critical).

## bd state

- **tuxlink-axq0** — umbrella, design approved (PR #565). Note records the next-action
  spike + the U1/U2/U3 decomposition.
- **tuxlink-n6xu** — Position 6-char enablement (depends on a1cc §5 controls + tile
  wiring). Open.
- **tuxlink-a1cc** — §5 shared map control surface (P2). Open.
- **tuxlink-89tr** — grim/build-verify: build-level verification PASSED for all 3
  shipped surfaces; visual WebKitGTK grim still pending (low priority now — #4 is being
  reverted; map-tiles fix confirmed at bundle level + by operator's 0.46.0 rebuild).
- **tuxlink-sdbd, -3iav, -jgom** — closed (their PRs merged).

## Repo / worktree / process state

- **Main checkout** on `bd-tuxlink-xygm/recover-handoffs`, in sync with origin, no
  rebase in progress. (Map CODE is on `origin/main`; all code work this session was in
  worktrees off `main`.)
- **PR #565** (the spec) is approved and **merging to main on CI-green** — a CI monitor
  was running at handoff time; if it didn't auto-merge, merge it: `gh pr merge 565 --merge --delete-branch`.
- **In-flight worktree:** `bd-tuxlink-axq0-find-a-station-design` (off main, branch
  `bd-tuxlink-axq0/find-a-station-design`) — holds the spec for PR #565. **Dispose after
  #565 merges** (ADR 0009 ritual; inventory was clean — only `node_modules` + the
  committed spec). U1 work starts in a fresh worktree off `main`.
- **All other session worktrees disposed** (3iav, sdbd, jgom, hc2w). Background helpers
  (mock server :8473, the wait-for-Pi-free grim watcher) **stopped**.
- The Pi was multi-session-contended the whole session (qyjr/mzm4/lfz4/xglf builds) —
  why no visual grim happened.

## Process notes (learning-sandbox)

- **Stopped before fabricating.** Twice this session the disciplined move was to NOT
  produce confident output on shaky ground: dropped a wrong release-please "moniker
  prefix" root-cause theory the moment a counterexample (#545) disproved it, and
  refused to write a `voacapl` plan with invented HF-prediction I/O. For RF-adjacent
  work, a guessed artifact is worse than a flagged gap.
- **Visual brainstorm via served mocks** (A→D, real N0DAJ data pulled live) converged a
  fuzzy "make Find-a-Station better" into a locked, decomposed spec the operator
  approved — the value was iterating on concrete mocks, not prose.
