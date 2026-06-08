# Catalog request builder — design

> Status: **locked** (operator brainstorm 2026-06-07, agent `basalt-mesa-dahlia`, visual-companion session).
> Smoke-walk item 12 (`tuxlink-a2gd`). Brainstorm #2 of 4. Builds atop the existing `src/catalog/` WLE-parity tree-picker (`tuxlink-ddiq`).

## Summary

Replace the clunky "navigate a tree, send a request message, wait for an email reply, handle it yourself" loop with a **location-aware request builder** that:

1. **Directly polls** the unauthenticated Winlink listing endpoint for **station lists** (instant, no message round-trip) — the same capability item 11 (`tuxlink-4bgn`) needs to ingest gateways into the radio config + Favorites.
2. Uses the existing **message-request rails** (`INQUIRY@winlink.org`) only for categories the endpoint can't serve (weather, propagation, bulletins, info docs).
3. **Parses known reply categories** into structured views, with **graceful fallback to the raw message** when a reply deviates from the expected format.

## Architecture — two transports, picked by category

### Direct HTTP poll (station lists)
- Endpoint family: `cms.winlink.org:444/listings/…` (e.g. `RmsVaraListing.aspx?serviceCodes=PUBLIC&historyhours=168`); per-mode listing URLs. This is what WLE itself polls when it "updates the station list," so it is an accepted client behavior.
- Rust command `catalog_fetch_stations({ modes, serviceCodes, historyHours })` performs the GET(s), parses the listing rows (callsign-SSID, frequency, grid, mode, last-heard), and returns structured gateways.
- **Polite-client requirements (hard):** cache responses (per mode, short TTL, e.g. 15–60 min), coalesce concurrent requests, respect a minimum refetch interval, set a descriptive User-Agent. The "WLE clients banned from public OSM tile servers" lesson (item 11c) applies directly — Tuxlink must not hammer winlink endpoints.
- **Grounding (implementation task, not assumed):** verify the exact endpoint shape + row format against real responses / prior-art (Pat, WLE) before relying on the parse. Treat AI-sourced endpoint claims as suspect (`feedback_ai_amateur_radio_reliability`).
- **Shared with item 11:** the parsed gateway list feeds (a) the builder's results, (b) Favorites star-to-add, (c) the radio config panes' station selection. One fetch/parse/cache layer, three consumers.

### Message request (everything else)
- Categories the endpoint does not serve (area weather, propagation, bulletins, info docs) compose a request to `INQUIRY@winlink.org` (`Subject: REQUEST`) via the existing `catalog_send_inquiry` rails, queued in the Outbox, answered on the next connect.

## Builder UX

Inline panel (no pop-up window), opened from **Message → Catalog Request** and from a new **"Find a gateway"** entry point in the radio config panes.

- **Form column (~286px):**
  - **Your location** — the operator's configured grid (auto, editable inline).
  - **Station modes** — checkboxes (VARA HF, Packet, ARDOP HF, VARA FM, …).
  - **Within** — radius slider/input (miles, default sensible e.g. 300).
  - **Also request (by message)** — info categories (Area weather, Propagation, Bulletins, …).
  - **Get stations →** — runs the direct poll.
- **Results column:** distance-sorted, mode-badged gateway rows (callsign-SSID, frequency, grid, distance). Each row: **★ to add to that mode's Favorites**; rows beyond the radius dim rather than vanish. Distance computed from the operator grid via `src/forms/position/maidenhead.ts`.
- **Footer:** when info categories are checked, a "Queue N request(s)" action sends the message request(s); a clear confirmation states they arrive in the Inbox after the next connect.

## Reply rendering — parse-with-fallback

For message-category replies (`feedback`-driven robustness):

- Per-category **parsers** (start with the highest-value, e.g. area weather) transform a known reply into a **structured view** (the "kills the raw-email annoyance" win).
- Each parser is **defensive**: if the reply deviates from the expected format (missing markers, unexpected layout, parse error), it **gracefully degrades to the raw message** view — never an error, never a blank. The raw message is always available as a fallback toggle.
- A reply is matched to a parser by category markers; unknown replies render raw.

## Data flow

```
Builder form ──(stations)──► catalog_fetch_stations ──► winlink listing endpoint (cached)
                                      │
                                      ├──► builder results (★→Favorites)
                                      └──► item 11 radio-config ingest
Builder form ──(info cats)──► catalog_send_inquiry ──► Outbox ──► CMS ──► Inbox reply
                                                                              │
                                                                  parse(category) ──► structured view
                                                                              └─(deviates)─► raw message
```

## Error handling
- Direct-poll failure (network, endpoint change) → non-blocking error + offer the **message-request** path for stations as a fallback ("couldn't reach the listing service — request by message instead?").
- Parser failure → raw message (above).
- Cache stale + offline → show last cached list with an "as of <time>" stamp.

## Testing
- **Rust:** listing parse against captured fixture responses (per mode); cache TTL + coalescing + min-refetch; distance filter; `catalog_send_inquiry` request composition; reply-parser happy path + **deviation→raw fallback** per category.
- **Frontend:** builder form (location/modes/radius/categories); results (distance sort, dim-beyond-radius, ★→Favorites calls the favorites store); queue-requests confirmation; reply view (structured vs raw toggle + auto-fallback).

## Out of scope (v1)
- Authenticated/private listings (PUBLIC service code only).
- Parsers for every category — ship the high-value ones; the rest render raw (graceful).
- Aggressive direct-polling of non-station endpoints (stay message-request for those).

## Open items for the implementation plan
- Exact endpoint URLs + row formats per mode (grounding task).
- Cache TTL + min-refetch interval values; User-Agent string.
- Which reply categories get a parser in v1 (proposed: area weather first).
- Default radius + units (mi/km from operator locale).
