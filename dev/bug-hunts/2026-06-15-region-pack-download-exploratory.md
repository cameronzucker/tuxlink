# Bug Hunt Report — region-pack offline-map download flow (exploratory)

Agent: spruce-finch-gorge · 2026-06-15 · read-only against `origin/main`

## Scope

The DOWNLOAD → EXTRACT → VALIDATE → INSTALL → UI flow:

- `src-tauri/src/basemap/commands.rs` — Tauri command surface, `SidecarExtractor.extract`, cancel registry, free-space math, `build_request`.
- `src-tauri/src/basemap/download.rs` — `install_pack`, atomic install, orphan sweep, `DownloadError`.
- `src-tauri/src/basemap/validate.rs` — header/schema/size validation.
- `src-tauri/src/basemap/region_manifest.rs` — manifest parse, SSRF allowlist.
- `src-tauri/src/basemap/packs.rs` — bbox math, pack-id safety.
- `src-tauri/src/basemap/mod.rs` — `PmtilesRegistry`, `read_range`.
- `src/map/offlineMaps.ts`, `src/map/OfflineMapsSettings.tsx`, `src/map/useDownloadProgress.ts` — the UI.
- `src-tauri/resources/basemap/region-manifest.json` — bundled manifest.

Explored deepest: `download_pack_blocking` lifecycle (cancel/progress/done event correctness, error-path event emission), the UI `runDownloadOp`/`useDownloadProgress` state machine (Retry + cancel races), and `SidecarExtractor.extract` failure/cancel paths. KNOWN findings B1 (pinned 404 planet URL) and B2 (stdout-null swallows go-pmtiles error) were given and are not re-listed except where they interact with a new finding.

## Bugs

### 1. No `download-done` (or `download-progress`) event is emitted on early-return failures → UI progress row gets stuck "Downloading…" forever
**Location:** `src-tauri/src/basemap/commands.rs:322` (build_request early return) and `:340-345` (duplicate-id early return), in `download_pack_blocking`.
**Severity:** significant
**Evidence:** The `DONE_EVENT` is only emitted in the `match result { … }` block at the *end* of `download_pack_blocking` (lines 386-417). Two paths return *before* that block ever runs:
- `let req = build_request(&manifest, &args).map_err(|e| e.to_string())?;` (line 322) — an unknown tier/continent id, or a bbox that fails `tier_bbox` (e.g. a degenerate clamp), returns `Err` with no event.
- the duplicate-in-flight rejection `return Err(format!("a download for {} is already in progress", req.id));` (lines 340-345) — no event.

In both cases the command's `Result` rejects, which the UI's `runDownloadOp` *does* catch (it `await`s the invoke). BUT: the duplicate-reject string is `"a download for … is already in progress"`, which does not contain `"download cancelled"`, so `runDownloadOp` sets `downloadError` and leaves `downloadKey` set → the progress row sticks showing the error with a Retry that will re-reject identically. More subtly, because these reject the *invoke promise*, the parent's catch handles it; the hidden danger is the asymmetry: every *other* terminal outcome (success, validation fail, extract fail, cancel) emits `DONE_EVENT`, so `useDownloadProgress` is built assuming a done event always arrives. Any consumer that keys off the done event (not the promise) — and the hook does, for its `status`/`percent:1`/`cancelled` transitions — never sees a terminal signal for these two paths. The hook's `view` stays `status:'downloading'`.
**Impact:** A `build_request` failure (most reachable via a refreshed manifest whose tier set changed under a stale UI, or a continent removed from the manifest) leaves `useDownloadProgress` latched in `downloading`; the determinate bar and "Downloading…" affordance never clear from the hook's perspective. The parent's promise-catch papers over it only for the error-text path, and even then renders the row stuck-with-error rather than idle.
**Fix:** Emit `DONE_EVENT { ok:false, error }` before *every* early `return Err` in `download_pack_blocking` (or wrap the body so a single exit point always emits). Simplest: move the cancel-flag registration + the build_request to the top, and funnel all exits through one emit.

### 2. Cancel is a silent no-op until the first progress event arrives
**Location:** `src/map/OfflineMapsSettings.tsx:162-166` (`onCancel`), depends on `useDownloadProgress.ts:101` (`trackedId` latched only from a progress/done event).
**Severity:** significant
**Evidence:** `onCancel` does `if (progress.trackedId) void cancelDownload(progress.trackedId);`. `trackedId` is `null` until the first `basemap:download-progress` (or done) event arrives and latches it (`useDownloadProgress.ts:101`, `:137`). The backend emits the first progress event only after the poll loop runs and the throttle gate (`should_emit` with `EMIT_THROTTLE = 400ms`, first sample always emits) — but the loop only starts after the sidecar `spawn()` succeeds, and the very first event is gated behind go-pmtiles producing a `.part` (size 0 is still emitted, so ~one POLL_INTERVAL ≈ 500ms). During the window between clicking a tier and the first event (sidecar spawn + DNS + first range fetch can be seconds on a slow link), the operator's Cancel button does nothing — `trackedId` is null. The backend cancel registry *is* keyed and ready (`download_cancels` populated synchronously at the top of `download_pack_blocking`), so the only missing piece is that the UI doesn't know the id yet.
**Impact:** Operator clicks Cancel during the early/slow phase of a large download (a continent at z14 — exactly when they'd want to bail), nothing happens, no feedback. They must wait for progress to start before Cancel works. On a stalled-but-not-erroring sidecar (e.g. a slow 200 from Protomaps) Cancel can be dead indefinitely.
**Fix:** The UI already knows the resolved backend id deterministically — it is the same id the backend derives (`tier_pack_id`/`continent_pack_id` from the args the UI sent). Compute the pack id UI-side (mirror `packs::tier_pack_id`) and pass it to `cancelDownload` directly, instead of waiting to learn it from an event. Alternatively, have the backend emit an immediate "started" event carrying the id before the extract begins.

### 3. Free-space pre-flight is bypassed entirely on a `statvfs` error (fails OPEN, not closed)
**Location:** `src-tauri/src/basemap/commands.rs:212-222` (`available_bytes` returns `u64::MAX` on error) feeding `install_pack`'s gate at `download.rs:195`.
**Severity:** significant
**Evidence:** `available_bytes` returns `u64::MAX` when `statvfs` fails. `install_pack` then computes `if available_bytes < req.needed_bytes` — `u64::MAX < needed` is always false, so the free-space gate is skipped. The doc-comment frames this as intentional ("do not block a download on a stat error … validation + size budget still bound the result"), but that reasoning is wrong for the failure it matters for: validation runs *after* the multi-GB extract completes, and the size budget only rejects an archive that is too *large*, never one that *ran the disk out of space mid-write*. A continent extract is 15-35 GB; if `statvfs` fails (or returns a surprising value on an exotic FS) the app will happily start a 35 GB extract onto a disk with 2 GB free.
**Impact:** On any `statvfs` failure the free-space safety net is gone; the download proceeds, fills the disk, and fails late (go-pmtiles write error → `ExtractFailed`) after wasting bandwidth/time and potentially destabilizing the Pi (full root fs). The Pi context (`packs_dir` under app-data) makes a full-disk a real operational hazard.
**Fix:** Fail closed on a stat error, or at minimum surface a distinct "could not determine free space" warning rather than silently proceeding with `u64::MAX`. Given the cost asymmetry (a 35 GB doomed extract vs. one refused download), returning `0` (or an explicit `Err`) on stat failure is the safer default.

### 4. `needed_bytes` headroom is too thin for the size-budget reality → mid-extract disk-full on legitimate packs
**Location:** `src-tauri/src/basemap/commands.rs:73-75` (`NEEDED_MARGIN = 6/5`, `BUDGET_MULT = 3`) and `:470-471`, `:493-494`.
**Severity:** minor (correctness-of-estimate; degrades to a late ExtractFailed, no corruption)
**Evidence:** `needed_bytes = typical * 6/5` (20% headroom) but `size_budget = typical * 3` (the validator accepts up to 3× typical). So an archive between `1.2×` and `3×` typical is *allowed by validation* yet was *never gated against free space* — the pre-flight only guaranteed 1.2× typical of free space. `typical_bytes` for the manifest's tiers/continents is an estimate; the Wide tier (`typical=1 GB`, `needed=1.2 GB`, `budget=3 GB`) can legitimately produce a 1.5-2 GB extract that the validator would accept but the pre-flight under-reserved for.
**Impact:** A download that the system considers valid (under budget) can still run the disk out partway, failing late, because the free-space reservation (1.2× typical) is far below the accepted ceiling (3× typical). Not corruption (cleanup guard removes the `.part`), but a poor, late failure on legitimate inputs.
**Fix:** Reserve free space against the *budget* (or a number much closer to it), not against `typical * 1.2`. `needed_bytes` should be ≥ `size_budget`, or the budget should be tightened toward the real distribution.

### 5. Retry reuses the same `downloadKey`, so `useDownloadProgress` never resets — stale rate/ETA/error carry into the retried run
**Location:** `src/map/OfflineMapsSettings.tsx:98-121` (`runDownloadOp` does not null `downloadKey` on the error path) + `:103` (Retry calls with the same `key`); `src/map/useDownloadProgress.ts:85-90` (reset effect keyed on `active`).
**Severity:** minor
**Evidence:** On a failed download, `runDownloadOp` sets `downloadError` and leaves `downloadKey === key` (only the cancel and success branches null it). The Retry closure (`setRetry(() => () => void runDownloadOp(label, fn, key))`, line 103) re-invokes `runDownloadOp` with the *same* `key`. Inside, `setDownloadKey(key)` is a no-op because the value is unchanged → React does not re-run `useDownloadProgress`'s reset `useEffect([active])` (line 85) nor its subscribe `useEffect([active])` (line 92). Therefore `trackedId.current`, `lastSample.current`, and `rateRef.current` retain values from the failed attempt, and `view.error`/`view.status` stay at the prior error until a fresh progress event overwrites them.
**Impact:** The retried download shows a stale smoothed rate and ETA computed from the previous (failed) attempt's last sample for the first event or two, and the hook's internal `view.error` lingers (the parent clears its own `downloadError`, so the visible row is mostly correct, but the bar's rate/eta readout is briefly wrong). On a retried run that latches a *different* pack id (can't happen today since id is deterministic, but the hook is written to be id-agnostic), `trackedId` would be wrong and progress events would be ignored.
**Fix:** Null `downloadKey` before re-dispatching on Retry (force the `active` change so the hook resets), or reset the hook's refs explicitly when a new `fn` is dispatched. Toggling `downloadKey` to `null` then back, or appending a monotonic attempt counter to the key, both force the reset.

### 6. `formatBytes` boundary gap: values in `[1000, 1_000_000)` that are also `< 1000` of the next unit render inconsistently with the GB/MB rounding, and the KB branch can show "1000 KB" instead of "1 MB"
**Location:** `src/map/OfflineMapsSettings.tsx:30-35`.
**Severity:** minor (cosmetic; not a flow break)
**Evidence:** Thresholds use decimal `1_000_000_000` / `1_000_000` / `1000`. `formatBytes(999_500)` → `Math.round(999500/1000)` = `1000 KB` (the value is below the 1 MB threshold of 1_000_000 but rounds to 1000 KB rather than promoting to "1 MB"). Similarly `formatBytes(999_999_999)` → `Math.round(999999999/1_000_000)` = `1000 MB` (below the 1 GB threshold). The reverse-boundary rounding produces a 4-digit unit value that should have rolled to the next unit.
**Impact:** Pack sizes/progress occasionally display "1000 KB" or "1000 MB" where "1 MB"/"1 GB" is expected — minor polish issue on the size readout, no functional consequence.
**Fix:** Round first, then pick the unit, or guard the rounded result against rolling into the next unit.

## Design Concerns

- **`install_lock` is held for the entire multi-GB extract** (`commands.rs:374-377`). A second *different*-id download blocks on this lock for the whole duration of the first, with no progress and no way for the user to know it's queued (the UI disables buttons, masking it). If the UI disable ever regresses or a future caller invokes the command directly, the second request silently hangs. The lock is only needed for the manifest read-modify-write, not the extract+validate; narrowing it (extract into temp without the lock, take the lock only around validate→rename→manifest, or use a per-id lock) would remove the serialization. Watched failure mode: a queued continent download appears frozen.

- **The cancel registry and the post-extract re-check (`download.rs:246`) close the success-after-cancel race, but `SidecarExtractor.extract` leaks its stderr-draining thread on the cancel and `try_wait`-error paths** (`commands.rs:154-158`, `:182-186`): those branches return without `stderr_handle.join()`. The thread will exit on its own once the killed child's stderr pipe closes, so it's not a true leak, but it is an unjoined thread per cancelled/errored download. Low risk; worth a comment or an explicit join.

- **B2 interaction with Finding 1:** because go-pmtiles writes its real error to *stdout* (captured to `Stdio::null()`), the `ExtractFailed` message is `"go-pmtiles exit Some(1): "` with empty stderr. Combined with the pinned-and-now-404 planet URL (B1), the *most common* real failure today (404) produces a useless error — and that error *does* travel through `DONE_EVENT` (so Finding 1 doesn't apply to it), but the operator sees no cause. Fixing B2 (capture stdout) is the higher-value half of making the flow legible.

- **`available_bytes` u64::MAX-on-error (Finding 3) + thin `needed_bytes` (Finding 4)** together mean the free-space gate is the weakest link in an otherwise careful atomic-install design: the install sequence is meticulous about never persisting corrupt state, but the *resource* pre-flight can wave through a download that exhausts the disk. The corruption-safety is good; the resource-safety is not.

## testing-pitfalls.md note candidate

The bug class in Finding 1 (terminal-event not emitted on early-return error paths) is invisible to the existing Rust tests because they test `install_pack` (the core) directly and never exercise `download_pack_blocking`'s event emission — there is no test asserting "every exit of the command emits exactly one `download-done`." A test that drives `download_pack_blocking` (or a refactored single-exit version) and asserts a `DONE_EVENT` fires on the unknown-tier and duplicate-in-flight paths would catch it. Worth adding to testing-pitfalls if the team tracks event-completeness invariants.
