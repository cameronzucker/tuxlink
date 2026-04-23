# Tuxlink — Implementation Pitfalls & Review Findings

> **Purpose:** Document implementation traps, design flaws, and corrected decisions that would cause production failures, security vulnerabilities, data correctness bugs, OR regulatory violations if shipped. This document is the primary code review reference for the tuxlink codebase.
>
> **Relationship to testing-pitfalls.md:** This document specifies *what* to implement and *why*. `docs/pitfalls/testing-pitfalls.md` specifies *how to verify* those implementations work correctly. They are complementary — cross-references are noted inline.
>
> **Last validated against codebase:** YYYY-MM-DD (replace when you audit against the current code)

---

## How to Use This Document

This document serves three audiences. Start here, then go directly to the section you need.

**If you're implementing code:** Go to the domain section matching your work area. Each entry has a clear *Flaw → Why It Matters → Fix → Lesson* structure. Follow the Fix. The Lesson teaches the generalizable principle so you'll catch the next instance of this pattern.

**If you're reviewing code:** Go to your domain section's **Review Checklist** at the end. Each item is a pass/fail check derived from the pitfalls above it. If a checklist item fails, read the referenced pitfall for context.

**If you're maintaining this document:** Every pitfall discovered during implementation, review, or debugging MUST be added here. See the maintenance sections at the end of this file. Partial updates cause drift.

---

## Table of Contents

| § | Section | You're working on... | Entries | Checklist |
|---|---------|---------------------|---------|-----------|
| 0 | [Live Radio Network Operations](#0-live-radio-network-operations) | Any code path that can transmit under the project's callsign | RADIO-1 | §0.C |
| 1 | [EXAMPLE-DOMAIN-1](#1-example-domain-1) | TODO — describe what this section covers | PREFIX-1 – PREFIX-N | §1.C |
| 2 | [EXAMPLE-DOMAIN-2](#2-example-domain-2) | TODO — describe what this section covers | PREFIX-1 – PREFIX-N | §2.C |
| — | [Orchestration](#orchestration) | Parallel subagent dispatch and output persistence | ORCH-1 | §Orchestration.C |
| A | [Historical Changelog](#appendix-a-historical-changelog) | Provenance, validation dates, review process meta-observations | — | — |
| B | [Unified Summary Table](#appendix-b-unified-summary-table) | All pitfalls at a glance, with severity and status | — | — |

---

# Section 0: Live Radio Network Operations

> **Reader context:** I'm building or reviewing code that could transmit
> under an amateur radio callsign — Winlink CMS sessions, packet radio
> TCP bridges, hamlib-driven rig commands, VARA modem sessions, or any
> code path that ends in an RF or network-bridge packet bearing the
> licensee's callsign.
>
> This section is **§0 because it supersedes every other pitfall.**
> If you are about to trip RADIO-1, stop. Do not continue reading the
> other sections. Do not write code. Surface it to the licensee.

---

### RADIO-1: Agent-autonomous transmission under the licensee's callsign

**The Flaw:** A test, script, CI job, scheduled task, or AI agent
invokes a code path that transmits on the amateur radio network under
the project's callsign, without the station licensee having given
explicit, scoped, per-invocation consent at the moment of the run.

Examples of this flaw in the wild:
- CI runs an integration test against `cms.winlink.org` on every push
  using credentials stored as a repo secret.
- A `cargo test` integration test hits real Winlink via an env var
  that's persisted in the developer's shell profile.
- An AI agent executing `superpowers:executing-plans` invokes a "run
  the live CMS smoke test" task that reads credentials from a config
  file without interactive consent.
- A `/loop` skill invocation runs a live-CMS smoke every 30 minutes
  "to monitor for regressions."

**Why It Matters:** Under 47 CFR Part 97, the station licensee is
personally responsible for every transmission bearing their callsign
(§ 97.101, § 97.103, § 97.113). Automated or unattended operation is
tightly constrained (§ 97.213) and does NOT cover "an AI agent decided
to run a test using cached credentials." Agent-autonomous transmission
without the licensee exercising real-time control is:

- A Part 97 control-operator violation at minimum.
- Potentially a third-party-traffic violation depending on content.
- Grounds for a Winlink CMS acceptable-use suspension from ARSFI,
  whose infrastructure is volunteer-operated and whose operators read
  repeated programmatic sessions as abuse.
- A reputational and legal hazard that attaches personally to the
  callsign holder, not to "the project."

Losing CMS access or attracting an FCC notice would be a project-level
operational disaster.

**The Fix:** Implement the full consent-gate protocol documented in
[`docs/live-cms-testing-policy.md`](../live-cms-testing-policy.md).
Concretely:

1. Every binary / script that can transmit lives in a dedicated
   `src-tauri/src/bin/` binary, NOT in `cargo test`-discoverable
   integration tests. Subagent shells must not be able to invoke it
   accidentally.
2. On startup, the binary prints a scoped consent banner: target,
   session count, expected duration, content, frequency / mode / band.
3. It reads from stdin and proceeds ONLY on the exact string `go`.
   Any other input (including EOF from a piped / non-interactive
   invocation) aborts with exit code 2.
4. Credentials are read from env vars or a dedicated operator-only
   keyring entry at run time; never persisted in a way that CI or
   scheduled agents can reach.
5. Every run logs to `dev/live-cms-sessions.log` with ISO-8601 UTC
   timestamp, callsign, test name, planned and actual session counts,
   outcome, and duration.

**ONE permitted exception:** the first-run wizard's Step 3 test send
(the "Send test message to SERVICE@winlink.org" button in the
production Tuxlink app). Rationale: the user just entered credentials
this session, clicked the button, and the UI clearly stated what would
happen. This is licensee-in-real-time-control, not agent-autonomous
operation. Any other exception must be surfaced for review.

**The Lesson:** Amateur radio regulation is not a UX best-practice
document; it's federal law with a licensee whose name is on the line.
The bar is not "don't abuse the service" — the bar is "the licensee
exercises control over every transmission." If you're not sure whether
a code path transmits, assume it does and apply the fix. The consent
gate is cheap; the incident is not.

---

### Section 0 Review Checklist

- [ ] **Check derived from RADIO-1** — No `#[test]` or `#[tokio::test]`
  function, and no `cargo test`-discoverable code path, invokes the
  real Winlink CMS, Winlink RMS, packet gateway, or any amateur
  network infrastructure bearing the project callsign. Live-network
  code lives exclusively in `src-tauri/src/bin/`.
- [ ] **Check derived from RADIO-1** — No CI workflow, cron schedule,
  `/loop` invocation, or agent-executable automation calls a binary
  that transmits. Verify by `grep -rn 'live_cms\|winlink.org\|cms.winlink' .github/ dev/ src-tauri/tests/`.
- [ ] **Check derived from RADIO-1** — Every transmit-capable binary
  prints a scoped consent banner and reads `go` from stdin before
  proceeding. Verify by walking the binary in question.
- [ ] **Check derived from RADIO-1** — Credentials are passed via env
  var or operator-interactive keyring prompt, never from committed
  config, committed secrets, or CI secret store.
- [ ] **Check derived from RADIO-1** — `dev/live-cms-sessions.log`
  exists (or the binary creates it) and receives one line per run.

---

# Section 1: EXAMPLE-DOMAIN-1

<!-- TODO: rename this section to your project's first domain (e.g. "Authentication & Security", "Data Pipeline", "API Handlers"). Delete this comment. -->

> **Reader context:** I'm building or reviewing [what this domain covers].
>
> TODO — describe the shape of the pitfalls in this section and why they matter.

---

### PREFIX-1: TODO — First Pitfall Title

<!-- TODO: replace this example with a real pitfall entry. Use the Flaw → Why → Fix → Lesson structure for complex findings, or a single condensed paragraph for simple ones. See §How to Add a Pitfall below. -->

**The Flaw:** TODO — what the code does wrong or what's missing.

**Why It Matters:** TODO — the production failure mode. What breaks, for whom, and why it's hard to detect.

**The Fix:** TODO — the specific code change or pattern to apply. Include a code example when the fix is non-trivial.

**The Lesson:** TODO — the generalizable principle. What should the reader watch for in future code?

---

### Review Checklist

<!-- TODO: one checkbox per pitfall above. Each item is a pass/fail check. Example format: -->

- [ ] **Check derived from PREFIX-1** — TODO

---

# Section 2: EXAMPLE-DOMAIN-2

<!-- TODO: rename, or delete this section if not needed. Duplicate the Section 1 template for each additional domain. -->

TODO.

---

## Orchestration

Pitfalls that arise when a session dispatches parallel subagents and consolidates their output. The canonical rules live in `docs/git-strategy.md` → §Multi-agent coordination → Output persistence. This section is the discovery hook for plan writers who arrive here via the `writing-plans-enhanced` (or equivalent) mandated-read path — it does NOT restate the rules in full.

### ORCH-1: Analysis Dispatches Must Persist Findings Before Returning

**Trigger:** Your plan dispatches parallel subagents (bug hunts, audits, phased analysis, parallel investigations) whose findings would be expensive to regenerate if lost.

**What you need to do:** Every such dispatched subagent MUST write its complete report to a persistent file BEFORE returning; the response message is not the sole record.

**Read the full rule:** `docs/git-strategy.md` → §Multi-agent coordination → Output persistence. That section carries the copy-pasteable prompt block (with `<PERSISTENCE_PATH>` substitution), file-path conventions, orchestrator commit cadence, and the cases where the rule doesn't apply.

**Why this is in implementation-pitfalls:** because the plan-writing skill mandates reading this file, and this rule has to be noticed at plan-write time (when the dispatch prompts are being drafted), not at execution time (when it's too late). The failure mode — orchestrator context compacting mid-consolidation and lossily dropping findings — is predictable and preventable if the plan author builds persistence into the dispatch prompts from the start.

### Review Checklist

- [ ] **Dispatch prompts include the mandatory-persistence block** — copy from `docs/git-strategy.md` §Output persistence; substitute `<PERSISTENCE_PATH>` with a durable per-subagent path (ORCH-1)
- [ ] **Plan specifies exact persistence paths, not "write somewhere useful"** — ambiguous paths default to `/tmp` under pressure, which doesn't survive (ORCH-1)
- [ ] **Orchestrator commits subagent artifacts wave-by-wave** — committed files land on the campaign branch before consolidation begins (ORCH-1)

---

# Appendix A: Historical Changelog

<!-- TODO: Add changelog entries as the document evolves. Format: -->
<!-- ## YYYY-MM-DD — <event> -->
<!-- - Added PREFIX-N (<title>) — <what and why> -->
<!-- - Updated PREFIX-M — <what changed> -->

TODO — add entries as this document evolves.

---

# Appendix B: Unified Summary Table

<!-- TODO: One row per pitfall for at-a-glance review. Keep in sync with the sections above. -->

| ID | Title | Severity | Status | Domain |
|----|-------|----------|--------|--------|
| ORCH-1 | Analysis Dispatches Must Persist Findings | HIGH | VALIDATED | Orchestration |
| PREFIX-1 | TODO | TODO | TODO | Section 1 |

Severity levels: `CRITICAL` (production data loss / security), `HIGH` (correctness bug under predictable conditions), `MEDIUM` (correctness bug under edge cases), `LOW` (cleanliness / clarity).

Status values: `VALIDATED` (prescribed fix is implemented and tested), `UNIMPLEMENTED` (pitfall documented but fix not yet in code), `SUPERSEDED` (replaced by another entry or no longer applicable).

---

# Appendix C: Document Maintenance Guide

## When to Update This Document

Update this document when any of the following occur:

| Trigger | Action |
|---------|--------|
| Bug hunt finds a generalizable pattern | Add a pitfall to the appropriate domain section |
| Health review flags a cross-cutting issue | Add or strengthen a pitfall |
| Implementation reveals a prescribed fix was wrong | Update the existing pitfall to match reality — the code is the source of truth |
| Code review catches a pitfall already documented here | Strengthen the entry with the new example |
| A pitfall's prescribed fix is implemented | Update the entry's status in Appendix B |
| A feature is removed or an approach abandoned | Mark the pitfall as SUPERSEDED with a note explaining why |
| testing-pitfalls.md adds a new section | Check if a cross-reference should be added here |

**Do NOT update this document for:**

- One-off implementation bugs that don't generalize to a pattern
- Code style preferences or formatting choices
- Performance optimizations without correctness implications

---

## How to Add a Pitfall

### Step 1: Choose the domain section

If the pitfall spans two domains, place it where the reader is most likely to look when they encounter the bug. Add a "See Also" cross-reference in the other section.

### Step 2: Assign the next ID

IDs are sequential within each section (`AUTH-3`, `DB-12`, etc.). Check the last entry in the section and increment. Use a short prefix that matches the section (2-5 letters, uppercase, descriptive).

### Step 3: Write the entry

**For complex findings** (non-obvious failure mode or architectural fix):

```markdown
### SECTION-N: Title

**The Flaw:** What the code does wrong or what's missing.
**Why It Matters:** The production failure mode — what breaks, for whom, and why it's hard to detect.
**The Fix:** The specific code change or pattern to apply. Include a code example when the fix is non-trivial.
**The Lesson:** The generalizable principle. What should the reader watch for in future code?
```

**For simple findings** (one-line pattern substitution, self-evident why):

```markdown
### SECTION-N: Title
[One paragraph: what's wrong, what to do instead, and why. No code example needed.]
```

**Use the right heuristic:** If an implementing agent could correctly apply the fix from just a one-line description without understanding the failure mode, use the condensed format. If they'd need to understand WHY to apply it correctly, use the full format.

### Step 4: Update the review checklist

Add a checkbox item to the section's review checklist (§X.C) that captures the key check for this pitfall.

### Step 5: Update the Table of Contents

Update the entry count in the TOC table (e.g., `AUTH-1 – AUTH-12` becomes `AUTH-1 – AUTH-13`).

### Step 6: Update the Summary Table

Add a row to Appendix B with the pitfall ID, title, severity, status, and domain.

### Step 7: Check for cross-references

- Does testing-pitfalls.md need a corresponding test guidance entry?
- Does another domain section need a "See Also" pointer?
- Does the same pattern exist elsewhere in the codebase? Grep for other instances.

---

## How to Update an Existing Pitfall

1. **Read the current entry** and understand its intent
2. **Check the code** to see what actually changed
3. **Update the entry** to reflect reality — never preserve a prescription that contradicts the code
4. **Update Appendix B** status if it changed (e.g., `UNIMPLEMENTED` → `VALIDATED`)
5. **Check Appendix A** — add a changelog line noting the update date and reason

---

## How to Mark a Pitfall as Superseded

Do NOT delete pitfall entries. Mark them:

```markdown
### SECTION-N: Title

> **SUPERSEDED (YYYY-MM-DD):** [Reason — e.g., "Feature removed in Phase 12" or "Replaced by SECTION-M which covers the broader pattern"]

[Original content preserved below for historical context]
```

Update Appendix B status to `SUPERSEDED`.

---

## Completeness Checklist

**A pitfall update is not complete until ALL of these are done.** Partial updates are how this document drifts — and a drifted document is worse than no document, because it creates false confidence in protections that don't exist.

- [ ] Entry written in the correct domain section with the correct format
- [ ] Entry has the next sequential ID for its section
- [ ] TOC entry count updated
- [ ] Appendix B summary table row added/updated
- [ ] Review checklist (§X.C) updated with the corresponding check item
- [ ] Cross-references checked: testing-pitfalls.md, other domain sections, See Also block
- [ ] If the pattern could exist elsewhere in the codebase: grepped for other instances
- [ ] Appendix A changelog updated with date and source

**If you skip any of these steps, the next agent to read this document will not find your pitfall.** The TOC is the routing table — without it, your entry is invisible. The summary table is the audit trail — without it, the next health review won't know your finding was addressed.

---

## Voice and Style Reference

This document uses persuasion principles to ensure agents follow critical practices:

- **Authority** for bright-line rules: "MUST", "Never", "Always", "No exceptions"
- **Implementation intentions** for triggers: "When writing a PATCH handler, ALWAYS use pointer types"
- **Social proof via failure modes**: "Without this, the webhook client follows redirects to internal metadata endpoints — every time"
- **Commitment** via checklists: the review checklists at the end of each section

When writing pitfall entries, apply these principles. A pitfall that says "consider using X" will be ignored under pressure. A pitfall that says "MUST use X — without it, Y happens every time" will be followed.

Reference: the `superpowers:writing-skills` skill (or equivalent in your skill library) carries the full persuasion-principles framework if you want to go deeper.
