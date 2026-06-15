# Bug Hunt Report — Region-pack offline-map download (holistic)

Agent: spruce-finch-gorge · 2026-06-15 · read-only against `origin/main`

## Scope

The full region-pack download → extract → validate → install → register → UI flow:

- `src-tauri/src/basemap/commands.rs` (command surface, `SidecarExtractor`, cancel registry, build_request, free-space, init_packs)
- `src-tauri/src/basemap/download.rs` (install_pack, atomic install, sweep_orphans, should_emit)
- `src-tauri/src/basemap/validate.rs` (validate_pmtiles, REQUIRED_LAYER_IDS, gzip-bomb cap)
- `src-tauri/src/basemap/region_manifest.rs` (parse, SSRF allowlist, bundled_default)
- `src-tauri/src/basemap/packs.rs` (Bbox, pack-id derivation, PacksManifest)
- `src-tauri/src/basemap/mod.rs` (PmtilesRegistry, read_range)
- `src-tauri/resources/basemap/region-manifest.json`, `provenance.json`
- `src/map/offlineMaps.ts`, `OfflineMapsSettings.tsx`, `useDownloadProgress.ts`, `basemapStyle.ts`
- `src-tauri/src/lib.rs` startup wiring (init_packs, packs dir)

Approach: read everything, then reason about the happy-path end-to-end and the
cross-component contracts (UI event payload vs backend emit; pack-id UI-vs-backend;
manifest schema vs validate.rs vs the actual build), plus failure/cancel/partial-state
correctness and what leaves orphaned/corrupt state.

The two KNOWN bugs (B1 pinned 404ing planet URL; B2 stdout-vs-stderr capture) are NOT
re-litigated below. Everything here is additional.

## Bugs

### 1. Successful install that fails to *register* leaves a manifest entry that is permanently unserveable until restart, and the UI silently shows the pack as installed-but-broken

**Location:** `commands.rs:386-407` (`download_pack_blocking`, the `Ok(entry)` arm) + `download.rs:766-279` (`install_pack` writes the manifest before the caller registers)
**Severity:** significant
**Evidence:** `install_into_temp` writes the packs-manifest entry as the LAST step of
`install_pack` (download.rs:275-278) and returns `Ok(entry)`. Only then does
`download_pack_blocking` call `registry.register_path(...)` (commands.rs:388-391). If
`register_path` fails (e.g. the just-renamed file can't be re-opened, a transient
EMFILE/permission blip), the code emits `DownloadDone{ ok:false }` and returns `Err` —
**but the manifest entry and the on-disk archive are already committed.** The pack is now
"installed" per `basemap_list_packs` yet absent from the `PmtilesRegistry`, so every
`tile://pmtiles/<id>` request 404s for the rest of the session.
**Impact:** The UI's `runDownloadOp` catch shows "Download failed", but the subsequent
`refresh()` lists the pack as installed (it IS in the manifest). The operator sees a
pack in "Installed map packs" that renders nothing on the map. It only self-heals on the
next app restart (init_packs re-registers from the manifest). The done-event `ok:false`
and the persisted-installed state contradict each other. Fix approach: on
`register_path` failure, either (a) treat it as non-fatal and still return `Ok(entry)`
(the pack IS installed and will register next boot — matching `init_packs`' best-effort
posture), or (b) roll back the install (delete the archive + manifest entry) before
returning `Err`, so "failed" means nothing persisted. (a) is more consistent with the
rest of the subsystem's "manifest is the source of truth, registry is a cache" model.

### 2. Progress bar denominator is the manifest *estimate*, not the real size — the bar saturates at 99.9% and the byte readout shows "1.4 GB / 1.0 GB"

**Location:** `commands.rs:352-353` (`total = req.typical_bytes`) + `useDownloadProgress.ts:113-115` + `OfflineMapsSettings.tsx:276`
**Severity:** significant (happy-path UX; not corruption)
**Evidence:** `total` is hard-wired to `req.typical_bytes` — the manifest's coarse
estimate (`wide` = exactly 1,000,000,000; continents = round tens-of-GB). The progress
emit sends `{bytes, total}` where `bytes` is the live `.part` size. A real extract's size
is data-dependent and routinely diverges from the estimate by tens of percent. When the
real extract exceeds the estimate, `percent = min(bytes/total, 0.999)` (useDownloadProgress.ts:115)
pins the bar at 99.9% for a potentially long final stretch, and the meta row renders e.g.
`1.4 GB / 1.0 GB` (OfflineMapsSettings.tsx:276) — total smaller than current. When the
real extract is much smaller, the bar reaches "done" (jumps to 100% on the done event)
having only ever shown ~30%.
**Impact:** The determinate progress bar is misleading on every download whose true size
differs from the estimate, which is the common case for byte-range geographic extracts.
The "X / Y" readout can show current > total. ETA (computed from `total - bytes`,
useDownloadProgress.ts:113) goes to 0/negative-clamped early and stops being meaningful.
Fix approach: there is no authoritative total until the extract finishes (go-pmtiles
streams), so either (a) clamp `total` up to `max(typical_bytes, bytes)` so the bar never
overflows and the readout never shows current>total, or (b) present an indeterminate bar
once `bytes` passes `~0.9*total`, or (c) surface "~estimate" framing in the denominator
label so the operator reads it as an estimate. At minimum fix the current>total display.

### 3. Free-space pre-flight is bypassed on the very first download because the packs dir does not exist yet → statvfs fails → `available_bytes` returns `u64::MAX`

**Location:** `commands.rs:212-222` (`available_bytes`) + `commands.rs:326` (called before the dir exists) + `download.rs:201` (`create_dir_all` happens later, inside `install_pack`) + `lib.rs:615-616` (`init_packs` never creates `basemap-packs/`)
**Severity:** minor
**Evidence:** `init_packs(data_dir.join("basemap-packs"), …)` does not create the dir;
`load_manifest`/`sweep_orphans` only read it. So on a fresh install `basemap-packs/`
does not exist until the first `install_pack` reaches `fs::create_dir_all` (download.rs:201).
But the free-space gate runs earlier: `download_pack_blocking` computes
`available_bytes(&state.packs_dir)` (commands.rs:326) BEFORE `install_pack` is called.
`statvfs` on a non-existent path errors, and `available_bytes` maps `Err(_) => u64::MAX`
(commands.rs:221) "do not block a download on a stat error". So `available_bytes` returns
`u64::MAX` and `install_pack`'s `available_bytes < req.needed_bytes` check
(download.rs:195) can never trip on the first download.
**Impact:** On a fresh install, the first pack download — including a 30 GB continent —
skips the disk-space pre-flight entirely and proceeds to extract until the disk fills.
The validation size-budget still rejects an over-large *finished* archive, but only after
the bytes are already on disk (a full small disk can be exhausted mid-extract). Self-heals
on the second download (dir now exists). Fix approach: `create_dir_all(&packs_dir)` in
`init_packs` (or in `download_pack_blocking` before `available_bytes`), so statvfs has a
real path. Note `u64::MAX`-on-error is also reasonable to keep, but the dir should exist.

### 4. `should_emit` throttle state is per-`on_progress`-closure but the closure is rebuilt per download — fine; however the throttle starves the FINAL progress sample, so the bar can freeze short of the real end-of-transfer value

**Location:** `commands.rs:165-169` (final `on_progress(written)` after `try_wait` returns) + `commands.rs:356-368` (throttled emitter) + `download.rs` final emit
**Severity:** minor
**Evidence:** When the sidecar exits, `SidecarExtractor::extract` takes a final size
sample and calls `on_progress(written)` once (commands.rs:167-168). That call still goes
through the EMIT_THROTTLE gate (commands.rs:357-358). If the previous emit was <400 ms
ago, the final sample is suppressed. The terminal `download-done` event then drives the
bar to 100% (useDownloadProgress.ts:142), so this is cosmetic — but the last byte/rate
readout the operator sees before "done" can be stale by one poll interval.
**Impact:** Cosmetic only (the done event corrects percent to 1). Noting because the
"final sample" comment in extract (commands.rs:166 "Final size sample before reporting
completion") implies it always reaches the UI, which the throttle can defeat. Fix
approach: bypass the throttle for the terminal sample, or have the done-event handler
also set `bytes=total`.

### 5. SSRF allowlist does not constrain the sidecar's redirect-following — a compromised/MITM `build.protomaps.com` can redirect go-pmtiles to an internal target

**Location:** `region_manifest.rs:120-143` (`validate_planet_url`) + module SECURITY note (region_manifest.rs:20-28) + `commands.rs:131-141` (the spawned sidecar performs the egress)
**Severity:** minor (already documented as out-of-scope; flagged for completeness)
**Evidence:** The module's own HONEST SCOPE note (region_manifest.rs:20-28) states this:
the allowlist is a string check on the *named* host; go-pmtiles (a separate process)
performs the actual HTTP egress and is not pinned to `redirect::Policy::none()` the way
`tiles::fetch` is. A 3xx from the allowed host to `http://169.254.169.254/...` would be
followed by the sidecar.
**Impact:** Real but bounded: requires compromising/MITMing the pinned host's TLS. The
code honestly documents it rather than overstating the mitigation. Not a defect in the
code-as-written; flagged so it is on the record alongside the download flow. Fix approach
(if desired): invoke go-pmtiles with a no-redirect / resolved-IP option, or front the
fetch through the Rust client that already sets `redirect::Policy::none()`.

## Design Concerns

- **Manifest `pmtiles_schema` is decorative and can silently drift from `validate.rs`.**
  `region-manifest.json` carries `pmtiles_schema.vector_layers` (9 ids) purely "for
  manifest reviewers" (region_manifest.rs:57-58); the runtime gate is the hard-coded
  `REQUIRED_LAYER_IDS` in validate.rs:57-67. Nothing asserts the two lists agree. If a
  future planet build changes its layer set, an operator could update the manifest's
  decorative list and the `planet_build`/`planet_url` and still have every download
  rejected by the stale `REQUIRED_LAYER_IDS` (or vice-versa). The manifest's
  `planetiler_version: 4` field is also inconsistent with `provenance.json`
  (`pmtiles_cli_version`) and the validator's recorded `planetiler:version` — three
  different provenance notions with no cross-check. Consider a test that asserts
  `manifest.pmtiles_schema.vector_layers == REQUIRED_LAYER_IDS`.

- **`source_build` on an installed pack is recorded from the manifest at download time,
  never verified against the archive.** `install_into_temp` sets
  `entry.source_build = req.source_build` (download.rs:268) from `manifest.planet_build`.
  validate.rs reads the archive's real `schema_version`/`planetiler_version` but NOT a
  build hash, so a pack extracted from build X while the manifest claims build Y records
  Y. Low stakes (informational field), but it means the "source_build" displayed/stored
  is the manifest's claim, not ground truth.

- **Cancel→Retry closure re-runs the SAME captured `fn` under a NEW busy key collision
  risk.** `runDownloadOp` stores `setRetry(() => () => void runDownloadOp(label, fn, key))`
  (OfflineMapsSettings.tsx:103). Retrying re-invokes with the identical `key`. Because the
  backend rejects a duplicate in-flight id (commands.rs:340-344) and the prior op cleared
  its cancel flag, this is fine in the normal case, but the `downloadKey`/`downloadError`
  state machine (showProgressRow at OfflineMapsSettings.tsx:172) depends on `busy` and
  `downloadError` being cleared in the right order across the retry; worth an explicit
  test of cancel-then-retry-then-cancel.

- **`available_bytes` returning `u64::MAX` on ANY statvfs error (not just missing dir)
  defeats the free-space gate whenever the filesystem call fails for any reason** (not
  just bug #3's first-run case). The "don't block on a stat error" posture means a genuine
  low-disk condition that also happens to make statvfs fail (rare) would skip the gate.
  The validation size-budget is the backstop, but it only fires after the bytes land.

## Notes for testing-pitfalls

The bugs above are not coverage gaps in the unit sense — `install_pack`, `validate`,
`tier_bbox`, and `read_range` are thoroughly unit-tested. They live in the SEAMS the
unit tests deliberately stub out:

- Bug #1 (register-after-install) lives in `download_pack_blocking`, which the unit suite
  cannot reach because it needs an `AppHandle` + a real `PmtilesRegistry`; the
  `install_pack` tests stop at the manifest write and never exercise the register step.
- Bug #2 (progress denominator) is invisible to `progress_callback_is_invoked_with_growing_bytes`
  (download.rs:618) because that test asserts only that `bytes` grows — it never compares
  `bytes` to `total`, and `total` is injected in the command layer, not download.rs.
- Bug #3 (free-space bypass on missing dir) is masked because every install test passes
  `u64::MAX` or a literal `available_bytes` directly to `install_pack` (download.rs:533
  etc.), bypassing the real `available_bytes`/`statvfs` path and the
  init_packs-doesn't-create-the-dir interaction.

The shared lesson: this subsystem's correctness lives in the COMMAND layer wiring (total
selection, register-after-install ordering, statvfs-before-create-dir), which is exactly
the layer the pure-core tests are designed to exclude. An integration test that drives
`download_pack_blocking` against a fake `Extractor` + a real `BasemapState` + registry
(asserting the registered-vs-listed invariant and the `total` value in the emitted event)
would catch #1 and #2. A test of `init_packs` on a non-existent data dir followed by a
download would catch #3.
