# Design: Station Intelligence operational usability

- **Date:** 2026-07-19
- **Status:** APPROVED (operator-supervised brainstorm, this session)
- **Agent:** harrier-sandbar-cardinal
- **Branch:** `bd-tuxlink-6i0ie/si-operational-usability` (off origin/main)
- **Anchor issues:** tuxlink-6i0ie (P1), tuxlink-hcmfb (P1), tuxlink-nkzng (P1), tuxlink-1w0d0 (P2)
- **Amends:** the approved passive-FT-8 spec
  (`~/.gstack/projects/cameronzucker-tuxlink/administrator-bd-tuxlink-ant8s-ardop-connect-fixes-design-20260705-034957-passive-ft8-listener.md`).
  That spec's intent stands; this design fixes how its L3/L5 presentation
  shipped. The "firstrun-v2 full-body setup surface" decision (QA round-3
  finding 2) is REVOKED: a first-run surface was wired to an every-open
  trigger and it unmounts the primary feature.

## Problem

v0.94.0 shipped Station Intelligence (`src/catalog/StationFinderPanel.tsx`,
opened from the dashboard ribbon) with the plumbing built and the
operator-facing product missing. Verified state on origin/main:

Shipped and wired: band-chip openness dots (`bandActivity`), the rail's
Live decodes tab (`LiveDecodesTab`, with pan-to-grid), the bottom
LiveBandStrip (waterfall + decode feed + stats), and the six FT-8 MCP tools
backed by the real decode ring.

Broken or absent:

1. FT-8 device setup auto-replaces the entire panel body whenever no capture
   device is configured (`setupActive` ternary at `StationFinderPanel.tsx:505`),
   unmounting map + rail. Chrome persists around the hole.
2. The map never receives decode data. `StationFinderMap` has no FT-8 props;
   the spec's L3 traffic map and L5 heat layer were never built. This is the
   "no discernible intelligence output" headline.
3. Decode list growth mutates the panel/window (mechanism unconfirmed);
   dead space below the waterfall.
4. No start/stop inside the panel (ribbon only).
5. Device picker is a row-per-device + per-row "Use this device" button
   pattern (slop); the per-row live level meter is the one affordance worth
   keeping.
6. Gateway frequency (`frequencyKhz`, present in the model) is displayed
   nowhere (tuxlink-hcmfb). Channel bandwidth is thrown away at ingest: the
   `Rms*Listing.aspx` text listings carry no bandwidth field (tuxlink-nkzng).

## Scope

One effort, "make the finder operationally usable": tuxlink-6i0ie (anchor) +
tuxlink-hcmfb (frequency) + tuxlink-nkzng (channels JSON API, the data
prerequisite for hcmfb and for bandwidth) + tuxlink-1w0d0 (WWV Refresh
overflow, same containment class).

Out of scope: tuxlink-9obx2 (pop-out; sequences after this lands, never on a
broken panel), the VOACAP/propagation Find-a-Station half (separate feature,
untouched), any decode/capture/pipeline work (the pipeline is done; this
design only wires its outputs), and any change to the uiState machine.

## 1. Panel structure

Map + station rail are ALWAYS mounted. The body ternary and the
`setupActive` promotion logic (`forceSetup` / `needsSetup` / `setupDismissed`
at `StationFinderPanel.tsx:213-217, 505-575`) are deleted outright, along
with the full-body `station-finder__setupbody` presentation. One
presentation remains, not two.

The bottom "Live band · FT-8" strip is FT-8's single home:

- **needs-setup state:** the strip body is a compact inline setup row:
  device dropdown (OS-convention select, single confirm), live input level
  meter following the highlighted device, and a "▶ Start listening" button.
  `Ft8SetupSurface` is reshaped into this strip form; the row-per-device
  DeviceList/DeviceRow pattern is deleted. The per-device meter poll
  (`useDeviceMeterPoll`) is retained, scoped to the highlighted device.
- **running states:** the same slot holds waterfall + decode feed, as today.
- **strip header:** gains a ▶/■ start/stop control. The DashboardRibbon
  badge remains as the ambient mirror; both drive the same listener service.
- Existing strip behavior (collapse persistence, force-expand on
  needs-setup/wedged/device-lost, flags overlay, dot severity) is unchanged.

## 2. Map intelligence

Three additions to `StationFinderMap`, all fed from `ft8.decodesRing`
(threaded from the panel; the map gains FT-8 props for the first time):

- **FT-8 heard layer** (toggle, default ON while the listener runs): each
  decoded station carrying a grid is plotted at its grid position, colored
  by the openness ramp (`--open-hot/-warm/-quiet`) keyed to SNR. Never the
  reach ramp: SIGNAL colors belong to VOACAP reachability, openness colors
  to FT-8 (axis discipline, tuxlink-obpa). Grid-less decodes do not plot but
  still count in feed and stats.
- **FT-8 heat layer** (toggle, default OFF): the spec's L5 density layer
  over the same Leaflet engine (`useLeafletMap` + layer group). Parallel to
  the heard layer; either or both may be on.
- **FT-8 evidence filter** (toggle, default OFF): the operator's subtractive
  lever. When ON, a gateway renders fully only if corroborated; otherwise it
  ghosts to ~20% opacity (never removed silently). A note chip on the map
  states the fraction and parameters, e.g.
  "evidence: 2 of 5 gateways corroborated (20m) · SNR ≥ −18 · last 30 min".

**Corroboration semantics (exact):** a gateway G is corroborated iff there
exists a decode D such that:

1. D carries a grid;
2. age(D) ≤ RECENCY (30 min default);
3. snr(D) ≥ the operator-adjustable threshold (default −24 dB, the decode
   floor, meaning all decodes count until the operator raises it);
4. band(D) is a band on which G has a channel passing the current
   band/mode/bandwidth filters (evidence is band-scoped: a 20m decode never
   corroborates a 40m-only gateway);
5. greatCircle(D.grid, G) ≤ R(D), where
   **R(D) = clamp(0.15 × greatCircle(operator, D.grid), 50 mi, 750 mi)**.
   The radius grows with path length because the useful ionospheric
   footprint does: ~50 mi at NVIS ranges, ~225 mi at one-hop (1,500 mi),
   capped at 750 mi for deep DX. 15% and both clamps are named constants.

Bands the listener never sampled inside the recency window yield no
corroboration; such gateways ghost like any uncorroborated gateway, and the
note chip's band scope keeps the epistemics legible. The filter states
reception facts only; it never claims two-way reachability ("heard", never
"open to").

Grid positions resolve to the grid-square center (6-char when the decode
carried one, else 4-char). Distances use the existing `distanceFromGrids`.

## 3. Bandwidth + frequency

**Backend (tuxlink-nkzng, built whole):** adopt the api.winlink.org channels
JSON API (per-channel frequency, bandwidth, SSID, hours; covers all modes
including VARA FM). Pat API key stored via the existing keyring pattern (no
disk creds). Responses cached with a TTL; offline falls back to the cache;
the text listings remain as a degraded fallback. The Gateway/Channel model
gains `bandwidth` and VARA FM end-to-end: `ListingMode::VaraFm`, mode chip +
swatch, `FILTER_MODES` entry, and prefill routing to the vara-fm pane
(`RadioMode` already exists).

**UI:**

- **BW filter chips** `500 · 2300 · 2750` join the filter row beside bands
  and modes (same multi-select grammar; a station stays visible if any
  channel matches enabled band AND mode AND bandwidth), and are **mirrored
  in the map layer box. One shared state renders in both locations**;
  toggling either updates both.
- **Badges:** every channel row and the frequency hero carry a bandwidth
  badge (amber for 500/narrow, blue for 2300/2750) so narrow reads at a
  glance. Bandwidth is never encoded on map pins (pins already carry reach
  color + mode shape; a third encoding fails at density).
- **Frequency hero (tuxlink-hcmfb):** the rail's Station tab leads with the
  selected gateway's dial frequency as its largest datum (e.g. "7.103.5 kHz"
  with mode, bandwidth badge, and center-frequency note). Every channel row
  shows dial + band + bandwidth + "Use →".

## 4. Containment and sizing

Invariant: data arrival never resizes the panel or the window. The decode
feed and the Live-decodes tab scroll inside fixed slots.

The reported window-growth mechanism is UNCONFIRMED (panel CSS vs
`secondary_window.rs` geometry). Implementation pins it with a reproduced
measurement (render harness, observe what grows as decodes arrive) BEFORE
any geometry change. The WWV Refresh overflow (tuxlink-1w0d0) gets the same
containment treatment as part of this effort.

## 5. States

The `uiState` machine (needs-setup / device-lost / wedged / yielded /
band-dead / off / transitional, plus clockUnsynced / jt9Degraded /
catFixedBand flags) is untouched. All states keep rendering in the strip
with existing severity/dot/force-expand behavior. This design changes
presentation surfaces only; decode, capture, and state derivation are
frozen.

## 6. Agent parity (definition of done)

Per the agent-native invariant, the new human capabilities must be
agent-reachable: the MCP surface gains the corroborated-gateway view:
gateways with their channels (frequency, bandwidth, mode including VARA FM)
and evidence status under stated parameters (threshold, recency, radius
constants echoed in the response). Exact tool naming/shape is a plan-time
decision; parity is the requirement. The six existing ft8 tools are
unchanged.

## 7. Verification (exit gates)

Wire-walk from the dashboard ribbon entry point, in the running app:

1. Open Station Intelligence with FT-8 unconfigured: map + rail + gateways
   fully live; strip shows inline setup; nothing takes over the body.
2. Pick device from the dropdown (meter live), Start from the strip.
3. Decodes reach the map (heard layer), the rail tab, and the feed; neither
   list grows the panel/window (measured, not eyeballed).
4. Evidence filter ON demonstrably subtracts and the note chip states
   fraction + parameters. Heat layer toggles independently.
5. A VARA gateway shows dial + bandwidth in the hero and channel rows; BW
   chips filter from both locations; VARA FM appears and prefills its pane.
6. Stop from the strip header; ribbon badge mirrors.
7. WWV Refresh no longer overflows.

Final visual approval: WebKitGTK render-harness PNGs (standing rule; the
brainstorm HTML mocks are directional, not approval artifacts). Tests:
`StationFinderPanel.ft8mount` + panel tests assert map+rail always mount;
containment gets a regression test; CI runs the full suite (local scoped
vitest is known to under-report).

## Mockup references (local, gitignored)

`.superpowers/brainstorm/1358490-1784495890/content/full-window-integration.html`
(the approved two-state full-window mock) and
`bandwidth-representation.html` (BW chip placement). On this machine only;
the spec text above is self-contained.

## Constants (named, tunable)

| Constant | Default |
|---|---|
| Evidence recency window | 30 min |
| Evidence SNR threshold | −24 dB (adjustable in UI) |
| Evidence radius factor | 15% of operator→heard distance |
| Evidence radius clamp | 50 mi floor, 750 mi cap |
| Ghosted-gateway opacity | ~20% |
| Channels API cache TTL | plan-time decision |
