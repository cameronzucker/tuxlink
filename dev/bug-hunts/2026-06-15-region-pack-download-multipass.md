# Bug Hunt Report — Region-Pack Offline-Map Download Flow

Agent: spruce-finch-gorge
Method: code-bug-hunter-multipass (5 passes) — read-only against origin/main (`b5d10ccd` working branch; files read via `git show origin/main:`).
Date: 2026-06-15

## Scope

The tuxlink region-pack DOWNLOAD → EXTRACT → VALIDATE → INSTALL → REGISTER → UI flow:

- `src-tauri/src/basemap/commands.rs` — Tauri command surface, `BasemapState`, `SidecarExtractor`, cancel registry, free-space, `build_request`.
- `src-tauri/src/basemap/download.rs` — `install_pack`, atomic install, `delete_pack`, `sweep_orphans`, `should_emit`.
- `src-tauri/src/basemap/validate.rs` — PMTiles header + schema + size validation.
- `src-tauri/src/basemap/region_manifest.rs` — manifest parse + `planet_url` SSRF allowlist.
- `src-tauri/src/basemap/packs.rs` — bbox math, pack-id safety, packs manifest.
- `src-tauri/src/basemap/mod.rs` — `PmtilesRegistry`, `read_range`, range parsing.
- `src/map/offlineMaps.ts`, `src/map/OfflineMapsSettings.tsx`, `src/map/useDownloadProgress.ts`.

All five passes performed. KNOWN issues B1 (stale pinned planet URL) and B2 (`stdout(Stdio::null())` discards go-pmtiles's real error) were excluded from the hunt per the brief.

---

## Bugs

### 1. First-ever download bypasses the free-space gate (statvfs on a not-yet-created packs dir returns `u64::MAX`)

**Location:** `commands.rs:326` (`let available = available_bytes(&state.packs_dir);`) + `available_bytes` at `commands.rs:211-222` + `download.rs:201` (`create_dir_all` happens INSIDE `install_pack`, after `available` is sampled) + `lib.rs:615` (`init_packs` does not create the packs dir).
**Severity:** significant
**Evidence:** `init_packs(data_dir.join("basemap-packs"), …)` (lib.rs:615) never calls `create_dir_all`; it only `load_manifest`/`sweep_orphans`/`register_path`, all of which tolerate a missing dir. The packs directory therefore does not exist until the FIRST `install_pack` call reaches `fs::create_dir_all(packs_dir)` (download.rs:201). But `download_pack_blocking` samples free space BEFORE that, at commands.rs:326, by calling `available_bytes(&state.packs_dir)`. `available_bytes` runs `statvfs(path)` on the still-nonexistent directory, which fails with `ENOENT`, and the `Err(_) => u64::MAX` arm (commands.rs:220) returns `u64::MAX`. `install_pack`'s gate `if available_bytes < req.needed_bytes` (download.rs:195) is then `u64::MAX < needed` → always false → the gate is skipped.
**Impact:** On a fresh install (the common case — operator's very first pack download), the pre-flight free-space check is silently disabled. An operator with a nearly-full disk who picks a continent pack (tens of GB) gets no up-front `InsufficientSpace` rejection; instead go-pmtiles runs until the filesystem fills, then fails partway with an opaque write error (compounded by B2). The gate works on the 2nd+ download (dir now exists), so this is intermittent and easy to miss in testing. Fix: create the packs dir during `init_packs` (and/or call `create_dir_all` before sampling free space, then statvfs the real dir; or statvfs the parent `data_dir`, which always exists, when the packs dir is absent).
**Found in:** Pass 3 — Failure Mode Reasoning.

### 2. Failed `register_path` after a successful install leaves the pack on disk + in the manifest but unserved, and the sweep deletes it on next restart

**Location:** `commands.rs:388-406` (the `Ok(entry)` arm: `register_path` failure returns `Err(e)` but does NOT roll back the just-written archive or manifest entry).
**Severity:** significant
**Evidence:** `install_pack` has, by the time it returns `Ok(entry)`, atomically renamed the archive into place AND written the packs manifest with the new entry (download.rs:257 + 275-277). The command layer then calls `registry.register_path(&entry.id, …)` (commands.rs:388). On `Err` it emits a failure `DownloadDone` and returns `Err(e)` — but the archive file and the manifest entry are left in place. The UI's `runDownloadOp` catch (OfflineMapsSettings.tsx:109-117) treats this as a failed download and shows "Download failed" + Retry. Meanwhile `basemap_list_packs` (which reads the manifest, commands.rs:276-282) WILL list the pack, and `tile://pmtiles/<id>` 404s because the registry never got it. The two surfaces disagree. Worse: a `register_path` failure here typically means the file is unreadable/corrupt at open time, yet the manifest still references it — and on the NEXT startup `sweep_orphans` keeps it (it IS in the manifest, download.rs:301-324), so the inconsistency persists rather than self-healing.
**Impact:** A transient open failure right after install produces a pack that appears installed (listed, counts toward disk-used) but never renders, with no path to recovery except manual delete. The operator sees a contradictory state: "Download failed / Retry" AND the pack in the installed list. Fix: on `register_path` failure, roll back — `delete_pack`/`download::delete_pack` (which removes both archive and manifest entry) before returning the error, so the failed download truly leaves nothing behind (the invariant the rest of the module is built around).
**Found in:** Pass 3 — Failure Mode Reasoning / Pass 2 — Cross-Sibling (every other failure path in this module guarantees "nothing left behind"; this one breaks the pattern).

### 3. `download-done` `ok:true` is emitted only AFTER a successful register, but `ok:false` register-failures and the success both race the command Result — the UI can latch a stale "done" against the wrong run

**Location:** `useDownloadProgress.ts:85-90` (reset effect keyed on `active`) + `OfflineMapsSettings.tsx:98-121` (`runDownloadOp` sets `downloadKey` then awaits `fn()`) + `commands.rs:393-413` (done events).
**Severity:** minor
**Evidence:** The hook resets its state in a `useEffect` keyed on `active` (the `downloadKey`). `runDownloadOp` sets `downloadKey=key` (line 102) and then `await fn()`; on success it sets `downloadKey=null` (line 108). The `download-done` event and the `invoke` promise resolution are independent: the backend emits `DONE_EVENT` (commands.rs:393) and returns `Ok(entry)` essentially together, but their delivery order to the webview is not guaranteed. If the command promise resolves and `runDownloadOp` sets `downloadKey=null` (resetting the hook to IDLE and tearing down its listeners) BEFORE the `download-done` event is dispatched, the terminal `done` is dropped — harmless for success (the row is already being cleared) but means the hook's `status:'done'` transition is never observed by any consumer. This is benign today because `showProgressRow` is gated on `busy`/`downloadError`, not on the hook status, but it makes the hook's documented `done` state effectively dead on the happy path. The reverse race (done arrives first) is fine. Noting as a contract-vs-reality gap rather than a user-visible failure.
**Found in:** Pass 1 — Contract Violations.

### 4. `basemap_cancel_download` is a no-op when the operator cancels a download that is queued behind `install_lock`

**Location:** `commands.rs:333-346` (cancel flag inserted into the registry BEFORE `install_lock` is acquired) vs. `commands.rs:373-376` (the lock is acquired only inside `install_pack`'s scope) + `commands.rs:424-433` (`basemap_cancel_download`).
**Severity:** minor
**Evidence:** Two concurrent `basemap_download_pack` calls for DIFFERENT ids both pass the duplicate-id check (commands.rs:339) and both insert their cancel flags. Both then contend on `install_lock` (commands.rs:374). One wins; the other blocks inside `install_pack`'s lock acquisition — but its cancel flag IS already registered, so `basemap_cancel_download` for the queued pack will set the flag (commands.rs:431). Good so far: when the queued download finally acquires the lock, `install_pack`'s `cancel.load()` pre-extract re-check (download.rs, via the post-extract check at download.rs:246 — actually the FIRST cancel check is inside the extractor's poll loop at commands.rs:154) will see it. HOWEVER the `SidecarExtractor` only checks `cancel` at the TOP of its poll loop AFTER spawning the child (commands.rs:130-154); `install_pack` itself has NO cancel check before calling `extract`. So a pre-set cancel flag still spawns go-pmtiles once, then kills it on the first loop iteration. Wasteful but not incorrect. The genuinely confusing part: while queued behind the lock, NO progress events fire (the emitter is only driven from inside the extract loop), so the operator sees a spinner/disabled UI with no progress and a Cancel button whose effect is invisible until the lock frees. The UI disables all other download buttons during any `busy` (OfflineMapsSettings.tsx `downloading = busy != null`), so two concurrent downloads are not reachable from a single window — but TWO app windows / a programmatic invoke can reach it, and the design comments (commands.rs:80-94) explicitly anticipate concurrent downloads. The pre-extract cancel check belongs in `install_into_temp` before `extractor.extract` (mirrors the post-extract check already there at download.rs:246).
**Found in:** Pass 4 — Concurrency Reasoning.

### 5. The throttled progress emitter can suppress the FINAL progress sample, leaving the bar short of 100% until the done event

**Location:** `commands.rs:355-368` (`on_progress` throttle) + `commands.rs:164-168` (final size sample is pushed through the same throttled `on_progress`).
**Severity:** minor
**Evidence:** On `try_wait` returning `Ok(Some(status))`, the extractor takes a final size sample and calls `on_progress(written)` (commands.rs:166-167) to report the completed byte count. But `on_progress` is throttled by `should_emit` against `EMIT_THROTTLE` (400ms): if the previous emit was < 400ms ago, this final sample is DROPPED. The bar therefore freezes at whatever the last emitted value was (e.g. 96%) rather than reaching the true final byte count. The hook caps `percent` at 0.999 while `status==='downloading'` anyway (useDownloadProgress.ts:115), so it never shows 100% until the `done` event flips it to 1 (line 142). So in practice the visible bar jumps from ~96% straight to 100% on done — cosmetically acceptable, but the "final size sample before reporting completion" comment (commands.rs:166) promises a behavior the throttle silently defeats. The final completion sample should bypass the throttle (force-emit on terminal).
**Found in:** Pass 1 — Contract Violations.

### 6. `useDownloadProgress` rate/ETA can latch a stale rate across a long stall and never recover; ETA computed from EMA can wildly mislead

**Location:** `useDownloadProgress.ts:103-114`.
**Severity:** minor
**Evidence:** `rateRef` is an EMA that only updates when a progress event arrives with `now > prev.at && p.bytes >= prev.bytes` (line 105). go-pmtiles writes the `.part` file, but the byte-range extraction from a remote planet can stall (network) for many seconds while the polled file size is unchanged. During a stall, progress events still fire every `EMIT_THROTTLE` with the SAME `bytes` — so `p.bytes >= prev.bytes` holds (equal), but `(p.bytes - prev.bytes)` is 0 → `inst = 0` → EMA decays toward 0 correctly. That part is fine. The real issue: `remaining / rate` (line 114) uses the manifest's `typical_bytes` as `total`, but the actual extract can exceed `typical_bytes` (the size budget is `typical*3`, download.rs/commands.rs:75). When `bytes > total`, `remaining = Math.max(0, total - bytes) = 0` → ETA shows "~1 sec left" (formatEta floor, OfflineMapsSettings.tsx:54) for a potentially long remaining transfer, and `percent` is pinned at 0.999. So for any pack whose real size exceeds the estimate, the operator sees a stuck "99% · ~1 sec left" for the entire overage. Not a correctness bug in the transfer, but a misleading-UI bug rooted in trusting `typical_bytes` as a hard total. Consider clamping the displayed total up to observed bytes, or labeling the estimate.
**Found in:** Pass 5 — Error Propagation (the UI silently misrepresents progress when the estimate is wrong).

### 7. `download-progress`/`download-done` events are global (not scoped to a window); a second window's hook latches onto another window's download

**Location:** `commands.rs:359, 393, 410` (`app.emit(...)` broadcasts to ALL webviews) + `useDownloadProgress.ts:97-101` (latch onto the FIRST `packId` seen).
**Severity:** minor
**Evidence:** `AppHandle::emit` broadcasts an event to every webview. The hook latches onto whichever `packId` emits first (line 101) and the comment justifies this with "only one download runs machine-wide at a time" (useDownloadProgress.ts:11-14). That assumption holds for the serialized backend, but if the Settings panel is open in two windows (or re-mounted), BOTH hooks subscribe to the same global stream and both latch onto the same in-flight `packId` — so a download started in window A drives the progress row in window B even though B never initiated it (B's `downloadKey` is null → `showProgressRow` is false, so it's hidden, but B's hook IS tracking and B's Cancel — if somehow shown — would cancel A's download). The blast radius is small because `showProgressRow` gates on the local `downloadKey`, but the cross-window coupling is latent and contradicts the per-download isolation the hook claims. Scoping the emit to the initiating window (`emit_to`) would remove the ambiguity.
**Found in:** Pass 4 — Concurrency Reasoning.

### 8. `delete_pack` removes the archive file before the manifest write, and a failed manifest write leaves a manifest entry pointing at a now-deleted file

**Location:** `download.rs:284-295` (`delete_pack`: `fs::remove_file` at line 288 happens BEFORE `manifest.remove` + `write_manifest_atomic`).
**Severity:** minor
**Evidence:** `delete_pack` removes the archive file first (line 288, errors ignored), then loads the manifest, removes the entry, and only writes the manifest if an entry was removed (lines 289-293). If `write_manifest_atomic` fails (disk full, permission, etc.), the function returns `Err` but the archive file is already gone while the manifest still lists the pack. On next startup `sweep_orphans` will NOT remove this manifest entry (sweep only deletes files, never prunes manifest entries for missing files), and `init_packs`'s `register_path` will fail for it (file gone) and merely log a warning (commands.rs:518-521). The pack then shows in `basemap_list_packs` forever, contributing phantom bytes to disk-used, unrenderable, undeletable-cleanly (a re-delete would no-op the file removal but could succeed the manifest write — so it's recoverable by retrying delete, unlike bug #2). Lower severity because retry recovers, but the ordering is backwards relative to install's careful "manifest written last" discipline (download.rs:272-277): delete should drop the manifest entry first (durable record of intent), then remove the file.
**Found in:** Pass 3 — Failure Mode Reasoning / Pass 2 — Cross-Sibling (install writes manifest LAST for crash-safety; delete writes it LAST too, but for delete that ordering is wrong — the file is the thing that should outlive a partial failure).

---

## Design Concerns

- **The "nothing left behind on failure" invariant is enforced in `install_pack` but broken by its caller.** `download.rs` is meticulous: temp cleanup guard, post-extract cancel re-check, manifest-written-last. But `download_pack_blocking` (commands.rs) adds a register step AFTER `install_pack` returns Ok, and that step has no rollback (bug #2). The invariant should be owned end-to-end by the command, or `install_pack` should take the registry and register before returning (so the whole thing is one atomic success/rollback unit).

- **Free-space gating depends on the packs dir already existing.** `available_bytes` returning `u64::MAX` on stat failure (commands.rs:220) is a deliberate "don't block on a stat error" choice, but combined with the dir-not-created-until-first-install ordering it disables the gate exactly when it matters most (first download, bug #1). Sampling statvfs against a guaranteed-existing ancestor (the app data dir) would make the gate robust.

- **Cancellation has three check sites with a gap.** Cancel is checked (a) inside the sidecar poll loop (commands.rs:154) and (b) post-extract before install (download.rs:246), but NOT (c) before `extract` is first called. A cancel set while queued behind `install_lock`, or set between flag-insert and extract-start, still spawns the sidecar process once. Adding a pre-extract check in `install_into_temp` closes the gap and avoids a needless go-pmtiles spawn+kill (bug #4).

- **`typical_bytes` is overloaded as both the progress-bar denominator and a soft estimate, but the UI treats it as a hard total.** When the real extract exceeds the estimate (legitimate — the budget allows up to 3×), the bar pins at 99% with a "~1 sec left" ETA for the entire overage (bug #6). The progress contract would be more honest if `total` tracked `max(typical_bytes, observed_bytes)` or the UI distinguished "estimated" from "actual."

- **Global event broadcast + first-packId-latch couples concurrent/multi-window downloads** (bug #7). The single-download-machine-wide assumption is enforced by the backend lock and the single-window UI disable, but the event layer doesn't honor it; a per-window `emit_to` would make the isolation real rather than circumstantial.

(No bugs found in: the `planet_url` SSRF allowlist — `region_manifest.rs:124-143` is exact-host, rejects creds/ports/punycode/suffix lookalikes, well-covered; the bbox clamp math in `packs.rs`; the PMTiles extent/truncation/gzip-bomb checks in `validate.rs` — `extent_out_of_bounds` correctly handles overflow and the gzip cap uses `>=` against the take limit; `is_safe_pack_id` traversal blocking; `read_range` EOF clamping and the `MAX_RESPONSE_BYTES` cap in `mod.rs`. These are correct as written.)
