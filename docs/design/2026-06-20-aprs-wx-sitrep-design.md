# APRS→Winlink Weather SITREP (tuxlink-hepq) — design

**Date:** 2026-06-20 · **Branch:** bd-tuxlink-xsv5/filter-loop-fix · **Agent:** moss-bog-cove
**Status:** APPROVED (format chosen by operator) → building

## Problem

Tuxlink hears APRS weather (WX) reports + positions over RF and currently only
*displays* them (map badges, Station Data). The strategic feature (`tuxlink-hepq`,
"a big one"): aggregate that accumulated tactical data into a **structured,
transmittable local-area weather situation report** sent as a Winlink message —
turning discarded RF data into ground truth for arriving EmComm personnel
("what are conditions, how should they pack/dress").

Prior state: only the `ni5b` **PNG map snapshot** shipped (a screenshot, and
broken — see below). The text *report* was deferred to hepq and never built.

## Decision — format

Operator chose the **hybrid** layout (over tactical-sectioned and compact-narrative):
plain-language ground-truth + what-to-expect **first**, then aggregate ranges, then
a per-station detail table, then an RF-honesty footer.

Sub-decisions (operator-approved defaults):
- **Units:** ham-conventional (°F / mph / in / hPa). Metric toggle deferred.
- **Aggregation:** ranges across heard stations **and** a per-station detail table.
- **Assessment line:** a short, explicitly-labeled, **data-derived** "on the ground"
  + "for arrivals" line. Conservative thresholds only — never fabricate a sky
  condition or a reading not on the wire.
- **Locations:** callsign + **Maidenhead grid** (`latLonToGrid`) — NEVER invented
  place names (the wire carries no town names; the mock's "Black Mtn" etc. were
  illustration only).
- **Winlink composition:** text **body** + subject `WX SITREP <area> <DDHHMM>Z`,
  composed via the existing draft + `compose_window_open` path (no Rust changes).
  PNG map attach is a later optional.

## RF-honesty rules (load-bearing)

- Report ONLY what was actually heard. Every WX field is `Option`/nullable; absent
  fields render `—`, never `0` or a guess.
- Always state station count + heard window + oldest-reading age.
- Per-station age shown ("4m ago"). Stale stations are still listed, with their age.

## Data source (already decoded)

`WxStation` (from `joinWxStations`) → `env.channels` keyed by `ChannelKind`
(`temperature`, `wind_dir`, `wind_speed`, `wind_gust`, `humidity`, `pressure`,
`snow`) + `env.rain: RainTotals {in1h,in24h,sinceMidnight}` + `at` (lastHeard) +
`lat`/`lon`. Pressure trend derived from each channel's `history` samples.

## Implementation

- `src/aprs/wxSitrep.ts` — pure `composeWxSitrep(stations, { nowMs, operatorGrid? })
  → { subject, body }`. Aggregation, compass, trend, honest assessment, hybrid
  render. Unit-tested (`wxSitrep.test.ts`).
- A "Weather SITREP" control in the map: builds the report from the live `wx`
  stations, `saveDraft({subject,body,to:''})`, `invoke('compose_window_open',{draftId})`.
- Empty/edge: no heard WX stations → control disabled (nothing honest to report).

## Also fixing (operator request)

The "Export PNG" map snapshot doesn't work: #836 removed `preserveDrawingBuffer`
on the wrong drunk-map theory (the real cause was the `setFilter` loop, fixed in
PR #839). Reading the WebGL canvas without it yields a blank image. Re-enable a
working capture.
