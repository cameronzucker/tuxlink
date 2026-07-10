# Handoff — 2026-07-10 — `jay-marsh-yew` (session 2 of 2) — center→dial P0 + docs shipped; NEXT SESSION: the missing P2P modes

Third handoff of this session-pair. Prior two (same branch):
`2026-07-09-jay-marsh-yew-wle-staged-channel-data-cracks-campaign.md` and
`2026-07-09-jay-marsh-yew-first-onair-connect-kd6oat.md` — read those for the
on-air campaign, the first-connect evidence, and the WDT registration wall.

## Merged to main this session

- **PR #1063** — RF-scale VARA data-socket timeout + abort data shutdown
  (`tuxlink-xzxk1` CLOSED). Proven on air the same night: B2F handshake
  completed over a 500 Hz RF link to KD6OAT.
- **PR #1064** — center→dial conversion (`tuxlink-9pzaj` P0 CLOSED):
  `center_to_dial_hz` at both tune boundaries (dial paths + Tune…/MCP
  `rig_tune`), VARA-FM exempted (FM listings ARE the RF frequency),
  fractional-kHz catalog centers preserved end to end, panel fields relabeled
  "Center freq (MHz)" with a USB-dial hint, MCP tool contracts pinned to
  center semantics. Two Codex adrev rounds; one CI round-trip (E0061 — the
  MCP port caller was missed when `ardop_tune_rig` grew the `sideband` param).
- **PR #1065** — docs: README declares VARA **on-air validated** (four on-air
  modes) + guided/agentic VARA setup; `16-vara-hf-deep-dive` rewritten around
  verified reality; wizard/transport/MCP/troubleshooting pages updated;
  verified staleness fixes (development.md CI claims, ux-anti-patterns menu
  spec, 27-settings paths, CONTRIBUTING rows).

## bd state ([fable] board)

CLOSED: `xzxk1`, `9pzaj`, `ntzzk` (was already merged in #1061), `bk15t`
(subsumed by hmoz8). OPEN — code work, no R2 needed: `hmoz8` (channels-API
ingest: SSID'd callsign + hours + per-channel BW auto-match — the big one),
`gbb05` (SSID stripping, P1), `m9kcd` (wait for REGISTERED before CONNECT),
`i3dg9` (session log never reaches jsonl — also the wire-evidence gap),
`o1e9w`, `46hof`, `nvgjy` (one-click dial), `c39af` (compression vocab +
session-type command — NOTE: upgraded relevance for P2P, below). Provisioning:
`y14cb`, `h874a`. OPERATOR: `ie7dy` (WDT client-type registration email).

## NEXT SESSION: the missing P2P modes (operator-set objective)

Where P2P stands per mode: **packet + ARDOP P2P work** (README maturity).
**VARA P2P is built but never on-air verified** — P2P rows with Listen arming
exist (`SessionIntent::P2p`), but two protocol gaps are now known to matter:

1. **`P2P SESSION` command is never sent.** The VARA spec (*VARA Protocol
   Native TNC Commands*, EA5HVK) is explicit: "This command must be used for
   P2P connections" — it sets the 4.6 s retry cycle for peer timing (the
   default `WINLINK SESSION` runs the 4.0 s RMS-DWELL cycle). Tuxlink has no
   session-type command at all (`OutboundCommand` lacks the variant). Fix
   belongs with `c39af` (which also carries the COMPRESSION vocab fix): add a
   `SessionType` outbound command, send `P2P SESSION` when
   `SessionIntent::P2p`, `WINLINK SESSION` otherwise (explicit > default).
2. **MYCALL SSID support** — P2P peers commonly use SSIDs; the spec allows
   `-1..-15, -T, -R`. Verify Tuxlink's MYCALL/CONNECT paths pass SSIDs
   through unmangled (relates to `gbb05`).

**The home lab is a complete P2P test bed** — this is the reason P2P is the
right next objective while `ie7dy` waits on WDT: P2P never touches the CMS, so
the registration wall is irrelevant, and the FT-710 ↔ G90+VARA2 pair (the
self-decode rig, memory `g90-selfdecode-rig`) already proved cross-rig ARQ
answers at 2300 Hz. A Tuxlink↔Tuxlink or Tuxlink↔WLE P2P session between the
two rigs validates the whole P2P path on the bench, operator-consented, no
external dependency. WLE is installed on R2 (`~/.wine-wle`, license-dialog
fresh) and can serve as the far-end P2P peer for interop.

Suggested next-session order: (1) read this handoff + the VARA spec PDF
(Pi scratchpad or re-fetch from n8jja/Pat-Vara); (2) implement `P2P SESSION`
+ compression vocab (`c39af`) with tests; (3) wire-walk the P2P UI flow
(listen arm + dial by intent); (4) stage the two-rig bench test for operator
execution; (5) `m9kcd` (REGISTERED wait) folds in naturally — P2P re-opens
sessions frequently.

## Machine / repo state

- **R2 is POWERED OFF** (operator). When it returns: VARA1/VARA2 + the
  diagnostic app (diag/xzxk1-onair @ 5c187c0d) need relaunching per the
  2026-07-09 handoff §machine-state; use rustup cargo
  (`PATH=$HOME/.cargo/bin`). VARA bandwidth config was left at 500.
- **Campaign scripts on R2 are now WRONG**: `/tmp/corrected_dials.py` and kin
  pass pre-converted DIAL frequencies; post-#1064 the app expects CENTERS.
  Retire or rewrite them before any reuse.
- **Wire-evidence capture rig** (operator wants B2F byte logs for the WDT
  case): socat hex tap between app and VARA (cmd 8300 + data 8301) +
  `RUST_LOG` debug + immediate session_log_snapshot save. Staged in prose in
  the previous message log; `i3dg9` is the durable fix.
- Worktrees pending ADR 0009 disposal: `tuxlink-yrrjq`, `tuxlink-xzxk1`,
  `tuxlink-9pzaj`, `docs-vara-refresh` (all merged-dead), plus this handoff
  worktree once its branch lands. Pre-existing: `tuxlink-graylinefix`,
  `verify-087`.
- This handoff branch (`agent-jay-marsh-yew/wle-differential-handoff`) is
  pushed but UNMERGED (self-merge policy-denied earlier) — it carries three
  handoff docs + bd snapshots. Operator: merge it, or the next session opens
  a PR for it.
- Known benign wart: commit `81fd0a2a` on the operator's `bd-tuxlink-ant8s`
  branch is a bd snapshot with a wrong (lint-fix) message — my cwd mistake,
  documented in handoff 2.

Agent: jay-marsh-yew
