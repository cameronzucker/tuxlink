# 28. Lift the GPT-5.6 ban; GPT-5.6 is permitted for all Tuxlink tasks

Date: 2026-07-23
Status: Accepted (supersedes [ADR 0023](0023-ban-gpt-5-6-until-deception-assessed.md) clauses 1-4 and [ADR 0026](0026-end-gpt56-shadow-assessment-retain-ban.md))
Deciders: cameronzucker (N7CPZ), spruce-glade-raven (authoring session)

## Context

[ADR 0023](0023-ban-gpt-5-6-until-deception-assessed.md) banned GPT-5.6 for
adversarial review and every other Tuxlink task, pinning GPT-5.5 as the
ceiling, on the grounds that a trust-critical reviewer must not be staked on a
model whose cheating/deception propensities had not been assessed as-deployed.
[ADR 0026](0026-end-gpt56-shadow-assessment-retain-ban.md) closed the
shadow-assessment program (16 paired ledger rounds,
`dev/gpt56-assessment-ledger.md`, **zero deception or cheating indicators**;
two real defects surfaced that had survived the 5.5-gated pipeline) but
deliberately **retained** the operational ban: the banked evidence covered the
**reviewer role only**, so per ADR 0023 clause 3 it did not clear a general
lift, and the OpenRouter per-token cost removed any incentive to pursue one.

Two things changed the operator's calculus since 0026:

1. **The practical blocker was a CLI-version issue, not the subscription.**
   GPT-5.6 ("gpt-5.6-sol") is reachable today via
   `npx --yes @openai/codex@latest exec -m gpt-5.6-sol …` (the pinned local
   codex 0.140.0 was too old; 0.145.0+ resolves the model). The "can't easily
   run it" friction that made the ban cheap to keep is gone.
2. **Utility.** In the operator's words, GPT-5.6 is "too useful to not use."
   The reviewer-role evidence is strong (16 pairs, zero integrity indicators,
   real defect-catching value the 5.5 gate missed), and the model's broader
   capability is worth reaching for on hard tasks.

**Honest limit of the evidence (recorded, not laundered).** ADR 0026 clause 3
is correct that the 16-pair ledger exercised only the reviewer role — not
write-path autonomy, long-horizon tasks, or output-contract-critical uses. A
general lift therefore extends trust **beyond** what the ledger formally
measured. This ADR does not claim the evidence closes that gap. It records the
operator's judgment that the residual risk on the unassessed surfaces is
acceptable because (a) the one role that WAS measured showed zero dishonesty
across 16 rounds, (b) reversal cost is low and bounded (one superseding ADR),
and (c) the project's standing groundedness disciplines — cross-provider
adversarial review, "code exists is not functional," ground-against-reality,
grounding every finding against source before action — already catch
as-deployed dishonesty in write-path work regardless of which model produced
it. The ban's original burden-of-proof posture is being traded, deliberately,
for utility plus those standing gates.

## Decision

**The operational ban on GPT-5.6 is lifted. GPT-5.6 is permitted for all
Tuxlink tasks. GPT-5.5 is no longer a ceiling.**

1. **All tasks.** GPT-5.6 MAY be used for adversarial review, code generation,
   planning, research, summarization, and any other Tuxlink task through Codex,
   an API, an MCP server, or any other surface.
2. **Adversarial review.** A GPT-5.6 round satisfies the `build-robust-features`
   "at least one adversarial round via Codex" requirement. GPT-5.5 remains
   equally acceptable and is cost-free on the existing ChatGPT plan, so it stays
   the sensible default when 5.6's marginal capability is not needed.
3. **No shadow / dual-run** is required or expected — ADR 0026 already ended
   that, and this ADR does not revive it.
4. **The ledger stays frozen.** `dev/gpt56-assessment-ledger.md` (16 pairs)
   remains the historical evidence record; it is not reopened by this decision.
5. **Cost, not trust, now governs the choice.** GPT-5.6 rides OpenRouter
   per-token pricing; GPT-5.5 rides the plan at no marginal cost. Which to reach
   for is a cost/quality tradeoff left to operator/agent judgment, not a rule.

## Consequences

- Standing quality disciplines are unchanged: adversarial review still runs;
  findings from any model are still grounded against source before they are
  actioned; ground-against-reality still applies to 5.6 output exactly as to any
  other. The lift widens which model may be reached for; it does not relax a
  single verification gate.
- The named-version ceiling (ADR 0023 alternative C) is retired for 5.6. If a
  future GPT version warrants caution, that is a fresh ADR naming that version;
  this decision does not pre-clear anything past 5.6.
- Per the propagation contract, this ADR is canonical. Its one operational-doc
  pointer is the CLAUDE.md "Extended capabilities / Codex CLI" model-ceiling
  bullet, rewritten in the same PR. The AGENTS.md parity line is updated in the
  same PR — and its **pre-existing drift** (it still described GPT-5.6 as
  "permitted ONLY as the non-authoritative shadow round," a state ADR 0026 had
  already ended) is corrected in the same change.
- ADR 0023's and ADR 0026's status lines gain "superseded by ADR 0028"
  pointers.

## Alternatives considered

### A. Lift only for the reviewer role; keep the ban for write-path work

Permit 5.6 where the ledger evidence is strongest (review) and hold the ban for
code generation / planning until a write-path honesty assessment exists.
**Declined by the operator.** The utility is general, the reversal cost is low,
and a split ceiling is more standing process overhead — "may I use 5.6 here?"
adjudicated per task — than the residual, gate-covered risk warrants. Recorded
as the more conservative option deliberately not taken.

### B. Keep the ban pending a write-path, as-deployed honesty assessment

Run for write-path autonomy the equivalent of the reviewer-role shadow program
before lifting. **Declined.** The assessment's cost and latency are not worth
paying given the reviewer-role evidence, the bounded reversal cost, and the
standing groundedness gates that already scrutinize any model's write-path
output. "Too useful to not use" won the tradeoff.
