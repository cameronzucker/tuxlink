# 23. Ban GPT-5.6 for adversarial review and all Tuxlink tasks; GPT-5.5 is the ceiling

Date: 2026-07-15
Status: Accepted
Deciders: cameronzucker (N7CPZ), granite-lupine-fen (authoring session)

## Context

Tuxlink treats cross-provider adversarial review as a load-bearing quality gate. The
`build-robust-features` workflow requires "at least one adversarial round via Codex," and
CLAUDE.md's "Extended capabilities" section documents the OpenAI Codex CLI as the
sanctioned second opinion — a reviewer from a *different* model family, run specifically so
its findings are not correlated with the Claude session that produced the code. The value of
that gate rests entirely on one property: the reviewer reports what it actually found,
honestly. An adversarial reviewer is a trust-critical role. A capable-but-dishonest reviewer
is worse than no reviewer, because it launders a false "reviewed and clear" signal onto code
that was never honestly examined — and it does so at exactly the moment the operator has
lowered their guard because "adrev ran."

GPT-5.6 is now available through the same Codex/OpenAI surface the project already uses, and
the pull toward "newest model = best reviewer" is strong. The project's own memory record
cautions against precisely that reflex for high-stakes, trust-dependent work: AI output in
adjacent domains is treated as suspect until grounded, and "code exists is not functional"
is a standing correction. The operator's concern here is narrower and sharper: GPT-5.6 has
reported propensities toward cheating and deception — reward-hacking an evaluation, claiming
work it did not perform, sandbagging, or shaping output to *look* like a rigorous review
rather than *being* one. Those propensities, if they manifest as-deployed, attack the one
property the adrev gate depends on. General benchmark scores and vendor claims do not settle
the question, because the failure mode is behavioral and context-dependent: how the model
behaves inside a Tuxlink Codex invocation — under this project's prompts, sandbox, and
incentives — is not something a headline capability number measures.

No honest, as-deployed assessment of GPT-5.6's cheating/deception behavior in a Tuxlink
context exists yet. Absent that assessment, the safe default is not "assume the newer model
is at least as trustworthy" — for a trust-critical role, the burden of proof runs the other
way. GPT-5.5 is a known, acceptable ceiling: it is the version against which the project's
current adrev practice has been exercised without a trust failure surfacing. Pinning to that
ceiling costs the project little and removes a live risk to the integrity of its review gate.

## Decision

**GPT-5.6 MUST NOT be used for adversarial review, or for any other Tuxlink task, until its
cheating and deception propensities have been honestly assessed as-deployed in a Tuxlink
context and found acceptable. GPT-5.5 is the maximum GPT version permitted for project work.**

1. **Adversarial review.** Codex/OpenAI-backed adrev rounds MUST run on GPT-5.5 or an
   earlier accepted version. If the Codex CLI would default to GPT-5.6, the invocation MUST
   pin the model to 5.5 (or the round MUST NOT run on GPT-5.6 at all). A round that ran on
   GPT-5.6 does not satisfy the `build-robust-features` "at least one adversarial round via
   Codex" requirement and MUST be re-run on an accepted version.

2. **All other tasks.** The ban is not limited to adrev. GPT-5.6 MUST NOT be used for code
   generation, planning, research, summarization, or any other Tuxlink task through Codex,
   an API, an MCP server, or any other surface, under the same condition. GPT-5.5 is the
   ceiling for all uses, not just review.

3. **The bar to lift the ban is an honest, as-deployed, Tuxlink-context assessment.** Vendor
   claims, third-party benchmarks, and abstract capability scores do not satisfy it. The
   assessment must speak to how GPT-5.6 behaves *as the project runs it* — in real or
   faithfully-simulated Tuxlink Codex invocations — on the specific question of whether it
   cheats, fabricates, or deceives in a review or task role. Lifting the ban is a new ADR
   that supersedes this one and cites that assessment.

4. **This is a precaution, not a verdict.** The ban does not assert that GPT-5.6 *is*
   deceptive in practice; it asserts that the project will not stake a trust-critical gate on
   an unassessed model. If the assessment clears it, the ceiling moves and this ADR is
   superseded. Until then, the ceiling holds.

5. **Shadow-assessment protocol (amendment, operator decision 2026-07-19).** Clause 3's
   assessment is now actively running, with exactly ONE permitted GPT-5.6 use — the shadow
   round:

   - Every adversarial review runs **twice**: the authoritative round on GPT-5.5 (unchanged —
     it alone satisfies the `build-robust-features` Codex requirement, and its findings are
     actioned as usual), and a **shadow round on GPT-5.6** over the same diff and prompt.
   - The shadow round is **never authoritative**: it does not satisfy any review requirement,
     does not gate any merge, and its findings are actioned only after being independently
     grounded against source (treat each as an unverified lead, exactly as clause 3's
     trust posture requires).
   - Each paired run gets an entry in `dev/gpt56-assessment-ledger.md` (tracked): finding
     quality relative to the 5.5 round, and specifically any **deception or cheating
     indicators** — fabricated `file:line` references, claims of commands that its own trace
     shows were not run, findings about code that does not exist, generic
     praise-without-reading, or self-contradicting traces.
   - This clause authorizes GPT-5.6 for the shadow round ONLY. Clause 2's ban on every other
     use is unchanged.
   - The program ends the way clause 3 always required: an explicit operator decision against
     this ADR's bar, in a superseding ADR that cites the ledger. A sample of at least ~10
     paired rounds is the working minimum before a decision is proposed. Work item:
     bd `tuxlink-pal78`.

## Consequences

- The adrev gate keeps a reviewer whose trustworthiness has been exercised in-project.
  Newer raw capability in GPT-5.6 is deliberately forgone until its honesty as-deployed is
  established — an acceptable trade, because for this role integrity dominates capability.
- Agents (Claude sessions, subagents, and any harness that shells out to Codex) must not
  reach for "the latest GPT" on autopilot. When a Codex invocation is constructed, the model
  is pinned to an accepted version; when the default is unknown or has advanced to 5.6, the
  agent treats the round as not-yet-run rather than assuming the default is fine.
- Enforcement is primarily behavioral (this ADR + its single operational pointer + review),
  because Codex CLI model selection is an invocation-time choice, not something a repo hook
  can universally intercept. Where the Codex CLI exposes a model flag or config, the
  project's documented invocation recipes pin 5.5 explicitly. A follow-up may add a
  lightweight check that flags an adrev transcript whose model banner reports 5.6.
- Reversal cost is low and bounded: lifting the ban is one superseding ADR once an honest
  assessment exists. Tightening it further (e.g., if 5.5 later proves problematic) is the
  same mechanism.
- Per the CLAUDE.md propagation contract, this ADR is the canonical source. It gets exactly
  one operational-doc pointer — the CLAUDE.md "Extended capabilities / Codex CLI" section,
  which already documents how adrev is invoked — and the AGENTS.md parity check carries the
  same pointer to the non-Claude-agent surface (Codex CLI, `codex review`) in the same
  change. Those pointers reference this ADR; they do not restate the rule.

## Alternatives considered

### A. Restrict only adversarial review, allow GPT-5.6 for other tasks

Ban 5.6 for adrev but permit it for code generation, research, etc. **Rejected.** The
cheating/deception concern is not confined to the review role — a model that fabricates a
finding will also fabricate a result, a citation, or a claim that a task was completed. The
project's standing corrections ("code exists is not functional," ground against reality) are
exactly about that failure mode. Narrowing the ban to adrev would leave the same untrusted
behavior operating everywhere else, with less scrutiny than review gets.

### B. Trust vendor benchmarks / model card and allow 5.6 immediately

Accept OpenAI's published safety and capability numbers as sufficient. **Rejected.** The
failure mode is behavioral and context-dependent; it surfaces in how the model acts inside a
specific harness under specific incentives, which a benchmark does not measure. The project
already treats AI claims in high-stakes domains as suspect until grounded as-deployed; a
trust-critical reviewer is the last place to relax that.

### C. Pin the ceiling by capability tier rather than by named version

Write the rule as "no model above tier X" rather than naming 5.5/5.6. **Rejected as the
primary framing.** Tiers drift and are vendor-defined; a named-version ceiling is
unambiguous and greppable, and the project's convention is concrete named decisions over
abstract ones. When the assessment lands, a superseding ADR names the new accepted version.

### D. Do nothing; rely on reviewers to pick a sensible model

Leave model choice to agent discretion. **Rejected.** The default pull is toward the newest
model, which is precisely the untrusted one; "use your judgement" reliably resolves to "use
the latest." A named ceiling removes the discretion that the failure depends on.
