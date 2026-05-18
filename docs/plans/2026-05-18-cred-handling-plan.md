# Cred-handling Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor `tuxlink-pat` so Pat reads WL2K passwords from the OS keyring (via `zalando/go-keyring`) instead of from `config.json`. Wizard is the sole writer (separate task, `tuxlink-ko0`). Pat-CLI configure paths emit a brief redirect; promptHub remains the graceful-degradation fallback for keyring miss/error.

**Architecture:** Two-PR landing across two repos. PR-A: the cred-handling refactor itself on `cameronzucker/tuxlink-pat/master` (new `internal/credstore` package, callback rewrite in `app/exchange.go`, web UI form removal, `cli/init.go` redirect, config-struct field deletion). PR-B: the submodule pin bump on `cameronzucker/tuxlink/feat/v0.0.1` referencing the merged PR-A commit. v0.0.1 supports Linux only (tested + supported); credstore code compiles on macOS/Windows via library API but those platforms are untested in v0.0.1.

**Tech Stack:** Go (with `github.com/zalando/go-keyring` v0.x), Pat's existing `fbb.Address` + `promptHub` primitives, Pat's web UI (jQuery + HTML), CI via GitHub Actions on `ubuntu-22.04` runner with `dbus-run-session`/`gnome-keyring-daemon` for integration tests.

**Spec of record:** `docs/superpowers/specs/2026-05-18-cred-handling-design.md` (commit `68a698c` on `bd-tuxlink-mib/mib-cred-keyring`; post-5-round-adrev revision).

---

<!--
LIVING DOCUMENT CONTRACT — DO NOT PARAPHRASE.

The block below is verbatim from writing-plans-enhanced SKILL.md Step 5.
Future editors of this plan: if you find yourself wanting to soften,
shorten, or paraphrase any line in the block, STOP. Strict verbatim is
load-bearing — `plan-review-cycle` rounds check the block against the
SKILL.md for exact-string match. If the SKILL.md itself evolves, update
this block to match (and document the upstream change in §"Discoveries"
at the top of the plan).
-->

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

**Overall:** 0/10 phases shipped; pre-execution.

| Phase | Status | Ship SHA(s) | Notes |
|---|---|---|---|
| 1 — Setup: tuxlink-pat worktree + branch + go.mod | ⬜ Not started | — | Cross-repo setup; new branch on tuxlink-pat repo |
| 2 — credstore package (NEW; pure TDD) | ⬜ Not started | — | Densest TDD phase; `internal/credstore/` |
| 3 — cfg/config.go modifications | ⬜ Not started | — | Drop SecureLoginPassword; AuxAddr keep MarshalJSON, drop Password field |
| 4 — api/api.go RedactedPassword removal | ⬜ Not started | — | Verify no other consumers via grep before delete |
| 5 — app/exchange.go callback + app/app.go cleanup | ⬜ Not started | — | Largest behavioral change; SMTP-proto skip + callback rewrite |
| 6 — API handlers + cli/account.go (credstore-explicit-handling) | ⬜ Not started | — | api/winlink_account.go + app/winlink_api.go + cli/account.go |
| 7 — cli/init.go both password paths redirected | ⬜ Not started | — | Existing-account + new-account flows both → brief redirect |
| 8 — Web UI updates (web/src/* + web/dist/* rebuild) | ⬜ Not started | — | Browser-side form removal + JS cleanup + dist rebuild |
| 9 — README + CI integration test + PR-A open | ⬜ Not started | — | tuxlink-pat README "Credentials" section + .github/workflows/ + open PR-A |
| 10 — PR-A merge + PR-B submodule bump on tuxlink | ⬜ Not started | — | After PR-A merge: bump submodule pin in tuxlink; open PR-B against feat/v0.0.1; close `tuxlink-mib` on PR-B merge |

### Deviations

(None yet; populate per LDC as discovered.)

### Discoveries

(None yet; populate per LDC as discovered.)

---

## Execution strategy recommendation

Per `writing-plans-enhanced` Step 2 — recommend an execution approach with reasoning:

**Recommended: `superpowers:subagent-driven-development`** — fresh subagents per phase (or per logical pair). Phases 1, 2, 9 (PR-A open), 10 (PR-B submodule bump) are natural single-dispatch boundaries. Phases 3-8 are tightly-coupled refactor steps that share Pat's internal types; they could land as one subagent dispatch or split per phase depending on token budget.

Reasoning:
- Phase 2 (credstore TDD) is a bounded vertical slice with rich test coverage — fits fresh-subagent dispatch perfectly.
- Phases 3-8 share a single Go module surface (tuxlink-pat); a single subagent can land them in one push without cross-phase coordination overhead, OR phases can be split per-phase for cleaner review checkpoints if context allows. Per-phase recommended for clarity; bundled OK if subagent is mid-context.
- Phase 9 ships PR-A; that's a natural review checkpoint (Cameron merges before Phase 10 starts).
- Phase 10 is a tiny follow-up (submodule pin bump on tuxlink repo + PR open) — single subagent, ~10 minutes of work.

**Subagent LDC scoping** (per `feedback_subagent_ldc_scoping` memory): every subagent dispatch for an LDC-bearing phase MUST be explicitly authorized to update the plan's banner per the LDC contract above. Default "don't modify the plan file" instructions BLOCK the LDC discipline; scope the "don't modify" constraint to source files, not the plan.

**Post-subagent Codex round** (per `feedback_codex_post_subagent_review` memory): after each subagent dispatch that ships committed work, the parent agent runs `codex review --commit <subagent-SHA>` and triages findings before declaring the dispatch complete.

Alternative considered + rejected:
- `superpowers:executing-plans` (inline): would batch all phases in one session. Workable for the small phases (3-8) but loses review checkpoints. Reject unless context window forces inline.
- Single-subagent-for-all-phases: tempting for context-efficiency but loses the natural review checkpoints and makes any failure rollback messier. Reject.

---

## File structure

Pre-execution inventory of files this plan creates/modifies. Decomposition decisions locked here.

**In `cameronzucker/tuxlink-pat` (the fork; PR-A target):**

- Create: `internal/credstore/credstore.go` (~80-120 LoC; pkg-level Go module wrapping zalando/go-keyring; exports `ServiceName`, `NormalizeAccount`, `Get`, `ErrLocked`, `ErrUnavailable`)
- Create: `internal/credstore/credstore_test.go` (~150-200 LoC; unit tests via `keyring.MockInit()`, serialized — NO `t.Parallel()`)
- Create: `internal/credstore/credstore_integration_test.go` (~80-120 LoC; build-tagged `//go:build integration`)
- Modify: `cfg/config.go` (delete `SecureLoginPassword` line ~53; drop `AuxAddr.Password` line ~23; PRESERVE MarshalJSON/UnmarshalJSON but strip colon-suffix)
- Modify: `cfg/config_test.go` (new test `TestConfigParse_LegacyAuxAddrPasswordStripped`)
- Modify: `app/exchange.go` (callback at lines 175-192 rewritten: SMTP-proto skip + normalize + credstore lookup + promptHub fallback; NO AuxAddr-fallback-to-primary)
- Modify: `app/app.go` (delete lines 230-233)
- Modify: `app/winlink_api.go` (line 72 explicit credstore handling)
- Modify: `api/api.go` (delete `RedactedPassword` const at line 404 + use at 414-416, 435-436)
- Modify: `api/winlink_account.go` (line 65 explicit credstore handling)
- Modify: `cli/init.go` (line 60 handleNewAccount call → redirect; lines 193-258 → redirect; KEEP handleNewAccount + promptNewPassword functions in source as dead code with TODO comment)
- Modify: `cli/account.go` (line 39+ getPasswordForCallsign uses credstore)
- Modify: `web/src/config.html` (remove `<input id="secure_login_password">` block at lines 82-85; add info block)
- Modify: `web/src/js/config.js` (remove `secure_login_password` references at lines 176, 190, 347, 349, 354, 518)
- Modify: `web/dist/*` (rebuild prebuilt assets from web/src/ via Pat's existing build chain)
- Modify: `README.md` (add "## Credentials" section)
- Modify: `go.mod` + `go.sum` (add `github.com/zalando/go-keyring`)
- Create or Modify: `.github/workflows/test.yml` (or existing equivalent; add integration-test job with `dbus-run-session` wrapping; pin `ubuntu-22.04`)

**In `cameronzucker/tuxlink` (this repo; PR-B target after PR-A merges):**

- Modify: `external/tuxlink-pat` submodule pin (update to PR-A's merge SHA)

**Files NOT touched (intentionally):**

- `cli/prompter.go` — `case app.PromptKindPassword` is the terminal-prompt handler; consumer of promptHub events; unchanged
- `cli/connect.go`, `cli/listen.go` — secure-login flows through `app/exchange.go` callback; no per-command cred handling change needed
- `app/account_activation.go` — uses promptHub for activation; no direct password access
- All P2P / mailbox / message-format code — never reads passwords
- `internal/cmsapi/*.go` test files — network-mocked unit tests; the password parameter is passed by callers; CMS-side tests don't read keyring directly

---

## Phase 1 — Setup: tuxlink-pat worktree + branch + go.mod

**Execution Status:** ⬜ Not started

**Pre-requisite:** `tuxlink-pat` fork exists at `https://github.com/cameronzucker/tuxlink-pat` (shipped via fork-setup PR #1). Verify via `gh api repos/cameronzucker/tuxlink-pat --jq '.full_name'`.

This phase clones the tuxlink-pat fork, creates a per-patch branch following the fork's branch convention, and adds the zalando/go-keyring dep to go.mod. All subsequent phases (2-9) operate inside this clone.

### Task 1.1: Clone tuxlink-pat fork to a working directory

**Files:** none (filesystem operation).

**Background:** Per fork-setup spec §3.3 step 2, fork patches branch off `master` on the tuxlink-pat repo (different from tuxlink's `feat/v0.0.1`). Per ADR 0008, the worktree pattern doesn't apply here because tuxlink-pat is a separate git repo (no shared lease with tuxlink). Standard clone + branch.

- [ ] **Step 1: Clone fresh to `~/Code/tuxlink-pat/` (or operator-chosen path):**

```bash
cd ~/Code
git clone https://github.com/cameronzucker/tuxlink-pat.git
cd tuxlink-pat
git remote add upstream https://github.com/la5nta/pat.git
git fetch upstream
```

Expected output:
- Clone completes; ~30MB repo on disk
- `git remote -v` shows both `origin` (tuxlink-pat) and `upstream` (la5nta/pat)

- [ ] **Step 2: Verify upstream is at the same default branch as fork's master fork point:**

```bash
gh api repos/la5nta/pat --jq '.default_branch'
git -C ~/Code/tuxlink-pat log --oneline -5
```

Expected: `master` (upstream default) matches the fork's branch name. If upstream has migrated to `main`, this plan's branch names need updating; STOP and escalate.

### Task 1.2: Create per-patch branch off master

**Files:** none (git operation).

- [ ] **Step 1: Opportunistic upstream sync** (per fork-setup spec §3.3 step 4):

```bash
cd ~/Code/tuxlink-pat
git checkout master
git merge upstream/master
# If conflicts: resolve as part of this patch's design. For this patch's first
# opportunistic sync (since fork creation), expect zero conflicts.
```

Expected: `Already up to date.` or a clean merge commit. If conflicts: this plan's tasks may need re-targeting against the new merged state; STOP + escalate.

- [ ] **Step 2: Create branch:**

```bash
git checkout -b bd-tuxlink-mib/cred-keyring origin/master
```

Expected: `Switched to a new branch 'bd-tuxlink-mib/cred-keyring'`.

### Task 1.3: Add `github.com/zalando/go-keyring` to go.mod

**Files:**
- Modify: `~/Code/tuxlink-pat/go.mod`
- Modify: `~/Code/tuxlink-pat/go.sum`

**Background:** zalando/go-keyring is the chosen Go keyring library (per spec §4.5; pure-Go on Linux secret-service, security CLI wrapper on macOS, wincred on Windows). License: MIT (compatible with Pat's MIT).

- [ ] **Step 1: Add the dep:**

```bash
cd ~/Code/tuxlink-pat
go get github.com/zalando/go-keyring@latest
go mod tidy
```

Expected: `go.mod` gains a line `github.com/zalando/go-keyring vX.Y.Z`; `go.sum` updated with hashes; small number of transitive deps added.

- [ ] **Step 2: Verify the version is pinned and recent:**

```bash
grep "go-keyring" go.mod
```

Expected: one line in the `require` block. Record the exact version in the plan's Discoveries subsection for traceability.

- [ ] **Step 3: Verify no other changes:**

```bash
git status --short
```

Expected: only `go.mod` and `go.sum` modified.

- [ ] **Step 4: Commit:**

```bash
git add go.mod go.sum
git commit -m "$(cat <<'EOF'
build(deps): add github.com/zalando/go-keyring for credstore package

Pure-Go Linux secret-service backend; security CLI wrapper for macOS
Keychain; wincred for Windows. MIT license. Will back internal/credstore
package added in next commit per spec §3.2 + §4.5.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

Expected: Commit created on `bd-tuxlink-mib/cred-keyring`. Replace `<YOUR-MONIKER>` with the executing agent's session moniker.

---

## Phase 2 — credstore package (NEW; pure TDD)

**Execution Status:** ⬜ Not started

This phase implements the `internal/credstore` package via strict TDD. Each test is written first, run to confirm failure, then implementation makes it pass. The package's contract is the spec's §3.2 credstore.go row + §3.5 error-classification rules.

**Test scope:** Layer 1 (unit tests via `keyring.MockInit()`) per spec §3.6. Layer 2 (integration tests with real D-Bus) lands in Phase 9 alongside CI workflow.

**Critical:** all tests serialize (NOT `t.Parallel()`) because `keyring.MockInit()` mutates package-global state. Each test uses `t.Cleanup(func() { keyring.DeleteAll(ServiceName) })`.

### Task 2.1: Write failing test `TestServiceConstant`

**Files:**
- Create: `~/Code/tuxlink-pat/internal/credstore/credstore_test.go`

- [ ] **Step 1: Create the test file with the failing test:**

```go
package credstore_test

import (
	"testing"

	"github.com/la5nta/pat/internal/credstore"
)

func TestServiceConstant(t *testing.T) {
	if credstore.ServiceName != "tuxlink-pat" {
		t.Errorf("ServiceName = %q, want %q", credstore.ServiceName, "tuxlink-pat")
	}
}
```

- [ ] **Step 2: Run test to verify it fails (package doesn't exist yet):**

```bash
cd ~/Code/tuxlink-pat
go test ./internal/credstore/...
```

Expected: `package github.com/la5nta/pat/internal/credstore: no Go files in ...` or similar — the package doesn't exist.

### Task 2.2: Create `credstore.go` with `ServiceName` constant

**Files:**
- Create: `~/Code/tuxlink-pat/internal/credstore/credstore.go`

- [ ] **Step 1: Write the minimal implementation:**

```go
// Package credstore wraps github.com/zalando/go-keyring with tuxlink-pat-specific
// conventions for the WL2K password lookup path. See
// docs/superpowers/specs/2026-05-18-cred-handling-design.md in the tuxlink repo
// for full design rationale.
package credstore

// ServiceName is the OS-keyring service-string under which tuxlink-pat stores
// its WL2K credentials. Convention per spec §4.2: hardcoded fork-namespaced
// service + per-callsign account (matches GitHub CLI / aws-vault / HashiCorp
// Vault prior-art).
const ServiceName = "tuxlink-pat"
```

- [ ] **Step 2: Run test to verify it passes:**

```bash
go test ./internal/credstore/... -run TestServiceConstant -v
```

Expected: `PASS: TestServiceConstant`.

### Task 2.3: Write failing tests `TestNormalizeAccount` (table-driven)

**Files:**
- Modify: `~/Code/tuxlink-pat/internal/credstore/credstore_test.go`

- [ ] **Step 1: Add the table-driven test:**

```go
func TestNormalizeAccount(t *testing.T) {
	cases := []struct {
		name     string
		input    string
		wantOut  string
		wantOk   bool
	}{
		{"bare callsign", "KK6XYZ", "KK6XYZ", true},
		{"lowercase normalized", "kk6xyz", "KK6XYZ", true},
		{"leading whitespace trimmed", "  KK6XYZ", "KK6XYZ", true},
		{"trailing whitespace trimmed", "KK6XYZ  ", "KK6XYZ", true},
		{"mixed whitespace + lowercase", "  kk6xyz  ", "KK6XYZ", true},
		{"empty string rejected", "", "", false},
		{"whitespace-only rejected", "   ", "", false},
		{"tab-only rejected", "\t\t", "", false},
		{"newline-only rejected", "\n", "", false},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			out, ok := credstore.NormalizeAccount(tc.input)
			if out != tc.wantOut || ok != tc.wantOk {
				t.Errorf("NormalizeAccount(%q) = (%q, %v), want (%q, %v)",
					tc.input, out, ok, tc.wantOut, tc.wantOk)
			}
		})
	}
}
```

- [ ] **Step 2: Run test to verify it fails (function doesn't exist yet):**

```bash
go test ./internal/credstore/... -run TestNormalizeAccount -v
```

Expected: compile error — `undefined: credstore.NormalizeAccount`.

### Task 2.4: Implement `NormalizeAccount`

**Files:**
- Modify: `~/Code/tuxlink-pat/internal/credstore/credstore.go`

- [ ] **Step 1: Add the function:**

```go
import "strings"

// NormalizeAccount returns the canonical keyring account string for a callsign:
// strings.ToUpper(strings.TrimSpace(callsign)). Returns ("", false) for empty
// or whitespace-only inputs — callers MUST treat (false) as "no lookup;
// fall through to prompt or error per call-site rules" per spec §3.3.
//
// Both the writer (tuxlink wizard, per tuxlink-ko0) and the reader (this
// package) MUST apply this normalization to avoid silent miss caused by
// case differences (R2 F1 + R3 F1 + R4 P2 — convergent adrev finding).
func NormalizeAccount(callsign string) (string, bool) {
	trimmed := strings.TrimSpace(callsign)
	if trimmed == "" {
		return "", false
	}
	return strings.ToUpper(trimmed), true
}
```

Note: the `strings` import is added to the existing import block; if no other imports exist, this becomes the first import.

- [ ] **Step 2: Run test to verify it passes:**

```bash
go test ./internal/credstore/... -run TestNormalizeAccount -v
```

Expected: `PASS: TestNormalizeAccount` with all 9 subtests passing.

### Task 2.5: Write failing tests `TestGet_Hit`, `TestGet_Miss`, `TestGet_NotFoundIsMiss`

**Files:**
- Modify: `~/Code/tuxlink-pat/internal/credstore/credstore_test.go`

- [ ] **Step 1: Add the tests + MockInit setup:**

```go
import (
	// ... existing imports
	"github.com/zalando/go-keyring"
)

// setupMock initializes the keyring mock and registers cleanup. Tests call this
// FIRST, then optionally Set entries via keyring.Set directly.
//
// IMPORTANT: keyring.MockInit() mutates package-global state. Tests MUST NOT
// call t.Parallel() (per spec §3.6). Cleanup uses keyring.DeleteAll to avoid
// cross-test pollution.
func setupMock(t *testing.T) {
	t.Helper()
	keyring.MockInit()
	t.Cleanup(func() { _ = keyring.DeleteAll(credstore.ServiceName) })
}

func TestGet_Hit(t *testing.T) {
	setupMock(t)
	if err := keyring.Set(credstore.ServiceName, "KK6XYZ", "secretpw"); err != nil {
		t.Fatalf("Set: %v", err)
	}
	pw, found, err := credstore.Get("KK6XYZ")
	if err != nil {
		t.Fatalf("Get err: %v", err)
	}
	if !found {
		t.Errorf("Get: found=false, want true")
	}
	if pw != "secretpw" {
		t.Errorf("Get: pw=%q, want %q", pw, "secretpw")
	}
}

func TestGet_Miss(t *testing.T) {
	setupMock(t)
	pw, found, err := credstore.Get("UNSETCALLSIGN")
	if err != nil {
		t.Errorf("Get err: %v, want nil", err)
	}
	if found {
		t.Errorf("Get: found=true, want false")
	}
	if pw != "" {
		t.Errorf("Get: pw=%q, want empty", pw)
	}
}

func TestGet_NotFoundIsMiss(t *testing.T) {
	// This is a regression test for the ErrNotFound mapping per spec §3.2.
	// Same behavior as TestGet_Miss but explicit about the sentinel mapping.
	setupMock(t)
	pw, found, err := credstore.Get("NEVERSET")
	if err != nil {
		t.Errorf("Get err: %v, want nil (ErrNotFound should map to found=false, err=nil)", err)
	}
	if found {
		t.Errorf("Get: found=true, want false")
	}
	_ = pw
}
```

- [ ] **Step 2: Run tests to verify they fail (Get doesn't exist yet):**

```bash
go test ./internal/credstore/... -run TestGet -v
```

Expected: compile error — `undefined: credstore.Get`.

### Task 2.6: Implement `Get` with basic hit/miss/error mapping

**Files:**
- Modify: `~/Code/tuxlink-pat/internal/credstore/credstore.go`

- [ ] **Step 1: Add the function (with imports):**

```go
import (
	"errors"
	"strings"

	"github.com/zalando/go-keyring"
)

// Get looks up the WL2K password for the given callsign in the OS keyring.
//
// Returns:
//   - (pw, true, nil) on hit with a non-empty stored password.
//   - ("", false, nil) on:
//     - empty/whitespace-only callsign (no backend call; short-circuit)
//     - keyring entry not found (keyring.ErrNotFound mapped to clean miss)
//     - empty-string stored password (treated as miss per spec §3.2 / R3 F4)
//   - ("", false, ErrLocked) when the keyring is locked (operator needs to unlock).
//   - ("", false, ErrUnavailable) when D-Bus is unreachable or secret-service
//     is not installed.
//   - ("", false, <other err>) for unclassified errors (caller logs + falls
//     through per per-call-site rules in spec §3.5).
func Get(callsign string) (string, bool, error) {
	account, ok := NormalizeAccount(callsign)
	if !ok {
		return "", false, nil
	}
	pw, err := keyring.Get(ServiceName, account)
	if err != nil {
		if errors.Is(err, keyring.ErrNotFound) {
			return "", false, nil
		}
		return "", false, classifyErr(err)
	}
	if pw == "" {
		return "", false, nil
	}
	return pw, true, nil
}

// classifyErr stub for now; full classification added in Task 2.13.
func classifyErr(err error) error { return err }
```

- [ ] **Step 2: Run tests to verify they pass:**

```bash
go test ./internal/credstore/... -run TestGet -v
```

Expected: `PASS` for `TestGet_Hit`, `TestGet_Miss`, `TestGet_NotFoundIsMiss`.

### Task 2.7: Write failing test `TestGet_EmptyStoredTreatedAsMiss`

**Files:**
- Modify: `~/Code/tuxlink-pat/internal/credstore/credstore_test.go`

- [ ] **Step 1: Add the test:**

```go
func TestGet_EmptyStoredTreatedAsMiss(t *testing.T) {
	setupMock(t)
	// Some keyring backends (e.g., Linux secret-service) accept Set("") and
	// store an empty string; others (Windows wincred) reject. Our credstore
	// normalizes this to "no entry" per spec §3.5 — uniform UX regardless
	// of backend behavior (R3 F4 caught).
	if err := keyring.Set(credstore.ServiceName, "KK6XYZ", ""); err != nil {
		t.Fatalf("Set: %v", err)
	}
	pw, found, err := credstore.Get("KK6XYZ")
	if err != nil {
		t.Errorf("Get err: %v, want nil", err)
	}
	if found {
		t.Errorf("Get: found=true (empty-stored treated as hit), want false")
	}
	if pw != "" {
		t.Errorf("Get: pw=%q, want empty", pw)
	}
}
```

- [ ] **Step 2: Run test to verify it passes** (already implemented in Task 2.6):

```bash
go test ./internal/credstore/... -run TestGet_EmptyStoredTreatedAsMiss -v
```

Expected: `PASS`. (The empty-string check in Get's last branch handles this; Task 2.6's implementation already covers it. This test is a regression guard.)

### Task 2.8: Write failing tests for short-circuit on empty/whitespace callsign

**Files:**
- Modify: `~/Code/tuxlink-pat/internal/credstore/credstore_test.go`

- [ ] **Step 1: Add the tests:**

```go
// shortCircuitBackend tracks whether Get invoked the backend; for verifying
// the short-circuit on empty-after-normalize without round-tripping a mock.
type shortCircuitTestBackend struct {
	called bool
}

func TestGet_EmptyCallsign_ShortCircuit(t *testing.T) {
	setupMock(t)
	// We can't easily intercept keyring.Get calls in zalando's mock,
	// so we instead verify by attempting to write to a known account
	// then reading with an empty callsign — if Get DID forward to the
	// backend, it'd succeed in returning the entry under "" (or error).
	// If Get short-circuits, we get (false, nil) without backend call.
	if err := keyring.Set(credstore.ServiceName, "", "shouldnotreturn"); err != nil {
		t.Fatalf("Set: %v", err)
	}
	pw, found, err := credstore.Get("")
	if err != nil {
		t.Errorf("Get err: %v, want nil", err)
	}
	if found {
		t.Errorf("Get: found=true (backend not short-circuited), want false")
	}
	if pw != "" {
		t.Errorf("Get: pw=%q, want empty", pw)
	}
}

func TestGet_WhitespaceCallsign_ShortCircuit(t *testing.T) {
	setupMock(t)
	// Same defense: set an entry under "   " (which would be the post-trim
	// account if Get DIDN'T short-circuit); verify Get returns false.
	pw, found, err := credstore.Get("   ")
	if err != nil {
		t.Errorf("Get err: %v, want nil", err)
	}
	if found {
		t.Errorf("Get: found=true, want false")
	}
	if pw != "" {
		t.Errorf("Get: pw=%q, want empty", pw)
	}
}
```

- [ ] **Step 2: Run tests to verify they pass** (already implemented in Task 2.6 — `NormalizeAccount` short-circuit on `ok=false`):

```bash
go test ./internal/credstore/... -run "TestGet_(Empty|Whitespace)Callsign_ShortCircuit" -v
```

Expected: `PASS` for both.

### Task 2.9: Write failing test `TestGet_CasingNormalization`

**Files:**
- Modify: `~/Code/tuxlink-pat/internal/credstore/credstore_test.go`

- [ ] **Step 1: Add the test:**

```go
func TestGet_CasingNormalization(t *testing.T) {
	// Convergent adrev finding R2 F1 + R4 P2: wizard may write lowercase
	// callsign; Pat reads via addr.Addr which is uppercased; without
	// normalization, two different entries → silent miss.
	setupMock(t)
	// Write under normalized form (what the wizard would do):
	if err := keyring.Set(credstore.ServiceName, "KK6XYZ", "secret"); err != nil {
		t.Fatalf("Set: %v", err)
	}
	// Read with lowercase input (should normalize and hit):
	pw, found, err := credstore.Get("kk6xyz")
	if err != nil {
		t.Fatalf("Get err: %v", err)
	}
	if !found || pw != "secret" {
		t.Errorf("Get(\"kk6xyz\") = (%q, %v); want (\"secret\", true) — normalization should match KK6XYZ entry", pw, found)
	}
	// And whitespace-wrapped:
	pw, found, err = credstore.Get("  kk6xyz  ")
	if err != nil {
		t.Fatalf("Get err: %v", err)
	}
	if !found || pw != "secret" {
		t.Errorf("Get(whitespace-wrapped lowercase) = (%q, %v); want (\"secret\", true)", pw, found)
	}
}
```

- [ ] **Step 2: Run test to verify it passes** (already implemented in Task 2.6 via `NormalizeAccount` in `Get`):

```bash
go test ./internal/credstore/... -run TestGet_CasingNormalization -v
```

Expected: `PASS`.

### Task 2.10: Write failing tests for error classification

**Files:**
- Modify: `~/Code/tuxlink-pat/internal/credstore/credstore_test.go`

- [ ] **Step 1: Add the tests using `keyring.MockInitWithError`:**

```go
import "errors"

func TestGet_ErrLockedClassified(t *testing.T) {
	// Per zalando/go-keyring's mock support, MockInitWithError sets a
	// global error that all subsequent Set/Get/Delete calls return.
	// We inject a synthetic "locked"-style error and verify credstore
	// classifies it as ErrLocked sentinel.
	lockedErr := errors.New("default keyring is locked")
	keyring.MockInitWithError(lockedErr)
	t.Cleanup(keyring.MockInit) // restore to non-error mock

	pw, found, err := credstore.Get("KK6XYZ")
	if found {
		t.Errorf("Get: found=true on locked, want false")
	}
	if pw != "" {
		t.Errorf("Get: pw=%q, want empty", pw)
	}
	if !errors.Is(err, credstore.ErrLocked) {
		t.Errorf("Get err: %v (errors.Is(ErrLocked) = false); want ErrLocked sentinel", err)
	}
}

func TestGet_ErrUnavailableClassified(t *testing.T) {
	// D-Bus unreachable / no-secret-service is a different error class —
	// distinguishes "you forgot to install gnome-keyring" from "you
	// haven't unlocked the keyring yet."
	unavailErr := errors.New("dbus: cannot connect to session bus: address not set")
	keyring.MockInitWithError(unavailErr)
	t.Cleanup(keyring.MockInit)

	pw, found, err := credstore.Get("KK6XYZ")
	if found {
		t.Errorf("Get: found=true on D-Bus error, want false")
	}
	if pw != "" {
		t.Errorf("Get: pw=%q, want empty", pw)
	}
	if !errors.Is(err, credstore.ErrUnavailable) {
		t.Errorf("Get err: %v; want ErrUnavailable sentinel", err)
	}
}
```

- [ ] **Step 2: Run tests to verify they fail (sentinels don't exist yet):**

```bash
go test ./internal/credstore/... -run TestGet_Err -v
```

Expected: compile error — `undefined: credstore.ErrLocked` / `undefined: credstore.ErrUnavailable`.

### Task 2.11: Implement error sentinels + `classifyErr`

**Files:**
- Modify: `~/Code/tuxlink-pat/internal/credstore/credstore.go`

- [ ] **Step 1: Replace the placeholder `classifyErr` + add sentinels:**

```go
// Exported error sentinels for caller error-classification per spec §3.5.
// Callers use errors.Is(err, ErrLocked) / errors.Is(err, ErrUnavailable)
// to dispatch on log level and fallback behavior.
var (
	// ErrLocked indicates the OS keyring is locked (operator must unlock via
	// OS tools — Seahorse, Keychain Access, etc.). Interactive callers log
	// at WARN level and fall through to promptHub.
	ErrLocked = errors.New("credstore: keyring locked")

	// ErrUnavailable indicates D-Bus is unreachable or secret-service is not
	// installed. Interactive callers log at ERROR level and fall through to
	// promptHub. Configuration problem; operator needs to install
	// gnome-keyring / kwallet-pam or equivalent.
	ErrUnavailable = errors.New("credstore: keyring backend unavailable")
)

// classifyErr maps zalando/go-keyring's per-backend errors to our exported
// sentinels by matching substrings in the error string. Per spec §3.2 + R3 F7
// (cross-platform ErrNotFound mapping is not contractually guaranteed; we
// classify defensively).
func classifyErr(err error) error {
	if err == nil {
		return nil
	}
	s := err.Error()
	// Linux secret-service / gnome-keyring "locked" markers:
	if strings.Contains(s, "is locked") || strings.Contains(s, "Locked") {
		return fmt.Errorf("%w: %v", ErrLocked, err)
	}
	// D-Bus connection markers (Linux):
	if strings.Contains(s, "cannot connect to") || strings.Contains(s, "dbus") {
		return fmt.Errorf("%w: %v", ErrUnavailable, err)
	}
	// Unclassified; return raw for caller logging.
	return err
}
```

Note: add `"fmt"` to the import block.

- [ ] **Step 2: Run tests to verify they pass:**

```bash
go test ./internal/credstore/... -v
```

Expected: all tests pass, including the new `TestGet_ErrLockedClassified` and `TestGet_ErrUnavailableClassified`.

### Task 2.12: Run the full credstore test suite + commit

**Files:** none (verification + git op).

- [ ] **Step 1: Run all credstore tests:**

```bash
cd ~/Code/tuxlink-pat
go test ./internal/credstore/... -v -race
```

Expected: all tests pass. `-race` ensures the mock's package-global state doesn't cause races within a single test (tests serialize, but within a test the mock is accessed by Get).

- [ ] **Step 2: Commit the credstore package:**

```bash
git add internal/credstore/
git commit -m "$(cat <<'EOF'
feat(credstore): NEW package wrapping zalando/go-keyring for WL2K creds

Exports:
- ServiceName const ("tuxlink-pat")
- NormalizeAccount(callsign) → (string, bool) — trim+upper+empty-reject
- Get(callsign) → (pw, found, err) — short-circuits empty-after-normalize;
  treats empty-stored as miss; classifies errors into ErrLocked +
  ErrUnavailable sentinels
- ErrLocked + ErrUnavailable for caller errors.Is dispatch

Tests serialize (NO t.Parallel) because keyring.MockInit globally mutates
package state. Covers: hit, miss, empty-stored-as-miss, empty-callsign
short-circuit, whitespace short-circuit, casing normalization,
ErrLocked classification, ErrUnavailable classification.

Per spec §3.2 + §4.8 (canonical key) + §3.5 (error classification).

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

Expected: Commit lands; new files visible in `git log --stat HEAD`.

---

## Phase 3 — cfg/config.go modifications

**Execution Status:** ⬜ Not started

This phase removes the `SecureLoginPassword` JSON field from `cfg.Config` and modifies `AuxAddr` to drop the `Password` field WHILE PRESERVING the custom MarshalJSON/UnmarshalJSON methods (R1 + R3 + R4 convergent finding: removing the methods outright breaks all `auxiliary_addresses` configs since the wire schema is JSON string, not struct).

### Task 3.1: Write failing test `TestConfigParse_LegacyAuxAddrPasswordStripped`

**Files:**
- Modify (or Create): `~/Code/tuxlink-pat/cfg/config_test.go`

- [ ] **Step 1: Add the test:**

```go
package cfg_test

import (
	"encoding/json"
	"strings"
	"testing"

	"github.com/la5nta/pat/cfg"
)

func TestConfigParse_LegacyAuxAddrPasswordStripped(t *testing.T) {
	// Legacy form per Pat <1.0.0: auxiliary_addresses entries may contain
	// "CALL:password" (the Address:Password marshal form). Post-cred-refactor
	// (per spec §3.2), we drop AuxAddr.Password but PRESERVE the custom
	// UnmarshalJSON to strip the colon-suffix and never re-emit the password.
	const legacyConfigJSON = `{
		"mycall": "KK6XYZ",
		"auxiliary_addresses": ["KK6ABC:legacypw", "KK6DEF"]
	}`
	var c cfg.Config
	if err := json.Unmarshal([]byte(legacyConfigJSON), &c); err != nil {
		t.Fatalf("Unmarshal failed on legacy config: %v", err)
	}
	if len(c.AuxAddrs) != 2 {
		t.Fatalf("AuxAddrs len = %d, want 2", len(c.AuxAddrs))
	}
	// Password portion stripped:
	if c.AuxAddrs[0].Address != "KK6ABC" {
		t.Errorf("AuxAddrs[0].Address = %q, want %q (password should be stripped)", c.AuxAddrs[0].Address, "KK6ABC")
	}
	if c.AuxAddrs[1].Address != "KK6DEF" {
		t.Errorf("AuxAddrs[1].Address = %q, want %q", c.AuxAddrs[1].Address, "KK6DEF")
	}
	// Re-marshal: verify NO password ever re-emitted:
	out, err := json.Marshal(c.AuxAddrs)
	if err != nil {
		t.Fatalf("Marshal: %v", err)
	}
	if strings.Contains(string(out), "legacypw") {
		t.Errorf("Re-marshal leaked password: %s", out)
	}
	if strings.Contains(string(out), ":") {
		t.Errorf("Re-marshal includes colon-form: %s; want plain string form [\"KK6ABC\",\"KK6DEF\"]", out)
	}
}
```

- [ ] **Step 2: Run test to verify it fails (AuxAddr still has Password field; current UnmarshalJSON populates it):**

```bash
cd ~/Code/tuxlink-pat
go test ./cfg/... -run TestConfigParse_LegacyAuxAddrPasswordStripped -v
```

Expected: test FAILS — either compile error if AuxAddr.Password is referenced elsewhere, or assertion failure (the test's re-marshal would emit `"KK6ABC:legacypw"` per the current MarshalJSON behavior).

### Task 3.2: Modify `cfg.AuxAddr` — drop Password, preserve marshaling, strip colon-suffix

**Files:**
- Modify: `~/Code/tuxlink-pat/cfg/config.go`

- [ ] **Step 1: Read the current `AuxAddr` definition + marshaling** (lines 21-43 per spec):

```bash
sed -n '21,45p' cfg/config.go
```

Confirm: `type AuxAddr struct { Address string; Password *string }`, `MarshalJSON`, `UnmarshalJSON`.

- [ ] **Step 2: Replace with the new definition + modified marshaling:**

```go
type AuxAddr struct {
	Address string
}

// MarshalJSON always emits the Address as a JSON string. Per spec §3.2 +
// cred-refactor decision §4.7: AuxAddr never carries a password; the colon-
// suffix form ("CALL:password") is legacy-readable-only and never re-emitted.
func (a AuxAddr) MarshalJSON() ([]byte, error) {
	return json.Marshal(a.Address)
}

// UnmarshalJSON accepts both:
//   - the canonical string form: "CALL"
//   - the legacy colon-suffix form: "CALL:password" (the password portion
//     is silently dropped — NOT stored, NOT logged, NOT re-emitted).
// Per spec §3.2: convergent R1 + R3 + R4 adrev finding — removing this
// custom UnmarshalJSON outright would break all configs with auxiliary_addresses,
// including valid plain "CALL" entries (since the wire schema is JSON string,
// not struct).
func (a *AuxAddr) UnmarshalJSON(p []byte) error {
	var s string
	if err := json.Unmarshal(p, &s); err != nil {
		return err
	}
	if i := strings.IndexByte(s, ':'); i >= 0 {
		s = s[:i] // drop colon-suffix; never preserve password
	}
	a.Address = s
	return nil
}
```

- [ ] **Step 3: Run test to verify it passes:**

```bash
go test ./cfg/... -run TestConfigParse_LegacyAuxAddrPasswordStripped -v
```

Expected: PASS.

### Task 3.3: Remove `cfg.Config.SecureLoginPassword` field

**Files:**
- Modify: `~/Code/tuxlink-pat/cfg/config.go`

- [ ] **Step 1: Find the field:**

```bash
grep -n "SecureLoginPassword" cfg/config.go
```

Expected: line 53 (per spec).

- [ ] **Step 2: Delete the line + comment (lines ~52-53):**

```bash
# Locate the lines to delete:
sed -n '50,55p' cfg/config.go
```

Then delete the line `SecureLoginPassword string \`json:"secure_login_password"\`` and any adjacent comment specific to it. (Use Edit tool if available, or text editor.)

- [ ] **Step 3: Run cfg/ tests to verify nothing else breaks:**

```bash
go test ./cfg/... -v
```

Expected: PASS for all cfg/ tests including the new legacy-parse test.

- [ ] **Step 4: Run the full Go test suite to surface any compile errors elsewhere from the field removal:**

```bash
go build ./...
```

Expected: compile errors in `app/`, `api/`, `cli/`, `web/` packages that reference `cfg.SecureLoginPassword`. THIS IS EXPECTED — those references are the next phases' targets. Note the failing files in the plan's Discoveries subsection.

### Task 3.4: Commit cfg changes

**Files:** none (git op).

- [ ] **Step 1: Stage + commit:**

```bash
git add cfg/
git commit -m "$(cat <<'EOF'
refactor(cfg): drop SecureLoginPassword field + AuxAddr.Password

AuxAddr keeps its custom MarshalJSON/UnmarshalJSON (the JSON-string form
is the on-the-wire schema; convergent R1+R3+R4 adrev finding caught
that removing the methods outright would break all auxiliary_addresses
configs). Legacy "CALL:password" form parses cleanly with the password
portion silently dropped; never re-emitted.

Subsequent commits in this branch update the consumers of these fields
(api/api.go RedactedPassword, app/exchange.go callback, cli/init.go,
api/winlink_account.go, cli/account.go, app/winlink_api.go, web/src/*).

Per spec §3.2.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

Expected: Commit lands. NOTE: the working tree may still have build errors at this point because subsequent files reference the removed field. The next phases address those incrementally. Mark this in the Discoveries subsection.

---

## Phase 4 — api/api.go RedactedPassword removal

**Execution Status:** ⬜ Not started

This phase deletes the `RedactedPassword` API-redaction machinery from `api/api.go`. The constant was used solely to protect `SecureLoginPassword` in API responses; with the field gone, the machinery has no purpose. The spec § verified-by-grep that the constant has no other Go consumer; this phase re-verifies before delete.

### Task 4.1: Re-verify no other Go consumers of `RedactedPassword`

**Files:** none (verification).

- [ ] **Step 1: Grep:**

```bash
cd ~/Code/tuxlink-pat
grep -rn "RedactedPassword\|RedactedString" --include="*.go" .
```

Expected: only references in `api/api.go` lines 404, 416, 435. If other consumers exist, STOP and revise this phase to handle them.

### Task 4.2: Delete the constant + its usage

**Files:**
- Modify: `~/Code/tuxlink-pat/api/api.go`

- [ ] **Step 1: Locate the lines:**

```bash
sed -n '400,445p' api/api.go
```

- [ ] **Step 2: Delete:**
  - Line ~404: `const RedactedPassword = "[REDACTED]"`
  - Lines ~414-416: the `if currentConfig.SecureLoginPassword != "" { ... currentConfig.SecureLoginPassword = RedactedPassword }` block
  - Lines ~435-436: the `if newConfig.SecureLoginPassword == RedactedPassword { newConfig.SecureLoginPassword = currentConfig.SecureLoginPassword }` block

Use the Edit tool or sed for surgical removal. Verify the surrounding structure (function braces, etc.) remains intact.

- [ ] **Step 3: Verify the file still compiles:**

```bash
go build ./api/...
```

Expected: api package compiles. (If `SecureLoginPassword` is referenced elsewhere in api/, those are separate fixes in Phase 6.)

### Task 4.3: Commit

- [ ] **Step 1: Stage + commit:**

```bash
git add api/api.go
git commit -m "$(cat <<'EOF'
refactor(api): remove RedactedPassword redaction machinery

Field it protected (cfg.SecureLoginPassword) no longer exists per
prior commit. Verified no other Go consumers via grep. Web UI's
[REDACTED] semantics move to web/src updates in a later commit.

Per spec §3.2.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 5 — app/exchange.go callback rewrite + app/app.go cleanup

**Execution Status:** ⬜ Not started

This phase is the largest behavioral change: rewriting `SetSecureLoginHandleFunc` to use credstore with SMTP-proto skip, normalization, and NO AuxAddr-fallback-to-primary. Plus a small `app/app.go` cleanup.

### Task 5.1: Write failing tests for the new callback behavior

**Files:**
- Modify (or Create): `~/Code/tuxlink-pat/app/exchange_test.go`

Spec §3.6 Layer 3 enumerates 7 test cases. Implementation depends on whether existing `app/exchange_test.go` exists; check first:

```bash
ls app/exchange_test.go 2>&1 || echo "needs to create"
```

- [ ] **Step 1: Add tests (full code; if file exists, append; if not, create with full package boilerplate):**

```go
package app

import (
	"context"
	"errors"
	"testing"
	"time"

	"github.com/la5nta/pat/internal/credstore"
	"github.com/la5nta/wl2k-go/fbb"
	"github.com/zalando/go-keyring"
)

// secureLoginCallbackHarness encapsulates the test setup for the
// SetSecureLoginHandleFunc callback. The actual callback is constructed
// inline in the production code (app/exchange.go); for testing, we
// extract it into a helper or invoke via a constructed App + session
// fixture. For now, extract the logic into a testable function:
// secureLoginLookup(ctx, addr, cfg, promptHubMock) (string, error)
// and call that from both the production callback and the test.

func TestSecureLoginCallback_PrimaryHit(t *testing.T) {
	keyring.MockInit()
	t.Cleanup(func() { _ = keyring.DeleteAll(credstore.ServiceName) })
	if err := keyring.Set(credstore.ServiceName, "KK6XYZ", "primarypw"); err != nil {
		t.Fatalf("Set: %v", err)
	}
	addr := fbb.AddressFromString("KK6XYZ")
	pw, err := secureLoginLookup(context.Background(), addr, &mockPromptHub{shouldNotFire: t})
	if err != nil {
		t.Fatalf("err: %v", err)
	}
	if pw != "primarypw" {
		t.Errorf("pw = %q, want primarypw", pw)
	}
}

func TestSecureLoginCallback_PrimaryMiss_PromptHub(t *testing.T) {
	keyring.MockInit()
	t.Cleanup(func() { _ = keyring.DeleteAll(credstore.ServiceName) })
	addr := fbb.AddressFromString("KK6XYZ")
	promptHub := &mockPromptHub{returnVal: "promptedpw"}
	pw, err := secureLoginLookup(context.Background(), addr, promptHub)
	if err != nil {
		t.Fatalf("err: %v", err)
	}
	if pw != "promptedpw" {
		t.Errorf("pw = %q, want promptedpw", pw)
	}
	if !promptHub.fired {
		t.Errorf("promptHub did not fire")
	}
}

func TestSecureLoginCallback_SmtpProtoSkipsCredstore(t *testing.T) {
	keyring.MockInit()
	t.Cleanup(func() { _ = keyring.DeleteAll(credstore.ServiceName) })
	// Pre-populate keyring with entries that would HIT if SMTP-proto wasn't skipped
	if err := keyring.Set(credstore.ServiceName, "SOMEONE@EXAMPLE.ORG", "shouldnotreturn"); err != nil {
		t.Fatalf("Set: %v", err)
	}
	// SMTP-proto address; addr.Addr = "someone@example.org", addr.Proto = "SMTP"
	addr := fbb.AddressFromString("someone@example.org")
	if addr.Proto != "SMTP" {
		t.Fatalf("setup: addr.Proto = %q, want SMTP (test assumes fbb classifies @-form-with-non-winlink-host as SMTP)", addr.Proto)
	}
	promptHub := &mockPromptHub{returnVal: "promptedpw"}
	pw, err := secureLoginLookup(context.Background(), addr, promptHub)
	if err != nil {
		t.Fatalf("err: %v", err)
	}
	if pw != "promptedpw" {
		t.Errorf("pw = %q, want promptedpw (SMTP-proto should skip credstore + go straight to prompt)", pw)
	}
	if !promptHub.fired {
		t.Errorf("promptHub did not fire for SMTP-proto address")
	}
}

func TestSecureLoginCallback_EmptyAddrSkipsCredstore(t *testing.T) {
	keyring.MockInit()
	t.Cleanup(func() { _ = keyring.DeleteAll(credstore.ServiceName) })
	addr := fbb.Address{Addr: "", Proto: ""}
	promptHub := &mockPromptHub{returnVal: "promptedpw"}
	pw, _ := secureLoginLookup(context.Background(), addr, promptHub)
	if pw != "promptedpw" {
		t.Errorf("pw = %q, want promptedpw (empty addr should skip credstore + go to prompt)", pw)
	}
	if !promptHub.fired {
		t.Errorf("promptHub did not fire for empty addr")
	}
}

func TestSecureLoginCallback_AuxHit(t *testing.T) {
	keyring.MockInit()
	t.Cleanup(func() { _ = keyring.DeleteAll(credstore.ServiceName) })
	// Future multi-account UX (or power-user manual setup) populates AuxAddr keys
	if err := keyring.Set(credstore.ServiceName, "KK6ABC", "auxpw"); err != nil {
		t.Fatalf("Set: %v", err)
	}
	addr := fbb.AddressFromString("KK6ABC")
	promptHub := &mockPromptHub{shouldNotFire: t}
	pw, err := secureLoginLookup(context.Background(), addr, promptHub)
	if err != nil {
		t.Fatalf("err: %v", err)
	}
	if pw != "auxpw" {
		t.Errorf("pw = %q, want auxpw", pw)
	}
}

func TestSecureLoginCallback_AuxMiss_PromptHub_NoFallbackToPrimary(t *testing.T) {
	// REGRESSION TEST for the dropped fallback per §4.7.
	// Setup: primary KK6XYZ has entry; AuxAddr KK6ABC does NOT.
	// When callback receives KK6ABC address, it MUST NOT return KK6XYZ's password.
	keyring.MockInit()
	t.Cleanup(func() { _ = keyring.DeleteAll(credstore.ServiceName) })
	if err := keyring.Set(credstore.ServiceName, "KK6XYZ", "primarypw_should_NOT_leak"); err != nil {
		t.Fatalf("Set: %v", err)
	}
	addr := fbb.AddressFromString("KK6ABC")
	promptHub := &mockPromptHub{returnVal: "promptedpw"}
	pw, err := secureLoginLookup(context.Background(), addr, promptHub)
	if err != nil {
		t.Fatalf("err: %v", err)
	}
	if pw == "primarypw_should_NOT_leak" {
		t.Errorf("REGRESSION: callback returned primary's password for AuxAddr session — the fallback should be dropped per §4.7")
	}
	if pw != "promptedpw" {
		t.Errorf("pw = %q, want promptedpw (AuxAddr miss should go to promptHub)", pw)
	}
}

func TestSecureLoginCallback_KeyringLockedFallsToPrompt(t *testing.T) {
	keyring.MockInitWithError(errors.New("default keyring is locked"))
	t.Cleanup(keyring.MockInit)
	addr := fbb.AddressFromString("KK6XYZ")
	promptHub := &mockPromptHub{returnVal: "promptedpw"}
	pw, _ := secureLoginLookup(context.Background(), addr, promptHub)
	if pw != "promptedpw" {
		t.Errorf("pw = %q, want promptedpw", pw)
	}
	if !promptHub.fired {
		t.Errorf("promptHub did not fire on locked keyring")
	}
}

// mockPromptHub is a test double for app.PromptHub. shouldNotFire marks
// tests where firing is a regression; returnVal is what fire returns.
type mockPromptHub struct {
	shouldNotFire *testing.T
	returnVal     string
	fired         bool
}

func (m *mockPromptHub) Prompt(ctx context.Context, timeout time.Duration, kind PromptKind, msg string, opts ...PromptOption) <-chan PromptResponse {
	if m.shouldNotFire != nil {
		m.shouldNotFire.Errorf("mockPromptHub.Prompt called; should not fire")
	}
	m.fired = true
	ch := make(chan PromptResponse, 1)
	ch <- PromptResponse{Value: m.returnVal, Err: nil}
	return ch
}
```

Note: the `mockPromptHub`'s `Prompt` signature must match Pat's actual `PromptHub.Prompt`. If the signature differs, adjust the mock to match — `PromptKind` and `PromptResponse` come from `app/prompt_hub.go`.

- [ ] **Step 2: Run the tests to verify they fail (no `secureLoginLookup` function yet):**

```bash
go test ./app/... -run TestSecureLoginCallback -v
```

Expected: compile error — `undefined: secureLoginLookup`.

### Task 5.2: Refactor `app/exchange.go` to extract the callback logic into `secureLoginLookup`

**Files:**
- Modify: `~/Code/tuxlink-pat/app/exchange.go`

The current callback inlines logic into `SetSecureLoginHandleFunc`. For testability, extract into a package-level function `secureLoginLookup`.

- [ ] **Step 1: Add the new function near the existing callback (around line 175):**

```go
// secureLoginLookup encapsulates the keyring-then-promptHub credential
// resolution for an incoming fbb.Address. Extracted from the inline
// SetSecureLoginHandleFunc callback for testability.
//
// Per spec §3.3:
//  - SMTP-proto addresses skip credstore (addr.Addr is full email, not callsign)
//  - empty/whitespace addr.Addr skips credstore
//  - credstore lookup uses normalized bare callsign
//  - on miss/locked/unavailable: log + fall through to promptHub
//  - NO AuxAddr-fallback-to-primary (per §4.7)
func secureLoginLookup(ctx context.Context, addr fbb.Address, ph promptHubInterface) (string, error) {
	// Step 1: SMTP-proto skip
	if addr.Proto != "" {
		return promptForPassword(ctx, ph, addr)
	}
	// Step 2: normalize + short-circuit on empty
	account, ok := credstore.NormalizeAccount(addr.Addr)
	if !ok {
		return promptForPassword(ctx, ph, addr)
	}
	// Step 3: credstore lookup
	pw, found, err := credstore.Get(account)
	if err != nil {
		if errors.Is(err, credstore.ErrLocked) {
			log.Printf("level=warn msg=\"credstore: keyring locked; falling back to prompt\" callsign=%s", account)
		} else if errors.Is(err, credstore.ErrUnavailable) {
			log.Printf("level=error msg=\"credstore: keyring backend unavailable; falling back to prompt\" callsign=%s err=%q", account, err.Error())
		} else {
			log.Printf("level=error msg=\"credstore: unclassified error; falling back to prompt\" callsign=%s err=%q", account, err.Error())
		}
		return promptForPassword(ctx, ph, addr)
	}
	if !found {
		// Clean miss; silent fall-through per spec §3.5
		return promptForPassword(ctx, ph, addr)
	}
	return pw, nil
}

func promptForPassword(ctx context.Context, ph promptHubInterface, addr fbb.Address) (string, error) {
	resp := <-ph.Prompt(ctx, time.Minute, PromptKindPassword, "Enter secure login password for "+addr.String())
	return resp.Value, resp.Err
}

// promptHubInterface lets us mock the promptHub in tests without taking on
// the full app.App fixture. Production code satisfies this via the real
// *PromptHub.
type promptHubInterface interface {
	Prompt(ctx context.Context, timeout time.Duration, kind PromptKind, msg string, opts ...PromptOption) <-chan PromptResponse
}
```

Note: add `"errors"` and `"log"` to the import block if not present.

- [ ] **Step 2: Replace the inline callback (lines 175-192) with a call to `secureLoginLookup`:**

```go
// Handle secure login
session.SetSecureLoginHandleFunc(func(addr fbb.Address) (string, error) {
	return secureLoginLookup(context.Background(), addr, a.promptHub)
})
```

This is a much smaller block than the original; the old code's `cfg.SecureLoginPassword` references + AuxAddr-fallback are GONE (the field doesn't exist; the fallback was dropped per §4.7).

- [ ] **Step 3: Verify the file compiles:**

```bash
go build ./app/...
```

Expected: app package compiles. (app/winlink_api.go may still have compile errors from cfg.SecureLoginPassword references — Phase 6 addresses those.)

If compile errors arise: confirm whether they're from this file (fix) or from siblings (defer to Phase 6).

- [ ] **Step 4: Run the new exchange tests:**

```bash
go test ./app/... -run TestSecureLoginCallback -v
```

Expected: PASS for all 7 test cases. If `TestSecureLoginCallback_SmtpProtoSkipsCredstore` fails its `addr.Proto == "SMTP"` assertion, check `fbb.AddressFromString`'s actual behavior against R3 F1's verified-via-source-code claim.

### Task 5.3: Delete `app/app.go:230-233` (in-memory password clear)

**Files:**
- Modify: `~/Code/tuxlink-pat/app/app.go`

- [ ] **Step 1: Locate:**

```bash
sed -n '225,240p' app/app.go
```

- [ ] **Step 2: Delete lines 230-233:**

```go
// DELETE these 4 lines:
// Don't use config password if we don't use config mycall
if !strings.EqualFold(a.options.MyCall, a.config.MyCall) {
	a.config.SecureLoginPassword = ""
}
```

The lines guard against using a config-file password for a different callsign than the config-file's `MyCall`. Per spec §3.2 + §3.3: the field is gone; keyring lookups are keyed by the active callsign per `normalizeAccount(addr.Addr)`; no in-memory clearing needed.

- [ ] **Step 3: Verify compile:**

```bash
go build ./app/...
```

### Task 5.4: Commit

- [ ] **Step 1: Stage + commit:**

```bash
git add app/exchange.go app/exchange_test.go app/app.go
git commit -m "$(cat <<'EOF'
refactor(app): exchange callback uses credstore + drops cfg.SecureLoginPassword

SetSecureLoginHandleFunc rewritten to extracted secureLoginLookup function
(for testability):
- SMTP-proto addresses skip credstore (addr.Addr is full email, not
  callsign per fbb.AddressFromString)
- Empty/whitespace addr.Addr skips credstore
- credstore.Get uses normalized bare callsign
- On miss/locked/unavailable: log + fall through to promptHub
- NO AuxAddr-fallback-to-primary path (R2+R5 adrev: auth-bypass risk +
  serves no v0.0.1 wizard path)

app/app.go: dropped lines 230-233 (in-memory SecureLoginPassword clear
when CLI -mycall ≠ config MyCall — obsolete because field doesn't exist
and keyring lookups are keyed by active callsign).

7 test cases via mockPromptHub: PrimaryHit, PrimaryMiss_PromptHub,
SmtpProtoSkipsCredstore, EmptyAddrSkipsCredstore, AuxHit,
AuxMiss_PromptHub_NoFallbackToPrimary (regression), KeyringLockedFallsToPrompt.

Per spec §3.2 + §3.3 + §3.5 + §4.7.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 6 — API handlers + cli/account.go

**Execution Status:** ⬜ Not started

This phase updates the three remaining call sites that referenced `cfg.SecureLoginPassword`: `api/winlink_account.go`, `app/winlink_api.go`, and `cli/account.go`. Each gets explicit `(pw, found, err)` handling per spec §3.5 — NEVER the discard pattern `_, _, _ = credstore.Get(...)` (R4 P2 caught the prior spec's lapse).

### Task 6.1: Update `app/winlink_api.go::passwordRecoveryEmailSet`

**Files:**
- Modify: `~/Code/tuxlink-pat/app/winlink_api.go`

- [ ] **Step 1: Locate line 72:**

```bash
sed -n '65,80p' app/winlink_api.go
```

Confirm: `passwordRecoveryEmailSet(ctx, a.Options().MyCall, a.Config().SecureLoginPassword)`.

- [ ] **Step 2: Replace with explicit credstore handling:**

```go
// API context: no promptHub fallback. Return clear error on miss/error.
pw, found, err := credstore.Get(a.Options().MyCall)
if err != nil {
	return fmt.Errorf("password recovery requires keyring credentials: %w (use the tuxlink wizard to set credentials)", err)
}
if !found {
	return fmt.Errorf("password recovery requires keyring credentials: not set (use the tuxlink wizard to set credentials)")
}
// ... existing call using pw instead of a.Config().SecureLoginPassword
set, err := passwordRecoveryEmailSet(ctx, a.Options().MyCall, pw)
```

Adjust per the actual surrounding code structure. Add `"github.com/la5nta/pat/internal/credstore"` to imports.

- [ ] **Step 3: Verify compile:**

```bash
go build ./app/...
```

### Task 6.2: Update `api/winlink_account.go`

**Files:**
- Modify: `~/Code/tuxlink-pat/api/winlink_account.go`

- [ ] **Step 1: Locate line 65:**

```bash
sed -n '40,75p' api/winlink_account.go
```

Confirm: `password = h.Config().SecureLoginPassword`.

- [ ] **Step 2: Replace with explicit credstore handling:**

```go
pw, found, err := credstore.Get(h.Options().MyCall)
if err != nil {
	http.Error(w, fmt.Sprintf("keyring error: %v (use the tuxlink wizard to set credentials)", err), http.StatusServiceUnavailable)
	return
}
if !found {
	http.Error(w, "no keyring-stored password (use the tuxlink wizard to set credentials)", http.StatusServiceUnavailable)
	return
}
password = pw
// ... existing length validation (line 45) and cmsapi.AccountAdd call (line 49)
```

Adjust per the actual surrounding code structure. Add credstore import.

- [ ] **Step 3: Verify compile:**

```bash
go build ./api/...
```

### Task 6.3: Update `cli/account.go::getPasswordForCallsign`

**Files:**
- Modify: `~/Code/tuxlink-pat/cli/account.go`

- [ ] **Step 1: Locate the helper:**

```bash
sed -n '30,80p' cli/account.go
```

The function is interactive (CLI context, has access to promptHub via app), so promptHub fallback applies. Replace `SecureLoginPassword`-first lookup with `credstore.Get`-first:

- [ ] **Step 2: Replace:**

```go
func getPasswordForCallsign(ctx context.Context, a *app.App, callsign string) string {
	pw, found, err := credstore.Get(callsign)
	if found && err == nil {
		return pw
	}
	if err != nil {
		log.Printf("credstore.Get: %v (falling back to prompt)", err)
	}
	// Fall through to promptHub (existing behavior):
	resp := <-a.PromptHub().Prompt(ctx, time.Minute, app.PromptKindPassword, "Enter account password for "+callsign)
	if resp.Err != nil {
		log.Printf("password prompt error: %v", resp.Err)
		return ""
	}
	return resp.Value
}
```

Add credstore import. Remove any reference to `cfg.SecureLoginPassword` if present.

- [ ] **Step 3: Verify compile:**

```bash
go build ./cli/...
```

### Task 6.4: Run the full test suite + commit

- [ ] **Step 1: Run all tests** (Phases 2-6 should all pass now):

```bash
cd ~/Code/tuxlink-pat
go test ./...
```

Expected: all tests pass except any tests that reference `SecureLoginPassword` directly (will be deleted/fixed inline as found).

- [ ] **Step 2: Commit:**

```bash
git add app/winlink_api.go api/winlink_account.go cli/account.go
git commit -m "$(cat <<'EOF'
refactor: API handlers + cli/account.go use credstore with explicit handling

Three call sites updated:
- app/winlink_api.go::passwordRecoveryEmailSet — explicit (found, err)
  handling; API error on miss/error (no promptHub fallback in API
  contexts per spec §3.5)
- api/winlink_account.go — same pattern; http.Error on miss/error
- cli/account.go::getPasswordForCallsign — interactive context; falls
  through to promptHub on miss/error (preserves existing behavior)

NEVER the discard pattern `_, _, _ = credstore.Get(...)` (R4 P2 caught
the prior spec's lapse).

Per spec §3.2 + §3.5.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 7 — cli/init.go both password paths redirected

**Execution Status:** ⬜ Not started

This phase addresses BOTH password-touching paths in `cli/init.go` (R4 P2 #6 caught: the prior spec only addressed lines 193-258, leaving `handleNewAccount` + `promptNewPassword` + `cmsapi.AccountAdd` still asking for and submitting passwords with no keyring write).

### Task 7.1: Locate both paths

**Files:** none (read).

- [ ] **Step 1: Inspect both paths:**

```bash
sed -n '55,70p' cli/init.go    # InitHandle's branch to handleNewAccount (line 60)
sed -n '95,150p' cli/init.go   # handleNewAccount + promptNewPassword
sed -n '190,260p' cli/init.go  # existing-account password block (lines 193-258)
```

Confirm both paths exist and reference the password flow.

### Task 7.2: Replace existing-account password block (lines 193-258)

**Files:**
- Modify: `~/Code/tuxlink-pat/cli/init.go`

- [ ] **Step 1: Delete lines 193-258 and replace with brief redirect:**

```go
// Skip password setup; the tuxlink wizard owns credential entry per
// docs/superpowers/specs/2026-05-18-cred-handling-design.md.
fmt.Println(`Skipping password — use the tuxlink wizard to set Winlink credentials.
For standalone Pat usage, use upstream la5nta/pat which retains config.json passwords.
See: https://github.com/cameronzucker/tuxlink-pat (README "Credentials" section)`)
```

The plain README link (no `#credentials` fragment anchor) is intentional per R1 F5 to avoid bookmark-fragile decay.

### Task 7.3: Replace handleNewAccount call (line 60)

**Files:**
- Modify: `~/Code/tuxlink-pat/cli/init.go`

- [ ] **Step 1: Find line 60 (in `InitHandle`):**

```bash
sed -n '55,65p' cli/init.go
```

Look for: `handleNewAccount(ctx, &cfg)`.

- [ ] **Step 2: Replace the call with the brief redirect:**

```go
// Replace:
// handleNewAccount(ctx, &cfg)
// with:
fmt.Println(`New-account creation requires the tuxlink wizard (sets credentials in OS keyring).
For standalone Pat usage, use upstream la5nta/pat which retains config.json passwords.
See: https://github.com/cameronzucker/tuxlink-pat (README "Credentials" section)`)
```

- [ ] **Step 3: Keep `handleNewAccount` + `promptNewPassword` functions in source** (dead code with TODO comment):

```go
// TODO: re-introduce as separate subcommand if standalone-Pat audience
// emerges (per spec §2 "Re-introducing the validatePassword + ... flow as
// separate `pat` subcommands. Not in this patch; can be revisited if
// standalone-Pat usage becomes a real audience for the fork.").
//
// nolint:unused — intentionally retained for future re-introduction.
func handleNewAccount(ctx context.Context, cfg *cfg.Config) { ... }
```

The `nolint:unused` comment prevents linters from flagging the dead code; the TODO documents the design intent.

- [ ] **Step 4: Verify compile + run any cli tests:**

```bash
go build ./cli/...
go test ./cli/...
```

Expected: cli/ builds. Tests may pass without code changes (no tests cover handleNewAccount directly).

### Task 7.4: Manual smoke test

**Files:** none.

- [ ] **Step 1: Build pat binary:**

```bash
cd ~/Code/tuxlink-pat
go build -o /tmp/pat-test .
```

- [ ] **Step 2: Run `pat configure`:**

```bash
PAT_MYCALL= /tmp/pat-test --config /tmp/test-config.json configure </dev/null
```

Expected:
- pat prompts for callsign + locator + mailbox path (other configure steps unchanged)
- when it would normally prompt for password (either via handleNewAccount or the existing-account block), it prints the redirect message instead
- pat exits cleanly without setting a password

If the smoke reveals the redirect message is malformed or appears at the wrong point in the flow, fix inline.

### Task 7.5: Commit

- [ ] **Step 1: Stage + commit:**

```bash
git add cli/init.go
git commit -m "$(cat <<'EOF'
refactor(cli): cli/init.go skips BOTH password-touching paths

Path A (existing-account, lines 193-258): brief redirect to tuxlink wizard;
validatePassword + getPasswordRecoveryEmail + cmsapi.PasswordRecoveryEmailSet
calls REMOVED from `pat configure` flow.

Path B (new-account creation via handleNewAccount, line 60): brief
redirect REPLACES the handleNewAccount call. handleNewAccount +
promptNewPassword + cmsapi.AccountAdd retained in source as dead code
with TODO comment for future re-introduction as separate subcommands.

Per spec §3.2 + R4 P2 (caught the prior spec only addressed path A).

Other `pat configure` steps (callsign, locator, mailbox path) unchanged.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 8 — Web UI updates (web/src/* + web/dist/* rebuild)

**Execution Status:** ⬜ Not started

This phase removes the `secure_login_password` form field from the web UI and rebuilds the prebuilt assets. Per spec §3.2: leaving the form in place after backend field removal silently drops user-saved passwords (R3 + R4 caught the prior spec's omission).

### Task 8.1: Remove `secure_login_password` form input from `web/src/config.html`

**Files:**
- Modify: `~/Code/tuxlink-pat/web/src/config.html`

- [ ] **Step 1: Locate lines 82-85:**

```bash
sed -n '78,92p' web/src/config.html
```

- [ ] **Step 2: Replace the input block with an info notice:**

```html
<!-- Replace lines 82-85 (the entire form-group containing secure_login_password): -->
<div class="form-group">
  <label>Secure Login Password</label>
  <div class="alert alert-info" role="alert">
    Set Winlink credentials via the tuxlink wizard.
    For standalone Pat usage, use upstream
    <a href="https://github.com/la5nta/pat" target="_blank" rel="noopener">la5nta/pat</a>
    which retains config.json passwords.
    See <a href="https://github.com/cameronzucker/tuxlink-pat" target="_blank" rel="noopener">tuxlink-pat README</a> for details.
  </div>
</div>
```

The `alert-info` class is Bootstrap-conventional (Pat's existing web UI uses Bootstrap). Match the spacing/indentation of the surrounding code.

### Task 8.2: Remove `secure_login_password` references from `web/src/js/config.js`

**Files:**
- Modify: `~/Code/tuxlink-pat/web/src/js/config.js`

- [ ] **Step 1: Locate all references:**

```bash
grep -n "secure_login_password\|secureLoginPassword" web/src/js/config.js
```

Expected: lines 176, 190, 347, 349, 354, 518 (per spec).

- [ ] **Step 2: Delete each reference:**

For each line, decide:
- Lines 176, 190, 347, 349, 354: GET-side rendering (populate the form field with `[REDACTED]` if set). Delete the entire conditional block since the field no longer exists.
- Line 518: POST-side assignment (`updatedConfig.secure_login_password = ...`). Delete this line so the field is never sent to the backend.

Use Edit tool for surgical removal; preserve surrounding logic.

- [ ] **Step 3: Add a defensive AuxAddr CALL:password rejection on form submit:**

```javascript
// In the form-submit handler, BEFORE the AJAX POST:
function validateAuxAddrs(addrs) {
  for (let i = 0; i < addrs.length; i++) {
    if (addrs[i].indexOf(':') >= 0) {
      alert('Auxiliary addresses cannot contain ":" (the legacy "CALL:password" form is no longer supported). Use the tuxlink wizard to set per-callsign passwords.');
      return false;
    }
  }
  return true;
}

// Call validateAuxAddrs(updatedConfig.auxiliary_addresses) before $.ajax(...).
```

### Task 8.3: Rebuild `web/dist/*`

**Files:**
- Modify: `~/Code/tuxlink-pat/web/dist/*`

- [ ] **Step 1: Check how Pat builds the dist:**

```bash
ls web/
cat web/package.json 2>&1 || echo "no package.json"
cat web/scss/main.scss 2>&1 | head -5 || echo "no scss"
ls web/src/ web/dist/
```

If Pat uses an npm-based build chain (e.g., webpack, parcel, sass), run it:

```bash
cd web/
npm install   # if package.json exists
npm run build # or whatever the build script is named
```

If Pat uses a Go-based asset bundler (`go-bindata` or similar), check the Makefile / `make.bash`.

If the dist files are committed verbatim (no build step; src and dist are kept manually in sync), manually copy the changes from `web/src/config.html` and `web/src/js/config.js` to `web/dist/config.html` and `web/dist/js/config.js`.

- [ ] **Step 2: Manual smoke in browser:**

Start `pat http` locally; open `http://localhost:8080/ui/config.html` (or wherever Pat serves the config UI); verify:
- the secure_login_password input is gone
- the info notice is visible
- no console errors in the browser devtools

### Task 8.4: Commit

- [ ] **Step 1: Stage + commit:**

```bash
git add web/src/config.html web/src/js/config.js web/dist/
git commit -m "$(cat <<'EOF'
refactor(web): remove secure_login_password form field; redirect to wizard

R3 + R4 adrev caught: removing only the backend cfg field leaves the
web UI form in place; users saving credentials there think they were
stored, but the new config struct silently ignores the field.

Changes:
- web/src/config.html: replace input block (lines 82-85) with info
  notice pointing at the tuxlink wizard
- web/src/js/config.js: remove all secure_login_password references
  (lines 176, 190, 347, 349, 354, 518); never include the field in
  POST payload; add defensive validation rejecting "CALL:password"
  AuxAddr form on submit
- web/dist/*: rebuilt from src

Per spec §3.2 + §3.1 architecture.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 9 — README + CI integration test + PR-A open

**Execution Status:** ⬜ Not started

This phase adds the tuxlink-pat README's "Credentials" section, adds the CI integration test workflow, runs the integration tests locally to verify, then opens PR-A against `cameronzucker/tuxlink-pat/master`.

### Task 9.1: Add "## Credentials" section to tuxlink-pat README

**Files:**
- Modify: `~/Code/tuxlink-pat/README.md`

- [ ] **Step 1: Append after the existing main content (before any "License" / "Contributing" sections):**

```markdown
## Credentials

tuxlink-pat reads WL2K passwords from the OS keyring (`secret-service` on Linux Gnome/KDE, `Keychain` on macOS, `CredentialManager` on Windows) via `github.com/zalando/go-keyring`. Credentials are NOT stored in `config.json`.

**Setting credentials:** use the [tuxlink wizard](https://github.com/cameronzucker/tuxlink). The wizard writes to the keyring under `(service="tuxlink-pat", account="<normalized-bare-callsign>")` (callsign trimmed and uppercased on both writer and reader sides).

**Standalone Pat usage:** if you want a standalone CLI Pat without the tuxlink wizard, use upstream [la5nta/pat](https://github.com/la5nta/pat) which retains the `config.json` password storage model. tuxlink-pat is opinionated about keyring-only storage and is NOT a drop-in replacement for upstream Pat.

**Tested platform:** Linux. The keyring code uses zalando/go-keyring's cross-platform API and will compile + run on macOS and Windows, but those platforms are not tested in tuxlink v0.0.1. Future tuxlink platform expansions inherit the same code path.

**Multi-account:** each callsign gets its own keyring entry. AuxAddrs each get their own entry (manually populated via OS tools like Seahorse or `secret-tool` until tuxlink's multi-account wizard UX ships). There is NO fallback to the primary callsign's password — each callsign stands alone.

**Locked / missing keyring:** if the keyring is locked or no entry exists for the callsign, Pat falls through to the existing 60-second password prompt (`PromptKindPassword` via promptHub) — preserves EmComm stand-up scenarios. P2P operations don't read the keyring at all.

For full design rationale, see the [cred-handling design spec](https://github.com/cameronzucker/tuxlink/blob/main/docs/superpowers/specs/2026-05-18-cred-handling-design.md) in the tuxlink repo.
```

### Task 9.2: Add integration test workflow

**Files:**
- Modify (or Create): `~/Code/tuxlink-pat/.github/workflows/test.yml`

- [ ] **Step 1: Inspect existing workflows:**

```bash
ls .github/workflows/
cat .github/workflows/*.yml 2>&1 | head -40
```

- [ ] **Step 2: Add or modify the test workflow to include the integration job:**

```yaml
name: Test

on:
  push:
    branches: [master]
  pull_request:
    branches: [master]

jobs:
  unit:
    runs-on: ubuntu-22.04   # PINNED per spec §3.6 (R1 F4: floating image = silent CI rot)
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-go@v6
        with:
          go-version-file: 'go.mod'
      - name: Unit tests
        run: go test ./...

  integration:
    runs-on: ubuntu-22.04   # PINNED
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-go@v6
        with:
          go-version-file: 'go.mod'
      - name: Install keyring deps
        run: sudo apt install -y libsecret-1-dev gnome-keyring dbus-x11
      - name: Run integration tests in D-Bus session
        run: |
          dbus-run-session -- bash -c '
            echo "" | gnome-keyring-daemon --unlock --replace --daemonize --components=secrets
            go test -tags=integration ./internal/credstore/...
          '
```

Adapt to existing structure if other jobs exist; merge into the same file rather than creating a new one.

### Task 9.3: Add `internal/credstore/credstore_integration_test.go`

**Files:**
- Create: `~/Code/tuxlink-pat/internal/credstore/credstore_integration_test.go`

- [ ] **Step 1: Write the integration tests:**

```go
//go:build integration

package credstore_test

import (
	"testing"

	"github.com/la5nta/pat/internal/credstore"
	"github.com/zalando/go-keyring"
)

func TestRealKeyring_RoundTrip(t *testing.T) {
	// Cleanup any prior test state:
	t.Cleanup(func() { _ = keyring.DeleteAll(credstore.ServiceName) })

	if err := keyring.Set(credstore.ServiceName, "KK6XYZ", "integration-test-pw"); err != nil {
		t.Fatalf("Set: %v", err)
	}
	pw, found, err := credstore.Get("KK6XYZ")
	if err != nil {
		t.Fatalf("Get err: %v", err)
	}
	if !found {
		t.Errorf("Get: found=false, want true")
	}
	if pw != "integration-test-pw" {
		t.Errorf("Get: pw=%q, want %q", pw, "integration-test-pw")
	}
}

func TestRealKeyring_DeleteCleanup(t *testing.T) {
	if err := keyring.Set(credstore.ServiceName, "KK6XYZ", "to-be-deleted"); err != nil {
		t.Fatalf("Set: %v", err)
	}
	if err := keyring.DeleteAll(credstore.ServiceName); err != nil {
		t.Fatalf("DeleteAll: %v", err)
	}
	_, found, err := credstore.Get("KK6XYZ")
	if err != nil {
		t.Errorf("Get post-delete err: %v, want nil", err)
	}
	if found {
		t.Errorf("Get post-delete: found=true, want false")
	}
}
```

### Task 9.4: Run integration tests locally + commit

**Files:** none (verification + git op).

- [ ] **Step 1: Run integration tests in a local D-Bus session** (same invocation CI uses):

```bash
cd ~/Code/tuxlink-pat
sudo apt install -y libsecret-1-dev gnome-keyring dbus-x11   # if not already installed
dbus-run-session -- bash -c '
  echo "" | gnome-keyring-daemon --unlock --replace --daemonize --components=secrets
  go test -tags=integration ./internal/credstore/...
'
```

Expected: integration tests PASS.

If they fail, debug locally before committing. Common failure: missing `gnome-keyring` or `dbus-x11` apt packages — install them.

- [ ] **Step 2: Commit:**

```bash
git add README.md .github/workflows/ internal/credstore/credstore_integration_test.go
git commit -m "$(cat <<'EOF'
docs+ci: tuxlink-pat README Credentials section + integration-test workflow

README: new "## Credentials" section explaining keyring storage model,
standalone-Pat path, tested-platform commitment (Linux only in v0.0.1),
multi-account semantics, locked-keyring fallback.

CI: new (or amended) test workflow with two jobs — unit (cross-platform
on any Go runner) + integration (Linux only, runs in dbus-run-session
with gnome-keyring-daemon). Runner pinned to ubuntu-22.04 (R1 F4: floating
ubuntu-latest = silent CI rot when actions/daemons update).

Integration tests: round-trip + delete-cleanup against real OS keyring.

Per spec §3.2 README row + §3.6 Layer 2 + §3.7 CI invocation pattern.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

### Task 9.5: Push branch + open PR-A

**Files:** none (git + gh ops).

- [ ] **Step 1: Push:**

```bash
git push -u origin bd-tuxlink-mib/cred-keyring
```

- [ ] **Step 2: Open PR-A against tuxlink-pat/master:**

```bash
gh pr create --base master --head bd-tuxlink-mib/cred-keyring \
  --repo cameronzucker/tuxlink-pat \
  --title "[<YOUR-MONIKER>] refactor(cred): pat reads WL2K from OS keyring (closes tuxlink-mib partial)" \
  --body "$(cat <<'EOF'
## Summary

First agentic patch against tuxlink-pat per ADR 0011 §3. Replaces `cfg.SecureLoginPassword` (plaintext on-disk) with OS-keyring reads via `github.com/zalando/go-keyring`. Tuxlink wizard becomes the sole keyring writer (separate task `tuxlink-ko0`); Pat-CLI configure paths emit a brief redirect.

**Spec of record:** [docs/superpowers/specs/2026-05-18-cred-handling-design.md](https://github.com/cameronzucker/tuxlink/blob/feat/v0.0.1/docs/superpowers/specs/2026-05-18-cred-handling-design.md) on the tuxlink repo @ commit 68a698c (post-5-round-adrev revision)
**Plan of record:** [docs/plans/2026-05-18-cred-handling-plan.md](https://github.com/cameronzucker/tuxlink/blob/feat/v0.0.1/docs/plans/2026-05-18-cred-handling-plan.md) on the tuxlink repo

## Major changes
- NEW `internal/credstore` package wrapping zalando/go-keyring with canonical-key normalization (trim+upper), empty-stored-as-miss, error classification (ErrLocked / ErrUnavailable sentinels)
- `app/exchange.go::SetSecureLoginHandleFunc` rewritten: SMTP-proto skip, normalized credstore lookup, NO AuxAddr-fallback-to-primary (dropped per spec §4.7 to close auth-bypass surface)
- `cfg/config.go`: `SecureLoginPassword` field deleted; `AuxAddr.Password` deleted but MarshalJSON/UnmarshalJSON PRESERVED (strips legacy `CALL:password` form on parse; never re-emits)
- `api/api.go`: `RedactedPassword` machinery deleted
- `cli/init.go`: BOTH password-touching paths (existing-account 193-258 AND new-account handleNewAccount/promptNewPassword) emit brief redirect message; functions retained as dead code with TODO comment
- `api/winlink_account.go` + `app/winlink_api.go` + `cli/account.go`: explicit `(found, err)` handling on credstore; never the discard pattern
- `web/src/config.html` + `web/src/js/config.js` + `web/dist/*`: removed `secure_login_password` form field + JS handlers; replaced with info notice
- README.md: new "## Credentials" section
- `.github/workflows/`: integration-test workflow with `dbus-run-session` wrapping; pinned `ubuntu-22.04` runner

## Test plan
- [x] Unit tests via `keyring.MockInit()` — all credstore tests pass (10 cases; serialized, no t.Parallel)
- [x] Integration tests via `dbus-run-session` + `gnome-keyring-daemon` — round-trip + delete-cleanup pass
- [x] `app/exchange_test.go` — 7 callback test cases including SMTP-proto skip + regression test for no-fallback-to-primary
- [x] `cfg/config_test.go` — `TestConfigParse_LegacyAuxAddrPasswordStripped` verifies legacy CALL:password form parses and password portion is dropped/never re-emitted
- [x] Manual smoke: `pat configure` shows redirect messages for both password paths
- [x] Manual smoke: web UI form is gone; info notice visible; no console errors

## Branch policy
This branch is RETAINED on merge per ADR 0011 §4 + fork-setup spec §3.2 (cherry-pick portability for future upstream PR contribution).

## Follow-up
After merge: PR-B opens against `cameronzucker/tuxlink/feat/v0.0.1` to bump the submodule pin and close `tuxlink-mib`.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Expected: PR URL printed. Update the plan's Execution Status table with the PR number + URL.

- [ ] **Step 3: Update plan LDC banner per the contract:**

The plan's Phase 9 banner flips ⬜ → 🚧 → ✅ as work progresses; when PR-A opens, update the Execution Status table row for Phase 9 with the PR # + URL + "🚧 In progress; PR open" pending Cameron review.

---

## Phase 10 — PR-A merge + PR-B submodule bump on tuxlink

**Execution Status:** ⬜ Not started

**Pre-requisite:** PR-A merged by Cameron on `cameronzucker/tuxlink-pat/master`. Verify before starting Phase 10.

This final phase bumps the tuxlink-pat submodule pin in the tuxlink repo to PR-A's merge SHA and opens PR-B against `feat/v0.0.1`. PR-B's merge closes `tuxlink-mib`.

### Task 10.1: Verify PR-A merged + record SHA

- [ ] **Step 1: Verify PR-A status:**

```bash
gh pr view <PR-A-number> --repo cameronzucker/tuxlink-pat --json state,mergeCommit
```

Expected: `state == "MERGED"`, `mergeCommit.oid` is non-empty. Record the SHA.

### Task 10.2: Bump submodule in tuxlink

**Files:**
- Modify: `external/tuxlink-pat` (submodule pin)

This work happens in the **tuxlink worktree** (the one this plan lives in: `worktrees/bd-tuxlink-mib-mib-cred-keyring/`).

- [ ] **Step 1: Update submodule:**

```bash
cd ~/Code/tuxlink/worktrees/bd-tuxlink-mib-mib-cred-keyring/external/tuxlink-pat
git fetch origin
git checkout <PR-A-merge-SHA>
cd ../..
git add external/tuxlink-pat
```

- [ ] **Step 2: Verify the submodule SHA matches:**

```bash
git submodule status
```

Expected: `<PR-A-merge-SHA> external/tuxlink-pat (...)` — the SHA matches PR-A's merge commit.

- [ ] **Step 3: Commit:**

```bash
git commit -m "$(cat <<'EOF'
build(pat): bump tuxlink-pat submodule to include cred-refactor (closes tuxlink-mib)

PR-A merged on tuxlink-pat: <PR-A-URL>
Merge SHA: <PR-A-merge-SHA>

Replaces cfg.SecureLoginPassword (plaintext on-disk) with OS-keyring reads
via github.com/zalando/go-keyring. Wizard (Task 9, tuxlink-ko0) is the
sole keyring writer; this submodule bump enables tuxlink to consume the
new Pat behavior on next release-profile build.

Spec of record: docs/superpowers/specs/2026-05-18-cred-handling-design.md @ 68a698c
Plan of record: docs/plans/2026-05-18-cred-handling-plan.md

Closes tuxlink-mib.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

### Task 10.3: Push + open PR-B

- [ ] **Step 1: Push:**

```bash
git push   # branch is already tracking origin from spec/revision commits
```

- [ ] **Step 2: Open PR-B against feat/v0.0.1:**

```bash
gh pr create --base feat/v0.0.1 --head bd-tuxlink-mib/mib-cred-keyring \
  --title "[<YOUR-MONIKER>] build(pat): bump tuxlink-pat submodule for cred-refactor (closes tuxlink-mib)" \
  --body "$(cat <<'EOF'
## Summary
- Bump tuxlink-pat submodule pin to PR-A's merge SHA (<PR-A-URL>)
- Includes spec (docs/superpowers/specs/2026-05-18-cred-handling-design.md @ 68a698c) + plan (docs/plans/2026-05-18-cred-handling-plan.md) + adrev disposition

## Test plan
- [x] Submodule SHA matches PR-A merge commit
- [x] `git submodule update --init --recursive` reproduces a clean state
- [ ] CI release-profile build passes (will verify on PR check)

## Closes
- tuxlink-mib

## Branch policy
This branch lands on `feat/v0.0.1` via merge-commit (no squash per ADR 0010). Branch is deleted on merge per tuxlink's `gh pr merge --merge --delete-branch` convention.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Expected: PR URL printed. Update the plan's Execution Status table.

### Task 10.4: Update plan LDC banner

- [ ] **Step 1: Flip Phase 10 banner to 🚧 with PR-B URL.** When Cameron merges PR-B, flip to ✅ with the merge SHA. `bd close tuxlink-mib` after merge.

---

## Post-execution: bd cleanup

After PR-B merges:

```bash
bd close tuxlink-mib --reason "Shipped via PR-A (tuxlink-pat#<N>) + PR-B (tuxlink#<N>)"
bd close tuxlink-ttp   # if still open and the side-PR merged separately
```

Newly unblocked: `tuxlink-ko0` (wizard Task 9), `tuxlink-nk7` (live-CMS smoke Task 6), `tuxlink-gdo` (AppImage dep doc), `tuxlink-54p` (v0.0.1 plan amendments).

---

## Risk register summary

(See spec §5 for full risk classes + watched failure modes. Reproduced here for plan-implementer convenience.)

| Risk class | Examples | Mitigation in this plan |
|---|---|---|
| Build-time (loud) | go mod tidy fails; libsecret-1-dev missing | CI workflow installs deps; standard Go error UX |
| Runtime (graceful degrade) | Keyring locked; D-Bus unreachable | credstore returns ErrLocked/ErrUnavailable sentinels; callback falls through to promptHub |
| Rot over time | ServiceName rot on rename; promptHub timeout undermines EmComm | Documented in spec §5.3; no v0.0.1 mitigation |
| Security | Wrong-password CMS lockout via retry | NO retry loop in credstore; one attempt + fall through |
| Cross-task | Existing config.json secure_login_password silently ignored | Acceptable v0.0.1 (no existing users); flagged for upstream-PR variant |

---

## Self-review checklist for the plan-writing agent

Per the writing-plans skill self-review (executed inline before declaring plan complete):

1. **Spec coverage:** every scope item in §2 of the spec maps to a task above?
   - §2.1 credstore reads → Phase 2 (credstore) + Phase 5 (callback)
   - §2.2 cfg field deletes → Phase 3
   - §2.3 RedactedPassword removal → Phase 4
   - §2.4 web UI updates → Phase 8
   - §2.5 callback rewrite → Phase 5
   - §2.6 cli/init.go both paths → Phase 7
   - §2.7 API handlers explicit handling → Phase 6
   - §2.8 credstore package contents → Phase 2
   - §2.9 go.mod dep → Phase 1
   - §2.10 README → Phase 9
   - §2.11 CI workflow → Phase 9
   - §2.12 unit tests → Phase 2
   - §2.13 integration tests → Phase 9
   - §2.14 config-parse regression test → Phase 3
   
   All covered. ✓

2. **Placeholder scan:** no `TBD`/`TODO`/`implement later`/`fill in details` in implementation code (only one TODO is in §7's `handleNewAccount` retention which is deliberate dead-code marker). ✓

3. **Type consistency:** the function signatures used in later tasks match those defined earlier?
   - `credstore.NormalizeAccount(string) (string, bool)` — defined in Task 2.4, used in Task 5.2 → matches ✓
   - `credstore.Get(string) (string, bool, error)` — defined in Task 2.6, used in Tasks 5.2, 6.1, 6.2, 6.3 → matches ✓
   - `credstore.ErrLocked` + `credstore.ErrUnavailable` — defined in Task 2.11, used in Task 5.2 → matches ✓

4. Any "Similar to Task N" without full code? No. ✓

Plan ready. Execution recommendation: subagent-driven-development with per-phase dispatches.
