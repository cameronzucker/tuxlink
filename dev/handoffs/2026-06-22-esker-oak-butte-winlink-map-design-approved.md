# Handoff — esker-oak-butte — Winlink map layer: design APPROVED, ready to build (tuxlink-s1o1)

**Agent:** esker-oak-butte · **Date:** 2026-06-22
**Headline:** Brainstorm-only session (UI feature → office-hours gate). The "Winlink stations +
contacts on the map" feature (tuxlink-s1o1) now has an **APPROVED design doc** grounded against the
real code, plus an **animated, browser-openable mock**. No implementation code written (office-hours
hard gate). Next session: eng-plan / build on a `bd-tuxlink-s1o1/` branch.

---

## What this session produced

- **Design doc (committed):** [`docs/design/2026-06-22-winlink-map-layer-design.md`](../../docs/design/2026-06-22-winlink-map-layer-design.md)
  — the canonical spec. Read it first.
- **Animated mock (gitignored scratch):** `dev/scratch/2026-06-22-winlink-link-animation-mock.html`
  — real Canvas2D protocol animation over a dark MeshMap-style basemap. Serve + open to watch the
  loop: `cd dev/scratch && python3 -m http.server 8899` then
  `http://127.0.0.1:8899/2026-06-22-winlink-link-animation-mock.html`. Keyframe stills:
  `dev/scratch/winlink-mock-{ping,dataout,datain,busy}.png`.
- **bd:** `tuxlink-s1o1` updated with the locked decisions (`bd show tuxlink-s1o1`). Two deferred
  follow-ons filed: **tuxlink-g8h9** (Tier 2 ack/retry via backend frame-event channel) and
  **tuxlink-5q31** (VARA live-animation, needs a VARA `ModemTransport` impl) — both `depends-on s1o1`.

## The feature, as designed (operator-decided this session)

A **toggle layer on the existing APRS theater-of-ops map** (not a new tab; no new dock — the modem
tab already owns connection state). Plots **gateways the operator is calling / has recently called**
that have a position (callsign → station-catalog grid), **recency-windowed like APRS** (configurable).
The headline is a **live, protocol-aware RF-path animation**: a curved arc operator↔gateway (distinct
from APRS digipeat's straight hop-by-hop segments), with **truthful-now grammar** driven entirely by
the real `modem:status` event (ARDOP):

- connecting handshake (keyed off `state === 'connecting'`), directional data comets
  (`connected-iss` → out / `connected-irs` ← in, rate ∝ throughput), busy shimmer (`arqFlags.busy`),
  error flash vs clean-disconnect fade, arc tint by `quality`/`snDb`.
- **Excluded from v1 (would be fabricated):** ack ticks, retry stutter → Tier 2 (tuxlink-g8h9).
- Icons: ◆ diamond = worked gateway (green=reached / amber=failed / dimmer=older); ● = APRS heard;
  blue ● = operator. Hover/click a diamond → connection-history popup.

## Why decisions landed where they did (don't relitigate)

1. **Not a contacts/address-book map** — the address book has no RF position; only catalog gateways
   you connect to have one. "Contacts" = gateways you have a connection relationship with.
2. **Toggle layer, not new surface** — operator wants one situational pane; this forces the
   icon/line disambiguation ("so it doesn't get crazy"), which is the design's center of gravity.
3. **Truthful-now grammar** — per the alpha=vetted/nothing-faked bar, v1 animates only real
   telemetry. ack/retry aren't exposed to the frontend, so they're out until the backend channel
   exists (Tier 2).

## Spec-review (1 adversarial round, 8 issues fixed)

An independent reviewer verified claims against code and caught real ones (all fixed in the doc):
connecting must key off `state` not a non-existent ping event; the **windowed gateway *set* query
does not exist** — only a per-callsign hook (`useContactConnectionRecord`) — so a new
`contacts_recent_gateways(window)` command is **build-work**; kebab-case wire state literals;
4 live-arc edge cases (no-position peer, toggle-off-mid-anim teardown, live-peer-outside-window,
clean-disconnect-vs-error); and a prerequisite. Verified-correct: VARA has no `ModemTransport` impl
(ARDOP-only), and ack/retry are genuinely absent from the frontend.

## State at handoff

- **Branch:** `bd-tuxlink-xygm/recover-handoffs` (operator's current branch; main checkout).
- **Committed this session:** the design doc + this handoff (directly on the current branch, per
  the no-PR-for-handoffs convention). bd state pushed via Dolt.
- **Working tree:** unchanged pre-existing untracked clutter from prior sessions left untouched
  (other `docs/design/*.md`, bug-hunts, root mock PNGs, 7 old stashes) — not mine, not swept.
- **Worktrees:** none created this session.
- **No code changed** → no quality gates run (none needed).

## Prerequisite + next steps (IMPORTANT)

- **PREREQUISITE:** none of the reused map code (`LeafletMap`, `AprsPositionsMap`,
  `StationFinderMap`, `DigipeatPathLayer`) is on `main` yet — it's on the unmerged Leaflet/cn84
  branches (epic **tuxlink-u3qe**, PR #849). s1o1 **depends on those landing first.** (Couldn't add
  the bd edge — bd won't let a task block an epic — so it's documented here + in the design doc.)
- **NEXT:** run `/plan-eng-review` or `build-robust-features` against the design doc to lock the
  component breakdown (the `modem:status`→animation driver, the new `contacts_recent_gateways`
  command + history→position join, the layer-toggle gating both featureGroup and arc rAF), then
  implement on a `bd-tuxlink-s1o1/` branch.
