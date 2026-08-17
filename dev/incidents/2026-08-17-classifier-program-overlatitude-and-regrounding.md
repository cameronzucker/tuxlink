# Classifier program: the over-latitude episode, and the regrounding review

2026-08-17, spruce-birch-dune. Written on operator direction after the
program was durably parked
(`dev/handoffs/2026-08-17-spruce-birch-dune-classifier-program-parked.md`,
PR #1365). This is two things in one document: the honest record of a
process failure, and the state review that puts the program's owner back
in a position to drive it. It ends with the structure the operator set
for what comes next. Every claim carries a pointer — a PR number, a file,
or a dated operator ruling — so any line can be spot-checked.

## The episode in one paragraph

Between 2026-08-09 and 2026-08-16 the five-classifier program ran across
roughly a dozen agent sessions on a recursive loop — measure, iterate,
measure again — that produced real merged substrate and real evidence,
some of which later collapsed. The loop worked well enough early that it
accumulated de-facto build authority: by 2026-08-16 a session (this one)
was preparing to wire the classifier into Elmer — the highest-stakes
integration in the program — with no implementation spec, no plan
document, and an evidence base it knew to be partly retracted, on the
strength of issue-note momentum alone. The operator stopped it twice:
once because the architecture brief he was handed was unreadable jargon,
once with the finding that matters: *"If we aren't actually architecting
a durable plan with a falsifiable spec as a complete workflow we can
close the loop on, you're not engineering, you're just doing stuff."*
His diagnosis of the root cause: *"I was allowing you to self-iterate
based on evidence from the bench and a goal design parameter. That was
clearly unsound since when things go off the rails I'm in the dark."*
The bench evidence did go off the rails — and the program's owner had no
artifact from which to notice, audit, or steer. Nothing unspecced
shipped; the stops landed before wiring code was written.

## Timeline

- **08-09** — Program filed post-DefCon (bd tuxlink-efk3k): request,
  trend, and content classifiers, Outlook's problem shape as the named
  reference. Same day, the operator's security addendum reframed the
  stakes (unauthorized state mutation and shell access, not just
  transmission) and added the security/injection role and the
  quarantined inbox reader; a follow-on exchange added the fifth role
  (capability-grant). Kickoff mechanics written to
  `dev/plans/2026-08-09-elmer-classifier-architecture-kickoff.md`.
- **08-09/10** — Embedding-model spike on the real 1,477-item catalog
  (`dev/spikes/2026-08-09-t1-catalog-embedding/`, PRs #1315/#1317):
  bge-small-en-v1.5 selected, 97.2% top-1. Operator ruled the eight ADR
  decision points (bd efk3k addendum 6); **ADR 0030 drafted on his
  direction and ratified by his merge directive** ("Merge if CI green:
  1325", merge ed9e3bc2).
- **08-10** — Tool-surface survey (PR #1327). Classifier crate
  `tuxlink-classify` built and validated on R2 (PR #1332): native
  bge-small backend, enriched catalog index, measured thresholds.
  Tool-surface corpus generated from the tool registry with a
  cannot-drift CI gate. Operator confirmed the step-3 shape after a
  three-round correction and a full work-stop: classifier-driven tool
  narrowing, lazy tool schemas, prefill warm-up (bd ch3e9 notes) — with
  two questions explicitly ruled EMPIRICAL (what happens when the
  shortlist misses; how much narrowing per backend tier). The
  experiment design to answer them was written and **operator-approved**
  (`dev/plans/2026-08-10-tool-narrowing-experiment-design.md`).
- **08-11 pre-dawn** — Operator pivot: the small-model arm is abandoned;
  Inkling is the only backend that matters; the classifier must benefit
  it, not harm it (bd ch3e9 notes, quoted there).
- **08-11** — The selection-layer battery ran
  (`dev/spikes/2026-08-10-tool-narrowing-inkling-recovery/FINDINGS-v4.md`):
  narrowing helped Inkling at the layer of *which tool gets picked*, and
  refuted always-include pins. In parallel, a bench outcome comparison
  (stock vs narrowed vs narrowed-plus-schema-furnishing) produced
  headline gains — and then **collapsed**: the operator voided all
  three-arm bench results (*"we just assume the only divergence is the
  one we found — there is no data here"*). The autopsy
  (`FLOOR-AUTOPSY.md`, PR #1340) vindicated the ruling with byte-level
  evidence: a serving-stack streaming bug had been truncating tool-call
  arguments in every arm **and in production**; the client-side repair
  shipped to production code (commit 4491f6ec, provider.rs). The
  fixture-validity program was chartered (bd tuxlink-10iw0, P1, still
  open).
- **08-12/13** — Content-classifier foundation merged (PR #1341, bd
  8zq7u): the typed conversion schema a privileged agent reads instead
  of raw mail, deterministic triage, an injection signal — built on the
  operator's ruling that the specced content classifier, not a loosened
  taint gate, is the fix for the autopsy's blocked-local-writes cluster.
  Classifier weights hosting shipped whole (PR #1346, bd 13ofm closed):
  pinned digests, verify-then-install pipeline, wizard step, Elmer
  panel gate. A bench-run forensic read (PR #1342) exposed the
  tool-surface defects that became the surface-repair campaign
  (#1352–#1362, closed 2026-08-16). The operator framed the Weekend
  Epic (08-13 handoff §4): classifier **wiring + in-repo measurement,
  NOT the bench** as stage 4.
- **08-16 (this session)** — The threshold recalibration debt was paid
  on R2 (`dev/evals/2026-08-16-ch3e9-tools-threshold-recalibration.md`,
  parked branch): the stale floor had genuinely gone wrong. The session
  then read the Elmer disclosure path and began deriving wiring
  decisions. **The operator stopped it**, found the process failure, and
  ordered the durable park.

## What the operator ruled (the authoritative list)

Dated, verbatim-intent, recorded in bd efk3k/ch3e9 notes and ADR 0030
unless noted:

1. **08-09**: five roles; quarantined reader; *"If you prompt inject my
   inbox reader agent the result should be… well, nothing, in that
   system."*
2. **08-09**: the Pi is not a realistic inference target; R2 is the
   eval platform; Pi numbers are worst-case context.
3. **08-10**: the eight ADR decision points — deployment matrix with
   **Elmer-unconfigured spawns NOTHING**; bge-small primary; T1 host
   choosable via a backend seam; enriched index conditional on the
   Inkling-parseability spike; thresholds are two plain config numbers,
   **advisory, never binding on a capable model**, corpus-generic
   verdict schema; **Apache-clean models only** ("no forbidden fruit");
   sequencing survey → prototype → security-last. Canonical: ADR 0030.
4. **08-10**: the step-3 shape (tool narrowing + lazy schemas + prefill
   warm-up), with the classifier disclosing tools TO the model — never
   model-driven progressive disclosure; two open points ruled EMPIRICAL.
5. **08-10**: synonym curation is *"just a token/friction reducer"* — if
   curation is load-bearing, the classifier has failed; and the **hard
   requirement**: the Inkling tier must always be able to get past the
   classifier to the full toolset.
6. **08-11**: small-model arm abandoned; Inkling-only; north star is
   *"a safe, wildly capable Collaborator Tier Elmer… a cautious but
   knowledgeable ham radio Jarvis."*
7. **08-11**: all three-arm bench results void; fixture must be
   validated seam-by-seam before bench numbers count as data (bd
   10iw0). The 87% simple-task floor was itself a red flag — correctly.
8. **08-11**: the taint gate's friction on local-only writes is not a
   toggle to flip; the content classifier is the mechanism (recorded in
   FLOOR-AUTOPSY §work-items).
9. **08-13**: Weekend Epic shape — stage 4 is wiring plus **in-repo**
   measurement, not the bench.
10. **08-16**: the stops, the park, and the resolution model (below).

## What agents decided without an operator ruling (the exposure)

- The bench fixture and its arms — the instrument whose divergences
  voided the outcome data. Built and trusted agent-side; every
  divergence was found reactively. This is the episode's core failure.
- The evolution of the spike instruments (v1→v4) within the approved
  experiment design — reasonable in isolation, but the *findings*
  accumulated into de-facto build authority no one ratified.
- All internal shapes of `tuxlink-classify` (API, DTO, threshold
  midpoint arithmetic, enrichment template details beyond the ratified
  "SECTION ID: description" rule) and of the conversion-schema
  foundation (#1341). Normal substrate latitude — listed for
  completeness, all Codex-reviewed and CI-gated.
- **The near-miss**: this session's wiring derivation — presentation
  policy, schema furnishing, degradation defaults — was underway with
  no spec and partly-void evidence when the operator stopped it. Zero
  code written; the exposure was hours, not artifacts.

## What is merged and running today

| What | Where | Load-bearing for |
|---|---|---|
| ADR 0030 (architecture, ratified) | `docs/adr/0030-five-classifier-architecture.md` (PR #1325) | Every classifier decision. NOTE: its Status line still reads "Proposed" although the operator's merge directive was the ratification act — a one-line correction owed. |
| Classifier crate: native embedding backend, catalog index, thresholds | `src-tauri/tuxlink-classify/` (PR #1332) | Eval harnesses, weights wizard target, conversion-schema module. **Not wired into Elmer — nothing consumes it at runtime.** |
| Enriched catalog + tool-surface corpora, registry generators, cannot-drift CI gates | `resources/catalog/`, `src-tauri/resources/agents/tool-surface.jsonl` | Classifier corpora; the corpus is also the designed payload for the future tool-inventory surface (bd 6vyk4). |
| Weights hosting: pinned digests, verify-then-install, wizard step, job, 3 MCP tools | `src-tauri/src/classify_weights/` (PR #1346) | User-reachable today; sideload ratification still pending operator (bd wvgon). |
| Content-classifier foundation: conversion schema, T0 triage, injection signal | `tuxlink-classify/src/inbox.rs` (PR #1341) | Substrate only; the content classifier itself is unbuilt (bd 8zq7u in progress). |
| Streaming tool-call argument repair | `tuxlink-agent-frontend/src/provider.rs` (commit 4491f6ec) | **Production Elmer, every streaming backend** — the episode's one unambiguous user-facing win. |
| Threshold recalibration (2026-08-16) | branch `bd-tuxlink-ch3e9/classifier-wiring` @ aeaf31ef — **parked, unmerged, no PR** | Nothing yet; main's stale entry is harmless because nothing consumes it at runtime. |

Open satellites: prefill warm-up (8dkcy), Elmer-inert + performance docs
(nsnre), tool-inventory surface (6vyk4), trend (1zn1e), capability-grant
(ct6zu), security/injection (vcjo2), fixture-validity program (10iw0,
P1, bench court), the two premise-falsified A/B draft PRs (#1319/#1320)
awaiting the posture ruling.

## Evidence ledger

- **Stands** (in-repo, model-free or byte-level): the catalog floor
  evals (97.2%, python and native —
  `dev/evals/2026-08-10-ch3e9-t1-floor-calibration.md`); the
  tool-surface shortlist charts and thresholds (93.6% top-12, 08-10 and
  08-16 runs); the Inkling-parseability spike (42/44, PR #1331); the
  CPU-viability measurements (PR #1324); the autopsy's wire-level
  findings (streaming truncation proven by replay; the taint-gate
  cluster; the 144KB `catalog_list` result).
- **Caveated**: the selection-layer battery (FINDINGS-v4) — never
  retracted, in-repo instrument, but it ran on the serving stack whose
  streaming bug the autopsy later proved. Its claims are about tool
  *selection*, mostly upstream of the argument-truncation; treat it as
  supporting evidence for the brainstorm, not as ratified ground truth.
- **Void, cite nothing**: every three-arm bench outcome —
  FINDINGS-THREEWAY (retraction header in-file), FINDINGS-BENCH-AB, and
  with them the only outcome evidence for **schema furnishing**. Until
  the fixture-validity program (10iw0) completes, the bench cannot
  supply evidence to this program.

## The failure mechanism, honestly

Documents existed — an operator-ratified ADR, an operator-approved
experiment design. What never existed was a **build spec** between them:
the artifact that says what will be wired, how we will falsify it, and
what the operator must rule on before it ships.

The operator's diagnosis pins where that absence actually bites, and it
is not where the work happens — it is at the **context boundaries**. His
observation, verbatim intent: the agent is *excellent* operating in a
single context window — the recursive loop runs and "pretty much just
goes." Things fall apart at compaction and handoff. And the lack of a
BRF structure — a literal spec/plan pair — to fall back on is what makes
the falling-apart "difficult or impossible to recover, since it requires
full re-grounding against… the whole repo, basically."

That is exactly what the record shows. Inside each session, the loop was
sound: measured, adversarially reviewed, CI-gated. But the program's
state crossed a dozen compaction/handoff boundaries as **session notes —
a lossy compression written by the outgoing context for the incoming
one**. Each incoming session inherited notes as if they were
authorization; the evidence base degraded (the retraction) faster than
the notes describing it did; and the decision briefs grew less readable
exactly as the decisions grew more consequential, because each
generation of notes compressed the jargon of the last. A spec/plan pair
is precisely the artifact designed to survive those boundaries — the
uncompressed, operator-ratified ground truth any session can re-enter
from. Its absence is why this recovery costs what it costs: this
document IS the full re-grounding against the whole repo that the
missing spec would have made unnecessary.

## The structure going forward (operator resolution, 2026-08-16)

Verbatim intent: *"I gave you, the very capable agent, too much latitude
while we had good early results with the recursive test → iterate → test
→ go-to-implement recursive improvement based on bench measurements.
That's fixable. We just need more structure after we record this episode
honestly. It'll basically turn into: the iterative results feed the
normal brainstorming stage of a brf cycle and then proceed as-normal per
the skill."*

Mechanically, for this program: the recursive measurement loop is
**demoted from build authority to evidence generator**. Its standing
outputs (the evidence ledger above) become inputs to the brainstorming
stage of a build-robust-features cycle for the classifier wiring; the
skill's own stages — brainstorm, spec with falsifiable acceptance
criteria, plan, adversarially-reviewed build — carry it from there, with
the operator ruling at the gates the skill already defines. The
context-boundary diagnosis is why this is the fix and not ceremony: the
spec/plan pair is the artifact that survives compaction and handoff, so
in-window excellence stops depending on lossy session notes to cross
between windows. This document is the honest episode record that
precedes that cycle; the program resumes when the operator ratifies this
review and opens the brainstorm.

Questions the brainstorm inherits (in plain words, from the stopped
wiring derivation): whether the selection-layer battery is sufficient
evidence to wire narrowing at all, or a clean in-repo measurement comes
first; whether schema furnishing ships on mechanism reasoning alone now
that its outcome evidence is void; what the default is per backend
(always narrow, narrow only for Inkling-class, or an Elmer setting);
and whether the catalog side of the original DefCon complaint (matching
"pull the weather for my local area" against 1,477 items, and the
hostile 144KB catalog dump) ships in the same cycle or its own.

Session: spruce-birch-dune. The park record and this review are the two
durable artifacts of 2026-08-16/17; the parked branch holds the
recalibration; everything else is exactly where the merged PRs above
left it.
