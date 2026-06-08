# 2026-06-08 sequoia-pika-maple — Post Office modes: design → adrev → execution-ready plan

## Arc

Took `tuxlink-6c9y` (Telnet RMS Post Office + Network Post Office modes) from "operator isn't sure what it does" to an **execution-ready implementation plan**, via the full build-robust-features pipeline: primary-source grounding → operator brainstorm → 5-round cross-provider adversarial review → convergence → execution-ready TDD plan (plan-reviewed). **No implementation code was written** — that is the next session's job, per the operator's explicit "write the plan this session, build it fresh."

All work is on the feature branch `bd-tuxlink-6c9y/telnet-post-office` (worktree `worktrees/bd-tuxlink-6c9y-telnet-post-office`), committed + pushed to origin.

## State (re-verified at session end)

- **Feature branch `bd-tuxlink-6c9y/telnet-post-office`:** clean, **0/0 vs origin** (fully pushed). Commits this session: grounding+spec+mock (`8924c3a`), spec v2 (`0c9d13d`), mock interop fix (`cd0a261`), node_modules installed, the plan (`2abf9ca`).
- **Worktree:** clean. Gitignored local-only reference preserved on disk: `dev/scratch/plan-grounding/*.md` (the 7 grounding readers — exact current-tree facts + proposed code), `dev/adversarial/2026-06-08-post-office-*-codex.md` (3 Codex transcripts), `dev/scratch/codex-*.txt` (prompts). `node_modules` installed.
- **`bsiy` is NOT yet on `origin/main`** — Phase C of the plan is gated on it.
- **Root checkout** (`bd-tuxlink-xygm/recover-handoffs`): untouched operator state (staged `.beads`, 1 unpushed commit, 3 peers' untracked handoffs). Not modified.
- **bd:** `tuxlink-6c9y` IN_PROGRESS; dep edge `6c9y → bsiy` added; operator decisions recorded via `bd remember` (`tuxlink-6c9y-operator-decisions-2026-06-08-adrev`).

## What was produced (all on the feature branch)

- **Grounding** (primary-source, decompiled WLE + Hamexandria + corpus): `docs/design/2026-06-08-telnet-post-office-grounding.md`.
- **Design spec v2** (adrev-converged): `docs/design/2026-06-08-telnet-post-office-design.md`.
- **UX mock:** `docs/design/mockups/2026-06-08-post-office-mocks.html`.
- **Implementation plan:** `docs/superpowers/plans/2026-06-08-telnet-post-office.md` ← the handoff target. 15 TDD tasks, 4 phases.

## Operator decisions (load-bearing)

1. **Both modes, Telnet/IP, manual host. AREDN auto-discovery OUT of scope** — verified parity with a *deprecated* upstream mechanism (WLE's discovery rides OLSR, which AREDN is removing for Babel; third-party OLSR scrapers already break on babel-only nodes). Manual `host:port` is the durable approach.
2. **Connection-determined routing + send-time message selection; NO compose-time routing flag.** Supersedes `tuxlink-u5hl`'s compose-time model. The narrowed `build_outbound_proposals` safety gate (removed for PostOffice/Mesh, kept for P2p/RadioOnly) + explicit selection is the leakage guard.
3. **Full inbound message selection in v1** → Post Office connect builds on the native telnet-exchange path (the `bsiy` decide-seam), reusing `bsiy`'s `build_selecting_decider`. Hence the `6c9y → bsiy` dep.
4. **Interop is field-compatible, not byte-identical.** WLE transmits **no** routing header on the B2F wire (`X-RMS-Routing` is `EncodeHeader`/`.mime`-only, not `B2AssembleMessage`); the `-L` login is the sole discriminator, which tuxlink already sends identically. `message.rs` needs no change.

## Pipeline evidence

- 5-round cross-provider adrev (Codex + 4 Claude lenses) found 4 P0s — all resolved in spec v2 (the keystone fix was *deletion*: strike a wrong `X-RMS-Routing` emission I'd added from a misread). Convergence (Codex) verified: 3 RESOLVED + 1 resolved-with-dependency-caveat, no new findings.
- Plan grounded by a 7-reader workflow against the current tree, then subagent-readiness-reviewed (3 lenses): 6 BLOCKER/MAJOR fixes applied (phantom call sites, a nonexistent `::default()` edit, component-name unification, an undefined `local` binding, the invoke `{req}` wrapper, the `Config`-literal enumeration) + MINOR polish. Verdict: handoff-ready.

## In-progress / pending

- **Implementation (the 15-task plan).** Phases **A** (backend foundations), **B** (frontend wiring + pane), **D** (docs) are **independent — start now**. Phase **C** (the `telnet_post_office_connect` command + inbound selection) is **gated on `bsiy` merging to `origin/main`** (it needs `bsiy`'s `Fn(&[Proposal]) -> Result<Vec<Answer>, ExchangeError>` decide-seam + the `inbound_selection` module). `bsiy` is reportedly substantially complete — re-check `git log origin/main | grep -i bsiy` before Phase C.
- **Pending operator action:** re-scope/close `tuxlink-u5hl` — 6c9y supersedes its Pattern A for Post Office/Mesh; its residual concern is P2P/RadioOnly leakage. Left to the operator (their issue).

## Next session

First action: **READ the plan** `docs/superpowers/plans/2026-06-08-telnet-post-office.md` (top-of-plan banner: line numbers are pre-`kld3` estimates — re-grep anchors by name; load-bearing shared identifiers are listed). Then execute Phases A/B/D via `subagent-driven-development`; do Phase C only after confirming `bsiy` is on `main`. Apply the CI verify gate (`cargo clippy --all-targets -D warnings` + full `pnpm vitest run`) before every push.
