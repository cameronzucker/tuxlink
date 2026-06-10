# Find-a-Station — propagation-aware station map + offline HF prediction

> **Status:** Approved (visual brainstorm, operator + agent `spruce-poplar-falcon`,
> 2026-06-10). Supersedes the text-list **Find a Gateway** panel
> (`CatalogBuilderPanel`) and reverts the misframed operator-location pin shipped
> in PR #550 (`tuxlink-3iav`).
> **Umbrella issue:** `tuxlink-axq0`. **Decomposes into:** offline HF prediction
> service (foundation), persistent station-list cache, Find-a-Station map UI.
> **RADIO-1:** not engaged. Nothing here transmits. The prediction engine is a
> read-only offline compute; transmit consent remains the operator's existing
> click on Connect in the modem pane. **No new airtime/TOT/consent safeguards.**

## 1. Frame

**Find a Station is a station *map*, not a form to set your location.** The
operator's location comes from the status bar; in this surface it is only the
*reference point* (map centre, distance, bearing for antenna aiming). The map
earns its space by showing **where the stations are**, ranking HF stations by
**predicted reachability right now**, letting the operator pick a **channel**
(mode × frequency), and handing that to the modem. Conceptually: winlink.org's
RMSChannels, but native, single-pane, and **offline-first**.

This reframes the work shipped under `tuxlink-3iav` (#550), which added a
"Pick on map…" affordance to *set the operator's location* in Find a Gateway —
the wrong problem in the wrong place. That affordance is removed here.

## 2. Problems enumerated (operator session, 2026-06-10)

- **Operator location was being set in the wrong place.** It belongs to the
  status bar, not the station-finder dialog.
- **The map should show stations, not collect a location.** A map in this dialog
  is only justified if it projects stations (location, frequency, mode) for
  visual, informed selection — antenna aiming included.
- **WLE sorts HF stations by proximity.** For HF that is wrong: the nearest
  station is often *un*reachable (skip), and a far one reachable, depending on
  band and conditions. WLE beats us today on raw functionality here (it shows a
  propagation forecast; we show none) — even though it may spawn three windows.
- **WLE's HF predictions are an online download from the CMS.** This is a
  Catch-22 for emcomm: booting from a go-bag *seeking* a path, with no
  connection, you cannot update the predictions you need to *find* a path.
- **A nearby-station text list beside the map is redundant** — the map is the
  list.
- **SPLAT is not viable in tuxlink** (it needs a Geographica back end and bundled
  terrain data — too heavy for the Pi target). VHF/UHF terrain prediction is out.

## 3. Grounding (verified, 2026-06-10)

Checked against merged `origin/main`, the live public listing API, and `voacapl`.

**Current data flow.** Station lists are an operator-triggered live poll:
`catalog_fetch_stations` (Rust, `src-tauri/src/catalog/`) → the public Winlink
endpoints `https://cms.winlink.org:444/listings/<Mode>Listing.aspx?serviceCodes=PUBLIC`.
The `StationsCache` is **in-memory only**: 30-min TTL + per-key coalescing +
stale-on-error *within a session*. It is **not persisted to disk** — a cold,
offline launch has no stations. (Packet's endpoint is served without
`historyhours`; `RmsVaraFmListing.aspx` 404s — VARA FM endpoint undiscovered,
out of scope.)

**Station data model already supports the map.** `Gateway` carries `callsign`,
`grid` (Maidenhead → projectable lat/lon), `frequenciesKhz: number[]` (an
*array*), `location`, `sysopName`. Mode comes from the per-mode `StationListing`.
The same callsign appears across multiple per-mode listings.

**Real multi-mode example — N0DAJ (Doug Jarmuth, DM34OA Wickenburg AZ),** pulled
live: VARA HF @ 3590.0/7103.0/7108.0/10147.0/14103.0/14115.0 kHz; ARDOP HF @
3590.0/7103.0/7108.0/14103.0/14115.0 kHz; Packet as three SSIDs — N0DAJ-10 @
145.710, N0DAJ-11 @ 145.010, N0DAJ-12 @ 441.300 MHz. **Two consequences drive the
model:** (a) the same HF dial (e.g. 7103.0) carries *both* VARA and ARDOP — so a
channel is mode **and** frequency, never frequency alone; (b) packet's connect
target is the **SSID** (`N0DAJ-10`), not the bare call.

**Prediction engine — `voacapl` (VOACAP for Linux,** github.com/jawatson/voacapl).
Open/free (Debian-packaged → DFSG-free; CC0/public-domain + GPL-3+ parts),
**compatible with tuxlink's GPL-3**. Headless CLI, `gfortran`-built, uses the
`itshfbc` coefficient data (`makeitshfbc`). Does point-to-point circuit
reliability: tx grid → rx grid, per frequency, per hour. The only time-varying
input is the **smoothed sunspot number (SSN)**, which is slowly-varying and
forecastable months ahead — so it is **bundled/cached, not downloaded per
session** (the approach The Tech Prepper's EmComm Tools uses; he also distributes
SSN over radio). This is what dissolves the Catch-22. We use the `voacapl` engine
directly, not vendor EmComm Tools (a whole-Pi bash framework, not a library).

## 4. Design overview — three units, foundation-first

| Unit | Issue (TBD) | Depends on | Scope |
|---|---|---|---|
| **U1 — Offline HF prediction service** | foundation | — | `voacapl` sidecar + cached/forecast SSN; point-to-point reliability API |
| **U2 — Persistent station-list cache** | — | — | Disk last-known-good; offline open; freshness |
| **U3 — Find-a-Station map UI** | consumer | benefits from U1 + U2 | Mock-D surface; supersedes `CatalogBuilderPanel`; reverts #550 pin |

**Build order: U1 first.** U1 carries the security/correctness boundary (a
bundled native engine + the physics operators trust) and gets the full
`build-robust-features` cross-provider adversarial-review treatment before build.
U2 is small backend plumbing. U3 can proceed against distance-only ranking and
*light up* when U1 lands — it degrades gracefully to "no forecast yet."

## 5. U1 — Offline HF prediction service (`voacapl` sidecar)

**Sidecar boundary.** A bundled `voacapl` binary (per-arch: arm64 + amd64) plus
the `itshfbc` coefficient data, invoked headless by the Rust backend. It is a
**pure offline compute**: no network, no credentials, no transmit, no disk writes
outside a scratch run dir. This is categorically unlike the stripped Pat sidecar
(which wrote creds to disk and ran a transmit-capable client); the operator
confirmed a compute-only sidecar is appropriate.

**Inputs per circuit:** operator grid + station grid (→ lat/lon), frequency,
month + UTC hour, cached **SSN**, and default TX power + antenna model. **Output:**
per-frequency, per-hour reliability (REL) and SNR, parsed from `voacapx.out` (or
via the `dst2csv` utility).

**SSN handling (offline-first, the Catch-22 fix).** Bundle a forecast table of
smoothed SSN covering the current solar-cycle horizon; cache it under
`app_data_dir()`. Update opportunistically when online (a small file), never as a
precondition for prediction. An over-radio SSN top-up path is a *later*
enhancement, not v1. Surface SSN provenance in the UI ("solar data N old").

**Map ranking — two modes the plan will choose between:** (a) **point-to-point
per station** — one `voacapl` circuit run per visible station for the selected
band/time (fast; ms per circuit; fine for a few hundred stations), or (b)
**area-coverage** — one run producing a reliability grid that the map samples at
station locations. v1 leans (a) for simplicity; (b) is an optimization if the
station count or recompute cadence demands it.

**Defaults are operator-grounded, not invented.** TX power and antenna model
materially change predictions; v1 ships sensible defaults (e.g. 100 W, isotropic
/ simple dipole) that are **operator-configurable**, and the spec does not assert
specific reliability numbers — those come from the engine. Per the
amateur-radio-reliability caution, the engine is the source of truth; we wire and
display it, we do not hand-tune the physics.

**No prediction for VHF/UHF.** Packet (VHF/UHF) channels are **not** modeled (no
SPLAT, no terrain, no Geographica). They are listed factually (frequency, SSID,
distance) with a neutral "VHF/UHF · local" note.

## 6. U2 — Persistent station-list cache

Today's `StationsCache` is in-memory; a cold offline launch has nothing. U2 adds
**disk persistence of the last-known-good listing per mode** (`app_data_dir()`),
loaded on launch. The fetch flow becomes **offline-first, not always-latest**:

- On open, show the **last saved list immediately** with a freshness caption
  ("updated N ago") — no blocking network call, **no modal prompt**.
- **"Check for newer list"** is the only network action and is operator-initiated;
  on success it refreshes + re-stamps; on failure it keeps the saved list and
  says so. (Mirror the existing stale-on-error semantics, persisted.)
- Cache **only** good parses; never overwrite good data with an error body.

## 7. U3 — Find-a-Station map UI (the Mock-D surface)

One large single pane (FZ-M1 caveat below). Layout from
`dev/scratch/2026-06-10-find-a-station-map-mockD-propagation.html` (local visual
artifact; this document is authoritative).

**Conditions + band bar (top).** A conditions readout (UTC + local time, SFI/SSN,
K-index if available, SSN provenance). A **band selector** ("Reachability on:
80/40/30/20 m") that drives the map colouring, plus **mode** toggles
(VARA/ARDOP/Packet) and a radius/search filter.

**Map (left, ~55%).** One **pin per station location** (callsign + SSIDs collapse
to one pin). For HF on the selected band/time, pins are **weighted by predicted
reachability** — colour + size from REL (good → fair → marginal → faded/dashed
"unlikely/skip"), with a soft reliable-zone shade. Distance rings are demoted to
a backdrop. A bearing line + recenter-on-me control. **Distance is not the
ranking; reachability is.**

**Right rail (~45%) — replaces the redundant station list with decision support:**
1. **Selected station header:** callsign, sysop, location/grid, modes, last-heard.
2. **Antenna aiming hero:** compass + **bearing** (e.g. 318° NW) + distance.
3. **Path propagation forecast:** per-band reliability bars for *this* path,
   "best band now", and a 24-h reliability sparkline (gray-line aware).
4. **Channels — grouped by frequency/band** (operator's chosen default): each
   dial shown **once** with the mode(s) that run on it (collapsing N0DAJ's shared
   7.103 into one row) + a per-channel reliability pip/%. Skip-over channels
   dimmed. Packet rows show SSID + "VHF/UHF · local", no reliability grade. Each
   row's **Use →** hands *that exact mode + frequency (+ SSID for packet)* to the
   active modem via the existing prefill path (`emitGatewayPrefill`).

**Carried over from `CatalogBuilderPanel`:** mode/radius filters, the
fetch/refresh, favorites (★), and prefill-to-modem. **Removed:** the
operator-location field/pin (#550), and the bottom nearby-station text list.

**FZ-M1 compact.** Side-by-side will not fit the rugged screen; collapse to
map-on-top / list-below with a toggle, and a collapsible propagation panel.
Mirror the existing `@media (max-width:1365px) and (any-pointer:coarse)` discipline.

## 8. Station / channel data model

- **Station** = aggregation key `(operator base callsign, grid/location)`. N0DAJ
  and N0DAJ-10/-11/-12 collapse to one station/pin.
- **Channel** = `(mode, frequencyKhz, ssid?)`. SSID is set for packet (the connect
  target); HF channels share the base call. The same dial under two modes is two
  channels. The aggregator expands each mode-listing's `frequenciesKhz[]` into
  channels and groups by station.
- **Prediction** attaches to HF channels only, keyed by `(operatorGrid,
  stationGrid, frequency, time, SSN)`.

## 9. Sequencing and process

1. **U1** through `writing-plans` → `build-robust-features` (cross-provider Codex
   adrev), given RF-correctness criticality and the bundled-native-engine
   boundary.
2. **U2** small TDD-against-spec backend unit.
3. **U3** TDD-against-this-design UI unit; ships distance-ranked first, lights up
   on U1. Supersede `CatalogBuilderPanel` and revert the #550 pin within U3 (or a
   dedicated cleanup task) so no half-state ships.

Each unit gets its own plan and bd issue at planning time, with dep edges
(U3 → U1, U3 → U2).

## 10. Non-goals and constraints

- **No transmit-path change.** RADIO-1 does not gate; no added airtime/TOT/consent
  safeguards. Prediction is read-only compute; the modem's existing Connect click
  is the Part-97 control-operator act.
- **No SPLAT / Geographica / terrain data.** VHF/UHF gets no propagation model.
- **No online dependency for prediction.** Offline-first is binding: predict from
  bundled engine + cached SSN; the network only ever *refreshes* station lists and
  (optionally) SSN, never a precondition.
- **Use the `voacapl` engine, not a hand-port.** A native re-implementation is
  rejected for v1 (RF-correctness risk); the validated reference engine is the
  source of truth. EmComm Tools is the inspiration for the *approach*, not a
  vendored dependency.
- **The offline `BaseMap` substrate stands.** Reuse the existing offline map +
  Maidenhead math; the station layer + reachability colouring are additive.

## 11. Approved decisions (this brainstorm)

1. Find a Station is a **station map**, not a location-setter; operator location
   comes from the status bar (reference point only).
2. **Pin = station (location); channel = mode × frequency × SSID** = the selectable
   unit handed to the modem.
3. HF stations ranked by **predicted reachability**, not proximity; the map
   recolours by **selected band + time**.
4. **Offline HF prediction via a `voacapl` sidecar** (compute-only); **SSN
   bundled/cached/forecast**, never a per-session download.
5. **No SPLAT** — VHF/UHF packet listed factually, no terrain prediction.
6. **Persistent last-known-good** station cache; **open on saved list + manual
   refresh**, no modal prompt.
7. Right rail = **antenna bearing + path propagation forecast**, replacing the
   redundant nearby-station list.
8. Channels grouped **by frequency/band** (default; redline at review).
9. This **supersedes Find a Gateway** and **reverts the #550 operator-location
   pin**; favorites + prefill-to-modem carry over.

## 12. Open items for the plans

- **U1:** point-to-point-per-station vs area-coverage map ranking; exact
  `voacapl` input-deck shape + output parse; per-arch build + `itshfbc` bundling
  in CI; default power/antenna config surface; SSN forecast source + cache format.
- **U2:** on-disk cache format + location + eviction; freshness-stamp wire shape.
- **U3:** channel grouping (frequency/band default) review; FZ-M1 compact layout;
  reachability colour scale thresholds; how the band selector interacts with the
  mode filters; recompute cadence (on band/time change) without UI jank.
- All: mockups produced during the 2026-06-10 brainstorm live locally under
  `dev/scratch/2026-06-10-find-a-station-map-mock{A,B,C,D}*.html`; this document
  is authoritative.
