# Handoff — Qwen teacher selected (235b→122b) + Tuxlink-as-sim architecture direction

**Agent:** kingfisher-cove-yew · **Date:** 2026-07-05
**Session shape:** shipped e7z7d (find_stations distance/bearing), built the Qwen distillation
teacher-selection experiment end-to-end, selected 235b→122b, and — the highest-value output —
firmed up a new architecture direction (Tuxlink-as-simulation-harness). Handing off BEFORE building
that architecture so it gets a clean context window.

## Shipped + merged this session
- **tuxlink-e7z7d FIX A** — `find_stations` returns `distance_km`+`distance_mi`+`bearing_deg` from the
  operator grid, sorted nearest-first. **Merged to main (PR #1016)**, wire-walk passed, CI green both
  arches. Sim-parity companion on `bd-tuxlink-6zkb6/discriminating-eval` (commit 42b318dd). Issue CLOSED.
- **Harness Step 1** (commit 866f05c1 on 6zkb6): `run_eval.py` gained `--provider openai` (drives
  OpenRouter via the existing `APIClient`) + `--tools {all,required,<path>}` (configurable tool subset).

## The Qwen teacher-selection result (the main thread)
Ran the discriminating gate (16 scenarios) via OpenRouter against: qwen3.5-122b-a10b (student),
qwen3.5-397b-a17b + qwen3-235b-a22b-2507 (teacher candidates), gpt-oss-120b (control).

- **Qwen >> gpt-oss** (pivot validated): 122b 6/14 scored vs gpt-oss 2/14.
- **Binary gate SATURATES** — 122b/235b/397b all cluster (5-6/14); it cannot rank teachers.
- **Qual is the instrument.** Rubric (completion/grounding/tool-use/honesty, 0-2 each, per scenario,
  scored by 3 subagents): **397b 6.06/8 ≈ 235b 5.81/8 > 122b 5.38/8** (within-grader noise between
  teachers = TIE). The teacher gap is **multi-turn completion** (teachers finish chains the 122b
  stalls on) — the axis TAU2 measures and BFCL/binary-gate miss.
- **TEACHER LOCKED: 235b→122b** (recorded on tuxlink-vqzq5). Rationale: qual-tied with 397b BUT
  self-hostable (235B@4bit ~117GB fits 128GB w/ KV headroom; 397B ~199GB does not), open-weight,
  22B active > 17B. Endgame: a 122b student on commodity/128GB hardware.
- **Honesty confound (operator caught it):** "student more honest" was partly a stall confound (it
  stalls before the send, so cannot false-sent). Corrected: false-sent is a hard-chain
  completion-coupled failure; gold-gen MUST judge-filter false-sent + fabrication.
- **Deepest finding:** the DOMINANT failure driver is the SIM's `{ok:true}` tool stubs + ambiguous
  mock connect/send returns (fabrication/stall/false-sent are mostly ENVIRONMENT artifacts). Substrate
  must lead training. → motivates the new architecture below.
- **deep-research** (verified/cited): 235b-a22b is the best open-weight fallback; thinking≈instruct
  (~1pt wash, large-advantage claims refuted); the closed Max flagships (3.7-max etc.) are API-only /
  no disclosed architecture → deprioritized. Frontier→235b elevation POSSIBLE (gold-SFT only, no
  logit-KD) but DEFERRED (sim-first; measure the frontier's on-task gap cheaply before a pod-scale FT).

Public writeup of all this committed: `docs/notes/2026-07-05-choosing-a-distillation-teacher.md`
(operator will link it / blog it on tuxlink.org).

## THE NEXT TASK — build in a fresh context (tuxlink-cnz5o, P1)
**Tuxlink-as-simulation-harness** (this worktree's reason): make the REAL Tuxlink MCP router the
training/eval environment by injecting **scenario-driven synthetic state at the PORT boundary**
(evolve the `tuxlink-mcp-testserver` mock ports into scenario-driven fixtures), instead of the separate
Python `StatefulSimulator` that must be kept in parity. Parity becomes tautological. UNIFIES four uses
of one scenario schema: train the student · gate regressions per-build · reproduce field agentic bugs
end-to-end · observe live agent behavior. Realizes the observability architecture recorded 2026-07-05.

- **Key design call:** inject at the tool-RESULT boundary (script tool outcomes), NOT deep
  transport/protocol simulation. Agentic behavior is a function of tool results, not modem internals.
- **Open scope question the PoC answers:** what fraction of scenario space is reproducible at the port
  boundary (strong guess: large majority).
- **Constraints:** additive + test-mode-gated (never touch production transmit paths / MonolithPorts).
  Build-coupling is a non-issue per operator (Tuxlink build is fast on anything past a Pi; dwarfed by
  model download).
- **Next-session flow (build-robust-features):** office-hours/design → seed **ADR 0021**
  (`docs/adr/0021-tuxlink-as-simulation-harness.md`, was about to be written) → 5-round adrev (Codex) →
  subagent-proof plan for a **PoC slice** (scenario-driven ports for the easy tier: config/position/
  station-cache/mailbox/solar/catalog + harness plumbing to drive an OpenRouter agent against the real
  MCP server for a couple of scenarios, comparing fidelity vs the Python sim) → build + R2 verify →
  wire-walk (operator flows) before ready.

## Worktrees / state
- **This worktree** (`worktrees/bd-tuxlink-cnz5o-tuxlink-as-sim-harness`, off origin/main): holds the
  writeup + this handoff; the ADR 0021 was NOT yet written (start there). Clean otherwise.
- **`worktrees/bd-tuxlink-6zkb6-discriminating-eval`**: the elmer-distill harness. Has committed
  Step-1 + sim-parity (pushed). Carries PRE-EXISTING uncommitted changes from a PRIOR session
  (` M dev/elmer-distill/docs/serving-refuser-runbook.md`, untracked `dev/elmer-distill/dev/`) —
  LEFT UNTOUCHED both this session and last; inventory before disposing. The eval transcripts
  (`dev/elmer-distill/eval-runs/{qwen122b-full,q3-235b-a22b,qwen397b-full,gptoss120b-full,q3-max,...}`)
  are the qual-analysis source data (gitignored run outputs).
- **OpenRouter key** is in the OS keyring: `secret-tool lookup service elmer-openrouter account teacher`
  (works from an agent shell; used for the sweeps).

## Backlog filed this session
- **tuxlink-cnz5o** (P1) — Tuxlink-as-sim architecture (THE next task).
- **tuxlink-vqzq5** (P1) — Qwen distillation track; carries the model IDs + teacher decision + the
  gold-gen honesty-filter requirement.
- **tuxlink-0lawk** (FIX B predict_path optional dials, gated on FT-8 u3m0g.2) ·
  **tuxlink-25l40** (global metric/imperial units toggle) ·
  **tuxlink-fgyh7** (Elmer New-conversation button non-render, converged-build bug) ·
  **tuxlink-4kyiq** (Elmer agent workflows, needs brainstorm).
- **tuxlink-2jqjb** (rig_meters/CAT): PARKED. Under Tuxlink-as-sim it collapses into one real tool
  backed by synthetic rig state (no separate mock). Its app-side real-read still needs the operator's
  CAT-serial read-model answer (meters are TX-time; modem holds the serial during a session).

## Pending decisions / notes
- **Elmer-vs-core-product prioritization** — operator flagged Elmer is a big detour from core product
  (WLE parity, a useful CMS loop) which may be what alpha testers are waiting on. Genuine call to make;
  worth an office-hours before committing more Elmer time.
- **R2 as one-box test bench** confirmed viable (x86 → native WINE VARA, no box64; BlueZ+btusb present).
  Needs a USB BT dongle (RTL8761B) for the UV-Pro. Bench consolidation, not urgent.
- Progressive tool disclosure: SHELVED (Probe 0 showed no upside; adrev "inverts down the ladder").
