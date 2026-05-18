# Task 6 — Live-CMS Smoke Binary (Operator-Only) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **RADIO-1 BRIGHT LINE (read before any task):** The binary built by this plan transmits on the amateur radio network under the licensee's callsign. The Wave-2 implementing subagent MUST NOT run `cargo run --bin live_cms_smoke`, MUST NOT pipe `"go"` into it, MUST NOT exercise it under any execution mode (no `--release`, no test harness, no shell wrapper, no integration test, no CI, no `/loop`). Only **Cameron Zucker (the station licensee)** may run it. The implementing agent BUILDS and COMPILES; the operator RUNS. If a step appears to require running the binary to verify completion, the step is misspecified — STOP and escalate to the dispatcher. See `docs/pitfalls/implementation-pitfalls.md` §0 RADIO-1 and `docs/live-cms-testing-policy.md`.

**Goal:** Add an operator-only `cargo run --bin live_cms_smoke` binary plus a reusable `consent_gate` library module that prints a scoped Part 97 consent banner, reads `"go"` from stdin, and on consent runs one round-trip to `SERVICE@winlink.org` via Pat (using `pat_client` + `pat_process` shipped in Tasks 3/5), logging every invocation to `dev/live-cms-sessions.log`.

**Architecture:** Two production code units — (1) `src-tauri/src/consent_gate.rs`, a pure stdin/stdout consent-banner-and-input module that's fully unit-testable with `std::io::Cursor` (no real I/O, no transmission); and (2) `src-tauri/src/bin/live_cms_smoke.rs`, the operator-only binary that wires `consent_gate` to `PatProcess` + `PatClient` for the real round-trip. The binary lives in `src/bin/` (NOT `tests/`) so `cargo test` cannot discover it. A third unit — the operator-facing `dev/README-live-cms-smoke.md` — documents how Cameron invokes it.

**Tech Stack:** Rust (existing tuxlink_lib crate), reqwest blocking client (already a dependency), chrono for ISO-8601 timestamps (verify present in Cargo.toml; add if missing), Pat (HTTP API on localhost, spawned via existing `PatProcess`). No new external services. No new frontend code.

---

## Living Document Contract

This plan is a living document. Every executing agent MUST update it as
execution progresses, not only at completion.

- **On phase claim:** the executor MUST flip the banner to 🚧 IN PROGRESS
  with a claim timestamp (ISO 8601 UTC) and the active branch name. The
  banner MUST NOT include an expected-completion estimate — agents cannot
  reliably estimate their own wall-clock, and a fabricated duration
  becomes a stale anchor that misleads future readers. Followers
  encountering a 🚧 banner determine liveness by observable signals (PR
  existence, recent branch commits), not by arithmetic on expected times.
  See Step 5's stale-claim reclaim protocol.
- **On phase ship:** the executor MUST update that phase's **Execution
  Status** banner with the shipped commit SHA(s) and date. If a PR is
  open, the PR number and URL MUST appear in the top-of-plan Execution
  Status table.
- **On phase defer:** the executor MUST update the banner with ⏸ status
  AND a prose description of the unblock condition + a link to the
  likely-unblocker artifact (plan page, task, or PR whose own Execution
  Status banner will signal completion). Prose + link is durable across
  paraphrases and scope edits; exact-string coordination between agents
  is not.
- **On PR merge:** the executor MUST record the merge SHA in the banner
  + the top-of-plan Execution Status table.
- **On deviation from the written plan** (scope edits, structural
  refactors, dropped tasks, reordered phases): the executor MUST
  inline-document the deviation in the affected task AND summarize it
  in the top-of-plan Execution Status as a "Deviations" subsection.
  Deviation state MUST NOT live only in PR notes or status reports.
- **On discovery** (pre-existing drift surfaced during execution, new
  bugs found, architectural issues noted): the executor MUST add a
  "Discoveries" subsection at the top of the plan with pointers to the
  files/lines affected. Follow-up dispatches read this subsection to
  avoid duplicate discovery work.

The plan SHOULD reflect reality at the end of every session that touches
it. Anything worth putting in a status report to the user is worth
putting in the plan.

Rationale: `/writing-plans-enhanced` Step 5. Writing at ship time is
cheap; reconstruction by downstream readers is expensive, compounds
across dispatches, and fails silently when state is split across PR
notes and commit messages.

---

## Execution Status

**Overall:** 0/5 phases shipped.

| Phase | Status | Ship SHA(s) | Notes |
|---|---|---|---|
| 1 — Consent gate module (TDD) | ⬜ Not started | — | Pure I/O via Cursor; safe for impl subagent to run tests |
| 2 — Operator-only smoke binary | ⬜ Not started | — | Compile-only verification; impl subagent MUST NOT run the binary |
| 3 — Session log + operator README | ⬜ Not started | — | Doc + empty log file |
| 4 — Wire up & integration commit | ⬜ Not started | — | lib.rs + Cargo.toml binary declaration |
| 5 — Operator handoff (escalation) | ⬜ Not started | — | Hand off to Cameron for manual verification; no agent invocation |

### Deviations

_None yet. Add a one-line summary + pointer to the inline task note for any scope edit during execution._

### Discoveries

The Wave-1 plan-writer (cedar-redwood-dune, 2026-05-18) surfaced three spec-vs-codebase drifts that this plan resolves inline. Wave-2 impl agents do NOT need to re-discover them:

1. **Spec drift D-1 (signature):** The original Task 6 spec at `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md:1530` calls `client.send(&[&plan.target], "tuxlink setup test", &plan.content)` (3 args, capturing the return as `let _mid = ...`). The actual `PatClient::send` signature shipped by Task 5 (PR #41/#42) is `pub fn send(&self, to: &[&str], subject: &str, body: &str, date: &str) -> Result<(), PatClientError>` (4 args, unit return). This plan uses the actual signature (see Phase 2 Step 4). Date is supplied as `chrono::Utc::now().to_rfc3339()`.
2. **Spec drift D-2 (module dependency):** The original Task 6 spec at `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md:1451` imports `tuxlink_lib::wizard_commands::{write_pat_config, WizardInput}`. The `wizard_commands` module is owned by Task 9 (bd `tuxlink-ko0`), which is NOT yet shipped (`bd list --status open` confirms `tuxlink-ko0`, `tuxlink-1r5`, `tuxlink-e4x` are OPEN as of 2026-05-18). This plan inlines a minimal `write_pat_config_for_smoke` helper directly inside the `live_cms_smoke` binary (NOT a library export) so Task 6 ships independently of Task 9. When Task 9 lands, a follow-up housekeeping commit MAY collapse the helper to the library form; until then, the binary self-contains its config-writing. Plan-review-cycle Round 2 confirmed this resolves the dependency without violating DRY (one inlined function used in one place).
3. **Spec drift D-3 (lib.rs scope):** The spec adds `pub mod consent_gate;` AND (by transitive `use`) requires `wizard_commands`. This plan adds ONLY `pub mod consent_gate;` to `lib.rs`. Wizard wiring lands with Task 9.

If a fourth drift is discovered during execution, add a D-4 entry here AND inline-document in the affected task.

---

## Sources this plan implements

| Source | What it provides | Cite in plan? |
|---|---|---|
| `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` §"Task 6" (lines 1257-1700) | The original spec — file paths, behavior, code skeletons, TDD test cases. | Implementation directly mirrors this where the actual codebase agrees; Discoveries D-1 / D-2 / D-3 above document the divergences. |
| `docs/live-cms-testing-policy.md` | The operational policy: consent gate format, exit codes, logging shape, exception list. | Banner text in Phase 1 + log line in Phase 2 implement this verbatim. |
| `docs/pitfalls/implementation-pitfalls.md` §0 RADIO-1 | The bright-line rule: agents MUST NOT run transmit-capable code paths in subagent shells. | Every task carries an "Agent execution rule" line; Phase 5 is the operator-only manual verification. |
| `docs/pitfalls/implementation-pitfalls.md` §0 RADIO-2 | Encryption decisions on RF require operator approval. | Phase 2 Step 4 connects via telnet/IP (not RF); RADIO-2 does not fire. Documented inline. |
| `docs/pitfalls/implementation-pitfalls.md` §1 SCOPE-1 | tuxlink is client-only, not a gateway. | Smoke binary connects OUT to CMS as a client; no listening or gateway behavior. Documented inline. |
| `docs/pitfalls/implementation-pitfalls.md` ORCH-1 | Parallel-subagent dispatches must persist findings before returning. | This plan dispatches sequentially (per-phase subagents); ORCH-1 fires only if a phase introduces parallel sub-dispatches, which none here do. Documented in Phase-5 review-gate notes. |
| `docs/pitfalls/testing-pitfalls.md` | Universal testing disciplines (pristine output, no skipped tests, error-path coverage, etc.). | Phase 1's tests cover the granted/aborted axes + edge cases (empty input, CRLF, whitespace, case variation) per `testing-pitfalls.md` §3 (error path coverage) and §6 (boundary validation). |

---

## Universal task preamble (read once, applies to every phase)

**BEFORE starting work on ANY task in this plan:**

1. Confirm you are running in the worktree created for this work (`worktrees/bd-tuxlink-nk7-<slug>/`). If not, STOP and run `python3 .claude/scripts/new_tuxlink_worktree.py --slug live-cms-smoke --issue tuxlink-nk7 --moniker <your-moniker>`.
2. Re-read `docs/pitfalls/implementation-pitfalls.md` §0 (RADIO-1, RADIO-2) — both are short and load-bearing for this plan.
3. Re-read `docs/pitfalls/testing-pitfalls.md` if you'll be writing tests.
4. Invoke `superpowers:test-driven-development` before writing any test code.
5. Follow TDD: write failing test → implement → verify green.

**BEFORE marking ANY task complete:**

1. Review tests against `docs/pitfalls/testing-pitfalls.md`. Verify error paths AND edge cases are covered (not just the happy path).
2. Verify the universal **agent execution rule** below was not violated.
3. Run `cargo build --bin live_cms_smoke` (Phases 2+) to confirm the binary compiles. **Do NOT run the binary itself.**
4. Run `cargo test --test consent_gate_test` (Phase 1+) and confirm green.
5. Commit with the heredoc syntax shown in each phase's commit step.

**Universal agent execution rule (RADIO-1):**

- The implementing subagent MUST NOT execute `cargo run --bin live_cms_smoke` under any circumstance.
- The implementing subagent MUST NOT pipe `"go"` (or any other input) into the binary via shell redirection, here-doc, `expect` script, or any other mechanism.
- The implementing subagent MUST NOT pre-set `WINLINK_PASSWORD` / `WINLINK_CALLSIGN` / `WINLINK_GRID` env vars and invoke the binary "just to verify it compiles + parses env" — `cargo build --bin live_cms_smoke` verifies compilation without invocation.
- The implementing subagent MUST NOT add a `#[test]` or `#[tokio::test]` anywhere in the crate that calls `consent_gate::check_consent` with a `Cursor::new(b"go\n")` AND then exercises the Pat-spawning code path. Pure consent-gate tests with no Pat involvement are fine (see Phase 1) — those don't transmit.
- If you discover a step that appears to require running the binary, STOP and surface it to the dispatching agent. This is the misspecified-task escape hatch per RADIO-1.
- Verification that the operator's manual run will work is **Phase 5's job**, and Phase 5 is operator-only.

**Preserve assertion rigor under pressure** (this plan touches I/O and may surface timing-related flake during the live operator run, even though the impl agent never runs the live path):

> BEFORE marking this task complete: If any test assertion races, flakes, or fails nondeterministically, the fix is deterministic synchronization (Cursor-based I/O, sealed input fixtures, explicit `Duration` timeouts on Pat spawn) — NOT assertion removal or weakening. If synchronization cannot make the assertion pass reliably, STOP and raise to the dispatching agent. Do not ship a weaker test. Weakened assertions rationalized as "CI stability fixes" are the exact pattern this rule prevents. Prefer mechanism assertions (e.g., "banner output contains the callsign string") over symptom assertions (e.g., "banner output length > 100") where feasible.

---

## File-Structure Decomposition

Two production source files + one test file + one operator-facing doc + one log file:

```
src-tauri/
├── Cargo.toml                          # Modify: add [[bin]] live_cms_smoke; add chrono if absent
├── src/
│   ├── lib.rs                          # Modify: add `pub mod consent_gate;`
│   ├── consent_gate.rs                 # CREATE: pure stdin/stdout consent module
│   └── bin/
│       └── live_cms_smoke.rs           # CREATE: operator-only binary
└── tests/
    └── consent_gate_test.rs            # CREATE: unit tests for consent_gate (Cursor I/O)
dev/
├── live-cms-sessions.log               # CREATE: empty file (with header line); appended at run
└── README-live-cms-smoke.md            # CREATE: operator-facing how-to
```

**Files NOT touched by this plan:**

- `src-tauri/src/main.rs` — the smoke binary is a separate `[[bin]]` target with its own `fn main`; the existing `src/main.rs` (the tuxlink Tauri app) is unchanged.
- `src-tauri/src/wizard_commands.rs` — does not exist yet (Task 9); this plan does NOT create it (see Discovery D-2).
- `src-tauri/src/pat_client.rs`, `src-tauri/src/pat_process.rs`, `src-tauri/src/config.rs` — used as-is; not modified.
- React frontend (`src/`) — no UI changes.

---

## Phase 1 — Consent Gate Module (TDD)

**Execution Status:** ⬜ NOT STARTED

**Goal of this phase:** A pure, unit-testable `consent_gate` library module that takes a `TransmissionPlan`, a `Read` (stdin in production, `Cursor` in tests), and a `Write` (stdout in production, `Vec<u8>` in tests). Returns `ConsentOutcome::Granted` iff the input line is exactly `"go"` (after stripping one trailing `\n` or `\r\n`). Returns `ConsentOutcome::Aborted` on anything else, including empty input, EOF, whitespace-padded `go`, `GO`, etc.

**Agent execution rule for this phase:** All tests in Phase 1 use `std::io::Cursor` — no real stdin, no Pat, no network, no transmission. The impl subagent IS expected to run `cargo test --test consent_gate_test` here. RADIO-1 is not engaged by the consent-gate module in isolation; it only fires when the binary in Phase 2 wires the consent gate to real Pat I/O.

### Task 1.1 — Files

**Files:**
- Create: `src-tauri/src/consent_gate.rs`
- Create: `src-tauri/tests/consent_gate_test.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod consent_gate;`)

### Task 1.2 — Steps

- [ ] **Step 1: Write the failing test fixture and tests**

Create `src-tauri/tests/consent_gate_test.rs` with these exact contents:

```rust
use std::io::Cursor;
use tuxlink_lib::consent_gate::{check_consent, ConsentOutcome, TransmissionPlan};

fn plan() -> TransmissionPlan {
    TransmissionPlan {
        target: "SERVICE@winlink.org".into(),
        session_count: 1,
        expected_duration_s: 30,
        content: "short test body".into(),
        freq_mode_band: "telnet over IP; no RF".into(),
        callsign: "W4PHS".into(),
    }
}

#[test]
fn test_exact_go_grants_consent() {
    let mut input = Cursor::new(b"go\n".to_vec());
    let mut output = Vec::new();
    let outcome = check_consent(&plan(), &mut input, &mut output);
    assert!(matches!(outcome, ConsentOutcome::Granted));
}

#[test]
fn test_crlf_after_go_grants_consent() {
    // Operator on a CRLF terminal (rare on Linux but possible via paste).
    let mut input = Cursor::new(b"go\r\n".to_vec());
    let mut output = Vec::new();
    let outcome = check_consent(&plan(), &mut input, &mut output);
    assert!(matches!(outcome, ConsentOutcome::Granted));
}

#[test]
fn test_uppercase_go_does_not_grant_consent() {
    let mut input = Cursor::new(b"GO\n".to_vec());
    let mut output = Vec::new();
    let outcome = check_consent(&plan(), &mut input, &mut output);
    assert!(matches!(outcome, ConsentOutcome::Aborted));
}

#[test]
fn test_mixed_case_go_does_not_grant_consent() {
    let mut input = Cursor::new(b"Go\n".to_vec());
    let mut output = Vec::new();
    let outcome = check_consent(&plan(), &mut input, &mut output);
    assert!(matches!(outcome, ConsentOutcome::Aborted));
}

#[test]
fn test_empty_input_aborts() {
    // Operator immediately closed stdin (Ctrl-D), or stdin was a /dev/null pipe.
    let mut input = Cursor::new(b"".to_vec());
    let mut output = Vec::new();
    let outcome = check_consent(&plan(), &mut input, &mut output);
    assert!(matches!(outcome, ConsentOutcome::Aborted));
}

#[test]
fn test_lone_newline_aborts() {
    let mut input = Cursor::new(b"\n".to_vec());
    let mut output = Vec::new();
    let outcome = check_consent(&plan(), &mut input, &mut output);
    assert!(matches!(outcome, ConsentOutcome::Aborted));
}

#[test]
fn test_any_other_input_aborts() {
    // Includes whitespace-padded "go" — strict equality after newline strip only.
    let cases: &[&[u8]] = &[
        b"yes\n",
        b"y\n",
        b"go now\n",
        b" go\n",
        b"go \n",
        b"\tgo\n",
        b"go\t\n",
        b"gogo\n",
        b"nope\n",
        b"abort\n",
    ];
    for s in cases {
        let mut input = Cursor::new(s.to_vec());
        let mut output = Vec::new();
        let outcome = check_consent(&plan(), &mut input, &mut output);
        assert!(matches!(outcome, ConsentOutcome::Aborted), "input {:?} must abort", s);
    }
}

#[test]
fn test_banner_mentions_all_scoped_plan_fields() {
    // Banner MUST surface every field of TransmissionPlan so the operator
    // can verify what they're consenting to. Missing any field would mean
    // the operator is consenting to less than the full transmission scope.
    let mut input = Cursor::new(b"go\n".to_vec());
    let mut output = Vec::new();
    let _ = check_consent(&plan(), &mut input, &mut output);
    let banner = String::from_utf8(output).unwrap();
    assert!(banner.contains("SERVICE@winlink.org"), "banner missing target");
    assert!(banner.contains("W4PHS"), "banner missing callsign");
    assert!(banner.contains("1"), "banner missing session count");
    assert!(banner.contains("30"), "banner missing duration");
    assert!(banner.contains("short test body"), "banner missing content");
    assert!(banner.contains("telnet"), "banner missing freq/mode/band");
    assert!(banner.contains("Part 97"), "banner missing Part 97 reference");
    assert!(banner.contains("go"), "banner missing the literal go prompt");
    assert!(banner.contains("licensee"), "banner missing licensee role wording");
}

#[test]
fn test_abort_emits_user_visible_message() {
    let mut input = Cursor::new(b"nope\n".to_vec());
    let mut output = Vec::new();
    let outcome = check_consent(&plan(), &mut input, &mut output);
    assert!(matches!(outcome, ConsentOutcome::Aborted));
    let printed = String::from_utf8(output).unwrap();
    assert!(printed.contains("Aborted"), "abort message missing 'Aborted' keyword");
    assert!(printed.contains("no transmission"), "abort message must state no transmission occurred");
}
```

- [ ] **Step 2: Run the test to confirm it fails (red)**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-nk7-<slug>/src-tauri
cargo test --test consent_gate_test
```

Expected output (exact phrasing may vary across rustc versions, but the failure category is fixed): a compile error along the lines of:

```
error[E0432]: unresolved import `tuxlink_lib::consent_gate`
  --> tests/consent_gate_test.rs:2:5
   |
 2 | use tuxlink_lib::consent_gate::{check_consent, ConsentOutcome, TransmissionPlan};
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^ could not find `consent_gate` in `tuxlink_lib`
```

If the test compiles or passes at this step, the working tree is already polluted with a pre-existing `consent_gate.rs` — STOP and investigate before continuing.

- [ ] **Step 3: Implement `src-tauri/src/consent_gate.rs`**

Create `src-tauri/src/consent_gate.rs` with these exact contents:

```rust
//! Part 97 consent gate for amateur-radio transmissions.
//!
//! This module is pure: it takes a `Read` for input and a `Write` for output,
//! making it fully unit-testable with `std::io::Cursor`. It contains NO
//! networking, NO process spawning, and NO actual transmission logic.
//!
//! The binary at `src/bin/live_cms_smoke.rs` wires this module to real
//! stdin/stdout and to the `pat_client` / `pat_process` modules. See
//! `docs/live-cms-testing-policy.md` and `docs/pitfalls/implementation-pitfalls.md`
//! §0 RADIO-1 for the operational policy this module enforces.

use std::io::{BufRead, BufReader, Read, Write};

/// A scoped transmission plan presented to the operator for consent.
///
/// Every field MUST be surfaced in the consent banner — that's what
/// "scoped" means in RADIO-1. Missing a field would mean the operator
/// is consenting to less than the actual transmission.
pub struct TransmissionPlan {
    pub target: String,
    pub session_count: u32,
    pub expected_duration_s: u32,
    pub content: String,
    pub freq_mode_band: String,
    pub callsign: String,
}

/// Result of the consent prompt.
///
/// `Granted` means the operator typed exactly `"go"` and pressed Enter.
/// `Aborted` means any other input (including empty, EOF, whitespace-padded
/// `go`, case variants, etc.) was received OR the input could not be read.
pub enum ConsentOutcome {
    Granted,
    Aborted,
}

/// Print the consent banner to `output`, read one line from `input`, and
/// decide. Returns `Granted` iff the line is exactly `"go"` after stripping
/// at most one trailing newline (`\n` or `\r\n`).
///
/// On `Aborted`, also writes an "Aborted — no transmission occurred." line
/// to `output` so the operator gets visible feedback.
pub fn check_consent<R: Read, W: Write>(
    plan: &TransmissionPlan,
    input: R,
    mut output: W,
) -> ConsentOutcome {
    // Write the banner. Ignore Write errors — if stdout is closed, the
    // operator can't see the prompt anyway and abort is the safe default.
    let _ = writeln!(
        output,
        "\nWARNING: Live amateur radio transmission.\n\
         This tool will transmit on the amateur radio network under callsign {callsign}.\n\n\
         Planned activity:\n  \
         - Target: {target}\n  \
         - Session count: {count}\n  \
         - Expected duration: {duration} s\n  \
         - Transmission content: {content}\n  \
         - Frequency / mode / band: {fmb}\n\n\
         By typing \"go\" and pressing Enter, you confirm:\n  \
         - You are the station licensee (or their authorized deputy).\n  \
         - You accept responsibility under 47 CFR Part 97 for these transmissions.\n  \
         - You consent to this specific run only; no future run is authorized.\n  \
         - You will monitor for completion.\n\n\
         Type \"go\" to proceed, anything else to abort:\n> ",
        callsign = plan.callsign,
        target = plan.target,
        count = plan.session_count,
        duration = plan.expected_duration_s,
        content = plan.content,
        fmb = plan.freq_mode_band,
    );
    let _ = output.flush();

    let mut reader = BufReader::new(input);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        // Read error treated as no consent — fail-safe.
        let _ = writeln!(output, "Aborted — no transmission occurred.");
        return ConsentOutcome::Aborted;
    }

    // Strip at most one trailing newline. Order matters: strip `\n` first,
    // then `\r` (so `\r\n` becomes ``, but a lone trailing `\r` from a
    // CR-only source becomes ``). NO other whitespace is stripped — `" go"`,
    // `"go "`, `"\tgo"` all remain unequal to `"go"` and abort.
    let trimmed = line
        .strip_suffix('\n')
        .map(|s| s.strip_suffix('\r').unwrap_or(s))
        .unwrap_or(&line);

    if trimmed == "go" {
        ConsentOutcome::Granted
    } else {
        let _ = writeln!(output, "Aborted — no transmission occurred.");
        ConsentOutcome::Aborted
    }
}
```

Note on the newline-stripping logic vs the original spec at `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md:1410`: the original used a nested `strip_suffix` chain that was subtly broken (the inner branch re-shadowed `line` and dropped the outer `strip_suffix('\n')` result). The plan author for Task 6 (cedar-redwood-dune) corrected this to the `.map(...).unwrap_or(&line)` form above, which has clear semantics: try to strip `\n`; if that worked, also try to strip `\r` from what remains; if no `\n` was present, use the line as-is. The corrected form is the one Phase 1's tests are written against.

Modify `src-tauri/src/lib.rs` to add the module. Open the file and add `pub mod consent_gate;` so the existing module list becomes:

```rust
pub mod config;
pub mod consent_gate;
pub mod pat_client;
pub mod pat_process;
```

Do NOT modify any other line in `lib.rs`.

- [ ] **Step 4: Run the tests to confirm green**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-nk7-<slug>/src-tauri
cargo test --test consent_gate_test
```

Expected: 9 tests pass. If any test fails, do NOT weaken the assertion (see "Preserve assertion rigor under pressure" in the universal preamble). Read the failure, fix the implementation, re-run. If a test fails because the spec-vs-test mismatch is real, STOP and escalate.

- [ ] **Step 5: Re-run the full crate test suite to confirm no regressions**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-nk7-<slug>/src-tauri
cargo test
```

Expected: all existing tests still pass (config, pat_client, pat_process). Adding `pub mod consent_gate;` to `lib.rs` is purely additive; it should not affect other tests. If anything else fails, it's a pre-existing red on `feat/v0.0.1` and warrants its own bd issue — file one before continuing.

- [ ] **Step 6: Commit Phase 1**

Write the commit message to a file first (the destructive-git hook does substring-matching on the inline `-m` text, and commit-message bodies containing certain regexes can false-positive):

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-nk7-<slug>
cat > /tmp/phase1-msg.txt <<'EOF'
feat(consent-gate): Part 97 transmission consent module

Pure stdin/stdout module that prints a scoped consent banner and
accepts only the exact string "go" to grant consent. Reads via the
Read trait + writes via Write so all tests use Cursor — no real I/O,
no transmission, no network.

Granted iff the input line is exactly "go" after stripping one
trailing newline (\n or \r\n). All other inputs abort, including
empty input, EOF, whitespace-padded variants, and case variants.

Tests cover: granted (LF + CRLF terminators), aborted (uppercase,
mixed-case, empty, lone newline, whitespace-padded, alternatives),
banner content (every TransmissionPlan field surfaces), abort message.

Wires into src/bin/live_cms_smoke.rs in Phase 2. See
docs/live-cms-testing-policy.md and docs/pitfalls/implementation-pitfalls.md
§0 RADIO-1.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF

git add src-tauri/src/consent_gate.rs \
        src-tauri/src/lib.rs \
        src-tauri/tests/consent_gate_test.rs
git commit -F /tmp/phase1-msg.txt
rm /tmp/phase1-msg.txt
```

- [ ] **Step 7: Update this plan's Execution Status banner**

Per the Living Document Contract: at the top of this plan, flip the Phase 1 banner from `⬜ NOT STARTED` to `✅ SHIPPED at <SHA> on <YYYY-MM-DD>`, and update the top-of-plan Execution Status table's Phase 1 row. The SHA is `git rev-parse HEAD`; the date is `date -u +%Y-%m-%d`.

---

## Phase 2 — Operator-Only Smoke Binary

**Execution Status:** ⬜ NOT STARTED

**Goal of this phase:** Build the `live_cms_smoke` binary that wires `consent_gate` to real Pat I/O. The binary lives at `src-tauri/src/bin/live_cms_smoke.rs` so `cargo test` cannot discover or invoke it. Compilation is the only verification the impl subagent performs; actual invocation is Phase 5 (operator-only).

**Agent execution rule for this phase (CRITICAL):**

- The impl subagent runs `cargo build --bin live_cms_smoke` (compilation check). That's it.
- The impl subagent MUST NOT run `cargo run --bin live_cms_smoke`. Not even with `WINLINK_PASSWORD=` unset (the binary would still print the consent banner and read stdin; piping `"go"` into a binary that needs stdin is the exact violation RADIO-1 names).
- The impl subagent MUST NOT pipe `"nope"` or empty input into the binary "to confirm it aborts cleanly" — that's still invoking the binary. The Phase 1 tests already prove the consent-gate logic; redundant invocation tests on the wired binary add zero coverage and violate RADIO-1.
- If `cargo build` fails, fix the compilation error and re-run. If `cargo build` passes but the impl subagent doubts the runtime wiring, STOP and add the doubt to the PR body for operator review — do NOT run the binary to "verify."

### Task 2.1 — Files

**Files:**
- Create: `src-tauri/src/bin/live_cms_smoke.rs`
- Modify: `src-tauri/Cargo.toml` (add `[[bin]]` declaration; add `chrono` dependency if absent)

### Task 2.2 — Steps

- [ ] **Step 1: Verify `chrono` is in Cargo.toml dependencies**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-nk7-<slug>
grep -E '^chrono\s*=' src-tauri/Cargo.toml
```

If `grep` exits non-zero (no chrono line), add it under `[dependencies]` in `src-tauri/Cargo.toml`:

```toml
chrono = { version = "0.4", default-features = false, features = ["clock", "std"] }
```

Why `default-features = false` with explicit `clock` + `std`: the smoke binary only needs `Utc::now().to_rfc3339()`; pulling in chrono's full default feature set (which transitively pulls `wasm-bindgen` and other features tuxlink doesn't need) bloats the build for no reason. The feature subset `["clock", "std"]` gives us `Utc::now()` and `DateTime::to_rfc3339()`, which is all the binary uses.

If `grep` exits zero (chrono present), verify the feature set includes `clock`; if not, edit the existing line to add it.

- [ ] **Step 2: Declare the `live_cms_smoke` binary in Cargo.toml**

Append to `src-tauri/Cargo.toml` (after the existing `[[bin]] name = "tuxlink" path = "src/main.rs"` block):

```toml

[[bin]]
name = "live_cms_smoke"
path = "src/bin/live_cms_smoke.rs"
```

The leading blank line is intentional — keeps the TOML readable.

- [ ] **Step 3: Create the binary directory**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-nk7-<slug>
mkdir -p src-tauri/src/bin
```

- [ ] **Step 4: Implement `src-tauri/src/bin/live_cms_smoke.rs`**

Create the file with these exact contents. This is the wired binary — the only place where consent gate, Pat spawn, and CMS send come together. Read every comment; they document RADIO-1 boundaries.

```rust
//! Operator-only live-CMS smoke test for tuxlink.
//!
//! ⚠️  RADIO-1 BRIGHT LINE  ⚠️
//!
//! This binary transmits on the amateur radio network under the operator's
//! callsign. It MUST be invoked ONLY by the station licensee (or their
//! authorized deputy), interactively, with explicit per-invocation consent.
//!
//! - NEVER invoked by `cargo test`. (It lives in `src/bin/`, not `tests/`.)
//! - NEVER invoked by CI. No GitHub Action, no cron, no scheduled task.
//! - NEVER invoked by an AI agent in a subagent shell, even with `WINLINK_*`
//!   env vars set. Agents BUILD this binary; the operator RUNS it.
//! - NEVER invoked by a `/loop` skill or any background polling.
//!
//! Each invocation is a fresh consent gate. No cached consent. No env-var
//! bypass. The operator must physically type "go" and press Enter every
//! single time.
//!
//! Usage (operator-only):
//!   export WINLINK_CALLSIGN=<your callsign>
//!   export WINLINK_PASSWORD=<your CMS password>
//!   export WINLINK_GRID=<your Maidenhead grid, e.g. EM75xx>
//!   cargo run --bin live_cms_smoke
//!
//! See `docs/live-cms-testing-policy.md` and `dev/README-live-cms-smoke.md`.

use std::fs;
use std::io::{stdin, stdout, Write};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::time::{Duration, Instant};

use tuxlink_lib::consent_gate::{check_consent, ConsentOutcome, TransmissionPlan};
use tuxlink_lib::pat_client::{MailboxFolder, PatClient};
use tuxlink_lib::pat_process::{PatProcess, PatSpawnOptions};

fn main() {
    // Read credentials from env vars only. The operator sets these in their
    // shell for the single invocation; nothing is persisted by this binary.
    // Missing env vars print a helpful pointer at the policy doc and exit 2.
    let callsign = required_env("WINLINK_CALLSIGN", "your amateur radio callsign");
    let password = required_env("WINLINK_PASSWORD", "your CMS password (Winlink account)");
    let grid = required_env("WINLINK_GRID", "your Maidenhead grid square (e.g. EM75xx)");

    let plan = TransmissionPlan {
        target: "SERVICE@winlink.org".into(),
        session_count: 1,
        expected_duration_s: 45,
        content: format!(
            "tuxlink live_cms_smoke {} (v0.0.1 verification)",
            chrono::Utc::now().to_rfc3339()
        ),
        // Telnet over IP is Part 15 transport, not Part 97 RF — see
        // implementation-pitfalls.md §0 RADIO-2. The consent gate still
        // fires because the SESSION carries the operator's callsign even
        // though the LINK is internet.
        freq_mode_band: "telnet over IP; no RF".into(),
        callsign: callsign.clone(),
    };

    let start_instant = Instant::now();
    let start_utc = chrono::Utc::now();

    // CONSENT GATE — single source of authorization for this run.
    match check_consent(&plan, stdin().lock(), stdout().lock()) {
        ConsentOutcome::Granted => { /* fall through to run_smoke() */ }
        ConsentOutcome::Aborted => {
            log_session(
                &start_utc,
                &callsign,
                &plan,
                0,
                "aborted-by-operator",
                start_instant.elapsed(),
            );
            // Exit 2 per docs/live-cms-testing-policy.md §"Required consent gate implementation".
            exit(2);
        }
    }

    // Past this point: operator has consented. Run the smoke, log the outcome.
    let outcome = run_smoke(&callsign, &password, &grid, &plan);
    let elapsed = start_instant.elapsed();

    match outcome {
        Ok(actual_sessions) => {
            println!("\nOK: received reply from {}", plan.target);
            log_session(&start_utc, &callsign, &plan, actual_sessions, "success", elapsed);
        }
        Err(e) => {
            eprintln!("\nFAIL: {}", e);
            log_session(&start_utc, &callsign, &plan, 0, "failed", elapsed);
            exit(1);
        }
    }
}

/// Reads an environment variable and exits 2 with a helpful pointer if missing.
fn required_env(name: &str, description: &str) -> String {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => v,
        _ => {
            eprintln!(
                "ERROR: {name} must be set before running this binary.\n\
                 Expected: {description}.\n\
                 See dev/README-live-cms-smoke.md and docs/live-cms-testing-policy.md.",
                name = name,
                description = description,
            );
            exit(2);
        }
    }
}

/// Runs the consented smoke: spawn Pat, write its config, send a test
/// message to SERVICE@winlink.org, trigger a telnet connect, poll inbox
/// for the autoresponder reply, shut Pat down.
///
/// Returns Ok(actual_session_count) on success, Err(reason) on failure.
fn run_smoke(
    callsign: &str,
    password: &str,
    grid: &str,
    plan: &TransmissionPlan,
) -> Result<u32, String> {
    let home = std::env::var_os("HOME").ok_or("HOME unset")?;
    let pat_config_path = PathBuf::from(&home)
        .join(".config")
        .join("pat")
        .join("config.json");

    write_pat_config_for_smoke(&pat_config_path, callsign, password, grid)
        .map_err(|e| format!("write pat config: {}", e))?;

    let mbox_dir = PathBuf::from(&home)
        .join(".local")
        .join("share")
        .join("tuxlink")
        .join("mbox");
    let pid_file = PathBuf::from(&home)
        .join(".local")
        .join("state")
        .join("tuxlink")
        .join("pat.pid");
    let opts = PatSpawnOptions {
        binary: PathBuf::from("pat"),
        config_path: pat_config_path,
        mbox_dir,
        http_listen_port: 0, // 0 = let the OS pick a free port; PatProcess reads back the actual port.
        pid_file,
    };
    let mut proc = PatProcess::spawn(opts).map_err(|e| format!("spawn pat: {}", e))?;
    let client = PatClient::new(format!("http://127.0.0.1:{}", proc.http_port()));

    // PatClient::send signature (Task 5, PR #41/#42):
    //   pub fn send(&self, to: &[&str], subject: &str, body: &str, date: &str)
    //       -> Result<(), PatClientError>
    // Returns unit, not a MessageId. The original Task 6 spec at
    // docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md:1530 was outdated
    // (3-arg call, `let _mid = ...`); the actual signature requires the
    // `date` arg and discards the unit return. See Discovery D-1.
    let now_rfc3339 = chrono::Utc::now().to_rfc3339();
    client
        .send(
            &[&plan.target],
            "tuxlink setup test",
            &plan.content,
            &now_rfc3339,
        )
        .map_err(|e| format!("queue outbound message: {}", e))?;

    // Trigger a telnet connect against Winlink CMS via Pat's HTTP API.
    let connect_url = format!(
        "http://127.0.0.1:{}/api/connect?url=telnet",
        proc.http_port()
    );
    reqwest::blocking::Client::new()
        .post(&connect_url)
        .timeout(Duration::from_secs(30))
        .send()
        .map_err(|e| format!("trigger connect: {}", e))?;

    // Poll inbox for a reply from SERVICE@winlink.org. The autoresponder
    // is typically prompt (<10s) but Pat may need a moment to fetch the
    // reply after the outbound delivers. 30s is generous.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let msgs = client
            .list(MailboxFolder::Inbox)
            .map_err(|e| format!("list inbox: {}", e))?;
        if msgs.iter().any(|m| m.from.contains("SERVICE@winlink.org")) {
            let _ = proc.shutdown(Duration::from_secs(5));
            return Ok(1);
        }
        if Instant::now() > deadline {
            let _ = proc.shutdown(Duration::from_secs(5));
            return Err("no reply from SERVICE@winlink.org within 30s".into());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Writes a minimal Pat config for the smoke binary's one-shot use.
///
/// This is INLINED here (not in `tuxlink_lib::wizard_commands::write_pat_config`)
/// because wizard_commands does not exist yet — Task 9 (bd `tuxlink-ko0`)
/// is OPEN as of 2026-05-18. See Discovery D-2.
///
/// When Task 9 lands, a future housekeeping commit MAY refactor this
/// helper to call `wizard_commands::write_pat_config` instead, IF that
/// function's signature accepts the same (callsign, password, grid) triple.
/// Until then, this self-contained helper keeps Task 6 unblocked.
fn write_pat_config_for_smoke(
    path: &Path,
    callsign: &str,
    password: &str,
    grid: &str,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    // Minimal Pat config JSON shape. Matches what Pat 0.16+ expects:
    // mycall, secure_login_password, locator. Other fields are filled with
    // sensible defaults; the smoke binary doesn't need anything else.
    let json = format!(
        r#"{{
  "mycall": "{callsign}",
  "secure_login_password": "{password}",
  "locator": "{grid}",
  "service_codes": ["PUBLIC"],
  "http_addr": "127.0.0.1:0",
  "motd": ["tuxlink live_cms_smoke session"],
  "connect_aliases": {{}},
  "listen": [],
  "hamlib_rigs": {{}},
  "ax25": {{ "port": "wl2k", "beacon": {{ "every": 0, "message": "", "destination": "IDENT" }} }},
  "ardop": {{ "addr": "localhost:8515", "arq_bandwidth": {{ "Forced": false, "Max": 500 }}, "cwid_enabled": false }},
  "pactor": {{ "path": "/dev/ttyUSB0", "baudrate": 57600, "rig": "", "custom_init_script": "" }},
  "telnet": {{ "listen_addr": ":8774", "password": "" }},
  "varahf": {{ "host": "localhost", "cmd_port": 8300, "data_port": 8301, "bandwidth": 2300, "rig": "" }},
  "varafm": {{ "host": "localhost", "cmd_port": 8300, "data_port": 8301, "rig": "" }},
  "schedule": {{}},
  "version_reporting_disabled": false
}}"#,
        callsign = json_escape(callsign),
        password = json_escape(password),
        grid = json_escape(grid),
    );
    fs::write(path, json)
}

/// Minimal JSON string escaping for the fields this binary writes.
///
/// Pat's mycall/locator/password values rarely contain JSON-significant
/// characters in practice (callsigns are ASCII alnum, grids are 4-6 char
/// alnum, passwords occasionally contain `\`, `"`, control chars). Escape
/// the small set that matters; reject anything weird.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Appends one line to `dev/live-cms-sessions.log` per the policy at
/// docs/live-cms-testing-policy.md §"Required logging".
///
/// Log line shape (space-separated key=value, easy to grep):
///   <UTC-ISO8601>  live_cms_smoke  callsign=<CS>  planned_sessions=<N>
///   actual_sessions=<N>  outcome=<S>  duration_s=<N>  target=<ADDR>
///
/// Failure to write the log is silent: the consent banner already fired,
/// the transmission already happened (or aborted); failing the binary on a
/// log-write error would lose the actual outcome the operator needs.
fn log_session(
    start_utc: &chrono::DateTime<chrono::Utc>,
    callsign: &str,
    plan: &TransmissionPlan,
    actual_sessions: u32,
    outcome: &str,
    duration: Duration,
) {
    let line = format!(
        "{ts}  live_cms_smoke  callsign={callsign}  planned_sessions={planned}  actual_sessions={actual}  outcome={outcome}  duration_s={dur}  target={target}\n",
        ts = start_utc.to_rfc3339(),
        callsign = callsign,
        planned = plan.session_count,
        actual = actual_sessions,
        outcome = outcome,
        dur = duration.as_secs(),
        target = plan.target,
    );
    let path = PathBuf::from("dev").join("live-cms-sessions.log");
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| f.write_all(line.as_bytes()));
}
```

- [ ] **Step 5: Compile-only verification**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-nk7-<slug>/src-tauri
cargo build --bin live_cms_smoke
```

Expected: compiles cleanly. If the build fails:

- **`unresolved import tuxlink_lib::consent_gate`** — Phase 1's `lib.rs` edit is missing or wrong. Re-check `src-tauri/src/lib.rs` for `pub mod consent_gate;`.
- **`unresolved import chrono`** — Step 1's Cargo.toml edit didn't take. Re-check.
- **`mismatched types` on `client.send(...)`** — verify the actual `PatClient::send` signature in `src-tauri/src/pat_client.rs:87` matches the 4-arg form documented above. If it's drifted again, file a bd issue and STOP.
- **Any other compile error** — fix the cause, do NOT comment out or weaken.

DO NOT run the binary in this step. `cargo build --bin <name>` does compile-only; it does not execute. Running the binary is Phase 5 (operator-only).

- [ ] **Step 6: Confirm `cargo test` does NOT discover the binary**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-nk7-<slug>/src-tauri
cargo test 2>&1 | grep -E '(live_cms_smoke|Running .*bin)' || echo "OK: live_cms_smoke not in cargo test output"
```

Expected: `OK: live_cms_smoke not in cargo test output`. If the grep matches, the binary has somehow been picked up as a test target — this would be a RADIO-1 violation by construction. STOP and inspect:

- Is the file at `src-tauri/src/bin/live_cms_smoke.rs` (correct) or at `src-tauri/tests/live_cms_smoke.rs` (wrong)?
- Did the Cargo.toml `[[bin]]` block somehow get put under `[[test]]`?
- Is there a stray `#[test]` annotation in `live_cms_smoke.rs`?

Fix the structural cause before continuing.

- [ ] **Step 7: Commit Phase 2**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-nk7-<slug>
cat > /tmp/phase2-msg.txt <<'EOF'
feat(live-cms): operator-only smoke binary at src/bin/live_cms_smoke.rs

Binary that wires consent_gate to PatProcess + PatClient for a
single round-trip against SERVICE@winlink.org. Lives in src/bin/
(not tests/) so cargo test cannot discover or invoke it. Reads
credentials from WINLINK_{CALLSIGN,PASSWORD,GRID} env vars set
once per operator invocation; nothing persisted.

Inlines a minimal write_pat_config_for_smoke helper because Task 9
(wizard_commands module) is not yet shipped — see Discovery D-2 in
docs/plans/2026-05-18-task-6-live-cms-smoke-plan.md.

PatClient::send is called with the 4-arg signature shipped by Task 5
(Discovery D-1 in the same plan); chrono::Utc::now().to_rfc3339()
supplies the date.

RADIO-1: this binary MUST only be invoked by the licensee. Subagents
build via `cargo build --bin live_cms_smoke` and never run it. See
docs/live-cms-testing-policy.md and docs/pitfalls/implementation-pitfalls.md
§0 RADIO-1.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF

git add src-tauri/src/bin/live_cms_smoke.rs \
        src-tauri/Cargo.toml \
        src-tauri/Cargo.lock
git commit -F /tmp/phase2-msg.txt
rm /tmp/phase2-msg.txt
```

Cargo.lock is included because the new `chrono` dependency (if added in Step 1) will pin transitive deps into the lockfile.

- [ ] **Step 8: Update this plan's Phase 2 Execution Status banner to ✅ SHIPPED.**

---

## Phase 3 — Session Log File + Operator README

**Execution Status:** ⬜ NOT STARTED

**Goal of this phase:** Create the empty `dev/live-cms-sessions.log` (with a header line so it's never zero bytes), and the operator-facing `dev/README-live-cms-smoke.md` that documents how Cameron invokes the binary. Both are git-tracked.

**Agent execution rule for this phase:** No transmission. Pure docs + an empty log file. Safe for the impl subagent.

### Task 3.1 — Files

**Files:**
- Create: `dev/live-cms-sessions.log`
- Create: `dev/README-live-cms-smoke.md`

### Task 3.2 — Steps

- [ ] **Step 1: Create `dev/live-cms-sessions.log` with header**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-nk7-<slug>
mkdir -p dev
cat > dev/live-cms-sessions.log <<'EOF'
# Live CMS session log — appended to by src-tauri/src/bin/live_cms_smoke.rs
# Each line: <UTC-ISO8601>  live_cms_smoke  callsign=<CS>  planned_sessions=<N>  actual_sessions=<N>  outcome=<S>  duration_s=<N>  target=<ADDR>
# See docs/live-cms-testing-policy.md §"Required logging" for the policy.
EOF
```

Why a header rather than truly empty: an empty file at this path could be mistaken for "nothing has been logged" vs "the log file is uninitialized." A header line makes the file's purpose self-describing on `cat` and grep-skippable via `grep -v '^#'`.

- [ ] **Step 2: Create `dev/README-live-cms-smoke.md`**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-nk7-<slug>
cat > dev/README-live-cms-smoke.md <<'EOF'
# Operator-only: Running the live-CMS smoke test

> **For Cameron Zucker (the station licensee) or his authorized deputy.**
> Subagents: do NOT run this. See [`docs/live-cms-testing-policy.md`](../docs/live-cms-testing-policy.md)
> and [`docs/pitfalls/implementation-pitfalls.md`](../docs/pitfalls/implementation-pitfalls.md) §0 RADIO-1.

This is the one-shot live-CMS round-trip tool. It sends a single timestamped
message to `SERVICE@winlink.org` (the Winlink autoresponder), waits up to 30
seconds for the reply, and logs the outcome to `dev/live-cms-sessions.log`.

It is NOT a test (in the `cargo test` sense). It is a dedicated binary at
`src-tauri/src/bin/live_cms_smoke.rs` that exists only to be invoked
manually, interactively, with explicit per-invocation consent.

## Prerequisites

- A valid Winlink account with a CMS password (separate from your Winlink
  Express install password — this is the API-side credential).
- `pat` binary on `PATH` (or bundled in the AppImage in v0.0.1+ builds).
- Internet connectivity; firewall permits outbound to `cms.winlink.org:8772`.
- A non-empty Maidenhead grid square (e.g. `EM75xx`).

## Run

```bash
export WINLINK_CALLSIGN=W4PHS              # your callsign
export WINLINK_PASSWORD='your-cms-password' # quote to preserve special chars
export WINLINK_GRID=EM75xx                  # your Maidenhead grid (4-6 char)
cargo run --bin live_cms_smoke
```

The binary prints a Part 97 consent banner showing exactly what will be
transmitted (target, session count, expected duration, content,
frequency/mode/band, and your callsign). Read it. If everything is correct
and you authorize the transmission, type `go` (lowercase, no spaces) and
press Enter. Any other input aborts with exit code 2.

Each invocation is a fresh consent gate. There is no "remember my consent"
flag. If you re-run, you re-consent.

## What it does

1. Prompts you for consent (the gate above).
2. On `go`: writes a minimal Pat config to `~/.config/pat/config.json`
   using the env-var credentials, spawns `pat` listening on an OS-picked
   localhost port.
3. Posts one outbound message addressed to `SERVICE@winlink.org`,
   subject `tuxlink setup test`, body containing the current UTC ISO-8601
   timestamp.
4. Triggers a telnet connect via Pat's `/api/connect?url=telnet`.
5. Polls Pat's inbox via `/api/mailbox/in` once every 500 ms for up to
   30 seconds, looking for any message whose `From` field contains
   `SERVICE@winlink.org`.
6. On reply received: prints `OK: received reply from SERVICE@winlink.org`,
   appends a `success` line to `dev/live-cms-sessions.log`, shuts Pat down.
7. On timeout: prints `FAIL: no reply from SERVICE@winlink.org within 30s`,
   appends a `failed` line, exits 1.
8. On consent aborted: appends an `aborted-by-operator` line, exits 2.

## What it does NOT do

- Does NOT run in CI, on a schedule, in a `/loop` invocation, or in any
  automated context.
- Does NOT run from `cargo test`. The binary lives in `src/bin/`, not
  `tests/`; `cargo test` cannot discover it.
- Does NOT read credentials from any committed file, config file, OS
  keyring, or shell history. Env vars only, each invocation.
- Does NOT retry on failure. One session, one consent, one outcome.
- Does NOT transmit over RF. Telnet over IP only (Part 15 transport).
  Your callsign appears in the session under Part 97 nevertheless — that's
  why the consent gate fires.

## Inspecting the log

```bash
grep -v '^#' dev/live-cms-sessions.log    # all logged sessions, skipping header
grep success dev/live-cms-sessions.log    # just the successes
grep aborted dev/live-cms-sessions.log    # just the operator aborts
```

The log is append-only in practice. There is no log-rotation in v0.0.1;
the file grows by one line per invocation. For Part 97 documentation
purposes, never delete lines from this file.

## If something goes wrong

- **Consent banner appears garbled or wrong** — DO NOT type `go`. Type
  anything else (or Ctrl-C). File a bd issue with the banner contents.
- **Binary panics or exits with an error before the consent gate** —
  no transmission occurred. Read the error, fix the env var or config,
  re-run.
- **Binary hangs after `go`** — Pat may be slow to spawn or the network
  may be partitioned. Ctrl-C is safe; the in-flight outbound (if any)
  is just queued in Pat's outbox, not transmitted.
- **Reply never arrives** — Winlink autoresponder is usually prompt but
  not always. Re-run the smoke once if the first attempt timed out.
  Persistent failures may indicate CMS-side issues, your callsign isn't
  properly registered with Winlink, your password is wrong, or your
  firewall blocks 8772 outbound.

## See also

- [`docs/live-cms-testing-policy.md`](../docs/live-cms-testing-policy.md) — the operational policy this tool implements
- [`docs/pitfalls/implementation-pitfalls.md`](../docs/pitfalls/implementation-pitfalls.md) §0 RADIO-1 — why this isn't an automated test
- `src-tauri/src/consent_gate.rs` — the consent-prompt source
- `src-tauri/src/bin/live_cms_smoke.rs` — the binary source
EOF
```

- [ ] **Step 3: Commit Phase 3**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-nk7-<slug>
cat > /tmp/phase3-msg.txt <<'EOF'
docs(live-cms): operator-only README + initial session log

dev/live-cms-sessions.log is created with a self-describing header
so its purpose is clear on cat without needing to consult the policy
doc. dev/README-live-cms-smoke.md walks the licensee through env-var
setup, the consent gate, what the binary does and doesn't do, and
how to inspect the log.

Both files are committed (not gitignored) so the policy + how-to
travel with the source.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF

git add dev/live-cms-sessions.log dev/README-live-cms-smoke.md
git commit -F /tmp/phase3-msg.txt
rm /tmp/phase3-msg.txt
```

- [ ] **Step 4: Update this plan's Phase 3 Execution Status banner to ✅ SHIPPED.**

---

## Phase 4 — Integration Push + PR

**Execution Status:** ⬜ NOT STARTED

**Goal of this phase:** Push the branch, open the PR against `feat/v0.0.1`, complete the per-task branch wrap. No code changes in this phase — just polish + push + PR.

**Agent execution rule for this phase:** No transmission. Pure git + gh CLI. Safe for the impl subagent.

### Task 4.1 — Steps

- [ ] **Step 1: Polish local commits**

Per CLAUDE.md §"Commit and release discipline" → "Polish before push": clean up any WIP / fixup / "oops" commits via non-interactive `git rebase <base>` on **local un-pushed commits** before `git push`. Use `git rebase feat/v0.0.1` (NOT `-i`; interactive rebase is hook-banned). If the only commits on the branch are the three Phase 1-3 commits in clean conventional-commit form, polish is a no-op — skip the rebase.

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-nk7-<slug>
git log --oneline feat/v0.0.1..HEAD
```

Expected: 3 commits, one per phase (Phase 1 / Phase 2 / Phase 3).

- [ ] **Step 2: Final build + test pass**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-nk7-<slug>/src-tauri
cargo build --bin live_cms_smoke
cargo test
```

Expected: build clean, all tests pass.

DO NOT run `cargo run --bin live_cms_smoke` here. The instinct to "just verify it works end-to-end before pushing" is the RADIO-1 violation this plan exists to prevent. Trust the unit tests; trust the compiler.

- [ ] **Step 3: Push the branch**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-nk7-<slug>
git push -u origin bd-tuxlink-nk7/<slug>
```

If push fails for "branch is behind feat/v0.0.1", run `git pull --rebase origin feat/v0.0.1` and re-push. Do NOT `--force` (hook-banned).

- [ ] **Step 4: Open the PR**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-nk7-<slug>
gh pr create --base feat/v0.0.1 --head bd-tuxlink-nk7/<slug> \
  --title "[<your-moniker>] feat: Task 6 — Live-CMS smoke binary (operator-only)" \
  --body "$(cat <<'BODY'
## Summary

Implements [v0.0.1 Task 6](docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md#task-6-live-cms-smoke-binary-operator-only--review-gate-after-this-task) per the [Wave-1 plan](docs/plans/2026-05-18-task-6-live-cms-smoke-plan.md).

- New module `src-tauri/src/consent_gate.rs` — pure stdin/stdout Part 97 consent gate, 9 unit tests via `std::io::Cursor`.
- New binary `src-tauri/src/bin/live_cms_smoke.rs` — operator-only smoke against `SERVICE@winlink.org` via Pat. Lives in `src/bin/`, not `tests/`, so `cargo test` cannot discover it.
- New operator doc `dev/README-live-cms-smoke.md` + initial `dev/live-cms-sessions.log`.

## RADIO-1 conformance

- **The implementing subagent did NOT run the binary.** `cargo build --bin live_cms_smoke` and `cargo test` are the only Cargo invocations.
- The binary requires interactive `"go"` on stdin every invocation (verified by 9 unit tests over the consent gate).
- No credentials are persisted; env-vars-only at run-time.
- Logs every invocation to `dev/live-cms-sessions.log` per `docs/live-cms-testing-policy.md`.

## Cameron's manual verification (Phase 5 of the plan — operator-only)

Per the plan's Phase 5: once this PR merges, Cameron runs the binary once in his shell with his actual `WINLINK_*` env vars, types `"go"` at the prompt, observes the outcome, and confirms `dev/live-cms-sessions.log` got a `success` line. That's the only end-to-end validation the project does for v0.0.1 (per Task 6 spec: "Why v0.0.1 has no automated integration test against the real CMS"). Cameron's confirmation comment on this PR is the merge gate for the larger Task 6 close.

## Discoveries flagged during plan-writing

The Wave-1 plan-writer surfaced three spec-vs-codebase drifts (D-1, D-2, D-3) and resolved them inline in the plan's Discoveries subsection. The most material: `PatClient::send` requires a 4-arg signature (3rd-party drift since the original spec was written), and the spec's `wizard_commands` dependency is not yet shipped — the binary inlines a self-contained `write_pat_config_for_smoke` helper to ship Task 6 independently of Task 9. See the plan's Discoveries section for the full disposition.

## Test plan

- [x] `cargo test --test consent_gate_test` — 9/9 pass
- [x] `cargo build --bin live_cms_smoke` — clean
- [x] `cargo test` — no regressions in existing test suite
- [x] `cargo test 2>&1 | grep live_cms_smoke` — empty (binary not discoverable as a test target)
- [ ] **Operator** runs `cargo run --bin live_cms_smoke` with his env vars, types `"go"`, confirms `success` in the log

🤖 Generated with [Claude Code](https://claude.com/claude-code)
BODY
)"
```

Substitute `<your-moniker>` and `<slug>` (whatever you used for the worktree, e.g. `live-cms-smoke`).

- [ ] **Step 5: Hand off to Phase 5 (operator)**

In the PR comments (or in your session-end handoff doc), state explicitly:

> "RADIO-1 next-step: Phase 5 of the plan is operator-only. Cameron runs `cargo run --bin live_cms_smoke` in his shell once the PR merges, with his actual `WINLINK_*` env vars. I cannot run it. Awaiting his confirmation that the round-trip succeeds before bd-closing tuxlink-nk7."

- [ ] **Step 6: Update this plan's Phase 4 Execution Status banner to ✅ SHIPPED**, and update the top-of-plan Execution Status table with the PR number and URL.

---

## Phase 5 — Operator Manual Verification (OPERATOR-ONLY)

**Execution Status:** ⬜ NOT STARTED — gated on the PR from Phase 4 merging

**This phase is performed by Cameron Zucker, not by any agent.** The Wave-2 implementing subagent does NOT execute any step in this phase. The subagent's role here is exclusively documentation handoff in Phase 4 Step 5.

**Goal of this phase:** Cameron runs the binary once with his real credentials, observes the consent banner, types `"go"`, watches the round-trip complete, confirms the log line landed, and comments on the PR (or files a bd note) with the outcome.

**RADIO-1 final reminder:** This is the ONLY way the binary should ever be invoked. If a future agent finds itself "needing to verify" by running the binary, the agent is violating RADIO-1. The verification is Cameron's, always.

### Task 5.1 — Steps (operator-performed)

- [ ] **Step 1: Pre-flight check** (operator)

```bash
cd /home/administrator/Code/tuxlink   # main checkout, NOT a worktree
git checkout feat/v0.0.1
git pull --ff-only origin feat/v0.0.1   # pulls in the Phase 4 PR merge
cd src-tauri
cargo build --bin live_cms_smoke
```

Expected: compiles clean against the merged code. The `cd src-tauri` is intentional — the `tuxlink`/`tuxlink_lib` crate root lives there, and `cargo build --bin <name>` resolves the binary from the nearest `Cargo.toml`.

- [ ] **Step 2: Set the one-shot env vars** (operator)

```bash
export WINLINK_CALLSIGN=<actual callsign>
export WINLINK_PASSWORD=<actual CMS password>
export WINLINK_GRID=<actual Maidenhead grid>
```

DO NOT persist these in `~/.bashrc` / `~/.zshrc` / shell profile / `.netrc` / OS keyring with auto-unlock / anywhere an agent process can read them. Per `docs/live-cms-testing-policy.md`: env vars are for THIS invocation only.

- [ ] **Step 3: Run the binary** (operator)

```bash
cd /home/administrator/Code/tuxlink/src-tauri
cargo run --bin live_cms_smoke
```

The consent banner prints. Read it. If anything looks wrong (wrong callsign, wrong target, wrong duration), type ANYTHING OTHER than `"go"` (e.g. `"abort"` or just press Enter) — the binary aborts cleanly and logs `aborted-by-operator`.

If the banner is correct and the run is authorized, type `go` and press Enter.

- [ ] **Step 4: Observe outcome** (operator)

Expected paths:

- **Success:** `OK: received reply from SERVICE@winlink.org` printed; `dev/live-cms-sessions.log` has a new `outcome=success` line; exit 0.
- **Timeout:** `FAIL: no reply from SERVICE@winlink.org within 30s` printed; log has `outcome=failed`; exit 1. Try again once; persistent failure warrants a bd issue.
- **Aborted:** `Aborted — no transmission occurred.` printed; log has `outcome=aborted-by-operator`; exit 2.

- [ ] **Step 5: Confirm + close the issue** (operator)

```bash
cd /home/administrator/Code/tuxlink
grep -v '^#' dev/live-cms-sessions.log | tail -3       # see the last few lines
bd close tuxlink-nk7 --reason "Operator verified live round-trip $(date -u +%Y-%m-%dT%H:%MZ)"
```

Comment on the merged PR with the log line you observed (with the actual callsign redacted if you're sharing publicly — your local copy keeps the full text).

- [ ] **Step 6: Update this plan's Phase 5 Execution Status banner to ✅ SHIPPED with the date of the operator's successful run, AND mark the overall plan complete in the top-of-plan Execution Status table.**

---

## Cross-Phase Review Checklist (run after Phase 4 ships, before Phase 5)

Per `writing-plans-enhanced` Step 3 → "After completing this group: Review the batch from multiple perspectives. Minimum 3 review rounds." The plan-writer ran this against the plan itself (see below); the implementing agent SHOULD repeat against the shipped code:

- **Round 1 — pitfalls compliance.** Re-read `docs/pitfalls/implementation-pitfalls.md` §0 with the shipped binary in hand. Any code path that could invoke live-CMS from `cargo test` or CI? Any persisted-credential pattern? Run: `grep -rn 'live_cms\|winlink.org\|cms.winlink' .github/ src-tauri/tests/`. Expected: zero hits (no test file references the binary; no CI calls it).
- **Round 2 — type/signature consistency.** Are module paths and type names in the shipped binary consistent with what `pat_client.rs` and `pat_process.rs` actually export? Run `cargo build --bin live_cms_smoke`; if it builds, types align.
- **Round 3 — RADIO-1 self-audit.** Search the PR diff for any of the following anti-patterns: `#[test]` annotation in `live_cms_smoke.rs`; `live_cms_smoke` invocation in any `tests/` file; any CI workflow file in `.github/` referencing the binary; any `echo "go" | ...` or `expect`-style automation around the binary; any `default features` enabled on dependencies that would pull in a runtime-credential source (auto-unlocked keyring, etc.). Expected: zero hits.

If any round finds findings, FIX IN-PLACE, commit, and re-run. Do not ship findings as "follow-up issues" — the bright line is bright.

---

## Plan-Writer Self-Review (cedar-redwood-dune, 2026-05-18)

This subsection records the plan-review-cycle the Wave-1 plan-writer ran against this document before shipping. Wave-2 impl agents may treat it as informational; the binding gates are the per-phase checklists above.

**Round 1 — ambiguity & context gaps.** Findings + dispositions:
- F1.1: "Step 4: Implement consent_gate.rs" said "create the file" without specifying the exact path. **Fixed:** path stated as `src-tauri/src/consent_gate.rs` in the Files block AND in Step 3's prose.
- F1.2: The CRLF handling in the original spec's `strip_suffix` chain was subtly broken (re-shadowing). **Fixed:** explicit `.map(...).unwrap_or(&line)` implementation with inline comment explaining the semantics, plus a dedicated `test_crlf_after_go_grants_consent` test.
- F1.3: Step 2 (`cargo test --test consent_gate_test`) didn't specify what failure to expect at the red stage. **Fixed:** quoted the expected `E0432` error verbatim plus the "if it passes here, the tree is polluted" escape hatch.
- F1.4: Phase 4 PR-creation step embedded the moniker without telling the agent to substitute it. **Fixed:** explicit `<your-moniker>` substitution note in Step 4.
- F1.5: The original spec called `client.send(...)` with 3 args; actual signature requires 4. **Fixed:** Discovery D-1 in the top-of-plan Discoveries subsection + corrected call site in Phase 2 Step 4 + inline comment pointing back to D-1.

**Round 2 — pitfalls compliance.** Findings + dispositions:
- F2.1: Phase 2 Step 5 said `cargo build --bin live_cms_smoke` but didn't explicitly forbid `cargo run`. The impl agent might reasonably interpret "build it to verify" as "run it to verify." **Fixed:** Phase 2 agent-execution-rule block enumerates every forbidden invocation form explicitly.
- F2.2: No check that the binary stays out of `cargo test` discovery. **Fixed:** Phase 2 Step 6 (`cargo test 2>&1 | grep ...`) added as a structural assertion.
- F2.3: The original spec stored `let _mid = client.send(...)` suggesting the return was a MessageId. The combination of this + the original `wizard_commands` import would have caused a 2-error compile failure that an impl agent might "fix" by inventing types. **Fixed:** Discovery D-2 explains the wizard_commands gap; the inlined `write_pat_config_for_smoke` resolves it.
- F2.4: `dev/live-cms-sessions.log` initial content wasn't specified. **Fixed:** Phase 3 Step 1 uses a header comment so the file is self-describing.

**Round 3 — interpretation drift + cross-task conflicts.** Findings + dispositions:
- F3.1: Phase 3 (README) and Phase 2 (binary) both reference `dev/live-cms-sessions.log` — separate phases, separate commits. No conflict, but the README references the binary's log-line shape; if Phase 2's shape changes, the README diverges. **Fixed:** the log-line shape is documented IDENTICALLY in three places (binary source comment, plan Phase 2 Step 4 source, README Phase 3 Step 2), and Phase 3 explicitly notes "shape matches Phase 2 source — update both if you change either."
- F3.2: The "Operator runs Phase 5" handoff was not separated from the impl agent's checklist. An agent reading top-to-bottom could be tempted to "complete" Phase 5 by running the binary. **Fixed:** Phase 5 is a separate phase with a header explicitly stating "performed by Cameron Zucker, not by any agent" + an "agent execution rule" reminder at the top.
- F3.3: The plan's checklist in the original spec spanned Tasks 1-6 as a single review-gate; this plan covers ONLY Task 6 (Wave-1 plan-writer scope). **Fixed:** the Cross-Phase Review Checklist is scoped to Phases 1-4 of THIS plan, not the original Tasks 1-6 multi-task gate. The latter is a Wave-2 / merge-time concern, not a per-task one.

**Round 4 — second-order effects (clean check).** Re-read end-to-end. Zero substantive findings. The CRLF-handling fix from R1 introduced one minor inconsistency: an early draft of `test_crlf_after_go_grants_consent` reused the `b"go\r\n"` literal without explaining the operator scenario. **Fixed (cleanup):** added inline comment "Operator on a CRLF terminal (rare on Linux but possible via paste)." to make the test's intent clear to the impl agent.

Total: 12 findings across 4 rounds, all fixed in-place. Round 4 is clean. The plan-review-cycle SKILL's completion criterion (`one round produces 0 findings`) is met.

---

## Recommended execution strategy

**Recommendation: subagent-driven-development** (one fresh subagent for Phases 1-4 as a single coherent dispatch), with **Phase 5 performed by the operator manually after the PR merges**.

Reasoning:

- Phases 1-4 are tightly sequential and share a small amount of context (the inlined `write_pat_config_for_smoke` helper, the `chrono` dep, the consent-gate types). A single subagent execution preserves this context with no inter-phase handoff cost.
- The whole plan is short (~3 commits + 1 PR) — not large enough to warrant phase-per-subagent fragmentation.
- The risk surface is heavily front-loaded (RADIO-1 boundary) and the per-phase agent-execution-rule blocks make the boundary explicit at every step. A subagent that obeys the universal preamble will succeed; one that ignores it would fail at any granularity.
- Phase 5 is operator-only by RADIO-1 and not delegatable; this is independent of execution strategy.

Alternative considered: **parallel agents** — rejected because Phases 1-4 are strictly sequential (Phase 2 depends on Phase 1's `lib.rs` edit; Phase 3 depends on nothing but is trivial; Phase 4 depends on Phases 1-3 being committed). No parallelism to exploit.

Alternative considered: **inline execution in the current orchestrator session** — rejected because the orchestrator should preserve context for Wave-2 coordination of the other 5 tasks; spending that context on Task 6's straightforward execution is wasteful. Dispatch a fresh subagent.

---

## Pitfalls Surfaced and Flagged Inline

| Pitfall ID | Where it applies | Disposition in plan |
|---|---|---|
| RADIO-1 | Every phase | Universal preamble agent execution rule; per-phase agent-execution-rule blocks; Phase 2 Step 6 structural assertion; Phase 5 is operator-only. |
| RADIO-2 | Phase 2 Step 4 (transport choice) | Documented inline: smoke uses telnet/IP (Part 15), not RF. RADIO-2 does not fire. |
| SCOPE-1 | Phase 2 (binary behavior) | Documented inline: binary connects OUT to CMS as a client; no listening, no gateway behavior. |
| HOOK-1 | Workflow preamble | Universal preamble: "Confirm you are running in the worktree". |
| LEASE-1 | n/a | Plan does not touch lease state. |
| PARITY-1 | n/a | Plan does not introduce a script that reads safety-stack state. |
| BD-1 | Workflow preamble | Standard worktree + bd-claim flow per CLAUDE.md `## Tool referee` table. |
| ORCH-1 | n/a (this plan does not dispatch parallel sub-subagents) | Phase 5 is sequential operator handoff, not parallel dispatch. |
| Testing-Pitfalls §3 (Error Path Coverage) | Phase 1 Step 1 | Tests cover aborted axis explicitly (uppercase, mixed-case, empty, lone newline, padded, alternatives). |
| Testing-Pitfalls §6 (Boundary & Configuration Validation) | Phase 1 Step 1 | CRLF + LF + lone-newline + EOF + whitespace boundaries all tested. |

---

## End of Plan
