# Handoff: 2026-05-31 — session end — modem-foundations shipped + planning queued

**Agent:** mink-swallow-kite
**Branch (main checkout):** `task-amd-main-ui`
**Session shape:** PR #163 (find-messages smoke-cluster) merged early; PR #172 + PR #175 merged through the day; political track on Winlink whitelisting set aside (operator-directed: "I give up on those people"); N0DAJ Wickenburg-AZ sysop identified for peer outreach (Douglas, RMS Packet + RMS Relay + RMS Trimode); autonomous modem-foundations pass during the operator's San Diego → Phoenix drive; brainstorm walked through 8 architectural questions; canonical specs committed; **PR #180 merged — modem foundations shipped.**

## TL;DR

The v0.5+ clean-sheet HF modem program now has a committed canonical
architecture: **12 files** spanning the program overview, 7 per-subsystem
specs, foundations bibliography (40+ citations), and the JPL-validated
HF bench-rig topology. The brainstorm-skill terminal state has been
reached. **Next session: invoke `writing-plans` in parallel across all
7 subsystem specs (operator-directed: "boil the lake").**

## What landed this session

- **PR #163** (`bd-tuxlink-1hu/find-messages`) — find-messages v0.1 (FTS5
  + Gmail-style inline operators + saved searches) released as 0.7.0;
  smoke-cluster fixups followed via PR #172.
- **PR #172** (`bd-tuxlink-1hu/find-messages`) — smoke-cluster polish +
  seed binary; merged with a clean radio-panel-shell P1 merge conflict
  resolution.
- **PR #175** (`bd-tuxlink-9phd/strip-pat-add-native-attachments`) —
  Pat-strip + native attachments; merged in parallel.
- **PR #180** (`bd-tuxlink-7qmc/modem-foundations`) — **this session's
  primary deliverable.** 12 files of canonical modem-program architecture.

## Architectural decisions baked into the canonical specs (overview §5.A)

1. **Multi-mode PHY ladder** with two architecturally-distinct families:
   - **Bit-adaptive OFDM** (DSL/xDSL-derived per-sub-carrier bit-loading) for main throughput modes
   - **Robustness modes family** for the floor: default is wide-band low-density-constellation OFDM (BPSK per sub-carrier, strong FEC; outperforms FT8-class narrow-FSK by ~100x at the same per-Hz SNR floor via Shannon-driven choice of going wider not denser); narrow-FSK reserved for situational crowded-band cases
2. **Payload-size-aware MAC routing** — short critical payloads route to robustness floor under degraded conditions; long messages stay in OFDM with ARQ
3. **ARQ mode-conditional** — applies above floor; floor uses FT8-pattern retransmit-the-whole-message
4. **Link adaptation is 2D** — (channel quality × payload size) → (mode family, mode within family, ARQ strategy)
5. **TCP host protocol** via existing `ModemTransport` (ADR 0015)
6. **AGPLv3-only** for tuxmodem + channel-sim crate
7. **Standalone public Watterson channel-sim crate** from day one
8. **Best-effort compute** target

## Multi-axis success criterion (overview §0)

Reframed from "exceed VARA" to **compelling alternative**: performance
competitive with VARA (close-to, not strictly exceeding) + open source +
well-documented + **AI-native for improvement** (substrate is designed
to be productive for AI-collaborative development; this is a first-class
success criterion per the project ethos). Per overview §4.6, AI-native
development substrate is now baked into the design discipline.

## Repository state

- Branch `bd-tuxlink-7qmc/modem-foundations` MERGED into `main` via PR #180; remote branch deleted by merge.
- Worktree `worktrees/bd-tuxlink-7qmc-modem-foundations/` disposed per ADR 0009 ritual (archived to `.claude/worktree-archives/bd-tuxlink-7qmc-modem-foundations-20260601T034519Z.tar.gz` first; rm -rf; git worktree prune).
- bd issue `tuxlink-7qmc` CLOSED.
- Main checkout currently on `task-amd-main-ui` (operator state).

## Canonical artifacts now in `main`

```
docs/superpowers/specs/2026-05-31-clean-sheet-modem-overview.md            (program umbrella)
docs/superpowers/specs/2026-05-31-clean-sheet-modem-1-channel-simulator.md (subsystem #1)
docs/superpowers/specs/2026-05-31-clean-sheet-modem-3-phy-waveform.md      (subsystem #3)
docs/superpowers/specs/2026-05-31-clean-sheet-modem-4-fec.md               (subsystem #4)
docs/superpowers/specs/2026-05-31-clean-sheet-modem-5-link-mac.md          (subsystem #5)
docs/superpowers/specs/2026-05-31-clean-sheet-modem-6-arq.md               (subsystem #6)
docs/superpowers/specs/2026-05-31-clean-sheet-modem-7-link-adaptation.md   (subsystem #7)
docs/superpowers/specs/2026-05-31-clean-sheet-modem-8-host-protocol.md     (subsystem #8)
docs/hardware/bench-rig-two-host-topology.md                               (HF bench rig)
docs/research/modem-foundations.md                                         (citation library)
```

## Queued next session — "boil the lake" parallel planning

Operator-directed (2026-05-31, end-of-session): invoke `writing-plans` in
**parallel across all seven subsystem specs**, not sequentially. The
brainstorming-skill terminal state is `writing-plans`; this session
reached it for the program-overview level. Next session converts each
canonical subsystem spec into an implementation plan.

**Recommended dispatch pattern next session:**

1. Read this handoff first.
2. Use the `superpowers:dispatching-parallel-agents` skill to dispatch
   seven parallel subagents, one per subsystem spec. Each subagent
   reads its assigned spec + the program overview + the foundations
   bibliography, then invokes `superpowers:writing-plans` to produce
   an implementation plan.
3. Subsystem assignments:
   - Agent A → subsystem #1 channel simulator
   - Agent B → subsystem #3 PHY / waveform
   - Agent C → subsystem #4 FEC
   - Agent D → subsystem #5 link / MAC
   - Agent E → subsystem #6 ARQ
   - Agent F → subsystem #7 link adaptation
   - Agent G → subsystem #8 host protocol
4. Each subagent files its plan to `docs/superpowers/plans/2026-XX-XX-clean-sheet-modem-N-<name>-plan.md` per writing-plans convention.
5. Operator reviews the resulting seven plans. The DSP-first sequencing (overview §3) means subsystem #1 (channel simulator) is the first implementation candidate even though the seven plans are produced in parallel.

**Subsystems NOT covered in this batch:**

- **#2 (RF measurement rig)** — substantially scoped in the
  `project_rf_measurement_rig_design` memory entry; implementation
  planning is hardware-acquisition-driven, not code-spec-driven
- **#9 (tuxlink integration)** — integration is downstream of #5/#6/#8;
  spec when the modem-stack subsystems concretize
- **#10 (standalone daemon packaging)** — same as #9

## Other state worth knowing

- **Find-messages worktree** (`worktrees/bd-tuxlink-1hu-find-messages/`)
  still warm with operator's cargo cache. PR #172 merged; the worktree
  can be disposed per ADR 0009 ritual when convenient. Operator may have
  `tauri dev` bound to it still — confirm before disposing.
- **31 other worktrees** were live across the repo at session start; their
  state was not audited this session. May want a worktree-inventory pass
  in a future session.
- **Political track on Winlink whitelist:** parked. Operator's N0DAJ
  Wickenburg outreach is the active sub-thread for getting an authoritative
  opinion before going public; that's operator-driven, not next-session
  agent work.

## Operator's paste-ready next-session prompt

```
Read dev/handoffs/2026-05-31-mink-swallow-kite-session-end-modem-foundations-shipped.md
first. Modem-foundations PR #180 merged. Brainstorming skill is done; the
canonical specs across 12 files codify a multi-mode bit-adaptive OFDM
architecture with wide-band low-density robustness floor, AGPLv3-only,
multi-axis "compelling alternative" success criterion.

Next move (operator-directed at session-end 2026-05-31): "boil the lake"
— dispatch seven parallel subagents via superpowers:dispatching-parallel-
agents, each running superpowers:writing-plans on one of the seven
canonical subsystem specs (#1 channel sim, #3 PHY, #4 FEC, #5 link/MAC,
#6 ARQ, #7 link adaptation, #8 host protocol). Each subagent produces
its plan to docs/superpowers/plans/. I review all seven when ready.

Subsystem #1 (channel simulator) is the first implementation candidate
per DSP-first sequencing (overview §3), but all seven plans get
produced in parallel.
```

Agent: mink-swallow-kite
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
