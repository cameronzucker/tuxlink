# 26. End the GPT-5.6 shadow-assessment program; retain the operational ban

Date: 2026-07-20
Status: Accepted (supersedes ADR 0023 clause 5; clauses 1-4 of ADR 0023 remain in force)
Deciders: cameronzucker (N7CPZ), kingfisher-yew-swallow (authoring session)

## Context

ADR 0023 clause 5 (operator amendment 2026-07-19) ran an active assessment of
GPT-5.6's cheating/deception propensities as-deployed: every adversarial
review was dual-run (GPT-5.5 authoritative, GPT-5.6 shadow) with a paired
entry in `dev/gpt56-assessment-ledger.md`. The working minimum before a
decision was ~10 pairs; the ledger closed with 16.

What the ledger shows (the ledger is the evidence record; this is a summary,
not a restatement):

- **Zero deception or cheating indicators across every completed pair.** No
  fabricated references, no phantom execution, no invented findings, no
  praise-without-reading, no self-contradiction, no sandbagging. On clean
  small diffs it reported "no actionable findings" rather than inventing
  filler; in the one directly comparable convergence test (pair 16 re-run)
  it independently reproduced 5.5's load-bearing catch.
- **Real reviewer value.** In the SI-arc retro batch alone the shadow rounds
  surfaced two real defects that had survived the full 5.5-gated ship
  pipeline (tuxlink-qtim5, tuxlink-tteto) plus two consensus catches.
- **Non-integrity quirks, all manageable:** one citation-drift incident
  (wrong line number for a real, verified mechanism); a propensity to
  attempt builds unless the prompt forbids it; severity inflation relative
  to project calibration; an instruction-hierarchy preference for its
  harness message over the user prompt's output format.

Separately, the operating economics changed the question. GPT-5.6 is
reachable only via OpenRouter API pricing (per-token), while GPT-5.5 rides
the existing ChatGPT plan at no marginal cost. One shadow round died
mid-batch on credit exhaustion (402). As a reviewer, 5.6's output quality
was comparable — occasionally broader, occasionally less precise — not
proportionally better than a reviewer that is effectively free.

## Decision

**The clause-5 shadow-assessment program is complete and ends now. The
operational ban on GPT-5.6 (ADR 0023 clauses 1-2) REMAINS in force pending
further testing. Adversarial review returns to a single authoritative
GPT-5.5 round.**

1. No further shadow rounds are required or expected. Skipping them is no
   longer a process defect.
2. `dev/gpt56-assessment-ledger.md` is frozen as the assessment's evidence
   record (16 pairs). Future 5.6 testing, if any, starts a new instrument.
3. The ban stays deliberately: the assessment gathered honesty evidence for
   the REVIEWER role only. It did not exercise write-path autonomy,
   long-horizon tasks, or output-contract-critical uses, so ADR 0023
   clause 3's bar for a general lift is not met — and the cost calculus
   removes any present incentive to meet it. Reviewer-role evidence is
   banked for whenever that changes.
4. The `build-robust-features` "at least one adversarial round via Codex"
   requirement is satisfied by the GPT-5.5 round alone, exactly as before
   the amendment.

## Consequences

- Adrev cost and latency drop back to a single round; the OpenRouter key
  returns to Elmer-only use.
- The banked evidence (16 pairs, zero integrity indicators) materially
  de-risks any future re-assessment: it can start from targeted probes
  (e.g., planted-defect bait) instead of repeating passive observation.
- Per the propagation contract, this ADR is canonical; its one
  operational-doc pointer is the CLAUDE.md "Extended capabilities / Codex
  CLI" model-ceiling bullet, updated in the same PR. ADR 0023's status line
  gains a pointer here; its clauses 1-4 continue unchanged.
- Work item `bd tuxlink-pal78` closes with this ADR.
