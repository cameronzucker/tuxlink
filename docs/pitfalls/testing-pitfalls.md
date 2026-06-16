# Testing Pitfalls

Test scenario checklist for reviewing coverage of any feature. Every item on this list exists because it catches bugs that have occurred in real codebases. Items marked with **🔥 Found in this project** were discovered here specifically. Unmarked items are universal — bugs we haven't made *yet* in this project, but that have bitten other projects hard enough to be worth testing against. Do not deprioritize an unmarked item because it lacks a marker.

> **Relationship to implementation-pitfalls.md:** `implementation-pitfalls.md` specifies *what* to implement and *why*. This document specifies *how to verify* those implementations work correctly. Cross-references between the two are noted inline.

---

## How to Use This Document

**If you're writing tests:** Go to the relevant topic sections below, read the checklist items, and verify your test suite covers each one that applies. Unchecked items are gaps — either add a test or explicitly note why the item doesn't apply to this feature.

**If you're reviewing tests:** Use the checklist to audit coverage gaps. A passing test suite with missing coverage is worse than a failing test suite with complete coverage — you don't know what's actually protected.

**If you're maintaining this document:** When a real bug slips through to production or staging because of a missing test, add the check item to the appropriate section with the 🔥 marker and a one-line note about the observed failure mode. See §How to Add a Testing-Pitfall at the end.

---

## 1. Test Output Pristine

Test output MUST be clean for the suite to pass — no stray errors, warnings, or stack traces. If a test legitimately produces errors (e.g. it's verifying error handling), capture them explicitly and assert on their content. Silent error spam in test output hides real failures.

- [ ] **No unexpected stderr in passing tests.** Any stderr output from a passing test must be explicitly asserted on, or the test is lying about what it verifies.
- [ ] **No unhandled promise rejections / uncaught exceptions.** These often appear as warnings rather than test failures; configure your runner to fail on them.
- [ ] **Deprecation warnings fail the suite or are explicitly tracked.** Silently-warned deprecations become hard breaks on the next runtime upgrade.
- [ ] **Test output doesn't contain debug prints.** Debug statements that escaped into production tests are sometimes the only evidence of a half-finished implementation.

---

## 2. Skipped Tests Are Not Passing Tests

A test that's `skip`ped, `xit`'d, `pending`, or `@Ignore`d is a test that's not running. A CI job that says "100 tests passed, 5 skipped" is NOT the same as "105 tests passed."

- [ ] **No unexplained skips in the suite.** Every skipped test has a comment explaining why it's skipped and under what condition it should be re-enabled.
- [ ] **Skips with a linked issue/ticket.** A skip without follow-up context is forgotten work.
- [ ] **CI distinguishes skipped from passed in its summary.** If the report doesn't separate them, skipped failures hide.
- [ ] **Skip counts are tracked over time.** Growing skip count = eroding coverage.

---

## 3. Error Path Coverage

Silent error swallowing is one of the largest bug categories in any codebase. Every error path must be tested explicitly — not just "the happy path works."

- [ ] **Each error branch has a test that triggers it.** If a function has 5 ways to return an error, there are 5 tests covering each one.
- [ ] **Error messages are asserted, not just error presence.** `expect(err).toBeTruthy()` doesn't catch "wrong error returned"; `expect(err.message).toMatch(/expected pattern/)` does.
- [ ] **Information leakage via error codes checked.** When a handler must return the same status code regardless of whether a resource exists (anti-enumeration), test that ALL error paths return the same status — including DB errors on post-lookup queries that leak existence.
- [ ] **Error-path side effects verified.** If an error path is supposed to roll back state / release a lock / clear a cache, assert that it did.
- [ ] **Error-path resource cleanup verified.** Acquired resources (file handles, DB connections, semaphores) must be released even on error. Test with `defer`-equivalent patterns or explicit cleanup assertions.

---

## 4. Negative Property Testing

Happy-path tests prove "it works" for one input. Negative property tests prove "it doesn't break" under stress, boundaries, and adversarial input. The latter catches the bugs that ship.

- [ ] **Cleanup and eviction.** When code accumulates state (maps, caches, queues), test that stale entries are eventually cleaned up. Don't just test "it works" — test "it doesn't leak."
- [ ] **Bounded growth.** For any in-memory data structure that grows with external input, test that it has a maximum size or eviction policy. Simulate 1000+ entries and verify memory is bounded.
- [ ] **Case sensitivity where identity matters.** When a string key is used for identity (email, username, path), test that case variations are treated consistently. `Admin@Example.com` and `admin@example.com` must be the same identity — or consistently different ones.
- [ ] **Empty / null / zero inputs.** Every parameter that accepts a value should be tested with empty string, null, zero, empty array, empty map. "Did not crash" is not the same as "handled correctly."
- [ ] **Oversized inputs.** Long strings, deeply nested structures, large collections. Where are your truncation / rejection boundaries, and are they enforced?
- [ ] **Unicode / encoding edge cases.** Multi-byte chars, combining sequences, RTL text, emoji, zero-width joiners, NUL bytes. Anywhere strings cross a boundary (storage, display, comparison) needs this.

---

## 5. Concurrency & TOCTOU

If the code can be executed concurrently, test it concurrently. Single-threaded happy-path tests don't catch race conditions.

- [ ] **Multi-step flows under concurrent access.** When a flow reads state then writes state (check-then-act), test two callers racing through the same flow simultaneously. Use a barrier / sync primitive to ensure they hit the critical section at the same time — `WaitGroup` / `Promise.all` alone doesn't guarantee simultaneity.
- [ ] **"Use once" tokens consumed correctly.** Any token that should be single-use (password reset, verification code, invitation) must be tested with two concurrent consumers. Exactly one must succeed.
- [ ] **Rate-limit enforcement under concurrency.** Count-then-insert rate limits can be bypassed by concurrent requests that all read the same count before any insert. Test with burst requests.
- [ ] **Idempotency under retry/concurrency.** If an operation should be idempotent (accepting an invitation twice, retrying a failed payment), test concurrent execution — the second attempt must not produce a 500 from a constraint violation.
- [ ] **Bootstrap / first-time races.** First-user, first-org, or any "only if none exist" flow tested with concurrent attempts. Exactly one must win.

---

## 6. Boundary & Configuration Validation

Configuration errors, bad boundaries, and missing validation are a surprisingly large portion of production incidents. Test the edges.

- [ ] **Default values are tested.** What does the code do when a config value is absent? Crash? Use a default? Silently use zero? All three are possible; the right behavior needs a test.
- [ ] **Invalid config is rejected at load time.** A system that loads invalid config, then crashes on first use of it, surfaces the error too late. Test that config validation runs at load.
- [ ] **Environment-specific behavior.** If code behaves differently in dev vs. prod (feature flags, degraded modes), test both paths. Don't assume dev-tested code works in prod.
- [ ] **Feature flag flip behavior.** Test both flag-on and flag-off paths. A feature behind a flag that's never tested with the flag off can't be safely rolled back.
- [ ] **Timeout and retry boundaries.** If a caller retries 3 times with 5s timeouts, test what happens on the 4th call and on a request that takes 4.9s. The edges matter.

---

## 7. Test Infrastructure Hygiene

The test suite itself is code. It decays if not maintained. Messy test infrastructure produces flaky tests, which produce lost confidence, which produce skipped tests (see §2).

- [ ] **No shared mutable state between tests.** Each test should set up its own state and tear it down. Tests that depend on previous tests' state are order-dependent and flaky.
- [ ] **Setup / teardown covers the failure case.** If setup partially succeeds then teardown fails, the next test starts from a corrupted state. Teardown must be robust to partial-setup states.
- [ ] **Test doubles are minimal and honest.** A mock that returns fixed data is testing the mock, not the code. Use real implementations where feasible; mock only external boundaries.
- [ ] **No hardcoded time-of-day or timezone assumptions.** Tests that pass at 09:00 UTC but fail at 23:00 UTC are flaky by design. Use injected clocks for time-sensitive tests.
- [ ] **No network calls in unit tests.** A unit test that hits a real API is an integration test with a misleading name. Either mock the boundary or move it to the integration suite.
- [ ] **🔥 Found in tuxlink-k61j: a contract that crosses N layers needs a test that crosses all N at once — per-layer fakes hide composed-seam bugs.** The LAN-tiles TMS Y-flip and the offline-cache regression both lived in the *seam* between layers, and every test exercised one side with an injected fake that severed the other side: `fetch.rs::build_tile_url` was tested with a hand-built `TileCoord` (so it never saw Leaflet's prior `tms`-option Y-flip — the webview flips Y *then* the backend flips it *again*, double-flipping every TMS tile); `serve_tile_with` was tested with an injected fetch closure (so it never composed the real breaker-vs-cache ordering, where the breaker short-circuits *before* the cache read and a degraded source stops serving on-disk cached tiles). Each side's unit test was green; the *composed* contract was broken end-to-end across ~5 remediation attempts. Pass criterion: for any contract that spans a serialization/IPC/URL boundary (here, the `tile://localhost/{z}/{x}/{y}` boundary), at least one test drives the **real wire shape** through the **real composed chain** — feed a `{z}/{x}/{y}` *string* (as the webview emits *post*-Leaflet-flip) into `serve_tile` against a mock upstream, for BOTH `Xyz` and `Tms`, and assert the path the upstream actually receives. A per-layer fake that bypasses the adjacent layer is "testing the mock, not the code" (see the bullet above).
- [ ] **🔥 Found in tuxlink-cnd: real-keyring tests run in a throwaway HOME, asserted, never the operator's login keyring.** gnome-keyring stores secrets under `$XDG_DATA_HOME` / `$HOME/.local/share/keyrings`. `dbus-run-session` isolates only the D-Bus *bus*, NOT those on-disk files, and isolating `XDG_CONFIG_HOME` (for `config.json`) does NOT cover the keyring. On 2026-05-20 a setup one-liner that ran `gnome-keyring-daemon --unlock` against the real keyring re-keyed the operator's login keyring irrecoverably — breaking secret access for tuxlink AND geographica (shared login keyring) until a reset. The real-keyring tests live in `src-tauri/tests/wizard_integration_test.rs` (`#[ignore]`d) and each now calls `assert_keyring_isolated()` first, which **fails the test closed** unless the resolved keyring dir is under the system temp dir — so a mis-invoked run aborts BEFORE any write. The load-bearing safety is the **ephemeral `HOME` set *before* `dbus-run-session`** (a freshly-activated daemon inherits it and opens a temp keyring); the daemon incantation is convenience, the assert is the backstop. Safe headless recipe:
  ```bash
  # Run the #[ignore]d real-keyring tests WITHOUT touching your login keyring.
  SANDBOX="$(mktemp -d)"                       # lands under $TMPDIR/tmp → assert passes
  HOME="$SANDBOX" \
  XDG_DATA_HOME="$SANDBOX/.local/share" \
  XDG_CONFIG_HOME="$SANDBOX/.config" \
  dbus-run-session -- bash -c '
    # Empty-password unlock creates a NEW empty keyring in the sandbox HOME —
    # never the operator password, never the real keyring.
    eval "$(printf "\n" | gnome-keyring-daemon --unlock --components=secrets)"
    export GNOME_KEYRING_CONTROL SSH_AUTH_SOCK
    cargo test --manifest-path src-tauri/Cargo.toml \
      --test wizard_integration_test --ignored
  '
  rm -rf "$SANDBOX"
  ```
  Pass criterion: tests run green and `assert_keyring_isolated()` did not abort. **NEVER** run `gnome-keyring-daemon --unlock` against your real `$HOME`. The safe (non-`#[ignore]`d) unit test `keyring_isolation_guard_detects_sandbox_vs_real_home` runs in normal `cargo test`/CI and regression-guards the assert in both directions.

- [ ] **🔥 Found in tuxlink-hoi1: absent-field-erases / multi-writer clobber — a full-replace persist where one writer omits a field (or holds a stale snapshot) erases stored data, yet every per-writer unit test passes.** `packet_config_set` full-replaced the whole `[packet]` config section from a flat DTO, and a DTO with no `link_kind` mapped to `link: None` — so any SSID/timing-only write silently wiped the configured radio link. Each writer's isolated unit test passed (they all sent a DTO that *included* `link_kind`); the destruction lived in the *composition* of one writer omitting a field that another writer had set. The class has two faces, both needing tests: (1) **absent-field-erases** — persist a DTO that omits field X against a record that *has* X, and assert X survives on disk (Rust: seed `packet.link = Some(UvproNative{mac})`, call the setter with `link_kind: None`, assert the link is still `Some(...)`). (2) **stale-snapshot clobber** — mount two writers of the same record, have writer A set field X, then have writer B (holding a pre-A snapshot, or omitting X) persist, and assert X survives. A corollary bites the *fix*: an optimistic-update **rollback** is itself a writer — when an older write's rejection rolls back to its captured snapshot, a *newer* successful write must not be reverted. Test it: older `setLink` rejects *after* a newer `setSsid` succeeds; assert the newer value survives (it failed without the guard — the rollback reverted to the stale pre-write snapshot). Pairs with the "test the production mount path, not just the unit" lesson and the §7 composed-seam entry: per-writer green ≠ composed-contract green.

---

## 8. Plan & Documentation Discipline (DRIFT-1 verification)

Pairs with [implementation-pitfalls.md §3](implementation-pitfalls.md#3-plan-and-documentation-discipline) (DRIFT-1: plan-text AMENDMENT does not auto-cascade to the code it amends). These recipes are runnable checks a PR reviewer or CI job can execute to verify the discipline holds at amendment time, not lazily-at-impl-time.

The implementation pitfall says: "Every AMD MUST ship with a paired bd issue if the prior task is shipped. Two acceptable forms: code-bearing (cite the bd issue) OR prose-only (state explicitly)." These recipes verify that contract.

- [ ] **Every AMD marker is either bd-cited or marked prose-only.** Recipe:
  ```bash
  # List every AMENDMENT marker location across all plan files.
  grep -nE "AMENDMENT 20[0-9]{2}-[0-9]{2}-[0-9]{2} \(AMD-[0-9]+" docs/plans/*.md
  # For each line, manually verify the surrounding paragraph either
  # (a) cites a bd issue ID matching tuxlink-XXX, OR
  # (b) contains the literal phrase "prose-only" with a no-code-impact rationale.
  ```
  Pass criterion: every marker matches one of the two patterns. A marker with no cite + no prose-only tag is a DRIFT-1 violation — file the missing bd issue or add the prose-only tag.

- [ ] **AMD-N count is consistent across plan + bd issue tracker.** Recipe:
  ```bash
  # Set of AMD numbers referenced in plan documents.
  grep -hoE "AMD-[0-9]+" docs/plans/*.md | sort -u > /tmp/plan_amds.txt
  # Set of AMD numbers referenced in bd issue bodies.
  bd list --json 2>/dev/null | grep -oE "AMD-[0-9]+" | sort -u > /tmp/bd_amds.txt
  # Any AMD in plans that is NOT referenced by any bd issue is suspicious —
  # verify it's prose-only (acceptable) or file the missing tracking issue.
  comm -23 /tmp/plan_amds.txt /tmp/bd_amds.txt
  ```
  Pass criterion: the `comm -23` diff lists ONLY AMDs explicitly tagged prose-only in the plan body. Any unexplained entry is a gap.

- [ ] **No `SUPERSEDED / supersede` wording without a paired AMD marker.** Recipe:
  ```bash
  # Find supersede-wording paragraphs in plan files.
  grep -nE "SUPERSED|supersed" docs/plans/*.md
  # Verify each is inside an AMENDMENT block — superseding existing plan
  # text without an AMD marker means the plan body is silently rewritten
  # and the audit trail is lost.
  ```
  Pass criterion: every supersede-wording match is within ~10 lines of an `AMENDMENT 20XX-MM-DD (AMD-N)` marker. Loose superseding text is a DRIFT-1-adjacent violation: even prose-only supersedes should be tagged.

- [ ] **PR description names the amended task + every consumer bd-issue.** Manual review — when reviewing a PR that lands a plan AMD, search the PR description for `AMD-N` and verify:
  1. The PR body lists every paired bd issue OR states "prose-only; no code impact."
  2. If the AMD changes a function signature / config field / API surface of a shipped task, the PR description identifies every downstream consumer (via `grep -r '<symbol>'`) and confirms each has a paired bd issue.

  Pass criterion: a PR landing an AMD that updates a code-bearing contract WITHOUT a consumer audit is the exact failure mode wizard-cluster R1 caught. Reviewer rejects until the audit ships.

- [ ] **Pipeline triage matches `feedback_discipline_triage_rule`.** Manual review — when a downstream bd-issue ships impl code in response to an AMD, verify the pipeline matches the memory criteria:
  - **Full upstream pipeline** (brainstorm → 5-round adrev → spec → plan-review → TDD) for hard-to-undo architectural decisions (trait shapes, error model design, cross-cutting refactors).
  - **TDD-direct against the spec** for plumbing where the bd-issue body IS the spec (config refactors, helpers, render functions, mechanical wire-up).

  Pass criterion: the PR body justifies its pipeline choice with a one-liner ("plumbing-class per discipline-triage-rule; bd issue IS spec" or "architectural per discipline-triage-rule; full pipeline + Codex round"). See feedback memory `feedback_discipline_triage_rule`.

---

## 9. Rendering & Performance Forecasts

Performance numbers are only as representative as the scene they measure. A fps figure from a stripped-down harness predicts nothing about a production scene with real assets, markers, and compositing — and a mocked harness silently strips exactly the work that dominates render cost.

- [ ] **🔥 MAP-PERF-1 — A perf forecast from the mocked render-harness is not an app-level prediction.** The front-end render-harness (`dev/render-harness/`) uses canned Tauri data and a trivial scene (no real tile decode, no markers, no pack compositing); its fps number MUST NOT gate a perf-sensitive map ship. Map perf claims require the on-Pi frame-timing smoke (`dev/perf-harness/`) with a region pack + station pins + the Maidenhead grid mounted at real resolution under software GL (llvmpipe), measuring p50/p95 frame time during scripted pan/zoom. (Found in tuxlink-vnk7, 2026-06-15: the ~45 fps "forecast" was measured on the front-end-only render-harness, so it never predicted real app perf — MapLibre GL JS forced through software GL on the Pi is a different fill-rate problem.)

---

## How to Add a Testing-Pitfall

When a bug reaches production (or staging, or late integration testing) because a test was missing:

1. **Identify the topic section** the missing test belongs in. If none of sections 1-7 fit, add a new numbered topic section.
2. **Write the check item** as a `- [ ]` checkbox. Lead with a bolded imperative ("**X is tested.**"), then one sentence explaining what the check covers and why.
3. **Mark with the 🔥 marker** if the bug was found in this project's own history: `**🔥 Found in [context]:** one-line note about the observed failure mode`.
4. **Cross-reference implementation-pitfalls.md** if there's a corresponding implementation entry.
5. **Resist the urge to be clever.** "Tests X under condition Y" is better than a novel testing philosophy. These are pass/fail checklist items, not essays.

The test suite is the enforcement mechanism for this document. If you add a check item and don't write the corresponding test, you've documented a gap, not closed one. Close it.
