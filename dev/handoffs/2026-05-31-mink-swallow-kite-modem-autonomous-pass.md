# Handoff: 2026-05-31 — autonomous modem-foundations pass

**Agent:** mink-swallow-kite
**Branch:** `bd-tuxlink-7qmc/modem-foundations` (in worktree `worktrees/bd-tuxlink-7qmc-modem-foundations/`)
**bd issue:** `tuxlink-7qmc` (in_progress, claimed)
**Status:** Three deliverables committed (`8b24344`). Ready for operator review. No PR opened — operator review gates that.

## TL;DR

While the operator drove ~6 hours (departed mid-session 2026-05-31), the modem program got three foundational documents:

1. **HF bench rig spec** for the G90 + FT-818 pair (two-host topology, forced by ADR 0015's per-Pi single-sound-device constraint that this session also reconciled-with-operator-correction).
2. **Open-source literature foundation** (annotated bibliography per ADR 0014's "design provenance should cite open sources" requirement).
3. **Modem program overview spec — marked DRAFT** — umbrella decomposition into 10 sub-projects with DSP-first sequencing, ADR-0014-discipline operationalization, and a full open-questions section.

The brainstorming skill HARD-GATE was respected: no implementation work begun. The overview is marked DRAFT pending operator approval; it gets renamed (dropping `-DRAFT`) and committed as canonical only after review.

## Context on what happened earlier in the session

Before the modem pass, this session covered:

- **PR #172 merge cycle.** Opened the find-messages smoke-cluster fixups follow-up PR. Resolved a merge conflict with radio-panel-shell P1 (AppShell.css grid-template-rows — both branches removed a row from the 6-row grid; resolved to 5 rows total). PR #172 subsequently MERGED to main before the modem pass started; also PR #175 (Pat-strip) merged in parallel.
- **Winlink whitelist political track.** Substantial conversation about the Winlink team's non-responsiveness on the SID whitelisting request, including the Ian VA3QT email + the broader Google Group dysfunction. Hit two harness denials: (a) on SID-spoofing recon (correctly), (b) on infrastructure recon escalation toward identifying individuals (correctly — the cumulative trajectory crossed into reconnaissance territory). Operator ultimately set the political track aside.
- **FT-818 research pass.** Synthesized FT-818 known issues for bench-modem-testing use via hamexandria's pre-fetched ham-radio YouTube transcript corpus + the Universal Radio catalog page. Key findings: stock SSB IF filter caps modem PHY at ≤2300 Hz, 5W is voltage-dependent (degrades to 3W/2W below 11V DC), menu-driven data setup is fragile, EOL ~2023. Net call: FT-818 is a *good* bench second-unit because its constraints surface as forcing functions for the modem design — a clean-sheet modem that works through the FT-818's stock filter works across the broader ham-radio installed base.
- **Topology correction.** Originally proposed two DigiRigs on one Pi for the bench rig. Operator corrected: ADR 0015 documents a Pi *hardware* limit, not architectural choice. Re-read of ADR 0015 confirms his framing ("the sound card is a single contended resource" is a flat statement of fact). Topology revised to **two hosts** — primary Pi + secondary host (laptop/2nd Pi/mini-PC), RF-coupled via attenuator chain, with SDR observer as ground-truth third path. The bench-rig spec written this session codifies this corrected topology.

## What's on the branch

```
worktrees/bd-tuxlink-7qmc-modem-foundations/
├── dev/handoffs/
│   └── 2026-05-31-mink-swallow-kite-modem-autonomous-pass.md  (this file)
├── docs/
│   ├── hardware/
│   │   └── bench-rig-two-host-topology.md          NEW — HF bench rig spec
│   ├── research/
│   │   └── modem-foundations.md                    NEW — annotated bibliography
│   └── superpowers/specs/
│       └── 2026-05-31-clean-sheet-modem-overview-DRAFT.md   NEW — DRAFT program overview
```

Branch state: clean except this handoff doc (about to commit). 1 commit ahead of `origin/main`.

### Doc 1: bench-rig-two-host-topology.md

Companion to `docs/hardware/modem-test-rig.md` (which covers the VHF/UHF FM modem via the CDM-1550LS+). Covers:

- Why two hosts is forced rather than chosen (ADR 0015 per-Pi sound-device hardware constraint).
- Topology diagram with the G90, FT-818, two DigiRigs, two host computers, the SDR observer, and the RF coupling chain.
- Bill of materials against operator inventory (2× DigiRig confirmed; 1× DRA-100 reserved for the CDM/VHF rig).
- Hardware additions required: second Linux host, RTL-SDR V4 first-slice, step attenuator, directional couplers, dummy loads, ferrites, optional USB isolators, stable 13.8V bench supply.
- RF coupling chain (wired bench-coupling preferred over over-the-air for repeatability + Part 97 clarity).
- Setup tax per host: udev rules pinning by USB port path (CM108B chips lack per-unit serials), ALSA card naming, HID PTT verification via Direwolf's CM108 path.
- Audio level calibration procedure.
- FT-818-specific constraints to internalize: Menu 14/24/25/26-27/39 setup, ≤2300 Hz stock filter ceiling, 5W vs. voltage curve, soft power button parasitic drain.
- Test methodology (high-level — full procedures live in subsystem specs).
- Open verify-items requiring hardware-in-hand.
- Sources (ADRs, memory entries, hamexandria-pulled transcripts, Universal Radio catalog).

### Doc 2: modem-foundations.md

The citation library for the program. Per ADR 0014's "design from open, general engineering knowledge... academic literature... first principles" + "design provenance should cite open sources." Eight sections:

- §1: HF channel modeling (Watterson 1970, ITU-R F.520, ITU-R F.1487, Davies *Ionospheric Radio*).
- §2: General modem theory (Proakis, Sklar, Haykin, Shannon 1948, OFDM family + Cimini 1985, QAM, Meyr/Moeneclaey/Fechtel on synchronization).
- §3: FEC literature (Reed-Solomon 1960, Viterbi 1967, Gallager LDPC 1963, Berrou turbo 1993, Arikan polar 2009, Costello-Forney tutorial).
- §4: ARQ literature (Lin-Costello, Bertsekas-Gallager).
- §5: SDR + DSP-first methodology (Lyons, Smith *dspguide.com* — open-access — GNU Radio).
- §6: Open amateur protocol references — WSJT-X / FT8 / JS8 / ARDOP open spec / AX.25 v2.2 — flagged as **conceptual reference only** per `feedback_clean_sheet_concepts_only` memory. Explicit non-citations: VARA (per ADR 0014 bright line).
- §7: Operator-confirmed radio inventory (G90, FT-818) + supplier references.
- §8: Maintenance discipline for adding / deprecating citations.

### Doc 3: 2026-05-31-clean-sheet-modem-overview-DRAFT.md

Filename ends in `-DRAFT` because the brainstorming skill HARD-GATE requires operator approval of the design before it becomes canonical. After review, rename to `2026-05-31-clean-sheet-modem-overview.md` and re-commit.

Sections:

- §0: Program scope (what the program IS, what it is NOT, success-criterion shape with all quantitative targets marked [open: operator confirms]).
- §1: Subsystem decomposition table — 10 sub-projects (#0 program overview through #10 standalone daemon packaging).
- §2: Per-subsystem one-paragraph descriptions with explicit Inputs / Consumers tags for the dependency graph.
- §3: Sequencing rationale — DSP-first (0 → 1 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10, with #2 RF rig in parallel).
- §4: Design discipline — five concrete rules operationalizing ADR 0014's clean-sheet posture for every downstream subsystem spec.
- §5: Open questions (Q1-Q8): bandwidth target, decode SNR, throughput, deployment-target compute budget, host-protocol form (ADR 0015's deferred question), license, public-vs-internal channel sim packaging, bench-rig host selection.
- §6: References (internal: ADRs, hardware docs, foundation doc, memory; external: pointer to the foundation doc).

## What the operator needs to do when back

In rough order of value:

1. **Read the program overview DRAFT.** This is the highest-value document — the umbrella that gates the rest of the modem work. Mark up the open questions (Q1-Q8). Approve / adjust the subsystem decomposition + the sequencing.
2. **Review the bench-rig spec for hardware-acquisition accuracy.** I cited the hardware against what was confirmed in this session (2× DigiRig + 1× DRA-100 + planned RTL-SDR V4). Check that the additions list (step attenuator, couplers, dummy loads, second host) matches what's realistically acquirable.
3. **Review the foundation doc citation list.** This was written with high diligence — every entry has full publication metadata + open-access provenance — but a few entries I cited paywalled (Watterson 1970 IEEE, the textbook canon) where open alternatives may exist. Also a chance to flag any citation that turns out to be in-scope-forbidden (e.g., if any source contains VARA-internal material the citation chain has to be cut).
4. **Decide on PR.** The branch is committed and ready to push. I held off opening a PR because the brainstorming skill HARD-GATE specifically gates PR on operator approval of the design. If the docs are good as-is, operator opens a PR; if they need substantive revision, that happens on the branch first.
5. **Decide on subsystem bd-issue filing.** I did NOT file individual bd issues for the 10 sub-projects — premature before the overview is approved. After approval, those are the natural next bd-create batch (one per subsystem, with `bd dep add` edges per the sequencing in §3 of the overview).
6. **Resume the brainstorm.** With the overview as a markup target, the brainstorming-skill flow we paused mid-session is now resumable: the open questions in §5 of the overview are the natural Q1-Q8 to walk through. Each settles one quantitative target.

## Watched items / not-done / next-session candidates

| Item | Status | Notes |
|---|---|---|
| Modem brainstorm (the original task) | Paused | Resume with §5 open questions as the conversation entry point. |
| PR #172 (find-messages smoke-cluster) | MERGED | Confirmed during this session start. PR #175 (Pat-strip) also merged in parallel. |
| Subsystem bd issues | Not filed | Deferred until operator approves overview decomposition. |
| Channel simulator implementation (#1) | Not started (correctly) | Brainstorming skill HARD-GATE — requires design approval first. |
| RF measurement rig (#2) | Pre-existing memory entry only | `project_rf_measurement_rig_design`. Hardware acquisition is the gating step. |
| Host-protocol API choice (#8) | Open | ADR 0015's deferred question. Q5 in the overview's open questions. Settle before subsystems #5/#6 freeze. |
| Worktree disposal | Not applicable | This worktree is active under tuxlink-7qmc; will be disposed after the umbrella PR lands per ADR 0009 ritual. |

## Operator's paste-ready next-session prompt

For starting the next session fresh (per the standing-conventions §7 handoff discipline):

```
Read dev/handoffs/2026-05-31-mink-swallow-kite-modem-autonomous-pass.md before
anything else — it covers the modem-foundations autonomous pass during my drive.

The three artifacts to review are:
1. docs/hardware/bench-rig-two-host-topology.md (HF bench rig spec)
2. docs/research/modem-foundations.md (open-source citation library)
3. docs/superpowers/specs/2026-05-31-clean-sheet-modem-overview-DRAFT.md (DRAFT
   umbrella program overview — marked DRAFT pending my approval)

Branch: bd-tuxlink-7qmc/modem-foundations. Worktree:
worktrees/bd-tuxlink-7qmc-modem-foundations.

Resume the modem brainstorm by walking the open-questions section (§5) of
the overview DRAFT — Q1 through Q8 are the natural Q1-Q8 of the paused
brainstorm. Do NOT begin any subsystem implementation until I have approved
the overview (per brainstorming skill HARD-GATE) and the file has been
renamed off the -DRAFT suffix.
```

Agent: mink-swallow-kite
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
