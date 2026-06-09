# tuxlink-dyop — LAN tile-server ingest Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an opt-in LAN HTTP tile source that gives the map picker higher-zoom precision, fetched through a Rust gatekeeper so the webview never touches the network and CSP never lists a network host.

**Architecture:** A Rust `tiles` module owns all egress: it validates the operator-configured source, fetches tiles only from a resolved private-LAN IP (no redirects, integer-validated `{z}/{x}/{y}`, size/content-type capped), caches them traversal-safely under `app_data_dir`, and serves them to the webview through a Tauri-local mechanism pinned by an up-front WebKitGTK CSP spike. `BaseMap` stays `EPSG:4326`; the source must serve geodetic tiles and is rejected on CRS mismatch. The feature is strictly opt-in and the app is fully functional offline without it.

**Tech Stack:** Rust (Tauri 2.11.2, reqwest 0.12, tokio), React + Leaflet (react-leaflet), WebKitGTK production webview.

**Spec:** `docs/design/2026-06-08-map-picker-v2-design.md` §8.1–8.9 (adversarially hardened, 5 rounds). This plan encodes those resolutions; where the spec and this plan disagree, the spec wins and this plan gets a corrective edit.

---

## Pre-flight (executor reads first)

- **Worktree:** Implement in a NEW worktree off `main` (after design PR #495 merges), claimed by `tuxlink-dyop`:
  `python3 .claude/scripts/new_tuxlink_worktree.py --slug dyop-lan-tiles --issue tuxlink-dyop --base main --moniker <your-moniker>`.
  Run `pnpm install` and `bash scripts/install-githooks.sh` in the new worktree first.
- **Moniker:** you are agent `<your-moniker>`; put `Agent: <moniker>` in every commit trailer. (The drafting session was `shoal-magnolia-fjord`.)
- **RADIO-1:** this feature touches NO RF/transmit path. Do not let any task run a transmit binary.
- **CSP invariant (every task):** no task may add a network or LAN host to `img-src`/`connect-src`. The only permitted CSP delta is the one local-scheme token the Phase 0 spike selects. Public OSM is never a permitted source.
- **TDD (every task):** before writing code, read `.claude/skills/test-driven-development/` (or invoke `/test-driven-development`) and `docs/pitfalls/testing-pitfalls.md`. Failing test → minimal impl → green. Before marking a task complete: re-check tests against `docs/pitfalls/testing-pitfalls.md`, verify error/edge paths are tested, run the relevant test subset green.
- **Review loop:** after each Phase, do a minimum of three review rounds from multiple perspectives; if the third still finds substantive issues, keep going. Update your journal, then continue.
- **Reuse pointers (read before Phase 1):** `src-tauri/src/forms/updater.rs` (`classify_transport` :162, reqwest client :189, `HTTP_TIMEOUT`/`MAX_ARCHIVE_BYTES` + `content_length` pre-check + streaming abort :398-435, `is_safe_version` :147, `INSTALL_LOCK` serialization), `src-tauri/src/config.rs` (`write_config_atomic`), `src-tauri/src/logging/state_dir.rs` (canonical-path / symlink-refusal posture), `app_data_dir()` usage (cache lives at `~/.local/share/tuxlink/tile-cache/`).

## File structure

| File | Responsibility |
|---|---|
| `src-tauri/src/tiles/mod.rs` | Module root; re-exports; the `TileSource` config type + the `TileGatekeeper` managed state. |
| `src-tauri/src/tiles/host.rs` | `classify_tile_host` — URL parse + scheme/credential rules + the resolved-IP allow/deny policy. Pure, fully unit-tested. |
| `src-tauri/src/tiles/coord.rs` | `TileCoord` — parse/validate `{z}/{x}/{y}` as bounded integers; TMS y-flip; cache-relative path. Pure. |
| `src-tauri/src/tiles/fetch.rs` | The reqwest client (resolved-IP pinned, `redirect::none`, timeout, size cap, content-type/magic check) + single-flight. |
| `src-tauri/src/tiles/cache.rs` | On-disk cache: per-source hash namespace, byte-cap + LRU, atomic temp+rename, clear/purge. |
| `src-tauri/src/tiles/crs.rs` | Source metadata probe (TileJSON/WMTS/mbtiles) + `require_geodetic` guard + alignment fixture helpers. |
| `src-tauri/src/tiles/serve.rs` | The serving mechanism chosen in Phase 0 (custom URI scheme OR `fetch_tile` command). |
| `src-tauri/src/tiles/commands.rs` | Tauri `#[command]`s: configure/clear/test source, `tile_source_status`. |
| `src/map/tileSource.ts` | TS types + invoke wrappers mirroring the Rust `TileSource` + status. |
| `src/map/TileLayerBridge.tsx` | The Leaflet layer that targets the chosen serving mechanism (Phase 0). |
| `src/map/BaseMap.tsx` (modify) | Accept a tile source; raise `maxZoom` only on validated tiles; render tiles over the raster backstop. |
| `src/map/MaidenheadOverlay.tsx` (modify) | Re-tune `levelFromZoom` for the full zoom range. |
| `src/settings/MapTileSourceSettings.tsx` | Settings UI for the source config (URL, CRS, XYZ/TMS, zoom, cache budget, attribution, label). |
| `docs/pitfalls/implementation-pitfalls.md` (modify) | New entry: tile-coordinate path-traversal (the filesystem twin of the SSRF entry). |

---

## Phase 0 — Serving-mechanism CSP spike (GATE: blocks all later phases)

The cross-provider review split on custom `tile` scheme vs `invoke`+`blob:`. This phase decides it against the **packaged** (not dev) CSP and pins the answer with a real WebKitGTK test. Do NOT start Phase 6/7 wiring until this is resolved.

### Task 0.1: Document the two candidates and the decision criteria

**Files:**
- Create: `docs/plans/dyop-phase0-csp-spike.md`

- [ ] **Step 1: Write the spike doc** capturing both candidates verbatim from §8.2, the exact current CSP (`src-tauri/tauri.conf.json`: `default-src 'self'; connect-src 'self' http://127.0.0.1:*; img-src 'self' data:; style-src 'self' 'unsafe-inline'`), and the decision criteria: (a) tiles render in a packaged WebKitGTK build, (b) `img-src`/`connect-src` list NO network/LAN host, (c) no memory leak under pan/zoom, (d) implementation complexity. Mark loopback-HTTP serving as FORBIDDEN (would need `img-src http://127.0.0.1:*`).
- [ ] **Step 2: Commit** `docs: dyop phase-0 CSP spike plan`.

### Task 0.2: Build a minimal custom-`tile`-scheme probe

**Files:**
- Modify: `src-tauri/src/lib.rs` (register an async URI-scheme handler `tile` returning a 1×1 PNG)
- Modify: `src-tauri/tauri.conf.json` (a SPIKE-ONLY branch adding `tile: http://tile.localhost` to `img-src`)

- [ ] **Step 1:** Register `tile` via `register_asynchronous_uri_scheme_protocol`, returning a hardcoded 1×1 transparent PNG with `Content-Type: image/png` for any path.
- [ ] **Step 2:** Add a throwaway `<img src="tile://localhost/0/0/0">` (Linux: `http://tile.localhost/0/0/0`) to a dev harness page; build a **packaged** app (`pnpm tauri build` or the converge-build) and confirm the image loads with the spike CSP and 404s/blocks without the `tile:` token. Record the exact CSP token WebKitGTK requires on Linux.

### Task 0.3: Build a minimal `invoke`+`blob:` probe

**Files:**
- Modify: `src-tauri/src/lib.rs` (a `spike_fetch_tile` command returning `Vec<u8>` of the 1×1 PNG)
- Modify: `src-tauri/tauri.conf.json` (SPIKE-ONLY: add `blob:` to `img-src`)

- [ ] **Step 1:** Implement `spike_fetch_tile` → bytes; in the harness, `invoke` it, `URL.createObjectURL(new Blob([bytes],{type:'image/png'}))`, set as `<img>`; verify it renders packaged with `blob:` in `img-src` and blocks without it.
- [ ] **Step 2:** Confirm `connect-src` is untouched (IPC is not CSP-governed).

### Task 0.4: Decide, record, and REVERT the spike scaffolding

- [ ] **Step 1:** Pick the mechanism. Default tie-breaker if both pass cleanly: prefer the mechanism with the smaller, more portable CSP delta and no platform-specific token UNLESS its leak/complexity cost (object-URL revocation) is judged worse for a Pi. Record the decision + evidence in `dyop-phase0-csp-spike.md`.
- [ ] **Step 2:** Revert ALL spike scaffolding (handlers, harness page, CSP edits) — the real wiring lands in Phase 6 with the production CSP delta. Commit `chore: revert dyop CSP spike scaffolding; decision recorded`.

> The remaining phases say "the serving mechanism" abstractly; Phase 6 instantiates the Phase-0 decision. Phases 1–5 (the Rust gatekeeper core) are independent of the serving choice and can proceed in parallel with the spike if needed.

---

## Phase 1 — `classify_tile_host` (SSRF host policy)

Model on `classify_transport` ([updater.rs:162](src-tauri/src/forms/updater.rs#L162)) but STRICTER: this is a destination-choice primitive, so it must do resolved-IP checks, not just scheme×loopback. Per §8.3.

### Task 1.1: URL-shape validation (scheme, credentials, host present)

**Files:**
- Create: `src-tauri/src/tiles/host.rs`
- Modify: `src-tauri/src/tiles/mod.rs` (`mod host;`)
- Test: inline `#[cfg(test)]` in `host.rs`

- [ ] **Step 1: Write the failing tests.**

```rust
#[test]
fn rejects_non_http_schemes() {
    assert!(validate_source_url("file:///etc/passwd").is_err());
    assert!(validate_source_url("gopher://x/").is_err());
    assert!(validate_source_url("ftp://x/").is_err());
}
#[test]
fn rejects_embedded_credentials() {
    assert!(validate_source_url("http://user:pass@192.168.1.5:8080/").is_err());
}
#[test]
fn accepts_plain_http_and_https_with_host() {
    assert!(validate_source_url("http://192.168.1.5:8080/tiles/").is_ok());
    assert!(validate_source_url("https://tiles.lan/").is_ok());
}
#[test]
fn rejects_missing_host() {
    assert!(validate_source_url("http:///x").is_err());
}
```

- [ ] **Step 2: Run, verify fail** (`cargo test -p tuxlink --lib tiles::host` — "function not found").
- [ ] **Step 3: Implement** `validate_source_url(&str) -> Result<reqwest::Url, String>`: parse; require scheme `http`|`https`; reject if `url.username() != "" || url.password().is_some()`; require `url.host_str().is_some()`.
- [ ] **Step 4: Run, verify pass.**
- [ ] **Step 5: Commit** `feat(tiles): validate tile source URL shape (scheme/creds/host)`.

### Task 1.2: Resolved-IP allow/deny policy

**Files:**
- Modify: `src-tauri/src/tiles/host.rs`

- [ ] **Step 1: Write the failing tests** over a pure `ip_is_permitted(IpAddr, allow_loopback: bool) -> bool`:

```rust
use std::net::IpAddr;
fn ip(s: &str) -> IpAddr { s.parse().unwrap() }
#[test]
fn allows_rfc1918_and_ula() {
    for s in ["10.0.0.1","172.16.5.4","192.168.1.50","fd00::1"] {
        assert!(ip_is_permitted(ip(s), false), "{s} should be permitted");
    }
}
#[test]
fn denies_public_loopback_linklocal_metadata_multicast_unspecified() {
    for s in ["8.8.8.8","1.1.1.1","127.0.0.1","::1","169.254.169.254",
              "169.254.1.1","fe80::1","224.0.0.1","0.0.0.0","::","::ffff:127.0.0.1"] {
        assert!(!ip_is_permitted(ip(s), false), "{s} should be denied");
    }
}
#[test]
fn loopback_allowed_only_with_dev_optin() {
    assert!(ip_is_permitted(ip("127.0.0.1"), true));
    assert!(!ip_is_permitted(ip("127.0.0.1"), false));
}
```

- [ ] **Step 2: Run, verify fail.**
- [ ] **Step 3: Implement** `ip_is_permitted`: normalize IPv4-mapped IPv6 (`to_canonical()` / manual `::ffff:` unwrap) FIRST; deny `is_loopback`, `is_unspecified`, `is_multicast`, IPv4 link-local `169.254.0.0/16`, IPv6 `fe80::/10`, IPv4 `0.0.0.0/8`; then allow ONLY IPv4 `10/8`+`172.16/12`+`192.168/16` and IPv6 ULA `fc00::/7`; loopback allowed iff `allow_loopback`. Everything else denied. (Cite the explicit metadata case `169.254.169.254` in a comment — it is covered by link-local but called out for reviewers.)
- [ ] **Step 4: Run, verify pass.**
- [ ] **Step 5: Commit** `feat(tiles): resolved-IP allow/deny policy (RFC1918/ULA allow, default-deny)`.

> NOTE for the executor: `ip_is_permitted` operates on the IP the client actually CONNECTS to — wired up at fetch time in Phase 3 (rebinding defense). Config-time validation (Task 7.x) calls it as a courtesy warning only, never as the security control.

---

## Phase 2 — `TileCoord` (integer validation + traversal-safe paths)

Per §8.4. The filesystem twin of the SSRF entry: `{z}/{x}/{y}` are webview-supplied and become both upstream URL path segments and cache paths.

### Task 2.1: Parse/validate z/x/y as bounded integers

**Files:**
- Create: `src-tauri/src/tiles/coord.rs`
- Modify: `src-tauri/src/tiles/mod.rs` (`mod coord;`)

- [ ] **Step 1: Write the failing tests.**

```rust
#[test]
fn accepts_in_range() {
    let c = TileCoord::new(3, 5, 2, /*max_zoom*/ 16).unwrap();
    assert_eq!((c.z, c.x, c.y), (3, 5, 2));
}
#[test]
fn rejects_x_y_out_of_2_pow_z() {
    assert!(TileCoord::new(1, 2, 0, 16).is_err()); // x must be < 2^1 = 2
    assert!(TileCoord::new(0, 0, 1, 16).is_err()); // y must be < 2^0 = 1
}
#[test]
fn rejects_zoom_above_cap() {
    assert!(TileCoord::new(17, 0, 0, 16).is_err());
}
#[test]
fn from_str_rejects_non_integer() {
    assert!(TileCoord::from_parts("..","0","0",16).is_err());
    assert!(TileCoord::from_parts("3","-1","0",16).is_err());
    assert!(TileCoord::from_parts("3","x","0",16).is_err());
}
```

- [ ] **Step 2: Run, verify fail.**
- [ ] **Step 3: Implement** `TileCoord { z: u32, x: u32, y: u32 }` with `new(z,x,y,max_zoom)` enforcing `z <= max_zoom`, `x < 2u32.pow(z)`, `y < 2u32.pow(z)`; and `from_parts(&str,&str,&str,max_zoom)` parsing each as `u32` (rejects `-1`, `..`, `x`, empty) then calling `new`.
- [ ] **Step 4: Run, verify pass.**
- [ ] **Step 5: Commit** `feat(tiles): bounded-integer TileCoord parse/validate`.

### Task 2.2: TMS y-flip (after validation) + cache-relative path

**Files:**
- Modify: `src-tauri/src/tiles/coord.rs`

- [ ] **Step 1: Write the failing tests.**

```rust
#[test]
fn tms_flip_is_consistent_and_in_range() {
    let c = TileCoord::new(2, 1, 0, 16).unwrap();
    assert_eq!(c.upstream_y(/*tms*/ true), (1<<2) - 1 - 0); // 3
    assert_eq!(c.upstream_y(false), 0);
}
#[test]
fn rel_path_is_integers_only() {
    let c = TileCoord::new(3, 5, 2, 16).unwrap();
    assert_eq!(c.rel_path(false), std::path::PathBuf::from("3/5/2.tile"));
}
```

- [ ] **Step 2: Run, verify fail.**
- [ ] **Step 3: Implement** `upstream_y(tms)` = `if tms { (1<<z)-1-y } else { y }` (z,x,y already validated, so this stays in range); `rel_path(tms)` builds a `PathBuf` from the validated integers via `.join()` (never string interpolation). The upstream URL builder uses the same integers via `Url`-path-segment APIs.
- [ ] **Step 4: Run, verify pass.**
- [ ] **Step 5: Commit** `feat(tiles): TMS y-flip + integer-only cache rel-path`.

---

## Phase 3 — Gatekeeper fetch (resolved-IP, no-redirect, capped)

Per §8.3. Mirrors the updater's reqwest discipline but adds resolved-IP pinning and `redirect::none`.

### Task 3.1: reqwest client with no-redirect + timeout

**Files:**
- Create: `src-tauri/src/tiles/fetch.rs`
- Modify: `src-tauri/src/tiles/mod.rs`

- [ ] **Step 1: Write the failing test** (a constructor test asserting policy, plus a mockito test that a 302 response is a hard error → `FetchError::Redirect`). Use `mockito` (already a dev-dep per updater tests).
- [ ] **Step 2: Run, verify fail.**
- [ ] **Step 3: Implement** `build_tile_client() -> reqwest::Client` with `.user_agent("tuxlink-tiles/0.0.1")`, `.timeout(Duration::from_secs(5))`, `.redirect(reqwest::redirect::Policy::none())`. A 3xx status from the tile GET maps to `FetchError::Redirect` (do not follow).
- [ ] **Step 4: Run, verify pass.**
- [ ] **Step 5: Commit** `feat(tiles): no-redirect, short-timeout tile client`.

### Task 3.2: Fetch-time resolved-IP pinning (rebinding defense)

**Files:**
- Modify: `src-tauri/src/tiles/fetch.rs`

- [ ] **Step 1: Write the failing test** with a mockito server bound to `127.0.0.1` and `allow_loopback=true`, asserting fetch succeeds; and a unit test that a resolver returning a public IP for the host causes `FetchError::HostDenied` even though the configured URL string "looked" private. (Use a seam: a `resolve: impl Fn(&str,u16)->io::Result<Vec<SocketAddr>>` parameter so the test can inject resolution.)
- [ ] **Step 2: Run, verify fail.**
- [ ] **Step 3: Implement** `fetch_tile_bytes(source, coord, allow_loopback)`: resolve host→`SocketAddr`s at fetch time; reject unless EVERY resolved IP passes `ip_is_permitted(ip, allow_loopback)` (reject mixed/any-public); connect by the vetted `SocketAddr` (reqwest `resolve()` / `ClientBuilder::resolve_to_addrs` pin, or a custom connector) so DNS can't rebind between check and connect.
- [ ] **Step 4: Run, verify pass.**
- [ ] **Step 5: Commit** `feat(tiles): fetch-time resolved-IP pinning (DNS-rebind defense)`.

### Task 3.3: Response size cap + image content-type/magic check

**Files:**
- Modify: `src-tauri/src/tiles/fetch.rs`

- [ ] **Step 1: Write the failing tests** (mockito): a 200 returning `text/html` → `FetchError::NotAnImage`; a 200 over the byte cap (via `Content-Length` and via streaming) → `FetchError::TooLarge`; a 200 returning a valid PNG magic → `Ok(bytes)`; a 404 → `FetchError::NotFound`.
- [ ] **Step 2: Run, verify fail.**
- [ ] **Step 3: Implement**: `const MAX_TILE_BYTES: u64 = 2 * 1024 * 1024;` pre-check `content_length()` then stream-abort over the cap (mirror [updater.rs:398-435](src-tauri/src/forms/updater.rs#L398)); require status 200 (404 → `NotFound`, other → `Status`); validate the body's leading magic bytes are PNG/JPEG/WebP (do not trust the upstream `Content-Type` alone — check magic). Return `(bytes, image_mime)`.
- [ ] **Step 4: Run, verify pass.**
- [ ] **Step 5: Commit** `feat(tiles): tile size cap + image magic-byte validation`.

---

## Phase 4 — CRS-mismatch guard

Per §8.1. A mismatched source renders plausible-but-wrong, so refuse rather than show.

### Task 4.1: Source metadata probe + geodetic requirement

**Files:**
- Create: `src-tauri/src/tiles/crs.rs`
- Modify: `src-tauri/src/tiles/mod.rs`

- [ ] **Step 1: Write the failing tests** (mockito serving a TileJSON with `"crs"`/`"bounds"`, and an mbtiles-style `metadata` shape): a TileJSON declaring EPSG:3857 / Web Mercator → `CrsCheck::Rejected`; one declaring EPSG:4326 / geodetic / `WGS84` → `CrsCheck::Geodetic`; a source with no probeable metadata → `CrsCheck::Unknown` (caller treats Unknown as reject-with-explanation unless the operator set the explicit `crs: geodetic` config flag).
- [ ] **Step 2: Run, verify fail.**
- [ ] **Step 3: Implement** `probe_source_crs(client, source) -> CrsCheck`: try TileJSON (`/tilejson.json` or the source root), then WMTS capabilities, then mbtiles `metadata`; map declared CRS strings to geodetic vs mercator; `Unknown` when none probeable.
- [ ] **Step 4: Run, verify pass.**
- [ ] **Step 5: Commit** `feat(tiles): probe source CRS; require geodetic (EPSG:4326)`.

### Task 4.2: Alignment fixture (equator/mid/high latitude)

**Files:**
- Modify: `src-tauri/src/tiles/crs.rs` (test-only fixture asserting the 4326 tile-index math matches `projection.ts`/`gridGeometry.ts` expectations at lat 0°, 45°, 80°)

- [ ] **Step 1: Write the failing test** computing the geodetic tile index for known lat/lon at z=6 and asserting it matches the equirectangular pixel mapping (no Mercator term) at equator, mid, and high latitude.
- [ ] **Step 2–4:** Implement the pure index helper; verify pass.
- [ ] **Step 5: Commit** `test(tiles): geodetic alignment fixture at 3 latitudes`.

---

## Phase 5 — Cache (bounded, atomic, single-flight)

Per §8.4. Lives at `app_data_dir()/tile-cache/`.

### Task 5.1: Per-source namespace + traversal-safe write/read

**Files:**
- Create: `src-tauri/src/tiles/cache.rs`
- Modify: `src-tauri/src/tiles/mod.rs`

- [ ] **Step 1: Write the failing tests** (tempdir): namespace = `sha256(normalized_url + crs + scheme)` hex; a write+read round-trips; the resolved file canonicalizes to UNDER `cache_root` (assert `starts_with`); a `TileCoord` cannot escape (already integer-validated, but assert the join+canonical check anyway).
- [ ] **Step 2: Run, verify fail.**
- [ ] **Step 3: Implement** `source_namespace(source) -> String` (sha2 over the normalized url+crs+scheme), `tile_path(cache_root, ns, coord)` building from validated integers then `canonicalize`-and-`starts_with(cache_root)` assert (mirror `logging/state_dir.rs` posture).
- [ ] **Step 4: Run, verify pass.**
- [ ] **Step 5: Commit** `feat(tiles): per-source cache namespace + traversal-safe paths`.

### Task 5.2: Cache-only-good + atomic temp+rename

**Files:**
- Modify: `src-tauri/src/tiles/cache.rs`

- [ ] **Step 1: Write the failing tests:** only `200 + image-magic + non-empty` is cached (a prior NotAnImage/NotFound result is never written); a write is atomic (write temp file in the same dir, fsync, rename) so a concurrent reader never sees a partial file; an ENOSPC-simulated write failure returns `Ok(uncached)` not `Err`.
- [ ] **Step 2: Run, verify fail.**
- [ ] **Step 3: Implement** `put(coord, bytes)` (temp+rename, mirror `config.rs::write_config_atomic`), `get(coord) -> Option<Vec<u8>>`; a failed write degrades to "served-but-uncached", never user-facing error.
- [ ] **Step 4: Run, verify pass.**
- [ ] **Step 5: Commit** `feat(tiles): cache only verified images via atomic temp+rename`.

### Task 5.3: Byte-cap + LRU eviction + clear/purge

**Files:**
- Modify: `src-tauri/src/tiles/cache.rs`

- [ ] **Step 1: Write the failing tests:** with a low cap, inserting beyond it evicts least-recently-accessed before writing (simulate 1000+ tiles, assert total bytes stays ≤ cap — per `testing-pitfalls.md` bounded-growth discipline); `clear()` empties the source subtree; removing a source purges its namespace.
- [ ] **Step 2: Run, verify fail.**
- [ ] **Step 3: Implement** a `meta.json` per namespace tracking `{total_bytes, entries:[{rel, bytes, last_access}]}`; default cap 384 MB (configurable, §8.7); evict-before-write by `last_access`; `clear`/`purge`.
- [ ] **Step 4: Run, verify pass.**
- [ ] **Step 5: Commit** `feat(tiles): bounded LRU tile cache + clear/purge`.

### Task 5.4: Single-flight de-dup

**Files:**
- Modify: `src-tauri/src/tiles/fetch.rs` (or `mod.rs`)

- [ ] **Step 1: Write the failing test:** N concurrent requests for the same coord cause exactly ONE upstream fetch (count via a mockito hit-counter) and one cache write.
- [ ] **Step 2: Run, verify fail.**
- [ ] **Step 3: Implement** an in-process `Mutex<HashMap<TileKey, Weak<Shared>>>` single-flight (mirror the `INSTALL_LOCK` serialization idea, keyed per tile).
- [ ] **Step 4: Run, verify pass.**
- [ ] **Step 5: Commit** `feat(tiles): single-flight tile de-duplication`.

---

## Phase 6 — Serving + Tauri wiring (instantiates Phase 0)

### Task 6.1: Implement the chosen serving mechanism

**Files:**
- Create: `src-tauri/src/tiles/serve.rs`
- Modify: `src-tauri/src/lib.rs` (register), `src-tauri/tauri.conf.json` (the ONE production CSP token from Phase 0)

- [ ] **Step 1: Write the failing test** asserting the serving entrypoint, given a `TileCoord`, runs the gatekeeper pipeline (host-validate → cache get/fetch → put) and returns bytes; a denied/invalid coord returns the documented error (not a panic).
- [ ] **Step 2: Run, verify fail.**
- [ ] **Step 3: Implement** per Phase-0 decision: EITHER the async `tile` URI-scheme handler (parse path → `TileCoord::from_parts` → pipeline) OR the `fetch_tile` command returning bytes. Add EXACTLY the Phase-0 CSP token to `img-src`. Do not expose an arbitrary-URL command.
- [ ] **Step 4: Run, verify pass** + a packaged WebKitGTK render check (per Phase 0 evidence).
- [ ] **Step 5: Commit** `feat(tiles): serve tiles via <chosen mechanism>; +1 img-src token`.

### Task 6.2: `TileGatekeeper` managed state + lifecycle

**Files:**
- Modify: `src-tauri/src/tiles/mod.rs`, `src-tauri/src/lib.rs`

- [ ] **Step 1–4 (TDD):** a `TileGatekeeper` holding the active `Option<TileSource>` + client + cache handle + circuit-breaker state; registered as Tauri managed state; no network on construction.
- [ ] **Step 5: Commit** `feat(tiles): TileGatekeeper managed state`.

---

## Phase 7 — Frontend: tile layer, zoom gating, CRS, status

### Task 7.1: `TileSource` TS types + invoke wrappers

**Files:**
- Create: `src/map/tileSource.ts`
- Test: `src/map/tileSource.test.ts`

- [ ] **Step 1–4 (TDD):** types mirroring Rust `TileSource` + `TileSourceStatus` (`bundled | lan-live | lan-cached | partial | unreachable | incompatible`); wrappers `configureTileSource`, `clearTileCache`, `testTileSource`, `getTileSourceStatus`.
- [ ] **Step 5: Commit** `feat(map): tile-source TS types + invoke wrappers`.

### Task 7.2: `TileLayerBridge` (targets the chosen mechanism)

**Files:**
- Create: `src/map/TileLayerBridge.tsx`
- Test: `src/map/TileLayerBridge.test.tsx` (react-leaflet mock per the existing canonical mock)

- [ ] **Step 1–4 (TDD):** a Leaflet layer with `subdomains: []`, `tms` from config, `maxNativeZoom` from validated source max. If Phase 0 chose `invoke`+`blob:`, a custom `GridLayer.createTile` that `invoke`s, builds a `blob:` URL, and — CRITICAL — calls `URL.revokeObjectURL` on `tileunload` (assert revocation-count == eviction-count in a leak test). If `tile` scheme, a `TileLayer` with the `tile://`/`http://tile.localhost` template.
- [ ] **Step 5: Commit** `feat(map): tile layer bridge (+ blob revocation if applicable)`.

### Task 7.3: `BaseMap` — render tiles over raster backstop; widen C11 contract deliberately

**Files:**
- Modify: `src/map/BaseMap.tsx` (the FROZEN C11 contract — widen ON PURPOSE, documented)
- Test: `src/map/BaseMap.test.tsx`

- [ ] **Step 1–4 (TDD):** add an optional `tileSource?: ConfiguredTileSource` prop (document the C11 widening in the header comment + reference this plan); keep `<ImageOverlay>` raster as the always-present base; render `<TileLayerBridge>` ABOVE it when a validated source is active (so a 404 shows raster beneath at low zoom). `maxZoom` rises to the validated source max (capped 16) ONLY when status is `lan-live`/`lan-cached`; stays 2 otherwise. Above raster-native zoom, a missing tile shows the §8.5 "no coverage" treatment, NOT stretched raster.
- [ ] **Step 5: Commit** `feat(map): BaseMap tile layer over raster; validated zoom raise (C11 widened)`.

### Task 7.4: Re-tune `MaidenheadOverlay.levelFromZoom`

**Files:**
- Modify: `src/map/MaidenheadOverlay.tsx`
- Test: existing overlay test + new cases

- [ ] **Step 1–4 (TDD):** extend `levelFromZoom` for the full zoom range (finer lattice / fade-out at high zoom so the lattice isn't a single coarse square at z14); keep z0–2 behavior unchanged.
- [ ] **Step 5: Commit** `feat(map): re-tune Maidenhead lattice for full zoom range`.

### Task 7.5: 6-char precision gated on validated real tiles

**Files:**
- Modify: `src/map/tileSource.ts` (a `sixCharAllowed(status, view)` helper)
- Note: the Position picker (sdbd) CONSUMES this; dyop only exposes the gate.

- [ ] **Step 1–4 (TDD):** `sixCharAllowed` is true only when the view under the pin is backed by validated `lan-live`/`lan-cached` tiles at sufficient zoom; false (→ 4-char) otherwise. Export for sdbd.
- [ ] **Step 5: Commit** `feat(map): expose validated-tile gate for 6-char precision`.

---

## Phase 8 — Settings source config + status pill

### Task 8.1: `TileSource` config persistence (Rust)

**Files:**
- Modify: `src-tauri/src/config.rs` (add an optional `map_tile_source` field), `src-tauri/src/tiles/commands.rs`

- [ ] **Step 1–4 (TDD):** `TileSource { url, crs: Geodetic, scheme: Xyz|Tms, min_zoom, max_zoom, cache_budget_mb, attribution: Option<String>, label }`; persisted via `write_config_atomic`; auth is None by default and if ever added goes to keyring, never disk. `configure_tile_source` validates via `validate_source_url` + a `test_tile_source` probe (CRS + one tile) before activating.
- [ ] **Step 5: Commit** `feat(tiles): persist map tile source config`.

### Task 8.2: `MapTileSourceSettings` UI

**Files:**
- Create: `src/settings/MapTileSourceSettings.tsx`
- Test: `src/settings/MapTileSourceSettings.test.tsx`

- [ ] **Step 1–4 (TDD):** inline settings panel (no pop-up window) with: URL field (states "must serve EPSG:4326 / geodetic tiles"), XYZ/TMS toggle (default XYZ; hint that `.mbtiles` is usually TMS), min/max zoom, cache budget, attribution string, label; a "Test source" button surfacing CRS-mismatch/SSRF rejections plainly; a "Clear tile cache" button. Warn-not-block on a public-resolving host.
- [ ] **Step 5: Commit** `feat(settings): map tile source configuration UI`.

### Task 8.3: Status pill provenance states

**Files:**
- Modify: the picker control surface pill component (shared, from a1cc) OR a `TileStatusPill.tsx` dyop ships and a1cc consumes
- Test: pill test

- [ ] **Step 1–4 (TDD):** render `z{n} · bundled` / `LAN live` / `LAN cached as of …` / `LAN live (partial)` / `tiles unreachable — bundled` / `incompatible tile source` from `TileSourceStatus`, plus the zoom-cap reason.
- [ ] **Step 5: Commit** `feat(map): tile-source provenance status pill`.

---

## Phase 9 — Fallback state machine wiring + circuit breaker

Per §8.5. Ties the gatekeeper states to the UI.

### Task 9.1: Source-level circuit breaker

**Files:**
- Modify: `src-tauri/src/tiles/mod.rs` (gatekeeper state)

- [ ] **Step 1–4 (TDD):** after K=3 consecutive per-tile failures, flip the source to `degraded` and stop per-tile fetches for a cooldown (serve bundled), re-probe once on cooldown expiry. No synchronous network at startup/mount; the zoom ceiling rises lazily after the first validated tile.
- [ ] **Step 5: Commit** `feat(tiles): source circuit-breaker + lazy zoom-raise`.

### Task 9.2: Cancel-on-pan/zoom

**Files:**
- Modify: `src/map/TileLayerBridge.tsx`

- [ ] **Step 1–4 (TDD):** abort in-flight tile requests on view change (AbortController for the `invoke`/fetch path; Leaflet `tileunload` for layer teardown) so a slow source can't pile up timeouts.
- [ ] **Step 5: Commit** `feat(map): cancel in-flight tiles on view change`.

---

## Phase 10 — Pitfalls doc + final review

### Task 10.1: New implementation-pitfalls entry

**Files:**
- Modify: `docs/pitfalls/implementation-pitfalls.md`

- [ ] **Step 1:** Add an entry **"Tile-coordinate path traversal (filesystem twin of SSRF)"**: webview-supplied `{z}/{x}/{y}` land on disk; parse as bounded integers, hash the host, build paths from integers, canonicalize + `starts_with(cache_root)`. Cross-reference the existing SSRF entry and §8.4. (Also do the AGENTS.md parity check — no rule change here, so likely no edit.)
- [ ] **Step 2: Commit** `docs(pitfalls): tile-coordinate path-traversal entry`.

### Task 10.2: Final cross-phase review loop

- [ ] Re-run the full `tiles` test module + the map/settings vitest + `cargo clippy --all-targets -D warnings` (re-run until exit 0 — it hides later-target lints) + the contract/snapshot tests CI runs. Verify CSP lists no network host. Do ≥3 review rounds; fix until clean. Update journal.

---

## Spec-coverage check (plan ↔ §8)

| Spec | Phase/Task |
|---|---|
| §8.1 CRS require-geodetic + guard + alignment fixture | 4.1, 4.2, 7.3 |
| §8.2 serving mechanism via packaged-CSP spike; loopback-HTTP forbidden | 0.1–0.4, 6.1 |
| §8.3 SSRF: resolved-IP, no-redirect, http(s)-only, no-creds, integer coords, size+content caps; warn-not-block | 1.1, 1.2, 3.1–3.3, 8.2 |
| §8.4 cache: integer keys, hash namespace, canonical+starts_with, byte-cap+LRU, atomic, single-flight, image-only, clear/purge | 2.1, 2.2, 5.1–5.4 |
| §8.5 fallback states, circuit breaker, no-startup-network, no-stretched-raster, cancel-on-pan | 7.3, 9.1, 9.2 |
| §8.6 maxZoom gated on validated tiles; 6-char gate; C11 widened; levelFromZoom retune | 7.3, 7.4, 7.5 |
| §8.7 source config (URL/CRS/scheme/zoom/budget/attribution/label); auth keyring | 8.1, 8.2 |
| §8.8 strictly opt-in; offline-first | 6.2, 9.1 |
| §8.9 (process) | this plan + the build-robust-features adrev already done |
| New tile-coord path-traversal pitfall | 10.1 |
