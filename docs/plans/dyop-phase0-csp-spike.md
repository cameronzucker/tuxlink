# dyop Phase 0 — Serving-mechanism CSP spike (decision record)

> **Status:** DECIDED — custom `tile` URI scheme; Linux production token `tile:`.
> Backed by packaged-build WebKitGTK evidence (positive Build A + negative Build B).
> This document is the sole surviving artifact of Phase 0. All spike scaffolding
> (URI-scheme handler, `spike_fetch_tile`/`spike_report` commands, harness page, CSP
> edits) is reverted — real wiring lands in Phase 6.

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

Build command: `pnpm -C <worktree> tauri build --no-bundle` (arm64 Pi; clean build
~25 min; the binary alone enforces the CSP, so `--no-bundle` skips .deb/AppImage).

**Verdict-read method (corrected during the spike):** the harness sets
`document.title`, BUT **Tauri does NOT propagate `document.title` to the OS/Wayland
window title** — `wlrctl toplevel list` showed `tuxlink: tuxlink` unchanged after
the harness ran. The reliable channel is a `spike_report(result)` Tauri command the
harness invokes once all probes settle; it `println!`s `TILE-RESULT:<summary>` to
the packaged binary's stdout, which the runner greps. A `grim` screenshot (the
harness also paints the verdict into the page body) is the corroborating signal.

**Three probes** (to disambiguate the exact Linux token):
- `tileScheme` = `tile://localhost/0/0/0` — Tauri's **Linux** custom-protocol URL form.
- `tileHttp` = `http://tile.localhost/0/0/0` — Tauri's **Windows/Android** form
  (design §8.2 stated this for Linux; the spike proves it is the Windows form).
- `blob` = `invoke('spike_fetch_tile')` → `URL.createObjectURL(Blob(...))`.

### Build A — POSITIVE control (all candidate tokens present)

Spike CSP for this build (`img-src` gains `tile: http://tile.localhost blob:`):

```
default-src 'self'; connect-src 'self' http://127.0.0.1:*; img-src 'self' data: tile: http://tile.localhost blob:; style-src 'self' 'unsafe-inline'
```

Build: exit 0, binary at `src-tauri/target/release/tuxlink` (verified the CSP
string is baked in via `strings`). Launched on the labwc/WebKitGTK rig with
`WAYLAND_DISPLAY=wayland-0`.

**stdout verdict:** `TILE-RESULT:tileScheme=LOADED|tileHttp=BLOCKED|blob=LOADED`
**grim corroboration:** page body showed `tileScheme=LOADED tileHttp=BLOCKED blob=LOADED`
(screenshot `dev/scratch/spike-buildA-positive.png`).

| Candidate (URL form) | Verdict | Read via |
|---|---|---|
| `tile` scheme — `tile://localhost/0/0/0` (Linux form) | **LOADED** | stdout `spike_report` + grim |
| `tile` scheme — `http://tile.localhost/0/0/0` (Windows form) | **BLOCKED** | stdout `spike_report` + grim |
| `invoke`+`blob:` — `blob:...` | **LOADED** | stdout `spike_report` + grim |

**Finding:** On Linux/WebKitGTK the working `tile` URL is `tile://localhost/...`
(scheme form), served by the async URI-scheme handler. The `http://tile.localhost`
form is BLOCKED/unresolvable on Linux — it is the **Windows/Android** form, not the
Linux form. The design §8.2 wording ("on Linux/WebKitGTK this resolves to
`http://tile.localhost`") is **incorrect for Linux**; Tauri 2.11.2's own docs
(`Builder::register_asynchronous_uri_scheme_protocol`) confirm: macOS/iOS/Linux use
`<scheme>://localhost/<path>`, Windows/Android use `http://<scheme>.localhost/<path>`.

### Build B — NEGATIVE control (production CSP, all tokens removed)

CSP for this build = the exact production CSP (no `tile:`, no `http://tile.localhost`,
no `blob:` in img-src):

```
default-src 'self'; connect-src 'self' http://127.0.0.1:*; img-src 'self' data:; style-src 'self' 'unsafe-inline'
```

Build: exit 0, binary at `src-tauri/target/release/tuxlink` (verified via `strings`
that the baked CSP is `img-src 'self' data:; style-src ...` with **zero** spike
tokens). Same harness, same handlers. Launched on the rig.

**stdout verdict:** `TILE-RESULT:tileScheme=BLOCKED|tileHttp=BLOCKED|blob=BLOCKED`

| Candidate (URL form) | Verdict | Read via |
|---|---|---|
| `tile` scheme — `tile://localhost/0/0/0` | **BLOCKED** | stdout `spike_report` |
| `tile` scheme — `http://tile.localhost/0/0/0` | **BLOCKED** | stdout `spike_report` |
| `invoke`+`blob:` | **BLOCKED** | stdout `spike_report` |

All three sources BLOCKED under the production CSP — proving the `tile:` and `blob:`
tokens are **load-bearing**, and the Build-A positives were genuinely CSP-gated, not
artifacts of a permissive default. (Note: the URI-scheme handler still runs and the
`spike_fetch_tile` IPC still succeeds — the bytes arrive — but WebKitGTK refuses to
let the `<img>` load the resulting source because img-src lacks the token. CSP gates
the *rendering*, not the byte delivery.)

### Exact CSP token WebKitGTK requires on Linux

- `tile` scheme candidate (Linux): **`tile:`** — the scheme token alone is sufficient
  (Build A loaded `tile://localhost/...` with `tile:` in img-src; Build B blocked it
  without). The `http://tile.localhost` host token is **not** needed on Linux (that
  is the Windows/Android form and was BLOCKED on Linux even when present in Build A).
- `blob:` candidate: **`blob:`** — sufficient and necessary (Build A loaded, Build B
  blocked). `connect-src` is untouched: the bytes arrive over the Tauri IPC bridge,
  which is not CSP-governed (confirmed — `spike_fetch_tile` returned bytes in BOTH
  builds; only the `<img blob:>` render was gated).

## Decision

**Chosen mechanism: custom `tile` URI scheme (candidate a).**

**Exact production CSP token Phase 6 adds to `img-src` (Linux):** `tile:`

Resulting Phase-6 production CSP (Linux):

```
default-src 'self'; connect-src 'self' http://127.0.0.1:*; img-src 'self' data: tile:; style-src 'self' 'unsafe-inline'
```

(If tuxlink ever ships a Windows build, that target additionally needs
`http://tile.localhost` in img-src — the Windows custom-protocol URL form. tuxlink is
Linux-first; the Linux token is `tile:`.)

### Rationale against the criteria

| Criterion | `tile` scheme | `invoke`+`blob:` |
|---|---|---|
| (a) packaged render | PASS (Build A LOADED) | PASS (Build A LOADED) |
| (b) no network/LAN host in img-src/connect-src | PASS (`tile:` is a Tauri-local scheme; no host) | PASS (`blob:`; no host) |
| (c) no pan/zoom leak | **PASS — structurally leak-free** (no per-tile JS allocation; the `<img src="tile://…">` is a plain Leaflet `TileLayer` request handled in Rust) | CONDITIONAL — requires `revokeObjectURL` on every `tileunload`; un-revoked object URLs are a Pi-class OOM (design §8.2) |
| (d) complexity | **Lower** — one async URI-scheme handler in Rust + a stock Leaflet `TileLayer` (`subdomains: []`) | Higher — a custom `GridLayer.createTile` + mandatory revocation lifecycle + a leak-assertion test (revocation-count == eviction-count) |

**Tie-breaker application:** the plan's tie-breaker prefers the smaller / more-portable
CSP delta with no platform-specific token, UNLESS its leak/complexity cost is judged
worse for a Pi. `blob:` is the more-portable token (no Linux/Windows URL-form split),
but its mandatory object-URL revocation is exactly the Pi-OOM hazard the exception
names. The `tile` scheme is structurally leak-free and uses Leaflet's stock
`TileLayer`. The exception therefore fires: **`tile` scheme wins** despite the
platform-specific URL form. The platform split is a one-line CSP concern (add
`http://tile.localhost` only on a future Windows target), not a runtime-correctness
hazard.

### Decision = honest binding guarantee preserved

Either candidate honors §8.2's guarantee — **no network/LAN host is ever added to
`img-src`/`connect-src`; the webview reaches tiles only through a Tauri-local
source.** With `tile:`, the webview's only tile source is the in-process async
URI-scheme handler; the SSRF-guarded gatekeeper (§8.3) remains the sole network
egress. Loopback-HTTP serving stays FORBIDDEN.

## Notes for Phase 6 (the chosen `tile`-scheme path)

- **Production CSP delta:** add exactly `tile:` to `img-src` (Linux). Do NOT add
  `http://tile.localhost` on Linux — it is the Windows form and was BLOCKED on Linux.
  Do NOT add any host to `img-src`/`connect-src`. Do NOT widen the forms-scoped
  `connect-src http://127.0.0.1:*`.
- **Register the bespoke `tile` scheme ONLY** via
  `register_asynchronous_uri_scheme_protocol("tile", …)` (Tauri 2.11.2; verified
  signature: `Fn(UriSchemeContext, http::Request<Vec<u8>>, UriSchemeResponder)`).
  NEVER register the general asset protocol.
- **Leaflet layer:** a stock `TileLayer` with the template `tile://localhost/{z}/{x}/{y}`
  (Linux) and `subdomains: []` (no `{s}` rotation). No custom `GridLayer`, no
  object-URL lifecycle, no `revokeObjectURL` — the scheme path has no per-tile JS
  allocation to leak.
- **The handler is the gatekeeper boundary:** in Phase 6 the `tile` handler validates
  integer `{z}/{x}/{y}` against the stored permitted source and delegates to the
  SSRF-guarded fetch (§8.3). It never accepts a caller-supplied full URL.
- **URL-form portability:** the `tile://localhost` (Linux) vs `http://tile.localhost`
  (Windows) split is a Tauri platform behavior, not a tuxlink choice. The Leaflet
  template and the CSP token must both be selected per target platform if tuxlink ever
  ships on Windows. Linux: `tile://localhost/...` + `tile:`.
- **IPC is NOT CSP-governed** (observed): `spike_fetch_tile` returned bytes in BOTH
  builds; only the `<img>` render was gated. This is why a future `invoke`-based
  fallback (if ever needed) would not touch `connect-src` — but the chosen path does
  not use `invoke` for tile bytes at all.
