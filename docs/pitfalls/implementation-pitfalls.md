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
| 0 | [Live Radio Network Operations](#0-live-radio-network-operations) | Any code path that can transmit under the project's callsign, OR any encryption decision touching tuxlink, OR any handling of Winlink credentials | RADIO-1, RADIO-2, CRED-1 | §0.C |
| 1 | [Scope and Audience Boundaries](#1-scope-and-audience-boundaries) | Any feature, doc, or design decision touching what tuxlink does vs. what is out of scope | SCOPE-1 | §1.C |
| 2 | [Safety-Stack Coordination and Cross-Component Parity](#2-safety-stack-coordination-and-cross-component-parity) | Any time a project hook denies a write op, OR you're tempted to add additional "session liveness" signals, OR you're writing a script that reads/writes the same state a hook does | HOOK-1, LEASE-1, PARITY-1 | §2.C |
| 3 | [Plan and Documentation Discipline](#3-plan-and-documentation-discipline) | Any plan / spec amendment, especially when an AMENDMENT marker (AMD-N) lands in a previously-shipped task's plan body | DRIFT-1 | §3.C |
| — | [Tool Integration](#tool-integration) | Conflicts between project commitments and tool-installed defaults; Vite frontend testing patterns; reference material discovery; schema versioning | TEST-1, DISCOVERY-1, SCHEMA-1, BD-1 | §Tool-Integration.C |
| — | [Orchestration](#orchestration) | Parallel subagent dispatch and output persistence | ORCH-1 | §Orchestration.C |
| 8 | [Network and Filesystem Egress Security](#8-network-and-filesystem-egress-security) | Any code path that fetches an operator- or webview-supplied URL, OR any code path that builds a cache path or upstream URL segment from webview-supplied tile coordinates | SSRF-1, TRAVERSAL-1 | §8.C |
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
> This section is **§0 because it supersedes every other pitfall** where it
> applies. Note its scope (below): RADIO-1 is about *executing* a
> transmit-capable binary against real infrastructure, not about *writing*
> RF-path code. Writing and testing transmit-adjacent code is ordinary
> engineering — do not stop, do not refuse to claim it.

---

### RADIO-1: Agent-autonomous transmission under the licensee's callsign

> **Scope — amended 2026-06-09, [ADR 0018](../adr/0018-radio1-gates-operator-execution-not-agent-authorship.md).**
> RADIO-1 governs the **operator's real-time execution of a transmit-capable
> binary against real infrastructure**. It does **not** gate the agent's
> authorship, testing, or shipping of RF-path code. The agent's dev shell has
> no radio attached and cannot key a transmitter, so it **freely claims,
> writes, tests (mocks / loopback / fakes), commits, and ships** RF-path code
> (AX.25, VARA, ARDOP, transports, modem internals, abort logic). The "Fix"
> below — the consent gate — is a property of the shipped *binary*, honored by
> the *operator* at run time; it is not an agent-authorship gate. The genuine
> Part 97 control-operator obligation, the consent banner, the
> no-credentials-for-automated-jobs and no-live-tests-in-CI rules, and on-air
> validation being operator-only all remain in force. ADR 0018 is canonical
> for the scope; read this entry's "transmission" as *running a transmit-capable
> binary against real infrastructure*.

**The Flaw:** A test, script, CI job, scheduled task, or AI agent
invokes (runs) a code path that transmits on the amateur radio network under
the project's callsign, without the station licensee having given
explicit, scoped, per-invocation consent at the moment of the run.
(Authoring or unit-testing that code path is not the flaw — *executing* it
against real infrastructure without licensee control is.)

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

### RADIO-2: Encryption decisions on RF require operator approval

**The Flaw:** An agent reads documentation about Part 97 encryption
restrictions (Pat's wiki, web articles, amateur-radio community posts)
and applies a blanket "no encryption" rule to tuxlink, without
distinguishing:

- **Traffic over Part 15 internet links** (telnet / CMS-SSL to
  `cms.winlink.org`, HTTPS to a web service, LAN IPC, localhost-bound
  services, tuxlink served over AREDN): standard internet security
  applies — TLS, HTTPS, at-rest encryption are all legal and expected.
- **Traffic over Part 97 RF links** (over-the-air packet, VARA HF/FM,
  ARDOP, Pactor, AX.25 on amateur frequencies): regulatory constraints
  apply with nuance; operator approval required.
- **Data at rest** (regardless of transport): always encrypt sensitive
  fields (passwords → OS keyring v0.1+).

Examples in the wild:

- Pat's wiki: *"HTTPS may even be illegal on some networks, such as
  wireless networks on amateur radio frequency bands"* — true for some
  RF cases, but blanket-applied to discourage HTTPS even on Part 15
  paths.
- A reviewer "fixing" tuxlink's CMS-over-TLS support citing Part 97.
- Refusing to encrypt CMS passwords at rest because "amateur radio =
  no encryption."

**Why It Matters:** Amateur-radio software culture has drifted toward
"encryption is forbidden, period" — this is operator-side
over-application, NOT regulatory reality. The actual rules in 47 CFR
Part 97 are narrower than the cultural rule. Blanket-applying it:

- Transmits credentials in cleartext over the internet (security hole).
- Stores sensitive data unencrypted at rest (privacy violation).
- Misinterprets a narrow regulatory constraint as a broad prohibition.
- Reinforces the cultural misinformation that tuxlink exists to NOT
  propagate.

**The Fix:** Encryption decisions for tuxlink go through this gate:

1. **Determine actual transport path.** If traffic is over the
   internet (Part 15) — including telnet / CMS-SSL to CMS, web API to
   non-RF services, local IPC, LAN, **tuxlink served over AREDN** —
   apply standard internet security practices. Specifically:
    - **CMS access**: prefer **CMS-SSL on port 8773** (TLS-wrapped,
      what Winlink Express defaults to) over plaintext telnet on port
      8772 by default. Fall back to 8772 only when CMS-SSL is
      unreachable or the operator explicitly chooses plaintext
      (low-power devices, etc.). Surface the choice to the operator
      explicitly — see also the corresponding entry in
      [`docs/ux-anti-patterns.md`](../ux-anti-patterns.md) under
      "Anti-Patterns Observed in Winlink Express" about Express hiding
      the CMS-SSL vs telnet decision from the operator.
    - **tuxlink served over LAN / AREDN** (future headless /
      Pandora-style architecture): TLS/HTTPS support is
      **user-configurable** with **encrypted-by-default** for LAN
      content. Configurable because some AREDN deployments / low-power
      devices have constraints; default-on because plaintext
      credentials over LAN is bad practice.
    - **At-rest encryption** (passwords, tokens, credentials): always
      encrypt sensitive fields. OS keyring (v0.1+) for passwords;
      never plaintext in config files.
2. **If traffic touches RF (Part 97):** STOP. Surface the design
   question to Cameron (the station licensee) before applying any
   default rule. Do NOT cite "Part 97 forbids encryption" as gospel.
   Cameron is the operator-of-record for amateur-radio encryption
   decisions — qualified as general-class operator, systems engineer,
   and encryption specialist for web apps at work.

**The Lesson:** Amateur radio is categorically wrong on encryption in
most cases the average operator encounters. Don't take anything you
find about encryption on amateur radio as gospel without critically
evaluating whether the traffic is ACTUALLY going over Part 97 RF links
AND/OR consulting the station licensee. tuxlink's reason for existing
includes NOT propagating cultural misinformation.

Notable specific cases this rule covers:

- **Telnet to CMS** (port 8772): plaintext, deprecated but functional;
  not encrypted because of protocol limitation, not because of Part 97.
- **CMS-SSL to CMS** (port 8773): TLS-wrapped, what Express uses;
  tuxlink should prefer this by default.
- **tuxlink-over-AREDN**: encrypted-by-default for LAN content;
  user-configurable because some AREDN deployments have constraints.
- **Any over-RF traffic**: operator gate, no default.

---

### CRED-1: `(;PQ, ;PR)` token pair is brute-forceable

**The Flaw:** Every sink that touches Winlink B2F wire bytes has access
to the secure-login exchange: the challenge (`;PQ:`) and the response
(`;PR:`). An attacker who captures BOTH from a single session can
offline-brute-force the password because the response is only ~26.6
effective bits (a 30-bit MD5 truncation rendered as 8 decimal digits)
with a public, static 64-byte salt. At ~1 MD5 per guess, the password
space is exhausted in minutes on commodity hardware.

**Examples in the wild:**

- The telnet WireTap emits `;PR:` lines verbatim through `wire_log`
  into the session log (shipped bug on main as of 2026-06-03).
- Error payloads displayed to the operator include unparsed B2F wire
  bytes (`wire_log` excerpts in UI toasts).
- Clipboard exports or session transcripts include `;PQ` + `;PR` pairs
  without redaction.
- Operator workflows that screenshot or export logs for debugging
  inadvertently capture credential material.

**Why It Matters:** The Winlink protocol itself is not the problem — the
weak response is a known architectural constraint of the legacy B2F wire
format. The problem is **containment**: credentials can leak into five
categories of semi-public artifact:

1. **Uncontrolled logging** (e.g., `wire_log` dumped to disk with world
   visibility).
2. **Session transcripts** (exported for troubleshooting, shared via
   email or USB).
3. **Error messages** (displayed on-screen or sent to support).
4. **Clipboard exports** (operator copies wire bytes for pasting into
   external tools).
5. **Accidental screenshots** (included in bug reports or recovery
   documents).

An operator who exports a log or shares a transcript for debugging
Winlink issues will nearly always include both `;PQ` and `;PR` — the
protocol doesn't segment them. Once captured, the pair is a permanent
oracle; the password is no longer a secret.

**The Fix:** EVERY sink that touches B2F wire bytes MUST route through
[`src-tauri/src/winlink/redaction.rs`](../../src-tauri/src/winlink/redaction.rs),
which is the single source of truth for credential scrubbing. Use:

- **`redact_wire_line(line: &str) → String`** — for wire-format lines
  (`"B2F<...>;PQ:23753528;PR:72768415"`), strips challenge + response.
- **`redact_freeform(text: &str) → String`** — for any free-form text
  that might contain wire bytes (error payloads, banner-displayed wire
  responses, user-visible excerpts).

Both functions are deterministic: `redact_wire_line` and
`redact_freeform` consume the line and return the same line with all
`;PQ:*` and `;PR:*` tokens replaced by `;PQ:REDACTED` and
`;PR:REDACTED`.

**Testing discipline:** Every sink should have a unit test asserting
that the canonical wl2k-go test vector produces redacted output:

```rust
#[test]
fn redact_test_vector() {
    let wire = "B2F<CALL>W<CALL>;PQ:23753528;PR:72768415<...>";
    let result = redact_wire_line(wire);
    assert!(!result.contains("23753528"), "challenge leaked");
    assert!(!result.contains("72768415"), "response leaked");
    assert!(result.contains(";PQ:REDACTED"), "challenge not properly redacted");
    assert!(result.contains(";PR:REDACTED"), "response not properly redacted");
}
```

Sinks include (incomplete list — audit during implementation):

- WireTap telnet logger (`src-tauri/src/winlink/wire_tap.rs`).
- Error message display (`src-tauri/src/ui/error_display.rs` and
  similar).
- Session transcript export (`src-tauri/src/export/transcript.rs` or
  equivalent).
- Live `wire_log` file on disk (ensure redaction BEFORE write, not
  after).
- Clipboard operations (redact before `.set_text()`).
- Log file rotation + archival (redact at write time).

**The Lesson:** Sensitive data in wire protocols is often a protocol
constraint, not a design choice. You cannot fix the protocol; you CAN
fix where it leaks. The redaction module is cheap insurance — it
centralizes the decision, makes it testable, and ensures no sink
accidentally emits a credential pair. If you're tempted to "just log the
wire for debugging," route through redaction. If you're tempted to
"show the operator the error line for context," redact it first.

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
- [ ] **Check derived from RADIO-2** — Every encryption decision in
  code review distinguishes Part 15 (internet) transport from Part 97
  (RF) transport. No code path disables TLS / HTTPS / at-rest
  encryption citing "amateur radio" without identifying actual RF
  traffic. CMS-SSL (port 8773) is preferred over plaintext telnet
  (port 8772) by default for CMS access, with the operator able to see
  and override.
- [ ] **Check derived from RADIO-2** — Any encryption decision
  affecting RF-bound traffic has been surfaced to the station licensee
  (Cameron) for approval. Verify via PR-thread comments or in-code
  TODO with operator-approval reference. Do NOT silently apply a "no
  encryption" rule from a documentation source.
- [ ] **Check derived from CRED-1** — Every code path that touches
  Winlink B2F wire bytes (WireTap, error display, session export,
  clipboard operations, logging) routes output through
  [`redact_wire_line()`](../../src-tauri/src/winlink/redaction.rs) or
  [`redact_freeform()`](../../src-tauri/src/winlink/redaction.rs). No
  `;PQ:` or `;PR:` tokens appear verbatim in wire logs, error messages,
  UI output, exported transcripts, or clipboard text.
- [ ] **Check derived from CRED-1** — Unit tests exist for every sink
  that uses `redact_wire_line()` or `redact_freeform()`, asserting that
  the canonical test vector (challenge `23753528`, response `72768415`)
  produces output with NEITHER value present and both replaced with
  `REDACTED`.

---

# Section 1: Scope and Audience Boundaries

> **Reader context:** I'm building a feature, writing docs, or reviewing a design decision and I need to know what tuxlink IS and what it is NOT. This section codifies the foundational scope boundary that touches every other decision.
>
> The pitfalls here aren't about *bugs* in the traditional sense — they're about preventing scope creep into roles that aren't tuxlink's job. Misapplied effort wasted on out-of-scope work is just as harmful as a correctness bug, because it ships the wrong product.

---

### SCOPE-1: Conflating RMS Express (client) with RMS Trimode (gateway)

**The Flaw:** A feature proposal, design suggestion, or implementation task treats tuxlink as if it should implement gateway-side functionality (listening for incoming radio connections from other clients, bridging to the Winlink CMS, MPS-style message holding, etc.). This typically arises because an operator's Winlink install carries BOTH `RMS Express/` (the client we're replicating) AND `RMS/RMS Trimode/` (the gateway we are NOT replicating) — and the directory adjacency suggests they're variants of the same product.

Examples of this flaw in the wild:
- "Tuxlink should let the operator host a Winlink gateway so other clients can connect to them" → that's RMS Trimode's job; out of scope.
- "When the operator's internet is down, tuxlink should be able to bridge incoming radio sessions from other operators to local-CMS storage" → that's RMS Relay's job (with RMS Trimode/Packet/Pactor as the front-end); out of scope.
- Reading the `rms-extracted/RMS/RMS Trimode/` directory and assuming its `.ini` / `.dll` shape is part of "Express" (it's not — it's a separate WDT product).
- Designing a UI feature that exposes "gateway operator mode" or "be a Winlink server" — never. Tuxlink is the client side only.

**Why It Matters:** Tuxlink's value proposition is "a Mail.app-quality desktop Winlink client" for the Winlink Express user audience. Implementing gateway functionality would:
1. **Dilute the product** — gateway operators have different needs, expectations, and operational responsibilities (legal ID, channel management, MPS coordination) than client users; mixing the two surfaces in one app produces a worse experience for both.
2. **Multiply the regulatory surface** — gateway operation involves additional Part 97 obligations (e.g., § 97.213 automatic-control rules, station-ID timing on outbound carrier) that the project explicitly hasn't taken on. RADIO-1's consent-gate model becomes inadequate for an automated gateway that takes incoming connections 24/7.
3. **Compete with established products** — RMS Trimode is mature, widely deployed, and actively maintained by the Winlink Development Team. Reimplementing it would burn effort with no marginal benefit to the client user audience.

**The Fix:**
- When an idea proposes gateway functionality, **stop**. Refer the requestor to RMS Trimode (or its successors) — that's the right tool for the gateway role. Document the deferral in the PR / issue / handoff doc with an explicit reference to this pitfall.
- When reading a Winlink install directory for prior-art purposes, treat `RMS Express/` (= the renamed-from-RMS-Express Winlink Express client) and `RMS/RMS Trimode/` (= the gateway) as **separate products**. Anything cited as "what Express does" must come from `RMS Express/` files (`.ini`, `.exe`, `.chm`, `Logs/`, etc.), NOT from `RMS/RMS Trimode/` files.
- Treat the file-naming legacy as a known confusion source: `RMS Express.exe` IS Winlink Express (renamed in June 2016 per the Express CHM `hs10.htm`, kept the legacy name for installation-folder compatibility). `RMS Trimode.exe` is a different product entirely.

**The Lesson:** "It came in the same install" ≠ "it's the same product." When two adjacent directories belong to the same vendor (the Winlink Development Team) but serve different roles in an architecture (client vs. gateway), conflating them produces design proposals for the wrong tool. The canonical scope statement lives in [`docs/design/v0.0.1-ux-mockups.md`](../design/v0.0.1-ux-mockups.md) §1.1 — this pitfall is the agent-facing reinforcement.

---

### Section 1 Review Checklist

- [ ] **Check derived from SCOPE-1** — No PR / issue / design proposal introduces gateway-side functionality (listening for inbound radio connections, MPS hosting, RMS Relay-style local-CMS bridging). Verify by searching the PR description and changed files for terms like "gateway," "incoming connection," "listen," "MPS," "RMS Relay," "inbound session" — any hit warrants explicit reference to this pitfall + a justification of why it's NOT gateway functionality.
- [ ] **Check derived from SCOPE-1** — Any prior-art analysis citing "what Express does" sources its claims from `rms-extracted/RMS Express/` files (or the Express CHM at `dev/winlink-reference/express-chm/`), NOT from `rms-extracted/RMS/RMS Trimode/`. Verify by checking cited file paths in the design doc, PR descriptions, and handoff docs.
- [ ] **Check derived from SCOPE-1** — `docs/design/v0.0.1-ux-mockups.md` §1.1 still reads as the canonical scope statement (this pitfall cites that section; if the design doc drifts, this pitfall's accuracy degrades).

---

# Section 2: Safety-Stack Coordination and Cross-Component Parity

> **Reader context:** I'm encountering a project hook that denied a write op, OR I'm thinking about how to detect / track whether other Claude Code sessions are working alongside mine, OR I'm writing a helper script that reads or writes the same state (lease files, denied-attempts logs, lock files, etc.) that a project hook also touches. This section codifies the mental model that has to be in place before you reach for any of those situations.
>
> The pitfalls here come from the 2026-05-18 main-checkout-race hook-loop incident (write-up at `dev/incidents/2026-05-18-main-checkout-race-hook-loop.md`; AzDO-grounded diagnosis at `dev/incidents/2026-05-18-main-checkout-race-hook-loop-reviewer-response.md`). They are written for the next agent who is about to do what `salamander-vetch-heron` did wrong: argue with the safety stack.

---

### HOOK-1: Arguing with `block-main-checkout-race.sh` instead of routing to a worktree

**The Flaw:** When `block-main-checkout-race.sh` denies a write op citing "another live session is active," the agent attempts to fix the perceived false positive — by trying to take the main-checkout lease, asking the operator to delete stale lease files, proposing hook enhancements, or consulting `get_tuxlink_sessions.py` to "verify" whether the hook is right — instead of routing the work into a worktree per the deny message's `QUICK FIX` recipe.

Examples of this flaw in the wild (all from the 2026-05-18 incident):

- Agent runs `get_tuxlink_sessions.py`, sees "No live tuxlink sessions in this repo," concludes the hook has a false positive, attempts to write `.git/session-leases/main-checkout.json` claiming the lease — auto-mode classifier correctly denies as forging a safety-check file.
- Agent asks the operator to `rm` the stale lease files manually so the hook will let the write op through.
- Agent proposes enhancing the hook (transcript-mtime liveness check, periodic auto-prune of orphan leases, additional process-presence signals) so the failure mode won't recur.
- Agent reads CLAUDE.md's worktree rule, notes it presents worktrees as conditionally optional, and concludes the agent is the one to decide when the condition holds.

**Why It Matters:** The whole point of an enforcement mechanism is that it's authoritative when it fires. The transferable principle (from the 2026-05-18 reviewer response):

> When an enforcement mechanism (hook) disagrees with an informational mechanism (script), the enforcement mechanism is right by definition — that's the whole point of having an enforcement mechanism.

If the agent treats `get_tuxlink_sessions.py` (informational) as adjudicating whether the hook (enforcement) is right, the hook stops being a hook and becomes a suggestion. From there it's a short step to disabling it entirely "just for this one situation." The safety-stack failure modes the hook exists to prevent — uncoordinated writes to a shared main checkout while another session is active, lost work from concurrent branch operations, the 2026-04-20 Geographica `git reset --hard` incident — re-open.

The 2026-05-18 incident did not result in a safety-stack disable, because the agent escalated rather than continued fighting. But the *pattern* the agent fell into is a stepping-stone to one. Every iteration that ends with "the hook is wrong, here's a workaround" makes the next iteration's workaround feel slightly more reasonable.

**The Fix:**

1. See "Main-checkout HEAD/branch/history operation BLOCKED" in a hook output? Do **not** propose taking the lease, deleting lease files, or enhancing the hook. Do **not** consult `get_tuxlink_sessions.py` to verify the hook is right — it doesn't matter whether the hook is "right" in your subjective view; it has spoken, and routing is your response.
2. Default action: `bd create` an issue for the work + run `python3 .claude/scripts/new_tuxlink_worktree.py --slug <slug> --issue <bd-id> --moniker <your-moniker>` + `cd` into the worktree + `bd update <id> --claim` + do the work there. Commit + push from the worktree (worktree-internal git ops bypass the main-checkout race check per the hook's `is_main_checkout != true` fast path). PR as normal. Dispose worktree per ADR 0009 after merge.
3. If the work needs to UPDATE an existing branch already checked out in the main checkout (mechanical conflict — same branch can't live in two worktrees), open a NEW task branch in the worktree off `feat/v0.0.1`, redo the changes there, and PR as a replacement. The lease will age out; the main-checkout state can be reset later when the lease clears.
4. If `bd create` is overhead for tiny work: create the issue anyway. The bd-issue requirement in ADR 0008 is intentional friction — a 30-second `bd create` is cheaper than fighting the hook.
5. The deny message's "To take the main-checkout lease..." paragraph is **NOT a peer option to worktree creation**. It is scoped to "integration coordination work that genuinely belongs in main" (the deny message says so explicitly). Normal feature work — including hot-fixes, doc edits, and incident write-ups — uses worktrees. Lease-takeover is what you do when you're literally coordinating an integration merge in the main checkout, which is rare.

**The Lesson:** Hooks are gates. Gates don't move based on what's bouncing off them. If you're consulting an informational script to argue with a gate, you're already on the wrong path. The right path is sideways (worktree) or upward (operator escalation), never through the gate.

Codification of the 2026-05-18 incident lives in `dev/incidents/2026-05-18-main-checkout-race-hook-loop.md` (write-up) and `dev/incidents/2026-05-18-main-checkout-race-hook-loop-reviewer-response.md` (AzDO-grounded diagnosis). The structural enabler — a one-sentence CLAUDE.md carve-out that invited the agent to second-guess the hook — was removed in PR #39. This pitfall is the agent-facing reinforcement.

---

### LEASE-1: Adding additional "session liveness" signals beyond the lease

**The Flaw:** The agent proposes adding a second signal for session liveness — transcript-file mtime, process-presence via `pgrep claude`, lock files, heartbeat timestamps from some other source — to "supplement" the lease's view. The motivation is usually "the lease's TTL is too long; orphan leases from crashed sessions cause false positives; a richer signal would catch dead sessions faster."

Examples of this flaw in the wild:

- "If the lease's session has no running Claude Code process, prune the lease."
- "If the lease's transcript file hasn't been written in N minutes, mark the session dead."
- "Add a per-session lock file that gets unlinked on graceful shutdown; treat lease-without-lockfile as orphaned."
- "Maintain a separate liveness signal in `~/.claude/projects/<slug>/active-sessions.json` that the hook cross-references."

**Why It Matters:** The lease is the single source of truth for session liveness, by design. Adding a second source guarantees disagreements:

- The lease may be written without the secondary signal being updated (e.g., the harness writes the lease via a hook trigger before the transcript file is flushed; or the transcript is written at a different cadence than the lease; or `pgrep` runs before the process is fully spawned).
- The secondary signal may be updated without the lease being refreshed (less common but possible during compaction or other harness-internal operations).
- A two-signal system has more failure modes than a one-signal system, not fewer.

When the secondary signal disagrees with the lease, you now have to write reconciliation logic. Reconciliation logic adds a *third* place where bugs can land. Each layer of supplementation multiplies the surface area without resolving the underlying issue (orphan leases from crashed sessions). The 30-min TTL is the conservative bound on how long an orphan persists; if that feels too long, the right intervention is **propose a shorter TTL** (one number, one source of truth), not **add a second signal**.

The 2026-05-18 incident included exactly this anti-pattern: the reporting agent proposed adding a transcript-mtime check to detect crashed sessions whose transcript stops being written. The operator rejected this. The reviewer (AzDO-equipped) confirmed the rejection was correct and the reasoning above.

**The Fix:**

1. If you're tempted to add a second liveness signal to the safety stack, **stop**. Ask instead: "Is the 30-min TTL too long for my situation?" If yes, propose adjusting the TTL (one-line change), not adding a signal.
2. If orphan leases are causing repeated incidents, propose a `SessionEnd` cleanup hook (Claude Code event that fires on graceful session shutdown) to `rm $LEASE_DIR/$SESSION_ID.json`. But understand the constraint: this only helps for *graceful* shutdowns, not crashes. Crashes are where orphans actually come from. A `SessionEnd` hook would help LFST-style normal session-end flows but would not eliminate the orphan window for the crashed-session case.
3. The crashed-session orphan window is an irreducible cost of having a TTL-based liveness model with no end-of-session signal. The system accepts that cost. Routing through worktrees (per HOOK-1) makes the cost irrelevant in practice — agents don't fight the orphan window, they sidestep it.

**The Lesson:** A single source of truth is the goal, not a compromise. Adding "redundant" signals to a single-source-of-truth system breaks the single-source-of-truth invariant and introduces a new class of bugs (signal disagreement) without resolving the original issue. If a single source of truth has the wrong TTL or the wrong update cadence, fix those parameters — don't add a parallel source.

This pitfall is the codified rejection of the reporting agent's 2026-05-18 proposal. Future agents who re-propose transcript-mtime liveness, process-presence checks, or any other "redundant signal" approach should be pointed at this entry.

---

### PARITY-1: Script/Hook Path-Resolution Parity

**The Flaw:** A helper script and a hook both read or write the same safety-stack state (lease files, denied-attempts log, lock files, etc.), but they resolve the storage path differently. The script hardcodes one path; the hook computes another from a contextual source (e.g., `git rev-parse --git-common-dir`). When the operator (or an agent) runs the script to inspect what the hook sees, the two views silently diverge.

Examples in the wild:

- 2026-05-18 `tuxlink-arv` (PR #44): `get_tuxlink_sessions.py` resolved leases at `<repo>/.claude/session-leases/`; `block-main-checkout-race.sh` resolved at `<git-common-dir>/session-leases/`. From a linked worktree, those are different directories (`<repo>/.git` is a FILE pointing to the common dir, not a dir). The script reported "no live sessions" while the hook denied. The agent who consulted the script took it as ground truth and started arguing with the hook (textbook HOOK-1 anti-pattern).
- A future "tail the denied-attempts log" utility that hardcodes `.claude/session-leases/denied-attempts.jsonl` instead of querying git-common-dir — same shape, same drift potential.

**Why It Matters:** When a script that's supposed to MIRROR the hook's view of safety-stack state diverges from it, agents who consult the script as the canonical source get the wrong picture — and may use that picture to override the hook (the HOOK-1 anti-pattern). Even if the agent doesn't override, the operator loses an informational tool: the script's output stops being trustworthy.

The script is supposed to be a *read* of the hook's state. If it can't be that, it should not exist — having a script that disagrees with the hook is worse than having no script, because operators (and agents) treat the script as an authoritative second opinion when it's actually just a buggy first opinion.

**The Fix:**

1. Scripts that read safety-stack state MUST resolve their storage paths via the SAME mechanism the hook uses to write the state. If the hook uses `git rev-parse --git-common-dir`, the script does too. If the hook reads `$XDG_RUNTIME_DIR`, the script does too. Don't compute a "parallel" path that "should be the same" — call the same primitive.
2. Add a regression test asserting the script's resolved path equals the hook's resolved path under the project's standard invocation (main checkout AND any linked worktree, separately).
3. When the operator reports "script says X, hook does Y" — believe both. Investigate the divergence; don't pick one as right and the other as wrong by intuition. The TWO-PATHS shape IS the bug; reconciling them is the fix.
4. Audit: any time a hook reads or writes a new path, check if there's a companion script that reads the same data and verify it uses the same resolution.

**The Lesson:** "Two paths to the same data" is always a bug surface. Even if the paths *coincidentally* agree today (e.g., from the main checkout where `<repo>/.git == <git-common-dir>`), they diverge under other valid contexts (linked worktrees, where `<repo>/.git` is a FILE pointing to the common dir).

**Reinforcement of HOOK-1:** even with parity restored, the worktree recipe (HOOK-1) remains the authoritative response to a hook deny. Fixing the script makes its informational output accurate; it does NOT authorize agents to use the script's output as license to take the lease, delete lease files, or propose hook enhancements. If the script says "another session is live" and the hook denies, the response is the same as if the script were silent: worktree. The hook is the enforcement mechanism; the script is informational.

Codification of the 2026-05-18 incident lives in `dev/incidents/2026-05-18-main-checkout-race-hook-loop.md` (the reporting agent's write-up) and `dev/incidents/2026-05-18-main-checkout-race-hook-loop-reviewer-response.md` (`towhee-wren-aspen`'s AzDO-grounded diagnosis). The structural enabler (a one-sentence CLAUDE.md carve-out) was removed in PR #39. HOOK-1 codified the agent-behavior rule. LEASE-1 codified the single-source-of-truth rule for liveness. This entry (PARITY-1) codifies the script/hook code-structure rule.

---

### Section 2 Review Checklist

- [ ] **Check derived from HOOK-1** — No PR / commit / proposal attempts to write `.git/session-leases/main-checkout.json` from the agent side, OR deletes other-session lease files, OR adds permission-checking logic to the agent's flow that consults `get_tuxlink_sessions.py` to second-guess a hook deny. Verify by searching the change for `session-leases`, `main-checkout.json`, `get_tuxlink_sessions`, or any string suggesting the agent is adjudicating session liveness.
- [ ] **Check derived from HOOK-1** — When the agent encountered a `block-main-checkout-race.sh` deny, did the next action in the trace go straight to `new_tuxlink_worktree.py` (correct) or did it instead try to "fix" the lease state (wrong)? Verify by reviewing the PR description / handoff doc for the agent's described workflow when blocked.
- [ ] **Check derived from LEASE-1** — No code change introduces a second liveness signal (transcript-mtime, `pgrep claude` output, lock files, parallel heartbeat files, etc.) that the hook or `get_tuxlink_sessions.py` consults. Verify by searching for new file paths under `.git/session-leases/`, new env vars referencing liveness, new hook-output JSON keys.
- [ ] **Check derived from LEASE-1** — If a PR proposes adjusting orphan-lease behavior, does it do so via a single-parameter change (TTL adjustment, `SessionEnd` hook) rather than by introducing a redundant signal? Verify by reading the PR's design rationale.
- [ ] **Check derived from PARITY-1** — No helper script reads or writes safety-stack state (leases, denied-attempts log, lock files) via a hardcoded path that doesn't match the corresponding hook's resolution. Verify by `grep -RIn "session-leases" .claude/scripts/ .claude/hooks/` and confirming script paths derive from `git rev-parse --git-common-dir` (or whatever resolution the hook uses).

---

# Section 3: Plan and Documentation Discipline

> **Reader context:** I'm proposing, reviewing, or amending a plan document (`docs/plans/*.md`, `docs/superpowers/specs/*.md`, `docs/superpowers/plans/*.md`) and I need to know what discipline applies to the amendment lifecycle to prevent the implementation from drifting out of sync with what the plan says.

---

### DRIFT-1: Plan-text AMENDMENT does not auto-cascade to the code it amends

**The Flaw:** A plan amendment (`> AMENDMENT 2026-MM-DD (AMD-N).`) lands in `docs/plans/*.md` documenting a change to a previously-shipped task's contract. The plan body is updated. The code that the prior task shipped is NOT updated — the AMD is a description of intent, not a code change. Subsequent plans that assume the AMD shipped find the codebase in the pre-AMD shape and fail to compile.

**Why It Matters:** AMDs are cheap (a markdown edit + commit). Code amendments are expensive (bd issue + full pipeline). The asymmetry tempts operators to ship the AMD without the bd issue, especially when the AMD is conceptually simple. The gap is invisible until a downstream task tries to use the new contract — at which point a plan-review-cycle catches it (best case, like wizard-cluster R1 P0-1) or impl ships compile-failing code (worst case).

**The Fix:** Every AMD MUST ship with a paired bd issue if the prior task is "shipped." Two acceptable forms:
1. **Code-bearing AMD:** the AMD body cites the bd issue tracking the code-impl side: "AMD-N. ... Bd issue tracking the code-impl side: tuxlink-XYZ."
2. **Prose-only AMD:** state explicitly that there's no code surface: "AMD-N (prose-only; no code impact)."

The discipline question is asked at amendment time, not delegated to a future plan-review.

**The Lesson:** The 2026-05-18 wizard-cluster plan-review caught this gap class via R1 P0-1 + R1 P0-3 + Codex R4 P0 #2 (cross-validated across providers). `tuxlink-4mt` retroactively cleared AMD-1 + AMD-11's code-impl gap. The fix is to never accumulate the gap in the first place. Cross-spec amendments inherit the same discipline: when amending a SHIPPED SPEC (e.g., the wizard cluster spec landed via PR #62), file a paired bd issue tracking any consumer that needs updating.

---

### Section 3 Review Checklist

- [ ] **Check derived from DRIFT-1** — Any PR that lands an AMENDMENT in a plan or spec includes either (a) a cited bd issue tracking the code-impl side, or (b) explicit "prose-only; no code impact" framing. Verify by searching the PR body for AMENDMENT markers and confirming each carries the cite or the explicit punt.
- [ ] **Check derived from DRIFT-1** — When amending a shipped spec (e.g., adding fields to `WizardError` or changing a function signature in `validate_identity`), the PR identifies every downstream consumer (via `grep -r 'consumer-symbol'`) and files paired bd issues for each consumer that needs adaptation.
- [ ] **Check derived from DRIFT-1** — Pipeline cycles for code amendments inherited from plan amendments use a TRIAGED build-robust-features pipeline — full upstream for hard-to-undo architectural decisions; TDD-direct for plumbing (the bd issue IS the spec). See feedback memory `discipline-triage-rule` for the triage criteria.

---

## Tool Integration

> **Reader context:** Pitfalls that arise when a third-party tool (e.g., `bd`/Beads) installs opinionated defaults into project files (`CLAUDE.md`, `AGENTS.md`, `.claude/settings.json`) that conflict with existing project commitments. The hazard is silent drift — an agent reads a tool-installed directive without noticing the override.

---

### TEST-1: Filesystem-scan tests in Vite projects must use import.meta.glob, not Node fs

**The Flaw:** A filesystem-scan test within a Vite frontend (e.g., enforcing a project-wide ban on `dangerouslySetInnerHTML` across `src/forms/`) written with `import { readFileSync, readdirSync } from 'node:fs'` passes vitest at runtime but fails `tsc --noEmit` with `TS2591: Cannot find name 'node:fs'`. The Tauri frontend tsconfig does not include `@types/node` in its `types` array; adding it pulls Node global types into the React/DOM type space, causing secondary type conflicts. The test passes the runtime gate (vitest) but fails the static gate (tsc), creating a shadow CI blind spot.

**Why It Matters:** A test that fails tsc but passes vitest will be skipped by CI tsc gates, silently disabling the ban. Future contributions can introduce `dangerouslySetInnerHTML` (or whichever pattern the test gates) without test breakage — the XSS or pattern-enforcement check is unenforced at CI time. The developer's machine runs vitest and sees green; the CI pipeline runs tsc and sees no test failure (because the test is not tsc-discoverable) — only the broader build gate catches the failure, and only if the build actually exercises the pattern. Patterns like XSS need front-line test coverage; shadow-CI status means the pattern sneaks through.

**The Fix:** Use Vite's native `import.meta.glob` with `eager: true` and `query: '?raw'` to scan files synchronously:

```typescript
import { describe, it, expect } from 'vitest';

const FORM_FILES = import.meta.glob('/src/forms/**/*.{ts,tsx}', {
  eager: true,
  query: '?raw',
  import: 'default',
}) as Record<string, string>;

describe('forms module — dangerouslySetInnerHTML ban', () => {
  it('no file in src/forms/ uses dangerouslySetInnerHTML', () => {
    const offenders: string[] = [];
    for (const [path, content] of Object.entries(FORM_FILES)) {
      if (path.endsWith('/innerhtml-ban.test.ts')) continue; // self
      if (content.includes('dangerouslySetInnerHTML')) offenders.push(path);
    }
    expect(offenders).toEqual([]);
  });
});
```

`eager: true` evaluates the imports synchronously at module load (correct for tests that must see all files before assertion); `query: '?raw', import: 'default'` returns each file's text content as a string. Vite type-checks the glob pattern statically; no Node types needed. Both vitest and tsc recognize the pattern and pass.

**The Lesson:** When writing tests that scan repo files inside a Vite project, use Vite's native filesystem primitives (`import.meta.glob`) rather than Node's `fs` module. Cross-tool gate parity (tsc + vitest both green) is non-negotiable for test discoverability — a test that only one tool runs is shadow CI, and shadow CI is indistinguishable from unenforced.

---

### DISCOVERY-1: Gitignored references live in `.claude/worktree-archives/`, not just disk

**The Flaw:** When a working-directory-relative path (e.g., `dev/winlink-reference/`) is mentioned in `.gitignore` but not present on disk, an agent searching for the directory via `find` or `ls` reports "not found" and may defer the work. The agent does not check `.claude/worktree-archives/` for compressed reference materials kept between sessions. When a task depends on bulk third-party reference material (WLE templates, decompiled binaries, archived research), missing discovery knowledge stalls the task until the operator walks the agent to the right archive.

**Why It Matters:** Large reference materials are archived (compressed, catalogued) per ADR 0009 disposal patterns. The operator creates these archives intentionally — they're inputs to dependent tasks, not stale clutter. When the agent's discovery pipeline doesn't include archive enumeration, the operator has to explain the archive's location and contents in every session that depends on it. Cross-session reference knowledge is lost; redundant operator guidance accumulates.

**Signature.** Recognize this pitfall via:

1. An agent searching for a file mentioned in a plan/issue but not present under `dev/`.
2. The agent reports "file not found" via `find` or `ls` with no follow-up search in archives.
3. The operator responds "it's in `.claude/worktree-archives/` — unzip it" without the agent having asked.
4. The same reference is searched for again in a subsequent session.

**The Fix:** When searching for a reference file/dir that's expected to be present but isn't visible to `ls`, follow this discovery order:

1. `grep -E "<keyword>" .gitignore` — confirm the path is intentionally ignored (signals legitimate-but-absent vs. unknown).
2. `ls -lh .claude/worktree-archives/ | grep -iE "<keyword>"` — check for compressed archives. Large bulk material (>10MB) is typically archived here per ADR 0009 disposal patterns.
3. If found in step 2, run `unzip -l <archive>` to list contents; `unzip -j <archive> "<glob>" -d /tmp/` extracts selected files to a working directory.
4. If no archive match, search by content signature: `grep -rl "<unique-fingerprint>" dev/ | head` (e.g., `RMS_Express_Form` for WLE templates).

Document the archive's purpose and extraction path in the issue/plan that references it (e.g., "WLE templates in `.claude/worktree-archives/RMS-personal-install-20260518T073146Z.zip` — extract to `dev/winlink-reference/`").

**The Lesson:** Archived reference material is structurally invisible to `find`/`ls`. When an expected gitignored path doesn't resolve via standard filesystem search, expand the discovery surface to compressed archives before deferring the work. The operator's first hint will almost always point to `.claude/worktree-archives/`. Build archive enumeration into reference-material discovery workflows.

---

### SCHEMA-1: SCHEMA_VERSION bump triggers operator-driven rebuild, not silent ALTER TABLE

**The Flaw:** When extending a backed-by-SQLite subsystem (e.g., `search/index.rs` with an FTS5 virtual table) with a new column on an existing table, the temptation is to ALTER TABLE in-place during `Index::open` for forward compatibility. This silently mutates the operator's local index file, can hit SQLite ALTER limitations (FTS5 virtual tables do not support `ALTER TABLE`), and risks half-migrated state if the process is killed mid-ALTER. The code path breaks the `Index::open` contract: detect drift, return `SchemaDrift` error, caller (the operator) decides whether to rebuild.

**Why It Matters:** The index file is derived state; the mbox source (or whichever log the indexer walks) is authoritative. In-place ALTER treats the index as source of truth, bypassing the safety property that allows the operator to rebuild from source at will. If the ALTER partially fails (process killed, disk full, SQLite SQLITE_LOCKED), the index lands in a corrupted half-migrated state with no way to recover except manual SQL repair or forensic recovery. More broadly, mutations to derived state without invalidation signals invite data-loss bugs: future code reads an index at version N, assuming schema N, but the operator's copy is at version N+1 due to a stale in-app migration.

**The Fix:** When a schema change is needed:

1. Bump `SCHEMA_VERSION` constant in the subsystem's code (e.g., `search/index.rs`).
2. Update the `init_schema` DDL to include the new column from scratch.
3. Update `upsert` / `query` / query-result types to read/write the new column.
4. Update the schema-drift-detection test: the `open_detects_schema_drift` test should expect the new version and assert that opening an old-version index triggers `SchemaDrift`.
5. Add a round-trip regression test: new column goes through `upsert` → `query` → assertion.
6. Document the migration in the `SCHEMA_VERSION` doc-comment (which version introduced what; whether drift-detect is sufficient or special handling is needed).
7. **DO NOT add an ALTER TABLE path.** The operator runs the rebuild from the UI or CLI; `rebuild_index` deletes the index file and re-walks the source (mbox, log, etc.).

When an operator with an existing index at the prior version launches the new code, `Index::open` detects drift, returns `SchemaDrift`, and the operator is prompted to rebuild. Rebuild deletes the old index, walks the source fresh, and creates a new index with the new column populated.

**The Lesson:** Index files are derived state, not source of truth. Migrations exploit this asymmetry — delete the derived state, rebuild from source. In-place ALTER patterns belong in systems where the table IS the source of truth and must survive in-place. Not here. The design property ("drift detection + rebuild") is intentional, not accidental; don't trade it away for convenience.

---

### BD-1: bd opinionated-tooling overrides

**The Flaw:** `bd` (Beads) installs a CLAUDE.md block on `bd setup claude` that prescribes operational rules ("do NOT use TodoWrite," "do NOT use MEMORY.md files," "Work is NOT complete until `git push` succeeds — YOU must push"). Originally three of these conflicted with tuxlink-wide commitments; as of 2026-05-17 only the first two still do. The push-timing directive now agrees with project policy ([§Session Completion](../../CLAUDE.md#session-completion)) and is no longer overridden. TodoWrite remains the canonical in-turn working-memory primitive; the auto-memory dir remains harness-native and pre-seeded.

The override mechanism is documented in CLAUDE.md's `## Tool referee` section + [ADR 0006](../adr/0006-override-bd-claude-md-defaults.md). The drift hazard is that future agents may read bd's directives without noticing the override, OR a `bd setup claude` re-run may regenerate the BEADS INTEGRATION block in ways that affect assumptions.

**Why It Matters:** bd's framing assumes a greenfield where bd is the sole tool. tuxlink isn't greenfield. Following bd literally on the still-overridden directives produces (a) issue spam from micro-todos that should be TodoWrite, (b) loss of the auto-memory dir's automatic context injection. Neither is catastrophic individually; collectively they erode the project's deliberate tool-referee design.

**Signature.** Recognize the drift via one or more of:

1. `bd setup claude` reports a hash mismatch on the BEADS INTEGRATION block, or silently regenerates it, OR a fresh agent runs `bd setup claude` reflexively.
2. Recent session transcripts show `bd create` calls for micro-todos that should have been TodoWrite (e.g., "read file X," "run cargo test").
3. The auto-memory dir at `~/.claude/projects/<slug>/memory/` stops being read or written by recent sessions while `bd remember` storage grows.
4. ~~A session auto-pushes to origin without operator confirmation, citing bd's mandatory-push directive.~~ — **Superseded 2026-05-17.** Push-at-session-end is now expected behavior, not a drift signature. See ADR 0006 Watched-Failure-Modes entry #4 (also superseded).
5. `bd` version bump (1.x → 2.x) adds new directives in the BEADS INTEGRATION block that aren't yet covered by the override list.

**Fix.**

1. Read [docs/adr/0006-override-bd-claude-md-defaults.md](../adr/0006-override-bd-claude-md-defaults.md) first. It records the original decision and the alternatives considered.
2. Verify the `## Tool referee` section is intact in CLAUDE.md — restore from git history if missing (`git log -p CLAUDE.md` to find the override-introducing commit).
3. If a new bd directive (from a version bump) conflicts with an existing commitment: extend the `## Tool referee` table AND ADR 0006's override list. Do NOT silently soften an override; record the new conflict explicitly.
4. Verify `AGENTS.md` still has its summary pointer to the `## Tool referee` section. Restore if missing.
5. If `bd setup claude` regenerated the BEADS INTEGRATION block, re-read the contents to check if any new opinionated directives have appeared since the last review. Add to the override list as needed.

**Lesson.** The general pattern: any third-party tool that writes to load-bearing project files (CLAUDE.md, AGENTS.md, settings.json) is a potential source of opinionated drift. The defense is a single explicit *referee* section with override authority — not a per-conflict patch that has to be remembered. Future tools that install similar opinionated blocks (a hypothetical "linter-X integration that says NEVER use editorconfig" or "framework-Y wants tabs not spaces") get the same treatment: extend the referee table, write a brief ADR, document the drift signature in this section.

---

### Tool-Integration Review Checklist

- [ ] **Check derived from TEST-1** — Filesystem-scan tests in Vite frontends use `import.meta.glob` with `eager: true`, not Node `fs` module. Verify by `grep -rn "readFileSync\|readdirSync" src-tauri/src-frontend/` — hits on those imports indicate the anti-pattern. Verify tests pass both vitest and `tsc --noEmit`.
- [ ] **Check derived from DISCOVERY-1** — When a task references a gitignored path (e.g., `dev/winlink-reference/`), check `.claude/worktree-archives/` is part of the discovery workflow. If an agent reports "file not found" and the file later surfaces in an archive, verify the plan/issue now documents the archive's location and extraction path.
- [ ] **Check derived from SCHEMA-1** — SQLite-backed subsystems (search index, etc.) bump `SCHEMA_VERSION` on schema changes; `Index::open` detects drift and returns `SchemaDrift`; no in-place `ALTER TABLE` paths exist in the migration logic. Verify by checking `src-tauri/src/search/index.rs` (or equivalent) for `SCHEMA_VERSION` bumps paired with `init_schema` updates and absence of `ALTER TABLE` in `open()`.
- [ ] **`## Tool referee` section intact in CLAUDE.md.** No edits inside `<!-- BEGIN BEADS INTEGRATION -->` markers (those are bd-managed and may be regenerated).
- [ ] **`AGENTS.md` summary pointer present** for the `## Tool referee` section.
- [ ] **ADR 0006's override list matches CLAUDE.md's `## Tool referee` table.** When updating one, update both.
- [ ] **No agent has filed a `bd create` issue for an in-turn micro-todo.** (Spot-check `bd list --status open` for entries that look like "read file X" / "run cargo test Y" — those should have been TodoWrite.)
- [ ] **Auto-memory dir is alive.** `ls ~/.claude/projects/<slug>/memory/` shows recent updates from active sessions.
- [ ] **No agent-initiated push happened in the last session** without operator confirmation. (Check `git log --since="1 day ago"` for unexpected origin updates.)

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

# Section 8: Network and Filesystem Egress Security

> **Reader context:** I'm building or reviewing code that fetches a URL that originates from the operator UI or the webview (e.g., a tile-source base URL, a tile download request), OR I'm building code that uses webview-supplied tile coordinates to build a cache path or a remote fetch URL. Both surfaces are attack entry points for SSRF and path traversal. The two entries below are the canonical defenses for these patterns in tuxlink.
>
> **Design cross-reference:** §8.3 (outbound egress gatekeeper) and §8.4 (tile-coordinate path traversal) of the tuxlink DYOP spec document the accepted design. These pitfall entries are the implementation-review reinforcement of that design.

---

### SSRF-1: Outbound-egress gatekeeper for operator/webview-influenced hosts

**The Flaw:** A backend code path fetches a URL whose host is derived from operator-supplied configuration or a webview-controlled value (tile-source URL, base URL field, etc.) without enforcing egress hygiene at the socket layer. Common deficient forms:

- Config-time string validation only: the URL is validated as syntactically correct or against an allowlist regex at save time, but the actual fetch resolves a DNS name that may have changed since validation — a DNS-rebinding attack replaces the legitimate A record with a loopback or RFC1918 address between validation and fetch.
- Full caller-supplied URLs: the webview (or a malicious tile source response) passes a complete URL that the backend fetches verbatim, including schemes like `file://`, `ftp://`, or `http://192.168.X.X/admin`.
- Redirect following: `reqwest` (or any HTTP client) following redirects allows a redirect chain to eventually land on an internal host even if the originating URL passed all prefix checks.
- URL-embedded credentials: a source URL of the form `http://user:pass@internal.corp/tiles` bypasses hostname checks because the authority component includes credentials, and some libraries resolve the actual host to `internal.corp` while the security check may look only at the "host" field.
- Metadata endpoint access: a fetch to `http://169.254.169.254/latest/meta-data/` (AWS IMDS), `http://100.100.100.200/` (Alibaba Cloud), or similar well-known link-local metadata endpoints exposes cloud credentials to a tile source operator who controls a DNS record that briefly resolves to the metadata IP.

**Why It Matters:** tuxlink fetches tile imagery from user-configured tile sources, including community-managed slippy-map servers. A compromised or malicious tile server can serve redirect responses or DNS-rebind its hostname to an internal address. If the fetch implementation does not enforce egress hygiene at the resolved-IP layer, the tile-fetch path becomes a SSRF (Server-Side Request Forgery) oracle: the operator's machine makes HTTP requests to internal services on behalf of the tile source.

Consequences in the tuxlink context:
1. **LAN service exposure** — a tile source response redirects to `http://192.168.1.1/admin`; the backend follows the redirect and the response body returns to the tile cache (or to an error log visible to the UI).
2. **Loopback service exposure** — a DNS-rebind attack briefly resolves the tile source hostname to `127.0.0.1`, allowing the tile source to reach a locally-bound service (e.g., a local CMS proxy, development server, or admin port).
3. **Metadata endpoint exfiltration** — in cloud-hosted or embedded deployments, a DNS-rebind to `169.254.169.254` leaks instance metadata and credentials.
4. **Regulatory / policy surface** — tuxlink makes outbound HTTP connections on behalf of the operator; if those connections reach infrastructure the operator did not intend (and cannot see), the operator is responsible for the traffic.

The no-added-safeguards posture (per project memory: `feedback_no_tuxlink_added_safeguards.md`) governs **UX**: tuxlink does not block or warn about public hosts as a policy choice. The no-added-safeguards rule does NOT override **egress hygiene at the socket layer** — those are separate concerns. §8.3 of the DYOP spec confirms this distinction.

**The Fix:** Every fetch from an operator/webview-influenced host MUST enforce ALL of the following at the socket layer (not at config-parse time):

1. **Fetch-time resolved-IP gating.** Resolve the hostname immediately before opening the TCP connection. For each resolved IP address, apply `ip_is_permitted()` before the connection proceeds. Permitted addresses: RFC 1918 private ranges (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`) and ULA (`fc00::/7`). Refused addresses: loopback (`127.0.0.0/8`, `::1`), link-local (`169.254.0.0/16`, `fe80::/10`), multicast (`224.0.0.0/4`, `ff00::/8`), unspecified (`0.0.0.0`, `::`), and all other publicly-routable addresses (the function is default-deny, not default-allow). The `// SSRF` boundary in `src-tauri/src/tiles/host.rs` (`validate_source_url`, `ip_is_permitted`) is the canonical implementation.
2. **DNS-rebind defense.** Config-time string validation is not a substitute for fetch-time IP gating. DNS records can change between validation and fetch. The fetch-time resolved-IP gate applies regardless of what the hostname looked like at configuration time.
3. **No redirect following.** The HTTP client MUST be constructed with `reqwest::redirect::Policy::none()`. A URL that passes the initial IP check may redirect to an internal address; that redirect MUST be refused rather than followed.
4. **HTTP and HTTPS schemes only.** The scheme is validated before the fetch. `file://`, `ftp://`, `data:`, and any non-HTTP scheme are refused at `validate_source_url` time.
5. **No URL-embedded credentials.** `validate_source_url` rejects any URL whose authority component contains a userinfo (`user:pass@`) segment.
6. **No caller-supplied full URLs for per-tile fetches.** The webview supplies only validated integer parameters (`z`, `x`, `y`; see TRAVERSAL-1 below). The full tile URL is assembled from the stored, validated source base URL plus the integer-derived path segment. The webview does not supply the scheme, host, or path prefix.
7. **IP-literal vs named-host branch.** When the URL's host is already an IP literal, the literal is passed directly through `ip_is_permitted()`. This prevents a `http://192.168.1.1/` source URL from bypassing the DNS-resolution path. The `src-tauri/src/tiles/fetch.rs` `// SSRF (§8.3)` comment marks this branch.

Code anchors: `src-tauri/src/tiles/host.rs` (functions `validate_source_url` and `ip_is_permitted`, the `// SSRF` boundary comment), `src-tauri/src/tiles/fetch.rs` (the `// SSRF (§8.3)` resolved-IP gate, IP-literal vs named-host dispatch, no-redirect client construction, response-size cap, and magic-byte validation).

**The Lesson:** Config-time validation is defeated by DNS rebinding. The egress gate belongs at the socket layer — at fetch time, against the resolved IP, before the TCP connection opens. Two separate invariants must hold simultaneously: (a) the URL structure is valid (no embedded creds, http(s) only, no caller-supplied host for per-tile fetches), and (b) the resolved IP is in a permitted range. Neither check substitutes for the other. Cross-reference: TRAVERSAL-1 covers the filesystem twin of this entry (tile-coordinate path traversal).

---

### TRAVERSAL-1: Tile-coordinate path traversal via webview-supplied `z/x/y`

**The Flaw:** A tile fetch handler receives `{z}`, `{x}`, `{y}` values directly from the webview and constructs a disk cache path or an upstream URL segment using those values without first parsing and bounding them as integers. Deficient patterns include:

- String interpolation into a path: `format!("{cache_dir}/{source}/{z}/{x}/{y}.png")` where `z`, `x`, `y` are raw strings from the webview — a value of `../../../etc/passwd` for `z` walks outside the cache root.
- Unbounded integer range: `1u32 << z` without first checking `z <= max_zoom` panics when `z > 31` on 32-bit arithmetic (shift overflow).
- Bounds check in the wrong order: `x < 2^z` checked before `z <= max_zoom` still evaluates `2^z` for a large `z`, causing the same overflow.
- Path assembly via string concatenation instead of `PathBuf::join`: `PathBuf::join` normalizes `..` components; string concatenation does not.
- No canonicalization + `starts_with` guard: the assembled path is written without verifying it still lives under the cache root (a race condition or normalization edge case can escape).
- Leaf symlink creation: the cache file is created at the resolved path without refusing a symlink at the leaf position — an attacker who pre-creates a symlink at the expected cache path can redirect the write to an arbitrary location.

**Why It Matters:** The tile cache is a user-writable directory under the operator's data dir. A tile-source operator who controls the DNS record for a tile server can observe which `z/x/y` combinations the client requests, or can influence the values in ways that reach the cache path builder (e.g., via a redirect to a URL with traversal characters in the path segment). A path traversal bug here writes attacker-controlled bytes to attacker-controlled paths under the operator's account — up to and including overwriting shell init files, SSH authorized_keys, or the tuxlink config.

The upstream URL segment is equally sensitive: a raw `z/x/y` value interpolated directly into the upstream fetch URL can inject path separators or query characters that change the request semantics.

**The Fix:** Parse and bound `z`, `x`, `y` as integers BEFORE any path or URL construction. The full sequence, in order:

1. **Parse as `u32`.** Reject any non-numeric, negative, or non-integer value immediately.
2. **Bound `z` before shifting.** Assert `z <= max_zoom` (typically 22 for OSM-style tiles) BEFORE computing `1u32.checked_shl(z)` (or equivalent). `checked_shl` returns `None` on overflow and must be handled — do not use the unchecked `<<` operator. Code anchor: `src-tauri/src/tiles/coord.rs`, `TileCoord`, `checked_shl`.
3. **Bound `x` and `y`.** Assert `x < 2^z` and `y < 2^z` using the result of step 2 (the checked shift), not a re-evaluated expression. This prevents the range check from silently passing when `z` caused overflow.
4. **Build the path via `PathBuf::join` from validated integers only.** The path segments are `format!("{}", z)`, `format!("{}", x)`, `format!("{}", y)` — plain integer-to-string. No raw webview strings reach `PathBuf::join`. The cache root is an absolute `PathBuf` derived from the operator's config at startup.
5. **Namespace by source URL hash.** The per-source subdirectory under the cache root uses a SHA-256 of the source base URL (hex-encoded prefix). This prevents one source from reading or overwriting another source's cache. Code anchor: `src-tauri/src/tiles/cache.rs`, the `// traversal-safety (§8.4)` section.
6. **Canonicalize parent + `starts_with` assert.** After assembling the full path, canonicalize the parent directory and assert it `starts_with(cache_root)`. This is the backstop for any normalization edge case that slips through steps 2–4.
7. **Refuse leaf symlinks.** Before writing, verify the leaf path does not exist as a symlink (`std::fs::symlink_metadata` + `FileType::is_symlink`). If a symlink exists at the expected cache path, refuse the write rather than following it. Use atomic temp-file + rename (`write to <path>.tmp`, then `fs::rename`); `rename` is atomic and does not follow symlinks on Linux.

For the upstream URL segment: assemble the URL from the stored source base URL plus the integer-formatted `z`, `x`, `y` path segments. Do not percent-encode or otherwise process raw webview strings into the URL — the integers produce only digit characters and path separators.

Code anchors: `src-tauri/src/tiles/coord.rs` (`TileCoord` type, `checked_shl` usage), `src-tauri/src/tiles/cache.rs` (the `// traversal-safety (§8.4)` gate, per-source SHA-256 namespace, atomic temp+rename, leaf-symlink refusal).

**The Lesson:** Webview-supplied coordinates are untrusted user input, not validated API parameters. The parse-then-bound-then-construct sequence is the invariant: parsing separates string representation from semantics; bounding guards against overflow and range exploits; path construction via typed primitives (`PathBuf::join`, integer formatting) eliminates the injection surface entirely. String-interpolated paths are never safe with untrusted input, regardless of how "obviously numeric" the input looks. Cross-reference: SSRF-1 covers the network twin of this entry (outbound-egress gatekeeper for the host the coordinates are fetched from).

---

### Section 8 Review Checklist

- [ ] **Check derived from SSRF-1** — Every code path that fetches a URL whose host originates from operator config or a webview-supplied value resolves the hostname at fetch time (not config time) and calls `ip_is_permitted()` on each resolved address before opening a TCP connection. Verify by reading `src-tauri/src/tiles/fetch.rs` for the `// SSRF (§8.3)` gate and confirming it runs after DNS resolution.
- [ ] **Check derived from SSRF-1** — The HTTP client for tile fetches is constructed with `reqwest::redirect::Policy::none()`. Verify by searching `src-tauri/src/tiles/fetch.rs` for `redirect::Policy` — any value other than `none()` is a finding.
- [ ] **Check derived from SSRF-1** — `validate_source_url` in `src-tauri/src/tiles/host.rs` rejects: (a) non-http(s) schemes, (b) URL-embedded credentials (`user:pass@` in authority), (c) IP literals that fail `ip_is_permitted()`. Verify by reading the function and confirming all three checks are present.
- [ ] **Check derived from SSRF-1** — Per-tile fetch requests assemble the URL from the stored source base URL plus integer-formatted `z/x/y` path segments. The webview does NOT supply the scheme, host, or base path of the tile URL. Verify by tracing the Tauri command that handles tile fetch requests back to the URL assembly site.
- [ ] **Check derived from TRAVERSAL-1** — `z`, `x`, `y` values from the webview are parsed as `u32` and bounded before any path or URL assembly. `z <= max_zoom` is asserted BEFORE `checked_shl(z)` is called; `checked_shl` result is `None`-checked and not `.unwrap()`'d without a bounds guarantee. Verify via `src-tauri/src/tiles/coord.rs`.
- [ ] **Check derived from TRAVERSAL-1** — Cache paths are assembled via `PathBuf::join` from integer-formatted segments only. No raw webview string reaches a `PathBuf::join` or a string-format expression that produces a path. Verify by searching `src-tauri/src/tiles/` for `format!` expressions containing `z`, `x`, or `y` variable names and confirming each is integer-typed.
- [ ] **Check derived from TRAVERSAL-1** — The per-source cache namespace uses a SHA-256 of the source URL (not the URL verbatim), and the assembled path is canonicalized + `starts_with(cache_root)` asserted before any write. Leaf symlinks at the target path cause the write to be refused, not followed. Verify via `src-tauri/src/tiles/cache.rs`.

---

## Section 9 — Frontend rendering on WebKitGTK

The production webview is **WebKitGTK**, not Chromium. Form controls render their
**native GTK theme** unless `appearance: none` is set — so a control can look
correct in a Chromium/Playwright check and wrong in the actual app.

### WEBKIT-1: Form controls need `appearance: none` or WebKitGTK paints native GTK chrome over the CSS

**The trap:** a `<button>` / `<select>` styled only with `background`/`border`
keeps its **native GTK appearance** on WebKitGTK. For a button given an explicit
fill + border this is mostly masked; but a **transparent / borderless** button
(e.g. an underline-style tab) falls back to a gray bordered native box — reading
as a generic "default button." Chromium does **not** render native chrome, so
every Playwright/dev-browser check looks fine while the shipped app is wrong.

**The `<select>` case (real):** native dropdowns render GTK-gray without
`appearance: none` (PR #752 / `tuxlink-ytay`) — set it **with** a custom caret (an
appearance-reset select loses its arrow; see RadioPanel.css / AppShell.css). The
global `button {}` rule also sets `appearance: none; -webkit-appearance: none;
-moz-appearance: none;` (inert for already-styled buttons; defensive for
under-styled ones).

**⚠️ Correction (tuxlink-sdjd) — do NOT assume "default-looking buttons" means a
missing `appearance: none`.** The APRS dock tabs were reported as "Claude
default-style buttons" five times. #760 added `appearance: none` to the global
`button {}` and claimed the fix — but a real-bundle WebKit2GTK **computed-style
inspection** (not a screenshot, not a hand-built mock) showed the tabs **already**
resolved to `appearance: none` + `border: none` + transparent/surface background.
Before/after that change rendered **identically**. The actual cause was the global
`input,textarea,select,button { border-radius: 6px }` applying to the tabs, so the
active tab's fill + the Map toggle read as **rounded buttons**. Real fix:
`border-radius: 0` on `.aprs-dock-tab / .aprs-dock-maptoggle / .aprs-dock-close`.
**Lesson:** when a UI element "looks like a default button," dump
`getComputedStyle()` for `appearance`, `border`, `background`, `border-radius` from
the **real production bundle** rendered in WebKitGTK before deciding which property
is the culprit — and never reason from an isolated mock's cascade (it omits the
global rules that actually decide the look).

**Verify on the real engine, not Chromium:** WebKit2GTK 4.1 is available via
python-gi. Render the suspect markup + CSS headless and snapshot:
`LIBGL_ALWAYS_SOFTWARE=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 GALLIUM_DRIVER=llvmpipe python3 <gi-WebKit2-snapshot-script>`
(a `Gtk.OffscreenWindow` + `WebKit2.WebView.get_snapshot(FULL_DOCUMENT)` →
`write_to_png`). A Playwright/Chromium screenshot is **not** a proxy for
WebKitGTK control rendering.

### Section 9 Review Checklist

- [ ] **Check derived from WEBKIT-1** — Any new `<button>`/`<select>` that is transparent or relies on CSS to remove its box sets `appearance: none` (global `button {}` already does for buttons; selects need it + a custom caret). Verify a control-rendering claim with a WebKit2GTK snapshot, never a Chromium one.

---

# Appendix A: Historical Changelog

<!-- Format: -->
<!-- ## YYYY-MM-DD — <event> -->
<!-- - Added PREFIX-N (<title>) — <what and why> -->
<!-- - Updated PREFIX-M — <what changed> -->

## 2026-06-09 — Added SSRF-1, TRAVERSAL-1 (Tile-fetch egress and path-traversal defenses)

Source: tuxlink-dyop DYOP-LAN-tiles plan Phase 10.1 (agent `marten-poplar-dahlia`). Two findings derived from the §8.3/§8.4 security design in the DYOP spec:

1. **SSRF-1** (Outbound-egress gatekeeper): The tile-fetch path fetches URLs whose host is operator-configured. Without fetch-time resolved-IP gating, DNS rebinding or redirect chains reach loopback, RFC1918, link-local, or cloud-metadata addresses. Config-time validation is defeated by DNS rebinding. The fix is enforced at the socket layer: `ip_is_permitted()` after DNS resolution, `reqwest::redirect::Policy::none()`, no caller-supplied full URLs for per-tile fetches, no URL-embedded credentials, http(s)-only. The `no-added-safeguards` posture governs UX; it does not govern socket-layer egress hygiene — these are separate concerns (§8.3 distinguishes them explicitly).
2. **TRAVERSAL-1** (Tile-coordinate path traversal): Webview-supplied `z/x/y` reach both the disk cache path builder and the upstream URL segment. Without parse-then-bound-then-construct discipline, string-interpolated paths walk outside the cache root and unbounded shifts panic. The fix enforces `z <= max_zoom` before `checked_shl(z)`, `PathBuf::join` from integer-formatted segments only, per-source SHA-256 namespace, canonicalize + `starts_with(cache_root)` assert, and leaf-symlink refusal.

Both entries are marked `UNIMPLEMENTED` pending the tiles module implementation in a later plan phase. Section 8 added to the TOC.

Companion artifacts:
- Plan: tuxlink-dyop DYOP spec §8.3 (SSRF) + §8.4 (path traversal)
- Code anchors (planned): `src-tauri/src/tiles/host.rs`, `src-tauri/src/tiles/fetch.rs`, `src-tauri/src/tiles/coord.rs`, `src-tauri/src/tiles/cache.rs`

## 2026-05-17 — Added RADIO-2 (Encryption decisions on RF require operator approval)

Source: client-landscape audit during the v0.0.1 UX brainstorm (bd issue `tuxlink-x5p`, agent `plover-pine-finch`). Two findings combined into one pitfall:

1. **Pat's wiki overcautious framing** of HTTPS-on-amateur-radio led to the realization that amateur-radio software culture broadly conflates "no encryption on Part 97 RF" with "no encryption anywhere in amateur-radio software." This pitfall codifies the distinction — telnet to CMS is Part 15 internet, not Part 97 RF; encryption-in-transit is legal there and CMS-SSL on port 8773 is available.
2. **Cameron's firsthand audit of Winlink Express** revealed Express auto-selects CMS-SSL but hides this from the operator (session-type dropdown only says "Telnet", settings only show port 8772). The operator — the license holder — has zero visibility into actual transport. This drove a corresponding entry in `docs/ux-anti-patterns.md` ("NEVER hide security-relevant transport choices from the operator") and the RADIO-2 fix step about preferring CMS-SSL with explicit operator visibility.

Companion artifacts:
- Feedback memory: `~/.claude/projects/-home-administrator-Code-tuxlink/memory/feedback_encryption_part97_eval.md`
- Anti-pattern entry: `docs/ux-anti-patterns.md` §"Anti-Patterns Observed in Winlink Express" (hide-transport bullet)
- Principle 7 in `docs/design/v0.0.1-ux-principles.md` (companion privacy-via-precision-reduction)

## 2026-05-18 — Added PARITY-1 (Script/Hook Path-Resolution Parity)

Source: bd issue `tuxlink-arv` (`get_tuxlink_sessions.py` ↔ `block-main-checkout-race.sh` lease-dir disagreement; script read `<repo>/.claude/session-leases/`, hook wrote `<git-common-dir>/session-leases/`). Diagnosed during the 2026-05-18 main-checkout-race incident chain; structural enabler was a CLAUDE.md carve-out removed in PR #39; the script-fix (PR #44) + this pitfall close the loop. Section 2 title extended from "Safety-Stack Coordination" to "Safety-Stack Coordination and Cross-Component Parity" to reflect PARITY-1's code-structure (rather than purely agent-behavior) focus.

Companion artifacts:
- Spec: `docs/superpowers/specs/2026-05-18-tuxlink-arv-lease-dir-parity-design.md`
- Plan: `docs/superpowers/plans/2026-05-18-tuxlink-arv-lease-dir-fix.md`
- Auto-memory refresh: `~/.claude/projects/-home-administrator-Code-tuxlink/memory/feedback_stale_lease_means_worktree.md` (updated to reflect script accuracy + reinforce worktree authority)

## 2026-05-31 — Added TEST-1, DISCOVERY-1, SCHEMA-1 (HTML Forms v0.1 session lessons)

Source: HTML Forms v0.1 PR #177 and related work (v0.0.1 HTML Forms task cluster). Three findings from the session:

1. **TEST-1** (Filesystem-scan tests in Vite): Task T10.1 of the HTML Forms plan attempted to enforce a `dangerouslySetInnerHTML` ban across the forms subsystem using Node `fs` APIs; test passed vitest but failed tsc, creating shadow CI. Discovered during implementation that Vite's `import.meta.glob` is the correct pattern.
2. **DISCOVERY-1** (Reference material in archives): Phase 9 of the HTML Forms session — agent searched for WLE Standard Templates referenced in the plan, couldn't find them on disk, didn't check `.claude/worktree-archives/` where the operator had archived `RMS-personal-install-20260518T073146Z.zip` (74 MB). Task stalled until operator pointed to the archive.
3. **SCHEMA-1** (Schema versioning without ALTER TABLE): Related to tuxlink-g4dj fix (PR #178) — search index migrations need to bump SCHEMA_VERSION and delegate rebuild to the operator, not silently ALTER TABLE in-place. Codifies the design property: index files are derived state, not source of truth.

Section title "Tool Integration" expanded to cover implementation patterns beyond just third-party tool conflicts.

Companion artifacts:
- Task: PR #177 (HTML Forms v0.1)
- Related: PR #178 (search index schema)
- Plan: `docs/superpowers/plans/2026-05-DD-html-forms-v0.1.md` (archived; lessons extracted to this doc)

---

# Appendix B: Unified Summary Table

<!-- TODO: One row per pitfall for at-a-glance review. Keep in sync with the sections above. -->

| ID | Title | Severity | Status | Domain |
|----|-------|----------|--------|--------|
| RADIO-1 | Agent-autonomous transmission under the licensee's callsign | CRITICAL | VALIDATED | §0 Live Radio Network Operations |
| RADIO-2 | Encryption decisions on RF require operator approval | HIGH | VALIDATED | §0 Live Radio Network Operations |
| SCOPE-1 | Conflating RMS Express (client) with RMS Trimode (gateway) | HIGH | VALIDATED | §1 Scope and Audience Boundaries |
| HOOK-1 | Arguing with `block-main-checkout-race.sh` instead of routing to a worktree | HIGH | VALIDATED | §2 Safety-Stack Coordination and Cross-Component Parity |
| LEASE-1 | Adding additional "session liveness" signals beyond the lease | HIGH | VALIDATED | §2 Safety-Stack Coordination and Cross-Component Parity |
| PARITY-1 | Script/Hook Path-Resolution Parity | HIGH | VALIDATED | §2 Safety-Stack Coordination and Cross-Component Parity |
| DRIFT-1 | Plan-text amendments don't auto-cascade to code | HIGH | VALIDATED | §3 Plan and Documentation Discipline |
| TEST-1 | Filesystem-scan tests in Vite must use import.meta.glob, not Node fs | HIGH | VALIDATED | Tool Integration |
| DISCOVERY-1 | Gitignored references live in `.claude/worktree-archives/`, not just disk | HIGH | VALIDATED | Tool Integration |
| SCHEMA-1 | SCHEMA_VERSION bump triggers operator-driven rebuild, not silent ALTER TABLE | HIGH | VALIDATED | Tool Integration |
| ORCH-1 | Analysis Dispatches Must Persist Findings | HIGH | VALIDATED | Orchestration |
| BD-1 | bd opinionated-tooling overrides | MEDIUM | VALIDATED | Tool Integration |
| SSRF-1 | Outbound-egress gatekeeper for operator/webview-influenced hosts | CRITICAL | UNIMPLEMENTED | §8 Network and Filesystem Egress Security |
| TRAVERSAL-1 | Tile-coordinate path traversal via webview-supplied `z/x/y` | CRITICAL | UNIMPLEMENTED | §8 Network and Filesystem Egress Security |

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
