# APRS animated digipeat path on hover (cn84) — design

- **Issue:** tuxlink-cn84 (epic tuxlink-18q2, "Full APRS rich experience")
- **Date:** 2026-06-19
- **Status:** approved (design); pending implementation plan
- **Author:** mink-yew-osprey

## Summary

On the APRS Tac Chat positions map, animate the path a heard packet traveled —
sender → digipeater(s) → operator — when the operator hovers a station pin, and
additionally trace each newly-heard frame once as it arrives. The motion mirrors
aprs.fi: a packet dot rides the path while it draws hop-by-hop, then the line
lingers and fades.

The feature extends the map's existing RF-honesty posture (uncertainty regions
for ambiguous fixes; stale-dimming) to the via-chain: segments between hops with
a real heard position render solid; a hop whose location is unknown (a `WIDEn-N`
alias, or a digipeater whose beacon has not been heard) renders as a dashed
connector with a `pos?` marker rather than a fabricated intermediate pin.

## Decisions locked in brainstorm (visual companion, 2026-06-19)

1. **Path style — hybrid honest path.** Solid segments through located hops;
   dashed `pos?` connector across unlocatable hops; degrade to a direct
   sender → operator line when no intermediate hop is locatable. (Rejected:
   direct-link-only as too thin; full-geo-path-only because it cannot render
   when any hop is unlocated, which is the common case.)
2. **Trigger — hover + live auto-trace.** Hover a pin (or, future, a feed row)
   draws/clears the path; a new `aprs-position:new` event triggers a single
   one-shot trace that fades. (Rejected: hover-only as not "alive"; always-on
   faint paths as map clutter — reserved as a possible future toggle.)
3. **Feel — aprs.fi-classic.** ~2 s hop-by-hop draw, a bright "packet" dot rides
   sender → operator, ~2 s linger, then fade. Exact durations are code-tunable.

## The core data problem

The via-chain a packet traversed is present in the received AX.25 frame
(`Frame.path.digis: Vec<Address>`), and each digipeater address octet carries an
**H-bit** ("has-been-repeated") distinguishing digipeaters that actually relayed
the frame from those merely requested. Today both are discarded before the data
reaches the UI:

- `ax25::frame::Path::decode` parses the digi addresses but drops the bit-7
  H-flag.
- `aprs::engine` calls `try_emit_position(sender, dest.call, info, now)` — the
  digipeater list is never passed.
- `InboundPos` (and its `aprs-position:new` DTO) has no path field; neither does
  `InboundMsgDto`.

So cn84 is **not** pure map UI. It requires surfacing the via-chain (with H-bits)
from the AX.25 layer through the engine DTO to the frontend store, then a
resolution step that correlates each hop callsign to a known position.

## Components

### 1. Backend — retain and surface the via-chain

**`src-tauri/src/winlink/ax25/frame.rs`**
- `Path::decode` retains each digi's H-flag. Add `repeated: Vec<bool>` to `Path`,
  index-parallel to `digis`. The TX builders (`framebuild.rs`) set it empty
  (interpreted as "none repeated"); only the RX decode populates it. The address
  octet bit-7 is already extracted by `Address::decode` (returned as the first
  bool of its `(Address, bool, bool)` tuple); for a *digipeater* position that
  bit is the H-bit (for src/dest it is the C-bit and is irrelevant here).

**`src-tauri/src/winlink/aprs/engine.rs`**
- New wire struct `ViaHop { call: String, repeated: bool }` (serde camelCase).
- `InboundPos` gains `via: Vec<ViaHop>`.
- `try_emit_position` gains access to the frame path (pass `&frame.path` or a
  pre-built `Vec<ViaHop>`); it builds `via` from `path.digis` + `path.repeated`,
  preserving on-wire order.
- **Honesty rule:** the *traversed* path used for rendering is sender +
  **only the digis with `repeated == true`** + operator. Non-repeated requested
  digis are carried in the DTO for completeness but are not drawn as hops. (If a
  frame somehow lacks H-bits — e.g. a synthetic/test frame — fall back to
  treating all via entries as traversed; documented, not silently dropped.)

Scope guard: v1 surfaces `via` on **positions only** (the map's data source).
Adding `via` to `InboundMsgDto` for feed-row hover is a noted follow-on.

### 2. Frontend types + store

**`src/aprs/aprsTypes.ts`**
- `export interface ViaHop { call: string; repeated: boolean }`
- `InboundPosDto.via: ViaHop[]`
- `HeardPosition.via: ViaHop[]` — the latest frame's via for that station
  (latest-position-wins, same as the coordinates).

**`src/aprs/useAprsPositions.ts`**
- Carry `via` from the inbound DTO into the accumulated `HeardPosition`.

### 3. Path resolution — pure function (the honesty logic)

**New: `src/aprs/digipeatPath.ts`**

```
resolveDigipeatPath(
  src: { call, lat, lon },
  via: ViaHop[],
  heardPositions: Map<string, {lat, lon}>,   // callsign → latest fix
  operator: { lat, lon } | null,
): PathSegment[]
```

- Build the ordered hop list: `src` → (each `repeated` via hop, in order) →
  `operator` (when known).
- Resolve each station hop to coordinates: `src` from its own fix; each via hop
  by callsign lookup in `heardPositions`; `operator` from the operator latLon.
- Emit segments:
  - Between two consecutive **located** hops → `{ kind: 'solid', from, to }`.
  - Across one or more **unlocated** hops between two located anchors →
    `{ kind: 'dashed', from, to, unknownLabels: string[] }` (the `pos?` markers
    are the unknown hop callsigns, placed at the segment midpoint).
  - If no intermediate hop is locatable → a single `src → operator` segment
    (`kind: 'dashed'` when via existed but none located; `kind: 'solid'` for a
    genuinely direct, no-digi frame).
  - If `operator` is null (no known operator grid) → terminate the path at the
    last located hop; do not invent an endpoint.
- No DOM, no maplibre — fully unit-testable.

### 4. Render + animation — maplibre-native

**In `src/aprs/AprsPositionsMap.tsx` (+ a small controller module)**
- Add a GeoJSON **path source** + **line layer**. Segment `kind` drives paint:
  solid green vs dashed amber (`line-dasharray`). Draw-in via animated
  `line-gradient` (a progress stop swept 0→1).
- Add a **packet-dot** circle layer fed by a one-point source; a
  `requestAnimationFrame` controller advances the point along the resolved
  polyline over the draw duration, then holds, then fades the whole path
  (source cleared).
- A single **animation controller** (plain TS, testable) owns timeline state
  (`idle → drawing → linger → fading`) and exposes `traceOnce(segments)` and
  `setHovered(segments | null)`. Two callers:
  - **hover:** pin `mouseenter`/`mouseleave` → `setHovered`.
  - **live:** an effect subscribed to new positions calls `traceOnce` for the
    station whose fix just arrived.
- The WebGL layer is a thin shell over the controller, so logic is tested
  without a GL context (jsdom has none — see the n4hz "test the production mount
  path" lesson).

## Data flow

```
RX AX.25 frame ─ Path::decode (retain H-bits)
  └─ engine try_emit_position(&frame.path) ─ InboundPos{ ...pos, via:[ViaHop] }
       └─ emit "aprs-position:new" (camelCase DTO with via)
            └─ useAprsPositions ─ HeardPosition{ ...pos, via }
                 ├─ map pin (existing)
                 └─ resolveDigipeatPath(src, via, heardPositions, operator)
                      └─ animation controller ─ maplibre path+dot layers
                           ├─ hover trigger
                           └─ live new-position trigger
```

## Error handling / honesty

- Unlocatable hop → dashed connector + `pos?` marker; never a fabricated pin.
- No operator grid → path terminates at last located hop (no invented endpoint).
- Operator grid is precision-reduced (4-char Maidenhead) → terminal hop anchors
  at the grid-square center, honestly approximate (consistent with the project's
  GPS precision-reduction default).
- Stale hops: resolution uses the latest known fix; a hop dimmed as stale still
  anchors a segment (it was the best real fix heard). No special-casing in v1.
- A frame with empty via and a known direct fix → a plain solid `src → operator`
  line (degenerate, correct).

## Testing

**Rust**
- `Path::decode` retains H-bits in on-wire order (repeated vs requested).
- `InboundPos.via` is populated, ordered, with correct `repeated` flags, via the
  existing in-process emit-sink test pattern in `engine.rs`.

**TypeScript (vitest)**
- `resolveDigipeatPath`: all-hops-known (all solid); a mid-path unlocated hop
  (solid–dashed–solid); no-intermediate-located (degrade to dashed direct);
  genuinely direct no-digi frame (solid direct); null operator (terminate early);
  alias-only via (`WIDE1-1,WIDE2-1` → dashed direct with two `pos?` labels).
- Animation controller: `idle→drawing→linger→fading` transitions; hover overrides
  a running live trace; `setHovered(null)` clears.

**Wire-walk (done-time gate; flows captured greenfield by operator)**
1. A heard station's pin is hovered → the hybrid honest path animates to you.
2. A new frame is heard → its path auto-traces once and fades.
3. A station heard only via `WIDE` aliases → path degrades honestly (dashed +
   `pos?`), with no false-exact intermediate pin.

## Out of scope (v1)

- Feed-row (chat message) hover tracing — needs `via` on `InboundMsgDto`.
- Always-on faint static paths for every station (rejected brainstorm option;
  possible future toggle).
- Any transmit-path animation.

## Definition of done

The three wire-walk flows pass on a real build; `resolveDigipeatPath` and the
animation controller are unit-tested; Rust H-bit retention + `InboundPos.via`
are unit-tested; CI green (typecheck, vitest, build, clippy, cargo test).
