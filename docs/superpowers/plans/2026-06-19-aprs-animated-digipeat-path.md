# APRS animated digipeat path (cn84) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** On the APRS Tac Chat map, animate a heard packet's sender → digipeater(s) → operator path on pin hover and once per newly-heard frame, aprs.fi-style, rendering honestly where hop positions are unknown.

**Architecture:** Surface the AX.25 via-chain + H-bit (currently discarded) through the engine DTO to the frontend store; resolve hops to coordinates with a pure function that draws solid segments through located hops and dashed "pos?" connectors across unlocatable ones; animate with a maplibre-native line + packet-dot layer driven by a testable trace controller.

**Tech Stack:** Rust (Tauri backend, AX.25/APRS engine), TypeScript + React 18, maplibre-gl v5, vitest.

## Global Constraints

- MSRV 1.75 — no API stabilized in 1.76+ (clippy `incompatible_msrv` is denied).
- Serde wire forms are **camelCase** (`#[serde(rename_all = "camelCase")]`); the TS DTO mirror must match exactly.
- RF-honesty: never plot a fabricated/estimated coordinate. Unknown hop → dashed connector + `pos?` label, never an invented pin.
- Rust: `--manifest-path src-tauri/Cargo.toml` (no workspace-root Cargo.toml). This Pi does not finish a cold cargo build — write Rust + tests, let CI compile.
- Frontend tests run locally: `pnpm vitest run <file>`.
- Commit trailer on every commit: `Agent: mink-yew-osprey` + `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- Worktree cwd gotcha: run a standalone `cd <worktree>` Bash call before any `git` op so the main-checkout hook reads the worktree cwd.
- v1 scope: positions (map pins) only. Do NOT add `via` to `InboundMsgDto` / feed-row hover; do NOT build always-on faint paths.

---

### Task 1: Retain the digipeater H-bit on AX.25 decode

**Files:**
- Modify: `src-tauri/src/winlink/ax25/frame.rs` (`Path` struct ~line 270; `Path::decode` ~line 296; `Path::encode` ~line 280)
- Modify (compile-fix only): every `Path { … }` literal — `src-tauri/src/winlink/aprs/framebuild.rs`, in-file `frame.rs` tests, `engine.rs` tests. Find with `grep -rn 'Path {' src-tauri/src`.
- Test: `src-tauri/src/winlink/ax25/frame.rs` (in-file `#[cfg(test)]`)

**Interfaces:**
- Produces: `pub struct Path { dest: Address, src: Address, digis: Vec<Address>, repeated: Vec<bool> }` where `repeated[i]` is the H-bit of `digis[i]` (true = that digipeater relayed this frame). On TX-built paths `repeated` is `vec![]`.

- [ ] **Step 1: Write the failing test**

Add to `frame.rs` tests:

```rust
#[test]
fn decode_retains_digi_h_bits() {
    // SRC>DEST,DIGI1*,DIGI2 — DIGI1 repeated (H=1), DIGI2 not (H=0).
    let dest = Address { call: "APZTUX".into(), ssid: 0 };
    let src = Address { call: "KE7XYZ".into(), ssid: 9 };
    let d1 = Address { call: "W7RPT".into(), ssid: 1 };
    let d2 = Address { call: "WIDE2".into(), ssid: 1 };
    // Build the 7-byte address fields by hand so we control the H (cr) bit.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&dest.encode(true, false)); // dest C-bit, not last
    bytes.extend_from_slice(&src.encode(false, false)); // src, not last
    bytes.extend_from_slice(&d1.encode(true, false));   // digi1 H=1, not last
    bytes.extend_from_slice(&d2.encode(false, true));   // digi2 H=0, last
    let (path, _off) = Path::decode(&bytes).unwrap();
    assert_eq!(path.digis, vec![d1, d2]);
    assert_eq!(path.repeated, vec![true, false]);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml decode_retains_digi_h_bits` (or let CI run it — this Pi may not finish a cold build).
Expected: FAIL — `Path` has no field `repeated`.

- [ ] **Step 3: Add the field and populate it on decode**

In the `Path` struct (~line 270) add the field:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    pub dest: Address,
    pub src: Address,
    pub digis: Vec<Address>, // 0..=2
    /// Per-digi has-been-repeated (H-bit) flag, index-parallel to `digis`.
    /// RX-only: TX-built paths leave this empty. `repeated[i] == true` means
    /// digi `i` actually relayed this frame (vs. a requested-but-unused alias).
    pub repeated: Vec<bool>,
}
```

In `Path::decode` (~line 296) capture the cr/H bit per address instead of discarding it:

```rust
    pub fn decode(bytes: &[u8]) -> Result<(Path, usize), FrameError> {
        let mut addrs = Vec::new();
        let mut hbits = Vec::new();
        let mut off = 0;
        loop {
            if bytes.len() < off + 7 {
                return Err(FrameError::Truncated);
            }
            let (a, cr, last) = Address::decode(&bytes[off..off + 7])?;
            addrs.push(a);
            hbits.push(cr);
            off += 7;
            if last {
                break;
            }
            if addrs.len() >= 4 {
                return Err(FrameError::BadAddressLength);
            }
        }
        if addrs.len() < 2 {
            return Err(FrameError::BadAddressLength);
        }
        let dest = addrs.remove(0);
        let src = addrs.remove(0);
        hbits.remove(0); // drop dest C-bit
        hbits.remove(0); // drop src C-bit
        Ok((Path { dest, src, digis: addrs, repeated: hbits, }, off))
    }
```

`Path::encode` is unchanged (it already writes H=0 on TX, line ~291); it does not read `repeated`.

- [ ] **Step 4: Fix the construction-site compile errors**

`grep -rn 'Path {' src-tauri/src`. For each literal that is NOT `Path::decode`'s return (framebuild.rs and the various tests), add `repeated: vec![]`. Example in `framebuild.rs`:

```rust
        path: Path {
            dest: id.tocall.clone(),
            src: id.source.clone(),
            digis: id.path.clone(),
            repeated: vec![],
        },
```

- [ ] **Step 5: Run tests to verify pass + no regressions**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink ax25::frame` (CI if local build won't finish).
Expected: PASS, including the new test and existing `frame.rs` tests.

- [ ] **Step 6: Commit**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-cn84-aprs-animated-path
git add src-tauri/src/winlink/ax25/frame.rs src-tauri/src/winlink/aprs/framebuild.rs
git commit -m "feat(aprs): retain digipeater H-bit on AX.25 decode

Path::decode now captures the per-digi has-been-repeated (H) bit into a
new Path.repeated vec, parallel to digis. TX-built paths leave it empty.
Foundation for surfacing the traversed via-chain to the map (cn84).

Agent: mink-yew-osprey
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Surface the via-chain on `InboundPos`

**Files:**
- Modify: `src-tauri/src/winlink/aprs/engine.rs` (`InboundPos` ~line 150; `try_emit_position` signature ~line 541 + emit ~line 605; call site ~line 278)
- Test: `src-tauri/src/winlink/aprs/engine.rs` (in-file tests, `RecSink` pattern ~line 931)

**Interfaces:**
- Consumes: `Path.repeated` from Task 1.
- Produces: `pub struct ViaHop { call: String, repeated: bool }` (serde camelCase); `InboundPos.via: Vec<ViaHop>`. Wire event `aprs-position:new` now carries `via: [{ call, repeated }]` in on-wire order.

- [ ] **Step 1: Write the failing test**

Add to `engine.rs` tests (mirror an existing `heard_*_emits_position` test that feeds a frame through the engine; reuse its frame-building helper, but set a digi path with one repeated hop). Assert the emitted `InboundPos.via`:

```rust
#[test]
fn heard_position_surfaces_traversed_via_chain() {
    let sink = RecSink::default();
    let positions = sink.positions.clone();
    let mut eng = AprsEngine::new(identity(), Box::new(sink));
    // A position beacon from KE7XYZ-9 digipeated by W7RPT-1 (H=1), WIDE2-1 (H=0).
    let frame = Frame {
        path: Path {
            dest: Address { call: "APZTUX".into(), ssid: 0 },
            src: Address { call: "KE7XYZ".into(), ssid: 9 },
            digis: vec![
                Address { call: "W7RPT".into(), ssid: 1 },
                Address { call: "WIDE2".into(), ssid: 1 },
            ],
            repeated: vec![true, false],
        },
        control: Control::Ui,
        // An uncompressed position info field (lat/lon/symbol). Reuse the exact
        // info bytes from `heard_uncompressed_position_emits_position_with_latlon_and_symbol`.
        info: b"!4807.00N/12215.00W>test".to_vec(),
    };
    let bytes = kiss_data_frame(&frame.encode_ui().unwrap());
    eng.on_kiss_bytes(&bytes, 1000); // use whatever the existing tests call to feed bytes
    let pos = positions.lock().unwrap();
    assert_eq!(pos.len(), 1);
    assert_eq!(
        pos[0].via,
        vec![
            ViaHop { call: "W7RPT-1".into(), repeated: true },
            ViaHop { call: "WIDE2-1".into(), repeated: false },
        ],
    );
}
```

(Match the frame-feeding call and info bytes to the existing position tests in this file — read `heard_uncompressed_position_emits_position_with_latlon_and_symbol` ~line 1317 and copy its mechanism verbatim.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml heard_position_surfaces_traversed_via_chain` (or CI).
Expected: FAIL — `InboundPos` has no field `via` / `ViaHop` undefined.

- [ ] **Step 3: Define `ViaHop`, add `InboundPos.via`, plumb the path through**

Add near `InboundPos` (~line 148):

```rust
/// One hop in a heard frame's digipeater via-chain, in on-wire order.
/// `repeated` is the AX.25 H-bit: true means this digipeater actually relayed
/// the frame (vs. a requested-but-unused alias). Mirrors the frontend `ViaHop`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViaHop {
    pub call: String,
    pub repeated: bool,
}
```

Add to `InboundPos` (after `ambiguity`, ~line 167):

```rust
    /// The frame's digipeater via-chain (callsign-SSID, in on-wire order) with
    /// each hop's has-been-repeated flag. Empty for a directly-heard frame.
    #[serde(default)]
    pub via: Vec<ViaHop>,
```

Change `try_emit_position` to accept the path (~line 541). Replace the `dest: &str` parameter with `path: &Path` and derive both the dest callsign and the via-chain from it:

```rust
    fn try_emit_position(&mut self, sender: &str, path: &crate::winlink::ax25::frame::Path, info: &[u8], now_ms: u64) {
        let dest = &path.dest.call;
        // ... existing body unchanged until the InboundPos emit ...
```

(Inside, the single use of `dest` is `parse_mice(dest, info)` — now `parse_mice(dest, info)` with `dest` the local `&path.dest.call`.)

Build `via` just before the emit and include it:

```rust
        let via: Vec<ViaHop> = path
            .digis
            .iter()
            .enumerate()
            .map(|(i, d)| ViaHop {
                call: fmt_callsign(d), // "CALL-SSID", reuse framebuild::fmt_callsign
                repeated: path.repeated.get(i).copied().unwrap_or(false),
            })
            .collect();
        self.sink.emit_position(InboundPos {
            sender: sender.to_string(),
            name,
            lat: pos.lat,
            lon: pos.lon,
            symbol_table: pos.symbol_table,
            symbol_code: pos.symbol_code,
            comment: pos.comment,
            ambiguity: pos.ambiguity,
            via,
        });
```

Add `use super::framebuild::fmt_callsign;` (or fully-qualify) at the top of `engine.rs` if not already imported.

Update the call site (~line 278):

```rust
        self.try_emit_position(&sender, &frame.path, &info, now_ms);
```

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml winlink::aprs::engine` (or CI).
Expected: PASS — new test green, existing position tests still green (they now also carry `via: vec![]` for no-digi frames, which the `#[serde(default)]` + explicit field satisfies).

- [ ] **Step 5: Commit**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-cn84-aprs-animated-path
git add src-tauri/src/winlink/aprs/engine.rs
git commit -m "feat(aprs): surface heard via-chain on aprs-position:new

InboundPos gains via: Vec<ViaHop> { call, repeated }, built from the
frame path's digis + H-bits and emitted in on-wire order. try_emit_position
now takes &Path so the digipeater list reaches the DTO. Enables the
honest digipeat-path render (cn84).

Agent: mink-yew-osprey
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: Frontend types + position store carry `via`

**Files:**
- Modify: `src/aprs/aprsTypes.ts` (`InboundPosDto` ~line 84; `HeardPosition` ~line 105)
- Modify: `src/aprs/useAprsPositions.ts` (~line 66)
- Test: `src/aprs/useAprsPositions.test.ts`

**Interfaces:**
- Produces: `export interface ViaHop { call: string; repeated: boolean }`; `InboundPosDto.via?: ViaHop[]`; `HeardPosition.via: ViaHop[]`.

- [ ] **Step 1: Write the failing test**

Add to `useAprsPositions.test.ts` (follow the existing pattern that fires a fake `aprs-position:new` payload and asserts on `result.current.positions`):

```ts
it('carries the via-chain from the inbound DTO onto the heard position', async () => {
  const { result, emit } = renderUseAprsPositions(); // existing harness in this test file
  await act(async () => {
    emit({
      sender: 'KE7XYZ-9', lat: 48.1, lon: -122.2,
      symbolTable: '/', symbolCode: '>', comment: '', ambiguity: 0,
      via: [
        { call: 'W7RPT-1', repeated: true },
        { call: 'WIDE2-1', repeated: false },
      ],
    });
  });
  expect(result.current.positions[0].via).toEqual([
    { call: 'W7RPT-1', repeated: true },
    { call: 'WIDE2-1', repeated: false },
  ]);
});
```

(Match `renderUseAprsPositions`/`emit` to the harness already used in this test file; if the file uses a raw `listen` mock, copy that mechanism instead.)

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm vitest run src/aprs/useAprsPositions.test.ts`
Expected: FAIL — `positions[0].via` is `undefined`.

- [ ] **Step 3: Add the types**

In `aprsTypes.ts`, add after `HeardStation` (or near the position DTOs):

```ts
/// One hop in a heard frame's digipeater via-chain, mirroring the Rust `ViaHop`
/// (serde camelCase). `repeated` is the AX.25 H-bit: true = this digipeater
/// actually relayed the frame (vs. a requested-but-unused alias).
export interface ViaHop {
  call: string;
  repeated: boolean;
}
```

Add to `InboundPosDto` (after `ambiguity`):

```ts
  /// Digipeater via-chain in on-wire order. Absent on legacy payloads ⇒ `[]`.
  via?: ViaHop[];
```

Add to `HeardPosition` (after `ambiguity`):

```ts
  /// The latest heard frame's via-chain for this station (latest-position-wins,
  /// like the coordinates). `[]` when the frame was directly heard / legacy.
  via: ViaHop[];
```

- [ ] **Step 4: Carry `via` through the store**

In `useAprsPositions.ts`, in the `next.set(identity, { … })` object (~line 66) add:

```ts
          ambiguity: p.ambiguity,
          via: p.via ?? [],
          at: Date.now(),
```

- [ ] **Step 5: Run tests to verify pass**

Run: `pnpm vitest run src/aprs/useAprsPositions.test.ts`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-cn84-aprs-animated-path
git add src/aprs/aprsTypes.ts src/aprs/useAprsPositions.ts src/aprs/useAprsPositions.test.ts
git commit -m "feat(aprs): carry via-chain through the position store

ViaHop type + InboundPosDto.via + HeardPosition.via; useAprsPositions
threads the latest frame's via onto each heard position (cn84).

Agent: mink-yew-osprey
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: `resolveDigipeatPath` — pure honest-path resolution

**Files:**
- Create: `src/aprs/digipeatPath.ts`
- Test: `src/aprs/digipeatPath.test.ts`

**Interfaces:**
- Consumes: `ViaHop` (Task 3).
- Produces: `resolveDigipeatPath(input: ResolveInput): PathSegment[]` and the `LatLon`, `PathSegment`, `ResolveInput` types below.

- [ ] **Step 1: Write the failing tests**

Create `src/aprs/digipeatPath.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { resolveDigipeatPath, type ResolveInput } from './digipeatPath';

const YOU = { lat: 47.0, lon: -122.0 };
const SRC = { call: 'KE7XYZ-9', lat: 48.1, lon: -122.6 };
const RPT = { lat: 47.8, lon: -122.4 };

function input(p: Partial<ResolveInput>): ResolveInput {
  return { src: SRC, via: [], located: new Map(), operator: YOU, ...p };
}

describe('resolveDigipeatPath', () => {
  it('direct no-digi frame → one solid src→you segment', () => {
    const segs = resolveDigipeatPath(input({}));
    expect(segs).toEqual([{ kind: 'solid', from: { lat: SRC.lat, lon: SRC.lon }, to: YOU }]);
  });

  it('all hops located → all solid', () => {
    const segs = resolveDigipeatPath(input({
      via: [{ call: 'W7RPT-1', repeated: true }],
      located: new Map([['W7RPT-1', RPT]]),
    }));
    expect(segs).toEqual([
      { kind: 'solid', from: { lat: SRC.lat, lon: SRC.lon }, to: RPT },
      { kind: 'solid', from: RPT, to: YOU },
    ]);
  });

  it('mid-path unlocated hop → solid then dashed with pos? label', () => {
    const segs = resolveDigipeatPath(input({
      via: [
        { call: 'W7RPT-1', repeated: true },
        { call: 'WIDE2-1', repeated: true },
      ],
      located: new Map([['W7RPT-1', RPT]]),
    }));
    expect(segs).toEqual([
      { kind: 'solid', from: { lat: SRC.lat, lon: SRC.lon }, to: RPT },
      { kind: 'dashed', from: RPT, to: YOU, unknownLabels: ['WIDE2-1'] },
    ]);
  });

  it('alias-only via, none located → dashed direct with all pos? labels', () => {
    const segs = resolveDigipeatPath(input({
      via: [
        { call: 'WIDE1-1', repeated: true },
        { call: 'WIDE2-1', repeated: true },
      ],
    }));
    expect(segs).toEqual([
      { kind: 'dashed', from: { lat: SRC.lat, lon: SRC.lon }, to: YOU, unknownLabels: ['WIDE1-1', 'WIDE2-1'] },
    ]);
  });

  it('non-repeated digis are ignored (not traversed)', () => {
    const segs = resolveDigipeatPath(input({
      via: [{ call: 'W7RPT-1', repeated: false }],
      located: new Map([['W7RPT-1', RPT]]),
    }));
    // W7RPT-1 did not relay → treat as direct.
    expect(segs).toEqual([{ kind: 'solid', from: { lat: SRC.lat, lon: SRC.lon }, to: YOU }]);
  });

  it('no operator → terminate at last located hop', () => {
    const segs = resolveDigipeatPath(input({
      operator: null,
      via: [{ call: 'W7RPT-1', repeated: true }],
      located: new Map([['W7RPT-1', RPT]]),
    }));
    expect(segs).toEqual([{ kind: 'solid', from: { lat: SRC.lat, lon: SRC.lon }, to: RPT }]);
  });

  it('no operator and no located downstream hop → no segments', () => {
    const segs = resolveDigipeatPath(input({ operator: null }));
    expect(segs).toEqual([]);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `pnpm vitest run src/aprs/digipeatPath.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `digipeatPath.ts`**

```ts
// src/aprs/digipeatPath.ts
//
// Pure resolution of a heard frame's digipeat path into drawable segments.
// RF-honesty: a segment is SOLID only between two hops we have a real heard
// position for; a run of hops with unknown positions (WIDEn-N aliases, unheard
// digis) is bridged by a DASHED connector carrying their callsigns as pos?
// labels — never a fabricated intermediate pin. Only digis that actually
// relayed the frame (H-bit set) count as traversed hops.

import type { ViaHop } from './aprsTypes';

export interface LatLon {
  lat: number;
  lon: number;
}

export interface PathSegment {
  kind: 'solid' | 'dashed';
  from: LatLon;
  to: LatLon;
  /// Callsigns of the unlocatable hops this dashed segment bridges (pos? markers).
  unknownLabels?: string[];
}

export interface ResolveInput {
  src: LatLon & { call: string };
  via: ViaHop[];
  /// Callsign-SSID → latest heard fix, for geolocating intermediate hops.
  located: Map<string, LatLon>;
  /// Operator's own position (grid-square centre), or null when unknown.
  operator: LatLon | null;
}

interface Hop {
  call: string;
  pos: LatLon | null;
}

export function resolveDigipeatPath(input: ResolveInput): PathSegment[] {
  const { src, via, located, operator } = input;

  // Ordered anchor list: src (always located) → traversed digis → operator.
  const hops: Hop[] = [{ call: src.call, pos: { lat: src.lat, lon: src.lon } }];
  for (const h of via) {
    if (!h.repeated) continue; // only digis that actually relayed
    hops.push({ call: h.call, pos: located.get(h.call) ?? null });
  }
  if (operator) hops.push({ call: 'YOU', pos: operator });

  // Walk located anchors; bridge runs of unlocated hops with a dashed segment.
  const segments: PathSegment[] = [];
  let lastLocated = 0; // hops[0] (src) is always located
  for (let i = 1; i < hops.length; i++) {
    if (hops[i].pos == null) continue; // defer — bridged below
    const from = hops[lastLocated].pos as LatLon;
    const to = hops[i].pos as LatLon;
    const between = hops.slice(lastLocated + 1, i).map((h) => h.call);
    segments.push(
      between.length > 0
        ? { kind: 'dashed', from, to, unknownLabels: between }
        : { kind: 'solid', from, to },
    );
    lastLocated = i;
  }
  // Trailing unlocated hops after the last located anchor are dropped: there is
  // no honest endpoint to draw to.
  return segments;
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `pnpm vitest run src/aprs/digipeatPath.test.ts`
Expected: PASS (all 7).

- [ ] **Step 5: Commit**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-cn84-aprs-animated-path
git add src/aprs/digipeatPath.ts src/aprs/digipeatPath.test.ts
git commit -m "feat(aprs): resolveDigipeatPath honest-path resolution (cn84)

Pure function: solid segments through located hops, dashed pos? connectors
across unlocatable ones, degrade to direct, terminate honestly when the
operator position is unknown. Only repeated (H-bit) digis are traversed.

Agent: mink-yew-osprey
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: Trace animation controller (pure frame computation)

**Files:**
- Create: `src/aprs/pathTrace.ts`
- Test: `src/aprs/pathTrace.test.ts`

**Interfaces:**
- Consumes: `PathSegment`, `LatLon` (Task 4).
- Produces: `pointAtProgress(segments, p): LatLon`; `computeTraceFrame(active, nowMs): TraceFrame`; types `TraceTimings`, `DEFAULT_TIMINGS`, `ActiveTrace`, `TraceFrame`, `TracePhase`.

- [ ] **Step 1: Write the failing tests**

Create `src/aprs/pathTrace.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import {
  pointAtProgress, computeTraceFrame, DEFAULT_TIMINGS, type ActiveTrace,
} from './pathTrace';
import type { PathSegment } from './digipeatPath';

const SEGS: PathSegment[] = [
  { kind: 'solid', from: { lat: 0, lon: 0 }, to: { lat: 0, lon: 10 } },
  { kind: 'solid', from: { lat: 0, lon: 10 }, to: { lat: 0, lon: 20 } },
];

describe('pointAtProgress', () => {
  it('p=0 is the start, p=1 is the end', () => {
    expect(pointAtProgress(SEGS, 0)).toEqual({ lat: 0, lon: 0 });
    expect(pointAtProgress(SEGS, 1)).toEqual({ lat: 0, lon: 20 });
  });
  it('p=0.5 is the midpoint of a two-equal-segment path', () => {
    expect(pointAtProgress(SEGS, 0.5)).toEqual({ lat: 0, lon: 10 });
  });
});

describe('computeTraceFrame (live one-shot)', () => {
  const active: ActiveTrace = { segments: SEGS, startMs: 1000, mode: 'live', timings: DEFAULT_TIMINGS };

  it('mid-draw: progress in (0,1), packet present, full opacity', () => {
    const f = computeTraceFrame(active, 1000 + DEFAULT_TIMINGS.drawMs / 2);
    expect(f.phase).toBe('drawing');
    expect(f.progress).toBeCloseTo(0.5, 2);
    expect(f.packet).not.toBeNull();
    expect(f.opacity).toBe(1);
  });

  it('linger: progress 1, packet gone, full opacity', () => {
    const f = computeTraceFrame(active, 1000 + DEFAULT_TIMINGS.drawMs + 10);
    expect(f.phase).toBe('linger');
    expect(f.progress).toBe(1);
    expect(f.packet).toBeNull();
    expect(f.opacity).toBe(1);
  });

  it('fading: opacity decreases toward 0', () => {
    const t = 1000 + DEFAULT_TIMINGS.drawMs + DEFAULT_TIMINGS.lingerMs + DEFAULT_TIMINGS.fadeMs / 2;
    const f = computeTraceFrame(active, t);
    expect(f.phase).toBe('fading');
    expect(f.opacity).toBeGreaterThan(0);
    expect(f.opacity).toBeLessThan(1);
  });

  it('after fade: idle, opacity 0', () => {
    const t = 1000 + DEFAULT_TIMINGS.drawMs + DEFAULT_TIMINGS.lingerMs + DEFAULT_TIMINGS.fadeMs + 1;
    const f = computeTraceFrame(active, t);
    expect(f.phase).toBe('idle');
    expect(f.opacity).toBe(0);
  });
});

describe('computeTraceFrame (hover hold)', () => {
  const active: ActiveTrace = { segments: SEGS, startMs: 1000, mode: 'hover', timings: DEFAULT_TIMINGS };
  it('holds at full opacity after draw (no fade while hovered)', () => {
    const f = computeTraceFrame(active, 1000 + DEFAULT_TIMINGS.drawMs + 100000);
    expect(f.phase).toBe('linger');
    expect(f.progress).toBe(1);
    expect(f.opacity).toBe(1);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `pnpm vitest run src/aprs/pathTrace.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `pathTrace.ts`**

```ts
// src/aprs/pathTrace.ts
//
// Pure timeline math for the digipeat-path trace animation. The maplibre layer
// is a thin shell that, each rAF tick, calls computeTraceFrame(active, now) and
// applies the result. Kept DOM-free + WebGL-free so it is fully unit-testable
// (jsdom has no WebGL). aprs.fi-classic feel: ~2s hop-by-hop draw, a packet dot
// rides src→you, ~2s linger, fade. Hover mode holds (no auto-fade) until cleared.

import type { LatLon, PathSegment } from './digipeatPath';

export interface TraceTimings {
  drawMs: number;
  lingerMs: number;
  fadeMs: number;
}
export const DEFAULT_TIMINGS: TraceTimings = { drawMs: 2000, lingerMs: 2000, fadeMs: 600 };

export type TraceMode = 'live' | 'hover';
export type TracePhase = 'idle' | 'drawing' | 'linger' | 'fading';

export interface ActiveTrace {
  segments: PathSegment[];
  startMs: number;
  mode: TraceMode;
  timings: TraceTimings;
}

export interface TraceFrame {
  phase: TracePhase;
  progress: number; // 0..1 draw-in along the whole polyline
  packet: LatLon | null; // riding dot during draw; null otherwise
  opacity: number; // whole-path opacity (linger=1, fades to 0)
  segments: PathSegment[];
}

/// Linear point at fractional progress `p` (0..1) along the concatenated segments.
export function pointAtProgress(segments: PathSegment[], p: number): LatLon {
  if (segments.length === 0) return { lat: 0, lon: 0 };
  const clamped = Math.max(0, Math.min(1, p));
  const target = clamped * segments.length; // equal weight per segment (hop-by-hop feel)
  const idx = Math.min(segments.length - 1, Math.floor(target));
  const frac = target - idx;
  const s = segments[idx];
  return {
    lat: s.from.lat + (s.to.lat - s.from.lat) * frac,
    lon: s.from.lon + (s.to.lon - s.from.lon) * frac,
  };
}

export function computeTraceFrame(active: ActiveTrace, nowMs: number): TraceFrame {
  const { segments, startMs, mode, timings } = active;
  const t = nowMs - startMs;
  const { drawMs, lingerMs, fadeMs } = timings;

  if (t < drawMs) {
    const progress = drawMs === 0 ? 1 : t / drawMs;
    return { phase: 'drawing', progress, packet: pointAtProgress(segments, progress), opacity: 1, segments };
  }
  // Hover: hold fully drawn until the controller clears it (mode flips/segments change).
  if (mode === 'hover') {
    return { phase: 'linger', progress: 1, packet: null, opacity: 1, segments };
  }
  const sinceDraw = t - drawMs;
  if (sinceDraw < lingerMs) {
    return { phase: 'linger', progress: 1, packet: null, opacity: 1, segments };
  }
  const sinceLinger = sinceDraw - lingerMs;
  if (sinceLinger < fadeMs) {
    return { phase: 'fading', progress: 1, packet: null, opacity: 1 - sinceLinger / fadeMs, segments };
  }
  return { phase: 'idle', progress: 1, packet: null, opacity: 0, segments };
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `pnpm vitest run src/aprs/pathTrace.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-cn84-aprs-animated-path
git add src/aprs/pathTrace.ts src/aprs/pathTrace.test.ts
git commit -m "feat(aprs): pathTrace timeline math for the digipeat trace (cn84)

pointAtProgress + computeTraceFrame: draw→linger→fade for live one-shots,
hold for hover. DOM/WebGL-free so it unit-tests without a GL context.

Agent: mink-yew-osprey
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: Render + wire into `AprsPositionsMap`

**Files:**
- Modify: `src/aprs/AprsPositionsMap.tsx` (add a `DigipeatPathLayer` child component; render it inside `<MapLibreMap>` ~line 503)
- Modify: `src/aprs/AprsPositionsMap.css` (path/dot styling if any non-paint CSS needed — most styling is maplibre paint)
- Test: `src/aprs/AprsPositionsMap.test.tsx` (extend the existing mocked-map harness)

**Interfaces:**
- Consumes: `resolveDigipeatPath` (Task 4), `computeTraceFrame`/`pointAtProgress`/`DEFAULT_TIMINGS` (Task 5), `HeardPosition.via` (Task 3), the `useMapOverlay`/`usePushData` map hooks, `gridToLatLon`.
- Produces: a `DigipeatPathLayer` that draws + animates the resolved path, triggered by pin hover and by the most-recently-arrived position.

- [ ] **Step 1: Write the failing test**

Extend `AprsPositionsMap.test.tsx`. The existing suite mocks the maplibre map object; assert that when a new position with a via-chain arrives, the path source receives a non-empty FeatureCollection (the live auto-trace started). Use the file's existing map mock + `setData` spy:

```ts
it('starts a path trace when a new position with a via-chain arrives', async () => {
  const { mapMock, rerender } = renderMapWith([]); // existing harness helper
  const positions = [{
    call: 'KE7XYZ-9', lat: 48.1, lon: -122.6, symbolTable: '/', symbolCode: '>',
    comment: '', ambiguity: 0, at: Date.now(),
    via: [{ call: 'W7RPT-1', repeated: true }],
  }];
  rerender(positions, 'CN87'); // operatorGrid known so the path has a YOU endpoint
  // advance one animation frame
  await act(async () => { mapMock.__flushRaf?.(); });
  const setData = mapMock.__sourceSetData('aprs-digipeat-path');
  expect(setData).toHaveBeenCalled();
  const lastFC = setData.mock.calls.at(-1)[0];
  expect(lastFC.features.length).toBeGreaterThan(0);
});
```

(Adapt helper names — `renderMapWith`, `mapMock.__sourceSetData`, `__flushRaf` — to whatever the existing test harness exposes. Read the top of `AprsPositionsMap.test.tsx` first and reuse its map mock; if it lacks a rAF flush, stub `requestAnimationFrame` in the test to invoke synchronously once.)

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm vitest run src/aprs/AprsPositionsMap.test.tsx`
Expected: FAIL — no `aprs-digipeat-path` source registered.

- [ ] **Step 3: Implement `DigipeatPathLayer`**

Add constants near the other source/layer ids (~line 66):

```ts
const PATH_SOURCE = 'aprs-digipeat-path';
const PATH_SOLID_LAYER = 'aprs-digipeat-path-solid';
const PATH_DASHED_LAYER = 'aprs-digipeat-path-dashed';
const PATH_DOT_SOURCE = 'aprs-digipeat-packet';
const PATH_DOT_LAYER = 'aprs-digipeat-packet-dot';

const PATH_LAYERS = ([
  {
    id: PATH_SOLID_LAYER, type: 'line', source: PATH_SOURCE,
    filter: ['==', ['get', 'kind'], 'solid'],
    layout: { 'line-cap': 'round', 'line-join': 'round' },
    paint: {
      'line-color': '#7fe6a3', 'line-width': 2.5,
      'line-opacity': ['coalesce', ['get', 'opacity'], 1],
    },
  },
  {
    id: PATH_DASHED_LAYER, type: 'line', source: PATH_SOURCE,
    filter: ['==', ['get', 'kind'], 'dashed'],
    layout: { 'line-cap': 'round', 'line-join': 'round' },
    paint: {
      'line-color': '#f0c987', 'line-width': 2, 'line-dasharray': [1.5, 1.5],
      'line-opacity': ['coalesce', ['get', 'opacity'], 1],
    },
  },
] as unknown[]).map((l) => l as Record<string, unknown> & { id: string });

const PATH_DOT_LAYERS = ([
  {
    id: PATH_DOT_LAYER, type: 'circle', source: PATH_DOT_SOURCE,
    paint: {
      'circle-radius': 4, 'circle-color': '#ffffff',
      'circle-stroke-color': '#0b1218', 'circle-stroke-width': 1,
    },
  },
] as unknown[]).map((l) => l as Record<string, unknown> & { id: string });
```

Add the component (place above `AprsPositionsMap`):

```tsx
/// Animated digipeat path (cn84). Triggers: pin hover (hold) + each newly-heard
/// position (one-shot live trace). Resolution is honest — solid through located
/// hops, dashed pos? across unknown ones (see resolveDigipeatPath).
function DigipeatPathLayer({
  positions, operator,
}: { positions: HeardPosition[]; operator: LatLon | null }) {
  const map = useMapContext();
  useMapOverlay(map, PATH_SOURCE, { type: 'geojson', data: EMPTY_FC }, PATH_LAYERS);
  useMapOverlay(map, PATH_DOT_SOURCE, { type: 'geojson', data: EMPTY_FC }, PATH_DOT_LAYERS);

  // Latest fix per callsign, for geolocating intermediate hops.
  const located = useMemo(() => {
    const m = new Map<string, LatLon>();
    for (const p of positions) m.set(p.call, { lat: p.lat, lon: p.lon });
    return m;
  }, [positions]);
  const locatedRef = useRef(located);
  locatedRef.current = located;
  const byCall = useMemo(() => {
    const m = new Map<string, HeardPosition>();
    for (const p of positions) m.set(p.call, p);
    return m;
  }, [positions]);
  const byCallRef = useRef(byCall);
  byCallRef.current = byCall;
  const operatorRef = useRef(operator);
  operatorRef.current = operator;

  const activeRef = useRef<ActiveTrace | null>(null);

  // Build the resolved segments for a station, or null if nothing to draw.
  const segmentsFor = (call: string): PathSegment[] | null => {
    const p = byCallRef.current.get(call);
    if (!p) return null;
    const segs = resolveDigipeatPath({
      src: { call: p.call, lat: p.lat, lon: p.lon },
      via: p.via,
      located: locatedRef.current,
      operator: operatorRef.current,
    });
    return segs.length ? segs : null;
  };

  const startTrace = (call: string, mode: TraceMode) => {
    const segments = segmentsFor(call);
    if (!segments) return;
    activeRef.current = { segments, startMs: performance.now(), mode, timings: DEFAULT_TIMINGS };
  };

  // rAF loop: push the current frame's geometry+opacity to the path + dot sources.
  useEffect(() => {
    if (!map) return;
    let raf = 0;
    const setSrc = (id: string, fc: FeatureCollection) => {
      const s = map.getSource(id) as { setData?: (d: unknown) => void } | undefined;
      s?.setData?.(fc);
    };
    const loop = () => {
      const active = activeRef.current;
      if (active) {
        const f = computeTraceFrame(active, performance.now());
        if (f.phase === 'idle' && active.mode === 'live') {
          activeRef.current = null;
          setSrc(PATH_SOURCE, EMPTY_FC);
          setSrc(PATH_DOT_SOURCE, EMPTY_FC);
        } else {
          setSrc(PATH_SOURCE, pathFC(f.segments, f.progress, f.opacity));
          setSrc(PATH_DOT_SOURCE, f.packet ? pointFC(f.packet) : EMPTY_FC);
        }
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, [map]);

  // Hover trigger on the pin layer.
  useEffect(() => {
    if (!map) return;
    const enter = (e: { features?: Array<{ properties?: { call?: unknown } }> }) => {
      const call = e.features?.[0]?.properties?.call;
      if (call != null) startTrace(String(call), 'hover');
    };
    const leave = () => {
      if (activeRef.current?.mode === 'hover') {
        activeRef.current = null;
        const s1 = map.getSource(PATH_SOURCE) as { setData?: (d: unknown) => void } | undefined;
        const s2 = map.getSource(PATH_DOT_SOURCE) as { setData?: (d: unknown) => void } | undefined;
        s1?.setData?.(EMPTY_FC); s2?.setData?.(EMPTY_FC);
      }
    };
    map.on('mouseenter', POSITION_PINS_COLOR_LAYER, enter as (...a: unknown[]) => void);
    map.on('mouseleave', POSITION_PINS_COLOR_LAYER, leave as (...a: unknown[]) => void);
    return () => {
      map.off('mouseenter', POSITION_PINS_COLOR_LAYER, enter as (...a: unknown[]) => void);
      map.off('mouseleave', POSITION_PINS_COLOR_LAYER, leave as (...a: unknown[]) => void);
    };
  }, [map]);

  // Live trigger: when the newest position changes, auto-trace it once. Hover
  // (if active) takes precedence and is not interrupted.
  const newest = positions.length ? positions.reduce((a, b) => (b.at > a.at ? b : a)) : null;
  const newestKey = newest ? `${newest.call}:${newest.at}` : '';
  useEffect(() => {
    if (!newest) return;
    if (activeRef.current?.mode === 'hover') return;
    startTrace(newest.call, 'live');
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [newestKey]);

  return null;
}
```

Add the two FC helpers near `buildPositionFC`:

```ts
/// One LineString feature per resolved segment, each carrying its kind + the
/// path opacity. Progress trims the polyline to the drawn fraction (draw-in).
function pathFC(segments: PathSegment[], progress: number, opacity: number): FeatureCollection {
  const drawnEnd = pointAtProgress(segments, progress);
  const total = segments.length;
  const features: unknown[] = [];
  segments.forEach((s, i) => {
    const segStart = i / total;
    const segEnd = (i + 1) / total;
    if (progress <= segStart) return; // not reached yet
    const to = progress >= segEnd ? s.to : drawnEnd; // partially drawn last segment
    features.push({
      type: 'Feature',
      properties: { kind: s.kind, opacity },
      geometry: { type: 'LineString', coordinates: [[s.from.lon, s.from.lat], [to.lon, to.lat]] },
    });
  });
  return { type: 'FeatureCollection', features };
}

function pointFC(p: LatLon): FeatureCollection {
  return {
    type: 'FeatureCollection',
    features: [{ type: 'Feature', properties: {}, geometry: { type: 'Point', coordinates: [p.lon, p.lat] } }],
  };
}
```

Add imports at the top of the file:

```ts
import { resolveDigipeatPath, type LatLon, type PathSegment } from './digipeatPath';
import { computeTraceFrame, pointAtProgress, DEFAULT_TIMINGS, type ActiveTrace, type TraceMode } from './pathTrace';
```

Render it inside `<MapLibreMap>` (~line 503), passing the operator latLon already computed as `me`:

```tsx
        <PositionLayers positions={positions} />
        <DigipeatPathLayer positions={positions} operator={me} />
        <OperatorPin location={me} />
```

- [ ] **Step 4: Run the test + typecheck**

Run: `pnpm vitest run src/aprs/AprsPositionsMap.test.tsx && pnpm typecheck`
Expected: PASS + clean typecheck. (If the existing map mock lacks `mouseenter`/`mouseleave` or `requestAnimationFrame`, stub them in the test harness; the production `map.on('mouseenter', …)` is standard maplibre.)

- [ ] **Step 5: Commit**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-cn84-aprs-animated-path
git add src/aprs/AprsPositionsMap.tsx src/aprs/AprsPositionsMap.test.tsx src/aprs/AprsPositionsMap.css
git commit -m "feat(aprs): animated digipeat path layer on the Tac Chat map (cn84)

DigipeatPathLayer draws the honest resolved path (solid green / dashed
amber) with a riding packet dot, triggered by pin hover and one-shot per
newly-heard frame. rAF loop applies computeTraceFrame to maplibre line +
circle sources. aprs.fi-classic feel.

Agent: mink-yew-osprey
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: Full-suite gate + wire-walk

- [ ] **Step 1: Run the full frontend suite + typecheck + build**

Run: `pnpm vitest run && pnpm typecheck && pnpm build`
Expected: all green. Fix any regressions before proceeding.

- [ ] **Step 2: Push the branch and open a draft PR (CI compiles the Rust)**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-cn84-aprs-animated-path
git push -u origin bd-tuxlink-cn84/aprs-animated-path
gh pr create --draft --base main --head bd-tuxlink-cn84/aprs-animated-path \
  --title '[mink-yew-osprey] feat(aprs): animated digipeat path on hover (cn84)' \
  --body 'Implements tuxlink-cn84 per docs/superpowers/specs/2026-06-19-aprs-animated-digipeat-path-design.md. Backend surfaces the AX.25 via-chain + H-bit; frontend resolves an honest path and animates it aprs.fi-style on hover + live receive.'
```

- [ ] **Step 3: Wire-walk gate (run the `wire-walk` skill; operator supplies flows greenfield)**

Trace these flows verbatim to `file:line`. Any broken primary flow = NOT shipped:
1. Heard station pin hovered → `resolveDigipeatPath` → `DigipeatPathLayer` hover handler → path animates to YOU.
2. New `aprs-position:new` → `useAprsPositions` → `positions` prop → `DigipeatPathLayer` live trigger → one-shot trace + fade.
3. Station heard only via `WIDE` aliases → `resolveDigipeatPath` dashed-direct branch → dashed connector + pos? label, no false-exact pin.

- [ ] **Step 4: Mark PR ready + close the issue when CI is green and wire-walk passes**

```bash
gh pr ready <#>
bd close tuxlink-cn84
```

---

## Notes for the executor

- **Backend tests may not finish locally on this Pi** — push and let CI compile/run the Rust (Tasks 1–2). Frontend vitest runs locally fine.
- **maplibre under jsdom has no WebGL** — that is why Tasks 4–5 hold all the logic and are exhaustively unit-tested; Task 6's component test only checks the wiring (source receives data), and the real render is verified by the wire-walk + an operator smoke on a converged build.
- **The packet-dot draw-in** is approximate (equal weight per segment, not geographic length) — intentional for the hop-by-hop feel; do not gold-plate to arc-length unless the operator asks.
