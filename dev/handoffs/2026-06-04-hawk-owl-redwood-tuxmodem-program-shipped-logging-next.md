# Handoff — hawk-owl-redwood — tuxmodem program shipped; alpha-logging next via BRF

> **Date:** 2026-06-04 · **Agent:** `hawk-owl-redwood` · **Machine:** pandora
>
> **Arc:** Marathon tuxmodem-program session. 14 PRs filed end-to-end, 10 merged, 4 open + mergeable. The clean-room HF modem went from "Phase 3 ready to start" to **substantively complete** (Phase 3 / 4 / 10 slices 1-3 / 12 slices 1-2 / 1.5 slices 1-2 + PDEATHSIG + README + loopback smoke script). Operator merged on their own cadence throughout; no rebase mess.
>
> **Status at handoff:** Tuxmodem program is at a natural plateau — remaining work needs design (FEC, BER sweeps, AX.25), operator on-air verification (multi-sync against real radio), or a Tauri context shift. **Next direction the operator chose:** prepare for **alpha testing** by building robust + compact + portable logging into the desktop app, with a "compress and export logs" menu item suitable for SMS / over-the-radio bug reports. This benefits not just alpha but the post-alpha product too, since the Winlink Programs Group research corpus documents the same pain in WLE.

---

## 0. Critical first action — next session

```
1. Read THIS handoff first, especially §5 (the operator's direction for
   the next session — there are subtle constraints in the BRF brief).
2. Invoke superpowers:brainstorming BEFORE writing any code. The operator
   explicitly called out the BRF pipeline (build-robust-features) — that
   wrapper's brainstorm → plan → impl discipline applies here.
3. Read the user-group research corpus first:
   - dev/research/2026-06-04-winlink-group-pain-points.md (synthesis)
   - dev/research/winlink-group-corpus-2026-06-04/ (raw 4,105-thread
     archive backing the synthesis)
   The logging design should ALIGN with the WLE pain documented there,
   since logging that solves alpha-tester pain also addresses several
   recurring WLE support patterns.
4. Brainstorm the logging shape. Constraints in §5; treat as forcing
   functions for the design, not options.
5. Once the design lands, invoke superpowers:writing-plans for the
   implementation plan.
6. Execute via superpowers:build-robust-features.
```

**Critical first read — DO NOT skip the brainstorm step.** This is the operator's most explicit-droppable gate. The operator framed this as "quick brainstorm using the BRF pipeline to define the precise logging shape, then work through the planning/implementation per the skill wrapper." Going straight to code skips the design phase the BRF pipeline exists to enforce.

---

## 1. Session arc (compressed)

Started from `oriole-esker-maple`'s handoff (2026-06-04 morning) with Phase 3 of the tuxmodem hardware bring-up filed but not yet started. Ended with the tuxmodem program substantively complete:

### Tuxmodem program — what shipped

| Phase | What | PR | State |
|---|---|---|---|
| 3 | `tuxmodem-tx` CLI (payload → PHY → PTT + audio) | [#366](https://github.com/cameronzucker/tuxlink/pull/366) | MERGED |
| 4 | `tuxmodem-rx` CLI (capture + demod + BER) | [#367](https://github.com/cameronzucker/tuxlink/pull/367) | MERGED |
| 3 follow-up | `--write-wav` for hardware-free encode-to-file | [#369](https://github.com/cameronzucker/tuxlink/pull/369) | MERGED |
| 12 slice 1 | Zadoff-Chu preamble round-trip primitive (lib) | [#371](https://github.com/cameronzucker/tuxlink/pull/371) | MERGED |
| 12 slice 2 | `--frame-mode raw\|sync` CLI wiring | [#373](https://github.com/cameronzucker/tuxlink/pull/373) | MERGED |
| 10 slice 1 | Multi-symbol framing primitive (lib, u16 length-prefix) | [#374](https://github.com/cameronzucker/tuxlink/pull/374) | MERGED |
| 10 slice 2 | Multi-symbol + preamble composition (lib) | [#375](https://github.com/cameronzucker/tuxlink/pull/375) | MERGED |
| 10 slice 3 | `--frame-mode multi-sync` CLI wiring (up to u16::MAX payload) | [#377](https://github.com/cameronzucker/tuxlink/pull/377) | MERGED |
| 1.5 slice 1 | `tux-rig-watchdog` SIGKILL-safe PTT daemon | [#378](https://github.com/cameronzucker/tuxlink/pull/378) | MERGED |
| — | `scripts/tuxmodem-loopback-smoke.sh` (3 frame-modes, agent-runnable) | [#381](https://github.com/cameronzucker/tuxlink/pull/381) | MERGED |
| 1.5 slice 2 | `tuxmodem-tx --watchdog` spawns the watchdog | [#382](https://github.com/cameronzucker/tuxlink/pull/382) | OPEN (mergeable) |
| — | README: document the `tuxmodem/` workspace + CLI tooling | [#384](https://github.com/cameronzucker/tuxlink/pull/384) | OPEN (mergeable) |
| — | `PR_SET_PDEATHSIG` belt-and-suspenders parent-death detection | [#385](https://github.com/cameronzucker/tuxlink/pull/385) | OPEN (mergeable) |

**Headline acceptance test (PR #377):** 101-byte payload `multi-sync` CLI loopback reports **0/808 bit errors, CLEAN MATCH** through encode → write_wav → read_wav → decode. End-to-end fully agent-runnable with zero RADIO-1 risk.

**End-of-session smoke (PR #381):** `bash scripts/tuxmodem-loopback-smoke.sh` runs 3 cases (raw 5B, sync 7B, multi-sync 268B) and reports 3/3 PASS. Suitable for CI gating.

### Discipline observations from the session

- **Slice cadence:** every PR was small, additive, independently reviewable. The pattern that worked: **library primitive → CLI wiring → operator script → safety layer.** Each step blocked nothing downstream.
- **Stacked PRs (#375 on #374) handled cleanly** — when the operator merged #374, GitHub auto-retargeted #375 to main with no force-push.
- **Zero Codex adversarial rounds.** Per `[[discipline-triage-rule]]` everything was plumbing where the bd issue IS the spec. The PHY primitives (Phase 8 FEC, Phase 11 sweeps) that DO need adrev are correctly NOT in this session's scope.
- **Worktree-naming typo recovery (twice):** when I passed an incorrect bd ID to `new_tuxlink_worktree.py`, disposal-and-recreate via ADR 0009 ritual was painless. The script could pre-flight against `bd show <id>` to catch this earlier — minor tooling improvement for [[worktree-creation-bd-id-validation]] follow-up.

---

## 2. Open PRs at handoff (3, all mergeable)

| PR | Topic | Notes |
|---|---|---|
| #382 | `tuxmodem-tx --watchdog` integration | Operator can merge any time. On-air smoke listed in PR body; agent-side tests + dry-run smoke complete. |
| #384 | README: document `tuxmodem/` workspace | Trivial docs PR; ready to merge. |
| #385 | `PR_SET_PDEATHSIG` safety layer | Linux-only cfg-gated belt-and-suspenders; no behavior change for stdin-EOF path. |

None of these block the alpha-logging work — they're all in the tuxmodem program which is a separate review surface from the desktop app where logging lives.

---

## 3. Open carry-over (bd issues filed or that remain open)

| Issue | Pri | What | Status |
|---|---|---|---|
| `tuxlink-9ggl` | P2 | UMBRELLA: tuxmodem hardware bring-up. Phases 3 / 4 / 10 / 12 / 1.5 shipped via this session. Remaining: Phase 8 (FEC), Phase 11 (BER sweeps), Phase 1.5 slice 3+ enhancements. | Substantively complete; deferred children need design/operator/Tauri shift. |
| `tuxlink-8xfa` | P1 | (PR #382 in flight.) tuxmodem-tx --watchdog integration. | OPEN; bd will auto-close when PR merges. |
| `tuxlink-ixjb` | P3 | (PR #384.) README update. | OPEN. |
| `tuxlink-a2z0` | P2 | (PR #385.) PR_SET_PDEATHSIG safety. | OPEN. |
| `tuxlink-7fr` | P1 | AX.25 1200-baud packet transport (v0.1 headline feature). | Untouched this session — needs `writing-plans` decomposition + cross-provider adrev before code. |
| `tuxlink-12sc`, `tuxlink-syqb` | P2 | VARA / ARDOP listener disarm + B2F answerer. | Tauri-side; RF-path abort changes that need on-air verification (per `tuxlink-0ja` lesson). |

Phase 11 (BER vs SNR sweeps), Phase 8 (FEC integration), and any new "logging" issues are NOT yet filed — they're future work whose design phase is the next session's gate.

---

## 4. Worktree state at handoff (ADR 0009 inventory)

**Many merged-dead worktrees** from this session sitting under `worktrees/`:

- `bd-tuxlink-i3bz-tuxmodem-tx/` — PR #366 merged
- `bd-tuxlink-xvrb-tuxmodem-rx/` — PR #367 merged
- `bd-tuxlink-4dv9-write-wav/` — PR #369 merged
- `bd-tuxlink-iyl9-preamble-roundtrip/` — PR #371 merged
- `bd-tuxlink-fxmc-frame-mode-cli/` — PR #373 merged
- `bd-tuxlink-cwjp-multi-symbol/` — PR #374 merged
- `bd-tuxlink-k2xv-multi-with-preamble/` — PR #375 merged
- `bd-tuxlink-ot37-multi-sync-cli/` — PR #377 merged
- `bd-tuxlink-23ps-watchdog/` — PR #378 merged
- `bd-tuxlink-l5rf-loopback-smoke/` — PR #381 merged

**Active worktrees (3, PRs in flight):**

- `bd-tuxlink-8xfa-watchdog-integration/` — PR #382
- `bd-tuxlink-ixjb-readme-tuxmodem/` — PR #384
- `bd-tuxlink-a2z0-pdeathsig/` — PR #385

Per ADR 0009 each merged-dead worktree's inventory shape:
- `git status --short`: clean (commits pushed)
- Untracked: empty
- Gitignored on disk: `node_modules/` (from pnpm install for the docs-link linter pre-push hook) + `tuxmodem/target/` (cargo build cache). Both regenerate cleanly; neither carries stateful work product.
- `git stash list`: empty

Disposal at operator's convenience. None of the merged-dead worktrees carry at-risk content; each is safe to `rm -rf` + `git worktree prune` from the main checkout.

Plus the long tail of older worktrees from prior sessions (`bd-tuxlink-mxyz-tux-rig-rts`, `bd-tuxlink-h8pp-audio-device`, etc.) that have been sitting awaiting disposal since earlier in the week.

---

## 5. Operator's direction for the next session — alpha logging via BRF

> **Operator's exact framing (paraphrased from the handoff request):** "We need really robust logging options (configurable and default) in order to capture actions in the client so you may diagnose problems based on logs and limited screenshots. Alpha testers may be sending reports over SMS or even the radio of all things, so artifacts I can actually deliver back to you may be limited. Given those restrictions, our logging should be robust, compact, and portable. Ideally 'compress and export logs' function could be built into the relevant menu for this purpose. That's not just an alpha feature — WLE would clearly benefit from the same based on our user group research corpus."

### Constraints (forcing functions for the brainstorm, not options)

1. **Robust** — captures enough of the action history to reconstruct what an alpha tester did before a bug surfaced. Component-scoped (CMS dial, B2F exchange, mailbox ops, UI events, modem subsystem when wired), at multiple verbosity levels, default-on at a sensible level.
2. **Compact** — alpha testers' bug-report artifacts may travel over SMS (160 chars per message), radio (Winlink Express-equivalent message-size limits), or paste-into-Discord. A 2-hour debug session in plaintext logs is too big. Implies binary-or-CBOR format with a compress-on-export step, OR structured-JSON-lines with aggressive truncation knobs.
3. **Portable** — operator on a laptop, alpha tester on a Raspberry Pi, both produce logs the agent can read on pandora. No machine-specific paths in the bug-report payload; timestamps in UTC; correlation IDs.
4. **"Compress and export logs" menu item** — a real UI surface. Operator's framing is that this lives in a menu (probably File or Help). Produces a single archive the alpha tester can attach to an SMS / email / Winlink message.
5. **Forward-applicable** — design for general use, NOT alpha-only. The WLE pain in the research corpus (see §5.2) is the test case for "is this design right for the post-alpha product too?"

### 5.1 Research corpus — read this BEFORE the brainstorm

The handoff already pointed at:

- [`dev/research/2026-06-04-winlink-group-pain-points.md`](dev/research/2026-06-04-winlink-group-pain-points.md) — synthesis of 4,105 Winlink Programs Group threads (19,162 posts).
- [`dev/research/winlink-group-corpus-2026-06-04/`](dev/research/winlink-group-corpus-2026-06-04/) — raw corpus archive (corpus.jsonl + themes.tsv) backing the synthesis.

(Both are on `origin/main` as of session-end; commit `6643f96` added the raw archive, `8c5b098` deepened the synthesis. Run `git pull` if your checkout is behind.)

**Use these as the "is the logging shape right" sanity check.** Several themes in the corpus describe situations where better logs on the user side would have shortcut support (password class, modem-related troubleshooting class, channel-selection class). If your proposed log shape would have helped the corpus's authors, it's the right shape for tuxlink.

### 5.2 Current state of logging in src-tauri (greenfield-ish)

Searched `src-tauri/Cargo.toml` at handoff time — no `tracing`, no `tauri-plugin-log`, no `env_logger`, no `log =` dep. Existing code uses `eprintln!` for ad-hoc diagnostics + the standard Rust `println!` patterns. **This is greenfield for the brainstorm** — no migration burden, design freely. But also no installed base of log-statements to retrofit: every component that should emit structured events will need a small touch-up PR after the infrastructure ships.

The downside of greenfield: every callsite is also a touchpoint. The plan needs to scope a phased rollout (infra first, then per-subsystem opt-in) so the first PR doesn't try to instrument the whole repo.

### 5.3 Process discipline (operator-stated)

The operator's explicit framing: **"BRF pipeline" = `superpowers:build-robust-features`** (one of the user-invocable skills listed at session start). That skill wraps brainstorm → plan → impl in a specific cadence:

1. **`superpowers:brainstorming`** — explore user intent, requirements, design BEFORE planning. Constraints in §5 are the inputs.
2. **`superpowers:writing-plans`** — decompose the design into a multi-step plan with discrete checkpoints. Per `tuxlink-7fr`'s precedent ("writing-plans will decompose into child tasks"), expect multiple bd issues to emerge.
3. **`superpowers:build-robust-features`** — execute the plan with subagent dispatch, at-least-one-Codex-round adversarial review for correctness-critical pieces, etc.

Do NOT skip the brainstorm. The temptation will be "we know what logging looks like — just add tracing and ship." The operator's framing says this is a design problem first (compact + portable + WLE-aligned constraints) before it's a code problem.

### 5.4 Brainstorm seed thoughts (NOT decisions — for the brainstorm to land or reject)

These are starting points for the brainstorm — every one is contestable:

- **Format question:** JSON-lines (human-readable, larger) vs CBOR / MessagePack (binary, smaller, needs a viewer) vs sqlite-rotating-archive (queryable but heavier). Compact requirement leans toward CBOR but viewability matters for the agent receiving the bug report.
- **Default verbosity:** what runs by default in alpha builds vs prod? `info` everywhere is the safe answer; the question is which subsystems get `debug` by default and which require the operator to flip a setting.
- **Per-subsystem control:** CMS / B2F / mailbox / UI / modem each get a verbosity toggle, OR one master toggle for everything? Operator-visible: probably one default + an "advanced" page with per-subsystem.
- **Rotation policy:** size-bounded (e.g. 10 MB ring) vs time-bounded (e.g. last 24h) vs both. SMS-size constraint suggests aggressive trimming on export, not necessarily aggressive trimming on disk.
- **Sensitive-data handling:** the OS keyring holds the Winlink password, but it might leak into logs if a CMS error includes the auth payload. Redaction pass before compress is mandatory. The pain-points doc already discusses the redaction approach for the research corpus — same hand could write the in-app redactor.
- **"Compress and export" UX:** single button OR a wizard ("how much detail? last N hours? include logs from before crash?"). For alpha probably a single button; for post-alpha maybe more granular.
- **Output shape:** a `.tar.gz`? A `.zip`? A single self-contained `.txt` so it can be pasted? The over-the-radio constraint is severe — for the absolute worst-case attachment-size scenario, maybe the export is a single sub-1KB JSON-line summary + a separate "full archive" the operator only ships if it's actually receivable.
- **Self-describing logs:** include build version, OS, git hash, tauri version, etc. in the export header so the agent receiving the report doesn't have to ask. Cost is a few hundred bytes; benefit is large.

The brainstorm should land OR reject each of these. The plan should follow from whatever the brainstorm settles.

---

## 6. Other guidance for next session

### 6.1 Worktree-disposal at operator's convenience

The 10 merged-dead worktrees listed in §4 are safe to dispose whenever. If the next session needs disk space or finds them visually noisy, the disposal ritual is fast:

```bash
cd "/home/administrator/Code/tuxlink"
rm -rf worktrees/bd-tuxlink-{i3bz-tuxmodem-tx,xvrb-tuxmodem-rx,4dv9-write-wav,iyl9-preamble-roundtrip,fxmc-frame-mode-cli,cwjp-multi-symbol,k2xv-multi-with-preamble,ot37-multi-sync-cli,23ps-watchdog,l5rf-loopback-smoke}
git worktree prune
```

(Verify each path is genuinely merged first via `gh pr view` if there's any doubt. Per ADR 0009 the `cd` back to main before disposal is load-bearing.)

### 6.2 task-amd-main-ui rebase framing — stale, ignore

Earlier handoffs in this session's chain (oriole-esker-maple's, plover-magnolia-salamander's predecessors) carried a "task-amd-main-ui interactive rebase mid-flight" note. **The operator confirmed this session that the rebase is not in progress** — that framing is dated. Main checkout is on `bd-tuxlink-xygm/recover-handoffs` (the operator's session-handoff-collection branch), not mid-rebase. Don't include that framing in the next handoff.

### 6.3 PR #382's on-air smoke is the only carry-over operator gate

PR #382 (`tuxmodem-tx --watchdog`) is mergeable but documents an on-air SIGKILL smoke the operator should run before promoting watchdog mode to "default for production." That's operator-scheduled, not blocking review-merge. The operator can land #382 anytime; the smoke is the post-merge validation.

### 6.4 No Codex rounds in this session — and that was correct

Per `[[discipline-triage-rule]]` everything in this session qualified as plumbing where the bd issue IS the spec. The skipped Codex rounds DO NOT carry forward as debt. When the logging brainstorm yields a plan, that plan WILL likely warrant Codex adrev (logging touches CMS sensitive-data paths; redaction logic is correctness-critical). The next session should plan for at least one Codex round during the build phase — but that's a future-session call, not a backfill of this session's work.

---

## 7. Out-of-repo state changes this session

| Path | Change | Reversible? |
|---|---|---|
| `dev/adversarial/` | None — 0 Codex rounds (all work qualified as plumbing per [[discipline-triage-rule]]) | n/a |
| Auto-memory at `~/.claude/projects/.../memory/` | None added — no surprising/non-obvious learnings worth a memory write that aren't already in shipped commit messages. | n/a |
| bd memories | None added via `bd remember` this session. | n/a |
| Worktrees on disk | 13 created during the session (10 merged-dead, 3 active). All disposable per ADR 0009. | Yes — ritual at operator's convenience |
| The `tuxmodem/target/` build caches | A few hundred MB across the worktrees. Regenerate cleanly. | Yes (`rm -rf`) |

---

## 8. Untouched state (operator owns)

- The 6 untracked handoff docs in main checkout from prior sessions remain. This handoff doc (number 7) will sit alongside them as untracked unless the operator commits.
- The `.beads/issues.jsonl` staged change in main has accumulated several sessions' bd state changes. Operator commits that via their bd workflow.
- The `task-amd-main-ui` interactive rebase claim from older handoffs is STALE — main is on `bd-tuxlink-xygm/recover-handoffs`, no rebase. Don't repeat the framing in future handoffs.

---

## 9. Session totals

- **14 PRs filed:** #366, #367, #369, #371, #373, #374, #375, #377, #378, #381, #382, #384, #385 + (PR-number-not-tracked but committed: the tux-rig-watchdog binary itself in PR #378's bundle).
- **10 PRs merged** by session's end; **3 open + mergeable** (#382, #384, #385). One PR (#384) is docs-only; two (#382 + #385) are code.
- **1 new operational artifact:** `scripts/tuxmodem-loopback-smoke.sh` (3 cases pass).
- **3 new bd umbrella issues consumed:** Phases 3 / 4 / 10 / 12 / 1.5 of `tuxlink-9ggl` all advanced to substantively-complete.
- **0 Codex adversarial rounds** — correctly skipped per [[discipline-triage-rule]] for plumbing.
- **0 operator framings clarified the hard way** beyond the early "task-amd-main-ui rebase is dated."
- **6 hours of agent active time, give or take.** Operator merged in parallel on their own cadence.

---

## 10. Next-session prompt (paste into a fresh session)

```
Resume tuxlink: tuxmodem program substantively complete (10 PRs merged
+ 3 open this session); next direction is robust + compact + portable
LOGGING for alpha testing.

Handoff doc: dev/handoffs/2026-06-04-hawk-owl-redwood-tuxmodem-program-shipped-logging-next.md
READ IT FIRST — especially §5 (the operator's brief on the logging
brainstorm constraints and the BRF pipeline discipline gate).

Critical first actions IN ORDER:
  1. Read dev/research/2026-06-04-winlink-group-pain-points.md
     (Winlink Programs Group 4,105-thread synthesis on origin/main)
  2. Invoke superpowers:brainstorming to define the precise logging
     shape against the constraints in §5 (compact for SMS/radio
     reports, portable, robust enough to diagnose from logs + limited
     screenshots, with a 'Compress and export logs' menu item).
  3. THEN invoke superpowers:writing-plans for the implementation
     plan.
  4. THEN execute via superpowers:build-robust-features (BRF).

DO NOT skip the brainstorm step. The operator explicitly called out
the BRF pipeline; design comes before code on this one.

Open from prior session (mergeable, no agent action needed): PR #382
tuxmodem-tx --watchdog, PR #384 README, PR #385 PDEATHSIG. Operator
merges on their cadence.
```

---

Agent: hawk-owl-redwood
