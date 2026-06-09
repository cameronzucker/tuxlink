# dyop Phase 0 — Serving-mechanism CSP spike (decision record)

> **Status:** DECISION PENDING until the packaged-build evidence below is filled in.
> This document is the sole surviving artifact of Phase 0. All spike scaffolding
> (URI-scheme handler, `spike_fetch_tile` command, harness page, CSP edits) is
> reverted after the decision is recorded — real wiring lands in Phase 6.

Spike agent: `marten-poplar-dahlia`. Branch: `bd-tuxlink-dyop/dyop-lan-tiles`.
bd issue: `tuxlink-dyop`. Design source: `docs/design/2026-06-08-map-picker-v2-design.md` §8.2.

## Why this spike exists

Phase 0 pins the **tile-serving mechanism** before any serving/frontend code is
written, because the two viable mechanisms differ in the CSP token they force and
in their leak/complexity profile, and the cross-provider design review split on
which to choose. The claim "CSP stays `'self'`" is false: **every** viable
mechanism adds one token to `img-src`. The honest, binding guarantee Phase 0
preserves is:

> **No network/LAN host is ever added to `img-src` or `connect-src`; the webview
> reaches tiles only through a Tauri-local source, and the gatekeeper remains the
> sole network egress for tiles.**

The decision MUST be backed by **packaged-build** WebKitGTK CSP evidence, not dev
mode. `tauri dev` injects a permissive CSP and proves nothing; only the packaged
artifact enforces the CSP from `tauri.conf.json`.

## Exact current production CSP

From `src-tauri/tauri.conf.json` (`app.security.csp`):

```
default-src 'self'; connect-src 'self' http://127.0.0.1:*; img-src 'self' data:; style-src 'self' 'unsafe-inline'
```

The `connect-src http://127.0.0.1:*` token is **forms-scoped** (the per-open HTML-
forms ephemeral HTTP server). It MUST NOT be relied on or widened for tiles.

## Candidates (verbatim from design §8.2)

- **(a) custom `tile` URI scheme** behind a Leaflet `TileLayer`: on Linux/WebKitGTK
  this resolves to `http://tile.localhost`, so `img-src` gains
  `tile: http://tile.localhost`; requires an async URI-scheme handler and
  `subdomains: []` (no `{s}` rotation). Bespoke `tile` scheme ONLY — never the
  general asset protocol.
- **(b) `invoke` returning tile bytes** → `blob:` object URLs via a custom
  `GridLayer`: `img-src` gains only `blob:`; requires `revokeObjectURL` on Leaflet
  `tileunload` as first-class behavior (un-revoked blobs are a Pi-class OOM) with a
  leak-assertion test.

**FORBIDDEN:** a loopback-HTTP tile server. It would require
`img-src http://127.0.0.1:*`, turning any webview script into a localhost-port
probe; the existing forms-scoped `connect-src http://127.0.0.1:*` must NOT be
relied on or widened. This option is out of scope for the decision below — it is
not a candidate, it is a non-starter.

## Decision criteria

A candidate is acceptable only if ALL of (a)–(c) hold; (d) is the tie-breaker.

| # | Criterion | How measured |
|---|---|---|
| (a) | Tiles render in a **packaged** WebKitGTK build | Harness `<img>` `onload` fires under the packaged binary on the rig (labwc + WebKitGTK). Read deterministically via `document.title` → `wlrctl toplevel list`. |
| (b) | `img-src` / `connect-src` list **NO** network/LAN host | The only added token is a Tauri-local scheme token (`tile:`/`http://tile.localhost` or `blob:`). No `http://<ip>`, no `http://127.0.0.1:*` for tiles, no public host. |
| (c) | No memory leak under pan/zoom | (a)→ object-URL candidate requires `revokeObjectURL` on `tileunload`; (b)→ scheme candidate has no per-tile JS allocation to leak. Empirically: scheme = structurally leak-free; blob = leak-prone-without-discipline. |
| (d) | Implementation complexity (tie-breaker) | Lines of Rust + JS, platform-specific tokens, lifecycle obligations. |

**Tie-breaker rule (from the plan):** prefer the smaller / more-portable CSP delta
with no platform-specific token UNLESS its leak/complexity cost (object-URL
revocation) is judged worse for a Pi.

## Packaged-build evidence

Build command: `pnpm -C <worktree> tauri build` (arm64 Pi; ~15–25 min/build expected).
Render verdict read via the harness setting `document.title` to
`TILE-RESULT:tile=<LOADED|BLOCKED>|blob=<LOADED|BLOCKED>` from each `<img>`'s
`onload`/`onerror`, then `wlrctl toplevel list` to read the title off the packaged
window. `grim` screenshot is a backup signal.

### Build 1 — POSITIVE control (both candidate tokens present)

Spike CSP for this build (`img-src` gains `tile: http://tile.localhost blob:`):

```
default-src 'self'; connect-src 'self' http://127.0.0.1:*; img-src 'self' data: tile: http://tile.localhost blob:; style-src 'self' 'unsafe-inline'
```

Harness loads two images:
- `tile` candidate: `<img src="http://tile.localhost/0/0/0">` (Linux form) — served by the `tile` async URI-scheme handler.
- `blob` candidate: `invoke('spike_fetch_tile')` → `URL.createObjectURL(Blob([bytes],{type:'image/png'}))` → `<img src="blob:...">`.

| Candidate | Verdict | Read via |
|---|---|---|
| `tile` scheme (`http://tile.localhost/0/0/0`) | _PENDING_ | |
| `invoke`+`blob:` | _PENDING_ | |

Window title observed: `_PENDING_`

### Build 2 — NEGATIVE control (production CSP, both tokens removed)

CSP for this build = the exact production CSP (no `tile:`, no `blob:` in img-src):

```
default-src 'self'; connect-src 'self' http://127.0.0.1:*; img-src 'self' data:; style-src 'self' 'unsafe-inline'
```

Same harness. Both images MUST be BLOCKED (`onerror`), proving the tokens are
load-bearing — i.e. the CSP genuinely gates these sources and the positive result
in Build 1 was not an artifact of a permissive default.

| Candidate | Verdict | Read via |
|---|---|---|
| `tile` scheme | _PENDING (expect BLOCKED)_ | |
| `invoke`+`blob:` | _PENDING (expect BLOCKED)_ | |

Window title observed: `_PENDING_`

### Exact CSP token WebKitGTK requires on Linux

_PENDING — recorded after Build 1: whether the working token for the scheme
candidate is `tile:`, `http://tile.localhost`, or both required together._

## Decision

_PENDING — filled after evidence. States: chosen mechanism, the EXACT single
production CSP token Phase 6 will add to `img-src`, and the rationale against the
criteria + tie-breaker._

## Notes for Phase 6

- If `invoke`+`blob:` is chosen: the `GridLayer.createTile` MUST call
  `URL.revokeObjectURL` on `tileunload`; a leak-assertion test (revocation-count ==
  eviction-count) is mandatory (design §8.2; un-revoked blobs are a Pi-class OOM).
- If `tile` scheme is chosen: register the bespoke `tile` scheme ONLY (never the
  general asset protocol); Leaflet `TileLayer` with `subdomains: []`.
- IPC (`invoke`) is NOT CSP-governed: `connect-src` is untouched by the blob
  candidate (the bytes arrive over the Tauri IPC bridge, not an HTTP fetch).
