# Handoff — APRS telemetry + weather shipped; source-reactive env panel (2phz) is NEXT

Agent: kite-harrier-shoal · 2026-06-18

## TL;DR
Shipped two APRS backend slices to `main`: telemetry engine-emit (**#792**) and weather
(WX) parse+emit (**#796**). The remaining piece of the env feature is the **source-reactive
environmental PANEL** (`tuxlink-2phz`) — design is LOCKED, build it next. Also shipped this
session: install-docs hardening (**#790** / GH issue **#786**).

**Do NOT re-fix the RF panics** — the two Codex-found hostile-RF panics in the WX parser are
already fixed + regression-tested + merged in #796. The `unreachable!` in the field loop is
verified safe (width-match and value-match share the identical letter set). Nothing
panic-related is open.

## Shipped this session
- **#792** — APRS telemetry engine emit: `telemetry_store.rs` + `aprs-telemetry:new`
  (`InboundTelemetryDto`). Merged.
- **#796** — APRS weather (WX) parse + emit: `weather.rs` + `aprs-weather:new`
  (`WeatherReportDto`). Built via `build-robust-features`: TDD (23+ tests) + 1 Codex
  cross-provider adrev round (caught+fixed 5 issues incl. 2 hostile-RF panics, the snow unit,
  a lone-wind false positive, object-keying). Merged. Closed `tuxlink-wu2x`.
- **#790** — install docs: lead with `apt install ./deb` (not `dpkg -i`), `apt update`,
  `--fix-broken` troubleshooting; build-from-source fenced developers-only. GH issue **#786**
  documents the alpha-tester install failure (broken apt state on a Pi). Closed `tuxlink-p6bd`.
- Closed stale-merged: `tuxlink-gnru`, `z46p`, `m64z` (shipped in #768/#777).

## NEXT — source-reactive environmental panel (`tuxlink-2phz`, now unblocked)
Design is LOCKED (office-hours). Artifacts:
- Design doc: `~/.gstack/projects/cameronzucker-tuxlink/administrator-bd-tuxlink-xygm-design-20260617-102445.md`
- Mocks (rendered): `dev/scratch/2026-06-17-telemetry-panel-mock*.html` (+ `.png`)

Build:
- `useEnvStations` hook — merge `InboundTelemetryDto` (telemetry) + `WeatherReportDto`
  (weather) **by callsign** into one per-station view-model; keep a **bounded per-channel
  history ring**, buffered FRONTEND from launch (the engine emits point-in-time DTOs; the
  graph needs the series). No backend change.
- A **`channelKind → renderer`** card (source-reactive — each station auto-composes from the
  channels it emits, no weather/telemetry mode): `wind_dir`→compass, `pressure`→graded chart
  + rise/fall trend, `rain`→totals + fill bar, `temperature`/`humidity`/generic `T#`→graded
  X/Y-grid chart, digital bits→LED pills (BITS sense). A station sending BOTH WX + T# shows
  both. RF-honesty cues: raw-vs-scaled, stale dimming, absent channels hidden.
- Graded **small-multiples on a shared time axis** (operator's call — read magnitude, not just
  trend). Dock tab beside `Tac Chat | Map`, live count + honest empty state, pop-out (map's
  second-window pattern).
- Fast-follows (NOT the first slice): map-popup "▸ view data" hook (open the tab focused on a
  clicked station); a metric/imperial unit toggle.
- Gates: brainstorm-before-UI is DONE (the mocks). It's UI → run the **wire-walk** gate at
  done; browser smoke (grim) is post-merge, not a pre-merge gate.

## Other open follow-ups (bd)
`tuxlink-w636` (CI `.deb` install-test, Trixie/Bookworm) · `tuxlink-8pwi` (ARDOP reads creds
from stale `tuxlink-pat` keyring service) · `tuxlink-hyfo` (attachment-save hardening) ·
`tuxlink-90xb` (per-pin symbol sprites, design-gated) · `tuxlink-njzm` (decouple from Winlink
CMS, exploration).

## In-flight threads (pointers only — handled elsewhere)
- **Private security-disclosure work** continues in the sibling `winlink-re` repo (vendor
  notified; CERT/CC submission prepared). It stays in that private repo — NOT here. See its
  continuation brief.
- **vfb3** (in-app Winlink account mgmt) shipped **#787** by a parallel agent. Open operator
  decision: how to handle the shared Winlink web-API access-code in the public repo (build it
  "Shape B" config-injected vs. request Tuxlink's own issued key).

## DECONFLICTION LESSON (multi-agent — read this)
A worktree based off `main` BEFORE a parallel agent's PR merges will, when diffed against the
CURRENT `origin/main`, appear to **revert** that parallel work. This session the WX branch
silently looked like it was deleting all of vfb3 (#787, merged mid-build); caught it on the
diff-scope check + Codex flagging an "unrelated" file, then `git rebase origin/main`. **Before
opening/merging any worktree PR: `git fetch origin main` + `git rebase origin/main`, and
confirm `git diff --stat origin/main..HEAD` touches ONLY your files.**

## Working-tree / worktree state
- Main checkout on `bd-tuxlink-xygm/recover-handoffs` carries a large UNCOMMITTED README
  rewrite + untracked handoff docs — pre-existing operator state, NOT this session's; left
  untouched.
- Active sibling worktree (another session): `worktrees/bd-tuxlink-qjgx-alpha-logging`.
- All of THIS session's worktrees (p6bd install-docs, 2phz telemetry-emit, wu2x wx-parse) are
  merged + disposed per ADR-0009.
