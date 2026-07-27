# The engineering record: start here

Tuxlink is a native Linux Winlink client, and it is also a documented practice
run in AI-agent-native software engineering: multi-agent development with
forensic attribution, eval-driven tool-surface iteration, adversarial review,
and incident discipline. This page is the map for a reader (human or agent)
who wants to reconstruct *how this project is engineered* without spelunking.
Policy: [ADR 0029](../docs/adr/0029-engineering-record-in-repo.md).

## Reconstruction trail

| Where | What you find |
|---|---|
| [`docs/adr/`](../docs/adr/README.md) | Every significant decision, dated, with rationale and reversals. Highlights for the agent-engineering story: [0008](../docs/adr/0008-worktrees-mandatory-under-bd-issue-ownership.md) (worktree ownership), [0009](../docs/adr/0009-worktree-disposal-ritual.md) (non-destructive disposal), [0017](../docs/adr/0017-branch-state-machine.md) (branch lifecycle enforcement), [0022](../docs/adr/0022-ban-autonomous-agent-issue-splitting-and-deferrals.md) (completeness as invariant), [0027](../docs/adr/0027-parity-manifest-ci.md) (agent/human parity enforced in CI), [0029](../docs/adr/0029-engineering-record-in-repo.md) (this record). |
| `git log --all --grep="Agent:"` | The full multi-agent history. Every agent session commits under a unique moniker trailer; any commit's authoring session is greppable, and a session's entire trail reconstructs with `git log --all --grep="Agent: <moniker>"`. |
| [`dev/battery/`](battery/) | The eval program: an 18-cell battery measuring how well local models author Tuxlink Routines against the live MCP tool surface, with a deterministic scorer, an independent LLM judge, flakiness-rate methodology (every rung runs 3x), and committed per-run analysis reports. The runbook documents the full harness. |
| [`dev/bug-hunts/`](bug-hunts/) | Structured multi-pass bug-hunt cycles (exploratory / holistic / multipass passes with consolidation). |
| [`dev/incidents/`](incidents/) | Post-mortems of process failures, written when fresh, including the ones caused by agents. Corrections and retractions are kept, not curated away. |
| [`dev/handoffs/`](handoffs/) | Session-continuity documents: each work session ends by writing state down for the next session (possibly a different agent on a different machine). |
| [`docs/pitfalls/`](../docs/pitfalls/) | Distilled implementation pitfalls that recurred or nearly shipped. |
| [`.beads/issues.jsonl`](../.beads/issues.jsonl) | The full issue tracker (bd/beads), tracked in-repo: work items, dependency edges, and investigation notes ride with the history. |
| [`dev/implementation-log.md`](implementation-log.md) | Reverse-chronological log of significant work items. |
| [`.claude/`](../.claude/) + [`.githooks/`](../.githooks/) | The enforcement layer: hooks that deny destructive git, enforce commit attribution and branch lifecycle, plus project skills. Prose rules rot; hooks do not. |

## What is deliberately not here

Raw adversarial-review transcripts, scratch space, battery run bundles
(gigabytes of per-cell transcripts on lab machines), and anything touching
credentials live outside the repo by policy. The committed layer is the
decisions, the analyses, and the corrections; see ADR 0029 for the line.

## Product versus record

End users consume [releases](https://github.com/cameronzucker/tuxlink/releases),
not this tree. The product surface is [`README.md`](../README.md) and
[`docs/user-guide/`](../docs/user-guide/); this directory is the workshop.
