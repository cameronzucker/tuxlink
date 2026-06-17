# Session handoff — 2026-06-17 — `dune-taiga-pine`

README visual overhaul (screenshots → cropped/floated layout → animated demo →
demo removed) + benlink `SET_SATELLITE_INFO` reverse-engineering.

## Branch / tree state

- All code work landed on `main` via merged PRs (#767, #769, #773, #778, #780,
  #782, #783, #784, #788, #789). Nothing of this session's work is stranded.
- The main checkout (`bd-tuxlink-xygm/recover-handoffs`) was held by other live
  sessions for most of this session, so everything ran in per-issue worktrees
  off `main`. Those worktrees are now **merged-dead** and pending disposal (see
  Cleanup).
- This session did **not** touch app code — README, README images, and the
  `dev/readme-screenshot-harness/` dev tooling only.

## Completed

### README visual overhaul (all merged to `main`)
- Replaced mockups with **real-app feature screenshots** rendered in WebKitGTK
  from a privacy-safe fixture build (`docs/readme/images/`): mailbox hero, the
  ARDOP HF / Packet (AX.25) radio panels, the simultaneous HF/VHF workspace, the
  color schemes, wizard, Request Center.
- **Layout, Geographica-style**: vertical panels **floated** (`align=right/left`)
  beside the real feature bullets (not crammed in a shrinking table); color
  schemes as a side-by-side light/dark comparison; full-width workspace shot.
- **Maturity section** updated to the operator's ground truth: three on-air
  modes (APRS tactical chat over Bluetooth KISS, 1200-baud AX.25 packet, ARDOP
  HF) + the simultaneous HF/VHF workspace as functional today.
- **Reusable capture tooling** in `dev/readme-screenshot-harness/`:
  `harness.tsx` (Tauri IPC shim + `view`/`dock`/`scheme` scene params +
  `window.__harness.emit/.scheme` hooks + a `tile://pmtiles/world` → vite
  redirect so the map renders real tiles), `snapshot.py` (WebKitGTK stills with
  selector/`clip:` cropping), `screencast.py` (Playwright interaction recording),
  and ffmpeg → animated WebP recipes. Documented with a "how the pieces fit" map
  + a gotchas list.

### Animated demo — built, then **removed** (operator call)
- Built a looping WebP demo of the adaptable workspace (messages ↔ APRS chat +
  map), iterated for crispness, then **removed it from the README** (#788/#789):
  for a pre-alpha product the rough APRS map open/load reads as buggy and
  undersells the app. The static screenshots stay; the mechanism is kept +
  documented for re-embedding once the map surfaces are polished.

### benlink `SET_SATELLITE_INFO` (opcode 77) RE — drafted, NOT submitted
- Full reverse-engineering of the BTECH UV Programmer's opcode-77 request
  payload (30-byte layout: name GB2312 ≤20 B, azimuth 9b, elevation 8b, range
  km u16, altitude km u16, countdown s u16), verified byte-for-byte against
  benlink's `Bitfield` codec with a golden vector.
- Artifacts are **local-only** in `dev/scratch/benshi-re/` (gitignored):
  `SATELLITE-INFO-RE-FINDINGS.md`, `draft-benlink-satellite_info.py`,
  `draft-benlink-pr2-body.md`.

## Pending / decisions owed

1. **benlink follow-up PR to `khusmann/benlink`** is drafted but **not opened.**
   Open question the operator must decide: ship the request struct now with the
   reply body marked "inferred from SET_* convention, unverified", **or** hold
   until one BLE/RFCOMM capture of the vendor app confirms the reply. Drafts in
   `dev/scratch/benshi-re/`.
2. **Polished animation, later.** Re-embed a demo once the APRS map open/load is
   smooth (pre-warm the map so tiles are drawn before it slides in). The
   screencast tooling + recipe + gotchas are ready in the harness README.

## Cleanup (housekeeping, not blocking)

Merged-dead worktrees from this session, all safe to dispose via the ADR 0009
ritual (their PRs landed; bd issues already closed):
`bd-tuxlink-{twi5,uvo5,zx30,rpac,x0m5,32am,bkhf,13gx,0ag2,p1q2}`, plus this
handoff's `w920` after it merges. ~node_modules per worktree; disk is not tight.

## Lessons logged

- **Verify the staged diff, not `git status` letters.** #788 shipped only a
  `.webp` deletion because a multi-path `git add` aborted on the already-`git
  rm`'d webp pathspec (git stages nothing when one pathspec fails to match),
  leaving a broken image ref on `main`; hotfixed in #789. Always
  `git diff --cached` before committing.
- **`device_scale_factor=2` does not raise Playwright video resolution** — it
  letterboxes the frame grey. Crispness is the encode `q:v`, not capture size.
- **GitHub animates WebP/GIF/APNG only** (no committed `<video>`; camo strips
  animated SVG).
- **Verify the whole rendered artifact**, not one detail — a crisp-text check
  missed a half-frame grey box.
