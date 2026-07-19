# README Repositioning + docs/ELMER.md Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite README.md for the widened audience, ship a standalone docs/ELMER.md agentic-capabilities doc, and refresh the screenshot set from the real running app.

**Architecture:** Docs-plus-images branch. A grounding task verifies every capability claim against the tree and produces a citations file; writing tasks consume it. The app build for screenshots runs in the background from Task 1. No product code changes.

**Tech Stack:** Markdown (GitHub-flavored, mermaid), grim (Wayland capture), pnpm lint:docs, Codex CLI (GPT-5.5) for the adversarial round.

**Canonical spec:** `docs/superpowers/specs/2026-07-17-readme-repositioning-design.md`. Read the section named in your task before starting.

## Global Constraints

- **Voice (binding on every sentence of README.md and docs/ELMER.md):** read `/home/administrator/.claude/cameron-writing-voice-profile.md` BEFORE writing. Hard rules: NO em-dashes anywhere; Context-1 register (teaching voice, complete sentences, bold definitional lead-ins, exact values/paths); no first person; present indicative for facts; calibrated hedging ONLY where genuine uncertainty exists (maturity/validation statements), never on proven capabilities; no "today/currently/for now/honest" in shipped text; misconceptions named then corrected; cite authoritative sources for general claims.
- **Honest maturity is an invariant:** proven = plain fact; unproven = named unproven with a precise qualifier. Never fabricate a number, a validation state, or a competitor capability.
- **AGPL badge guard:** README keeps `License-AGPL%20v3-blue.svg` linking LICENSE. Any diff touching the badge row is checked character-by-character.
- **NO TRANSMISSION during screenshot staging.** Receive-only. No Connect, no beacon, no routine run that reaches a transmit step, no arm. RADIO-1 gates operator execution; this plan does not include any.
- **Privacy check before committing any capture:** no third-party message bodies beyond public APRS party-line traffic, no precise third-party positions beyond what APRS already broadcasts publicly, nothing from the operator's personal mailbox unless it is a self-addressed test message.
- **Image weight:** each PNG ≤ 500 KB after optimization where tooling allows; total new image weight ≤ 4 MB. Reuse existing images where the UI is unchanged.
- **Gate for every task that edits docs:** `pnpm lint:docs` passes.
- **The main checkout is untouched.** All work in `worktrees/bd-tuxlink-d8f3l-readme-elmer-pass`, branch `bd-tuxlink-d8f3l/readme-elmer-pass`.
- **Commits:** conventional type+scope; trailer block with the session moniker and `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.

## File structure

| File | Responsibility |
|---|---|
| `dev/scratch/d8f3l-facts.md` (gitignored) | Verified claim ledger: every capability fact with its verification command output and file:line citation |
| `docs/ELMER.md` | The standalone agentic-capabilities deep doc |
| `README.md` | Full rewrite |
| `docs/readme/images/*.png` | New captures (exact filenames fixed in Task 4) |

---

### Task 1: Grounding — verified fact ledger + background build kickoff

**Files:**
- Create: `dev/scratch/d8f3l-facts.md` (dev/scratch is gitignored; this is working material, not a shipped artifact)

- [ ] **Step 1: Kick off the screenshot build in the background NOW** (it is the long pole; later tasks run while it compiles):

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-d8f3l-readme-elmer-pass
pnpm install --frozen-lockfile
nohup pnpm tauri build --bundles deb > /tmp/claude-1000/d8f3l-build.log 2>&1 &
echo $! > /tmp/claude-1000/d8f3l-build.pid
```

Expected: build runs 30–90 minutes on this Pi. Do NOT wait; continue to Step 2. (Rationale: the operator excluded converge-build because it omits real feature dependencies; this build runs from the pass's own worktree and the app will launch against the operator's real config/models/tiles.)

- [ ] **Step 2: Verify every claim below and record each in `dev/scratch/d8f3l-facts.md`** as `CLAIM / VERIFY COMMAND / RESULT / CITATION`. A claim that fails verification is recorded as FAILED with what is actually true; writing tasks use the corrected fact.

```text
1. MCP tool count. VERIFY: count tool registrations in src-tauri/tuxlink-mcp-core/src/router.rs
   (grep -c the registration macro/pattern actually used; read the file header first).
   Record the EXACT number; "79" from the brainstorm inventory is a candidate, not a fact.
2. Elmer uses the identical router in-process. VERIFY: read src-tauri/src/elmer/executor.rs
   for InProcessMcpInvoker + tokio duplex; cite lines.
3. Taint semantics: untrusted read locks send authority until restart; survives arming.
   VERIFY: grep taint in src-tauri/tuxlink-mcp-core/ and cite the enforcement site + test.
4. Armed egress: bounded windows, countdown, disarm. VERIFY: cite the EgressGuard source + UI surface.
5. Agent-send loop capabilities (predict_path, per-station dial w/ target/freq_hz/qsy_candidates,
   abort on every transport). VERIFY against docs/superpowers/specs/2026-07-01-elmer-agent-send-design.md
   AND the shipped tool signatures in the router.
6. Routines: designer/schedule/journal/dashboard/consent modes (attended vs automatic w/ design-time ack),
   10 routines MCP tools. VERIFY: count RoutinesPort tools in the router; cite ConsentGate transmit_mode.
7. Model support: Ollama/OpenAI/OpenRouter/Anthropic/custom presets; keyring-only keys; live switch.
   VERIFY: src/elmer model config surfaces + the keyring write site.
8. Transports maturity table (ARDOP/VARA/packet/telnet/UV-Pro/APRS chat/beacon/FT8/WWV): copy the
   validation wording from README(origin/main) maturity section + docs/user-guide/16-vara-hf-deep-dive.md
   + 37-ft8.md + 36-off-air-space-weather.md; note deltas the spec §3 requires (dockable+Routines
   built/CI-tested, field validation pending at next release; beacon operator-pending; VARA P2P pending).
9. VARA first-run caveat. VERIFY: bd show tuxlink-0nfe2; summarize the manual steps in one line.
10. wine-vara-setup vendoring. VERIFY: src-tauri/resources/wine-vara-setup/VENDORED.md exists; repo URL.
11. Comparison-table competitor cells. VERIFY each Winlink Express / Pat claim against docs/knowledge/
    (pat-winlink.md and the WLE corpus). Every cell gets a citation. If the corpus is silent on a cell,
    the row states only Tuxlink's capability and leaves the competitor cell as an honest "not in [client]"
    ONLY when the corpus supports it; otherwise drop the row.
12. Multi-window: three surfaces, persistence, ✕ semantics, ~30 MiB/window figure.
    VERIFY: docs/user-guide/38-pop-out-windows.md + the dockable spec; cite the measured-figure provenance
    (routines design spec measurement note).
13. WWV: decode schedule (:18/:45), STT model fetch script path. VERIFY: docs/user-guide/36 + scripts/.
14. Existing screenshots inventory: ls docs/readme/images/; note each file's subject and whether the
    UI it shows changed since capture (git log the relevant src/ areas if unsure).
```

- [ ] **Step 3: Commit nothing** (dev/scratch is gitignored). Report the ledger path and the build PID/status.

---

### Task 2: docs/ELMER.md

**Files:**
- Create: `docs/ELMER.md`

**Interfaces:**
- Consumes: `dev/scratch/d8f3l-facts.md` (all Elmer/MCP/Routines facts + citations).
- Produces: the doc README Task 3 links as `docs/ELMER.md`.

- [ ] **Step 1: Write the doc** per spec §4's seven-section outline. Requirements beyond the outline:
  - Every architectural claim carries an inline citation to the in-repo file (relative links: `src-tauri/src/elmer/executor.rs`, spec docs, ADR 0018).
  - One mermaid diagram: Elmer pane and external MCP clients as two entry paths converging on the single tool router, then EgressGuard, then transports. Keep it under ~20 nodes.
  - The security section is the longest section.
  - The tool count is the Task-1 verified number, stated once.
  - The limits section states per-transport validation inheritance using the Task-1 maturity wording, and names what Elmer refuses (unarmed egress, tainted send authority) in plain language.
  - Length target 250–400 lines. No screenshots in this doc (the README carries the Elmer screenshot).

- [ ] **Step 2: Verify** — every relative link resolves (`pnpm lint:docs` passes); zero em-dashes (`grep -c "—" docs/ELMER.md` returns 0); mermaid block renders (paste into a local `mmdc` if available, else visual-check the syntax against GitHub's mermaid docs).

- [ ] **Step 3: Commit**

```bash
git add docs/ELMER.md
git commit -m "docs(elmer): standalone agentic-capabilities deep doc (tuxlink-d8f3l task 2)"
```

---

### Task 3: README.md full rewrite (text; image slots reserved)

**Files:**
- Modify: `README.md` (from the origin/main base already in this worktree — confirm `git diff origin/main -- README.md` is empty before starting)

**Interfaces:**
- Consumes: the fact ledger; `docs/ELMER.md` (link target).
- Produces: image references using the EXACT filenames Task 4 will capture (list below) so Task 4 drops files in without editing prose.

- [ ] **Step 1: Rewrite** per spec §3, section order preserved. Image slots use these exact paths (Task 4 captures to these names; existing files that survive the staleness check keep their current names):

```text
docs/readme/images/tuxlink-multiwindow-workspace.png   (new hero: main + popped Tac Map + popped APRS Chat)
docs/readme/images/tuxlink-elmer.png                    (Elmer pane, visible tool call)
docs/readme/images/tuxlink-routines-designer.png        (designer canvas)
docs/readme/images/tuxlink-ft8-waterfall.png            (FT8 listener)
docs/readme/images/tuxlink-vara-setup.png               (VARA setup wizard surface)
KEEP-IF-CURRENT: tuxlink-mailbox.png, tuxlink-ardop-hf.png, tuxlink-color-night-red.png,
tuxlink-color-daylight.png, tuxlink-first-run-wizard.png, tuxlink-request-center.png
DROP: tuxlink-workspace.png (superseded by the multi-window hero)
```

  Content requirements not already exact in spec §3: comparison-table rows use ONLY Task-1-verified cells; the VARA/Wine section is 2 short paragraphs (what it is + the guided installer naming wine-vara-setup; the ARM/ARDOP line; the one-line first-run caveat); the Elmer teaser is ≤ 2 paragraphs + link; the maturity section keeps its four bold lead-in buckets and adds the spec-mandated updates verbatim from the fact ledger.

- [ ] **Step 2: Verify** — `pnpm lint:docs` passes (image links to not-yet-existing files WILL fail the linter: if so, `touch` zero-byte placeholders is BANNED; instead reorder — commit the README text with existing images only and let Task 4's commit swap the new references in. Check the linter's behavior first and pick the order that keeps every commit green). Zero em-dashes in the diff. Badge row byte-identical to origin/main's.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs(readme): full rewrite — voice profile, Routines/Elmer/multi-window/FT8/WWV/VARA sections, refreshed maturity (tuxlink-d8f3l task 3)"
```

---

### Task 4: Screenshot capture session

**Files:**
- Create: the five new PNGs listed in Task 3; replace any KEEP-IF-CURRENT image whose UI drifted.
- Modify: `README.md` only if the linter-ordering note in Task 3 Step 2 deferred image references.

**Preconditions:** the Task-1 background build finished (check `/tmp/claude-1000/d8f3l-build.log` tail for success; the .deb lands under `src-tauri/target/release/bundle/deb/`). The app binary also exists at `src-tauri/target/release/tuxlink`.

- [ ] **Step 1: Launch the app against the real environment.** Run the release binary directly (no install, no sudo): `WAYLAND_DISPLAY=$(ls /run/user/1000/ | grep -m1 wayland-) src-tauri/target/release/tuxlink &` — record the PID; kill ONLY this PID at session end. The operator's real config, STT model, and tiles are in their standard XDG paths and load automatically.
- [ ] **Step 2: Stage and capture with grim, one surface at a time.** For each: stage → `grim -g "$(slurp)" out.png` or full-screen `grim` + crop → Read the PNG → judge (crush/clipping/empty panels/privacy) → re-stage until right. Staging list: (1) hero: open Tac Map and APRS Chat, pop BOTH out (this exercises the shipped dockable feature), arrange main+two windows, capture the full desktop; (2) Elmer: ask it a docs question that triggers a visible `docs_search` tool call (no arm needed); (3) Routines designer with a real-looking routine open; (4) FT8 waterfall with live RX if the rig is attached and receiving, else the surface with its honest empty state and NOTE that in the report for an operator decision; (5) VARA setup wizard surface (wizard UI only; VARA does not run on ARM). RECEIVE-ONLY THROUGHOUT: no Connect, no arm, no beacon.
- [ ] **Step 3: Staleness pass on KEEP-IF-CURRENT images** — open each old PNG beside the live surface; recapture any that drifted; keep filenames.
- [ ] **Step 4: Optimize** — `command -v pngquant && pngquant --ext .png --force docs/readme/images/tuxlink-*.png` (skip silently if absent); verify each ≤ 500 KB, total new weight ≤ 4 MB (`du -ch`).
- [ ] **Step 5: Privacy pass** — Read every new/changed PNG once more against the Global Constraints privacy rule before staging.
- [ ] **Step 6: Kill the app (exact PID only), commit**

```bash
git add docs/readme/images/ README.md
git commit -m "docs(readme): fresh real-app screenshot set — multi-window hero, Elmer, Routines, FT8, VARA wizard (tuxlink-d8f3l task 4)"
```

**REVIEW LOOP: Tasks 1–4 form the first logical group — run the multi-perspective review now (claims-vs-citations audit; voice-profile conformance sweep; link/image integrity).**

---

### Task 5: Codex adversarial round + dispositions

- [ ] **Step 1:** Run Codex (GPT-5.5, ADR 0023 — pin if needed) over the finished docs:

```bash
cat > /tmp/claude-1000/d8f3l-codex-prompt.txt <<'EOF'
Adversarial review of the diff against origin/main in this worktree (run: git diff origin/main..HEAD).
Focus files: README.md, docs/ELMER.md. Attack angles: (1) claim accuracy — any capability stated
more strongly than the repo evidences (read the cited files); (2) audience fit — does ELMER.md hold
up for a skeptical staff engineer evaluating agentic software; (3) voice — em-dashes, hedging on
proven facts, first person, temporal framing; (4) the license badge must read AGPL v3;
(5) competitor-claim fairness in the comparison table. Output findings as markdown at the end.
EOF
cat /tmp/claude-1000/d8f3l-codex-prompt.txt | npx --yes @openai/codex review - 2>&1 \
  | tee dev/adversarial/2026-07-17-readme-elmer-codex.md
wc -l dev/adversarial/2026-07-17-readme-elmer-codex.md   # ~1500+ = real; ~5 = argparse stub, re-run per CLAUDE.md recipe
```

  If Codex quota-blocks with a "usage limit … HH:MM" message: DEFER (wait for the stated time), do not skip (standing rule). If Codex is genuinely down, self-adrev with a fresh subagent per the self-adrev rule and say so in the report.
- [ ] **Step 2:** Disposition every finding (fix or reasoned rejection recorded in the task report); re-run `pnpm lint:docs` after fixes.
- [ ] **Step 3: Commit** fixes: `git commit -m "docs: apply Codex adversarial findings (tuxlink-d8f3l task 5)"`.

---

### Task 6: Ship

- [ ] **Step 1:** Full gates: `pnpm lint:docs` green; `grep -c "—" README.md docs/ELMER.md` returns 0 for both; badge row check; image weight check.
- [ ] **Step 2:** Push branch; open PR (title `[<moniker>] docs: README repositioning + ELMER.md`; body: spec pointer, fact-ledger note, screenshot provenance line per the verification-provenance rule, adrev disposition summary). CI on a docs+images diff runs the standard jobs; verify green by head SHA.
- [ ] **Step 3:** Merge per house policy (CI green = merge gate), `bd close tuxlink-d8f3l`, worktree disposal per ADR 0009 (archive `dev/scratch/d8f3l-facts.md` + the adrev transcript), handoff.

---

## Plan self-review record

- **Spec coverage:** §2 deliverables → T2/T3/T4 (+voice already done); §3 README sections → T3 (content requirements embedded); §4 ELMER outline → T2; §5 screenshots incl. operator correction (no converge-build; worktree build; ARM/VARA limit; privacy; weight) → T1 Step 1 + T4; §6 out-of-scope respected (no license change, no process story); §7 process → T5 (Codex) + T6 (PR) + Global Constraints (worktree, lint gate).
- **Placeholder scan:** none; the two deliberate judgment points (FT8 empty-state fallback, linter-ordering for image refs) state the decision rule, not "TBD".
- **Consistency:** image filenames appear once in T3 and are consumed by T4; the fact-ledger path is identical across T1/T2/T3/T6; no code symbols exist to drift.
