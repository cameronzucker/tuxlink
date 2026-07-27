# 29. The repo carries its engineering record: two audiences, separated by location, not omission

Date: 2026-07-27
Status: Proposed (drafted by agent session; awaiting operator ratification and voice pass)
Deciders: cameronzucker (N7CPZ), chasm-wren-crag (authoring session)

## Context

The 2026-05-17 "release-ready public repo" posture treated Tuxlink as a
shipped product first: dev scratch stayed local, adversarial transcripts were
gitignored, and the tree was kept lean for an imagined end user browsing it.

Two things have changed:

1. **The repo has a second first-class audience.** Tuxlink's engineering
   practice (eval batteries, adversarial review, multi-agent orchestration
   with forensic attribution, incident discipline) has already contributed to
   one job outcome for the operator, and the repo is expected to serve as the
   primary agent-engineering portfolio for the next. The readers now include
   hiring managers and, increasingly, *their agents*: budget-limited readers
   who reconstruct how a project is engineered from what is actually
   committed.
2. **The record already half-lives here.** `docs/adr/`, `dev/bug-hunts/`,
   `dev/handoffs/`, `dev/incidents/`, and `.beads/issues.jsonl` are tracked,
   and the commit graph carries per-session `Agent:` trailers. But the most
   valuable recent material leaks out of the repo: battery run analyses and
   post-mortems accumulate on lab machines in gitignored run trees, bug-hunt
   and handoff files sit untracked in the operator checkout, and stray
   artifacts (screenshots, one-off exports) land loose at the repo root.

The perceived tension, "clean shipped-product repo" versus "visible iterative
struggle," dissolves under one reframe: **for a shipped product, clean means
intentional, not sparse.** Mature open projects carry enormous public process
archaeology without reading as messy, because everything is where it belongs
and visibly on purpose. End users of Tuxlink consume *releases* (release-please
artifacts, the promoted stable channel per the release discipline), not the
working tree; nothing in `dev/` reaches them.

## Decision

The repository carries its own engineering record as a first-class
deliverable. The two audiences are separated by **location discipline**, not
by omitting the record.

1. **Product surface** (`README.md`, `docs/user-guide/`, release artifacts)
   is held to product polish. It may carry a single short pointer to the
   engineering record; it does not embed it.
2. **Engineering record** (`docs/adr/`, `docs/pitfalls/`, `dev/battery/`,
   `dev/bug-hunts/`, `dev/handoffs/`, `dev/incidents/`,
   `dev/implementation-log.md`, `.beads/issues.jsonl`, commit narrative with
   `Agent:` trailers) is committed deliberately:
   - **Per-run battery analysis reports are committed** to `dev/battery/`
     when a run's analysis completes (the ladder runbook already mandates
     this; it is now policy, not convention). Raw run bundles stay on the lab
     machines; the analysis, configuration manifest, and post-mortems are the
     committed layer. A run declared invalid gets its invalidation write-up
     committed too. Honest corrections are part of the record's value; the
     record is not curated to look clean.
   - **Bug-hunt cycles, handoffs, and comparable process artifacts are
     committed promptly**, not left untracked in a checkout. An artifact
     worth writing is worth committing.
3. **Local-only classes are unchanged**: raw adversarial transcripts
   (`dev/adversarial/`), `dev/scratch/`, worktree archives, run bundles, and
   anything touching credentials or secrets. Gitignore already encodes these.
4. **Root hygiene**: no loose artifacts at the repository root. Screenshots,
   captures, and exports go to `dev/scratch/` (local) or into a named,
   committed location with intent (e.g. a bug-hunt's evidence section).
5. **An entry map exists at `dev/README.md`** so a budget-limited reader,
   human or agent, finds the record instead of having to discover it. It is
   the single operational pointer for this ADR under the documentation
   propagation contract.

## Consequences

- Both audiences are served by the same tree: users get polish via the
  release channel; technical readers get the reconstruction trail via
  `dev/README.md`, the ADR index, and `git log --all --grep="Agent:"`.
- Repo size grows with committed analyses. Acceptable: text is cheap, run
  bundles stay out.
- The operator's checkout backlog of untracked `dev/` artifacts becomes a
  named debt to triage, not ambient state.
- **Audience-broadening audit**: with recruiters' agents as readers, the
  committed record should be periodically checked for material the operator
  does not want public (infrastructure hostnames, third-party names in
  correspondence). Committed tailnet hostnames were flagged 2026-07-27 as an
  operator call to make deliberately.

## Watched failure modes

- **Dumping-ground drift**: "commit the record" degrading into unsorted
  artifacts. The location rules and the entry map are the guard; a file that
  fits no named location prompts a naming decision, not a root drop.
- **Polish theater**: curating the record to hide wrong turns would gut
  exactly the value this ADR protects. Corrections, retractions, and invalid
  runs stay in.
- **Entry-map rot**: `dev/README.md` drifting from the tree. Treat it like
  AGENTS.md parity: a PR that adds or moves a record category updates the map
  in the same PR.

## Alternatives considered

- **Second repo for the engineering record**: rejected. The commit graph with
  its `Agent:` trailers *is* the record's spine and cannot be extracted; a
  linked side repo is one hop further than budget-limited readers reliably
  travel, and it forks the propagation contract.
- **Keep the clean-product posture and omit the record**: rejected. It
  discards the demonstrated professional value of the record for a
  cleanliness benefit that end users, who consume releases, never perceive.
- **Mirror everything including raw transcripts and bundles**: rejected on
  size, signal-to-noise, and the standing local-only policy for adversarial
  raw material.
