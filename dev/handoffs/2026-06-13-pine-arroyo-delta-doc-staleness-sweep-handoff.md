# Handoff — pine-arroyo-delta — doc staleness sweep (rewrites pending)

Date: 2026-06-13 · Agent: pine-arroyo-delta

## One-line

Doc-staleness **audit is done** (4-agent, code-verified); the **rewrites are the next session's job**. Plus a big session of completed/merged work (Sonde, releases, README) summarized below.

## Active in-flight: tuxlink-ivc1 — doc staleness sweep + README tightening

- **Branch:** `bd-tuxlink-ivc1/doc-staleness-sweep` (off `main`), **pushed**, **no PR yet** (deliberate — open the PR once the rewrites land, to avoid the merge-dead-mid-work trap that bit #675/#676 this session).
- **Commit landed:** `988b8b30` — moved Install + Uninstall out of README into `docs/install.md` (brief Releases-page pointer remains); fixed install.md staleness (it claimed prebuilt AppImages "forthcoming" — releases ship .deb/.rpm/.AppImage x86_64+arm64; added the missing WebKitGTK 4.1 prereq). README 358 → 305 lines.
- **Full audit findings:** `bd show tuxlink-ivc1` (notes) — the canonical, branch-independent work list.

### THE WORK LIST (verified stale, fix these — evidence already confirmed against code)

| File(s) | Stale claim | Correct today |
|---|---|---|
| 02, 03, 08, 15 | menu paths to removed `Tools → Settings` leaves ("Connection", "ARDOP HF", "Identity") | ARDOP/VARA config lives in the **radio panel**; Connection leaf removed; Tools→Settings has only GPS & Privacy + Map tiles + the unified Settings panel |
| 03, 27, 32 | "one callsign per install" / "Settings → Identity" (singular) / "no Identity panel" | FULL+tactical multi-identity shipped under **Settings → Identities** (IdentitySwitcher + IdentitiesSettings) |
| 19, 29, 32 | outbound attachments "not sent yet" / "no resize tools" | outbound attachments **shipped** + attach-time image resize/transcode (Small/Med/Large); **crop still unshipped** |
| 18, 23 | "Tools → Catalog request" | **Message → Request Center…**; GRIB form lives in Request Center (not HTML Forms) |
| 16 | VARA peer-to-peer described as a working flow | **pending** — VaraRadioPanel.tsx:5 "No RF connect-to-peer" |
| 20 | ICS-309 / Position / Check-In "scheduled for follow-up" | already have **native composers** (src/forms/*); Damage Assessment is view-only |
| 26 | broadcast-precision dropdown lists 8-char + Full GPS | only **4-char (default) + 6-char** ship; keep the Maidenhead explainer but clarify |
| 27 | intro "no large preferences window" | there **is** a unified Settings panel (Tools → Settings) |
| ux-anti-patterns.md | "tuxlink manages Pat / Pat daemon is an implementation detail" | no Pat daemon — NativeBackend replaced the sidecar (PR #175) |
| 32 | omits new Tuxlink-unique features | add **APRS tactical chat** + **native Benshi UV-Pro control** to "what Tuxlink has that Express doesn't" |

**~18 files audited CLEAN** (01, 04–07, 11, 14, 17, 21–22, 24–25, 28, 30–31, 33–34, `development.md`). Do not touch.

**Out of scope (do NOT rewrite):** `docs/plans/`, `docs/superpowers/{plans,specs}/`, `docs/adr/`, `docs/design/mockups/` — dated historical records; their "Pat sidecar" / "tuxmodem" references are accurate *for their date*.

### OPEN DECISION — `12-cat-and-rigctld.md` (operator chose: rewrite)

This bundled help topic documents a **non-existent** "Tuxlink rigctld integration" (a Settings → Radio → rigctld panel + live frequency display). Hamlib rig control is deferred to v0.1+; no rigctld client exists in the code. **Operator's call: rewrite** (option A): reframe rigctld/CAT as **external, operator-run** infrastructure that Tuxlink does not yet integrate with (e.g., for Dire Wolf PTT) — delete the fictional Tuxlink-integration / Settings-panel / live-frequency-display sections. Then reframe the **Hamlib-model rows in 10-digirig.md and 13-radio-specific-notes.md** as "for external rigctld setups" to match. Keep inbound links from 09/11 intact.

### Discipline for the rewrites
- **Voice:** tuxlink declarative / present-indicative; no temporal hedging ("currently/for now/today"), no defensive self-assertion. (NOT gstack casual chat voice.)
- Corrections are already code-verified (evidence in bd-ivc1); re-verify only if a fix is ambiguous.
- `pnpm install` then `pnpm lint:docs` (validates cross-links) before pushing — this worktree needs `pnpm install` first (fresh worktree).
- Continue on `bd-tuxlink-ivc1/doc-staleness-sweep`; **open the PR only when the rewrites are done.**

## Completed + merged this session (context, no action needed)

- **Sonde rebrand + extraction (tuxlink-twx0):** tuxmodem → Sonde, whole workspace; extracted to `cameronzucker/sonde` (private, **green CI** — first real compile of the renamed code); removed from tuxlink (Op B6, PR #643). PRs #639/#643 merged. ADR 0019.
- **Two-channel releases (tuxlink-cj1b):** release-please PR now opens as a **draft** (agents can't ad-hoc merge); `release-merge.yml` daily cron cuts **nightly pre-releases** (hidden from "Latest"); `promote-release.yml` is operator-only for stable milestones. PR #667 merged; draft-PR enforcement **confirmed** (release PR #669 opened draft). CLAUDE.md/AGENTS.md updated.
- **README adrev + fusion (tuxlink-fvpd #675, tuxlink-bybr #676):** strategic/tactical fusion positioning, APRS tactical chat + Benshi UV-Pro features, native-protocol-engine reframe, Sonde-as-ambition, comparison table (WLE/Pat/Tuxlink), alpha-note moved below the value. Both merged. (Codex cross-read was quota-blocked; Gemini Flash manual cross-read converged on "undersold the tech" — now fixed.)

## Cleanup backlog (not blocking)

Merged-dead worktrees from this session are ADR-0009 disposal candidates: `bd-tuxlink-twx0-sonde-rename`, `bd-tuxlink-p838-sonde-removal`, `bd-tuxlink-w7yl-release-cadence`, `bd-tuxlink-cj1b-two-channel-releases`, `bd-tuxlink-fvpd-readme-fusion-pass`, `bd-tuxlink-bybr-readme-adrev-copy`. (twx0 also holds a local-only `_split_sonde_test` branch.)
