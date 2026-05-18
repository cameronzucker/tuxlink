# Spec: cred-handling refactor — Pat reads WL2K from OS keyring (tuxlink-pat patch)

**Date:** 2026-05-18
**Agent:** shoal-condor-clover
**bd issue:** `tuxlink-mib` (P1; cred-handling refactor blocks Task 6 resume `tuxlink-nk7`, Task 9 wizard `tuxlink-ko0`, AppImage dep doc `tuxlink-gdo`, plan amendments `tuxlink-54p`)
**Branch:** `bd-tuxlink-mib/mib-cred-keyring` (worktree at `worktrees/bd-tuxlink-mib-mib-cred-keyring/`)
**Status:** Pre-adrev draft. To be revised post 5-round cross-provider adversarial review (≥1 Codex round) per ADR 0011 §3.
**Target repo for the patch:** `cameronzucker/tuxlink-pat` (the fork). Tuxlink-side submodule bump lands as a follow-up PR against `cameronzucker/tuxlink/feat/v0.0.1`.

## 1. Context

[ADR 0011](../../adr/0011-fork-pat-for-tuxlink.md) committed tuxlink to forking upstream `la5nta/pat` as `tuxlink-pat`, and identified the cred-handling refactor as the first agentic patch against the fork. The motivating defect: Pat 1.0.0 persists WL2K passwords to `~/.config/pat/config.json` in plaintext. Cameron's words from the ADR context:

> I will NOT rely on a project which was so lazy as to write creds for a real production system to .json. We'll fork it and refactor with robust agentic development as we go.

Fork-setup task (`tuxlink-84i`, [spec](./2026-05-18-fork-setup-design.md), [plan](../../plans/2026-05-18-fork-setup-plan.md)) shipped end-to-end at PR #54 merge (2026-05-18). `tuxlink-pat` is now a live submodule of tuxlink at `external/tuxlink-pat/`, with release-profile Go-build integration verified by CI.

This spec covers the **next** task in ADR 0011's sequencing: the first agentic patch on the fork. Per ADR 0011 §3, this patch goes through the full `build-robust-features` pipeline:

1. ~~Brainstorming~~ (this spec is its output)
2. **5-round adversarial design review with at least one cross-provider Codex round** ← NEXT
3. `writing-plans-enhanced` for the implementation plan (with `plan-review-cycle` ≥3 rounds)
4. TDD implementation
5. Codex round on the implementation diff
6. PR against `tuxlink-pat/master`

The brainstorm settled the following decisions, captured in §5 with full reasoning:

- **§5.1 Scope:** v0.0.1-tuxlink-only. No migration tool. No backward compat with existing config.json passwords. Upstream PR variant comes later as a separate, more-conservative design.
- **§4.2 Keyring naming:** `service="tuxlink-pat"`, `account="<callsign>"`. Matches GitHub CLI / aws-vault / HashiCorp Vault convention (researched 2026-05-18; see §4.2).
- **§5.3 Architecture:** keyring is a secure CACHE for the password the wizard captured; Pat's existing promptHub fallback is the universal graceful-degradation path. EmComm stand-up preserved.
- **§5.4 Pat-CLI surface:** wizard is the SOLE keyring writer. `pat configure` skips the password step with a brief redirect message. tuxlink-pat is positioned as tuxlink's engine, NOT a standalone-Pat replacement.
- **§5.5 Go library:** `github.com/zalando/go-keyring`. Cross-platform via the same API.
- **§5.6 Platform scope:** v0.0.1 commits to Linux only (tested + supported). credstore code compiles on macOS/Windows via library API (untested in v0.0.1; usable in a future cross-platform tuxlink expansion).

## 2. Scope

**In scope:**

1. Replace `cfg.SecureLoginPassword` (and `AuxAddr.Password`) reads in `tuxlink-pat` with OS-keyring reads via a new `internal/credstore` package.
2. Remove `secure_login_password` JSON field from `cfg/Config`; remove `Password *string` from `cfg/AuxAddr`; remove the custom MarshalJSON/UnmarshalJSON that parse the `"Address:Password"` form.
3. Remove the `RedactedPassword` API-redaction machinery from `api/api.go` (the field it protected no longer exists).
4. Rewrite `app/exchange.go`'s `SetSecureLoginHandleFunc` to: try credstore (primary keyring) → AuxAddr-fallback to primary callsign's keyring entry → fall through to promptHub.
5. Rewrite `cli/init.go`'s password-write block: emit a brief redirect message ("Skipping password — use the tuxlink wizard..."), continue with non-cred configure steps.
6. Rewrite `api/winlink_account.go` + `app/winlink_api.go` + `cli/account.go` to pull password from credstore where they currently read `cfg.SecureLoginPassword`. If the keyring miss + promptHub timeout occur in those contexts, return clear errors.
7. Add `github.com/zalando/go-keyring` to `go.mod`.
8. Update `tuxlink-pat/README.md` with a new "Credentials" section pointing to the tuxlink wizard.
9. Update `tuxlink-pat`'s CI to run integration tests under `dbus-launch` for the credstore package (Linux only in v0.0.1).
10. Add unit tests (`internal/credstore/credstore_test.go`) using zalando's `keyring.MockInit()` test helper.
11. Add integration tests (`internal/credstore/credstore_integration_test.go`, build-tagged `integration`).
12. Add config-parse regression tests verifying graceful handling of legacy config.json files containing the removed `secure_login_password` field.

**Out of scope** (cross-linked to other bd issues):

- Tuxlink wizard's Rust-side keyring write (`tuxlink-ko0`, Task 9 — blocked-on-this).
- Live-CMS smoke binary's keyring-read code (`tuxlink-nk7`, Task 6 — blocked-on-this).
- AppImage `libsecret-1-0` system-package dep documentation (`tuxlink-gdo` — blocked-on-this).
- v0.0.1 plan amendments for Tasks 5/6/9/11 (`tuxlink-54p` — blocked-on-this).
- Upstream PR to `la5nta/pat` (a more-conservative variant; new bd issue post-merge per ADR 0011 §4).
- macOS / Windows CI integration tests for the keyring backend (future v0.X platform expansion; credstore code compiles on those platforms but is untested in v0.0.1).
- Multi-account wizard UX in tuxlink (future tuxlink work; this patch supports Pat-side multi-account via AuxAddrs but the wizard handles single account in v0.0.1).
- `pat configure`'s `validatePassword` + `getPasswordRecoveryEmail` + `cmsapi.PasswordRecoveryEmailSet` integration flow at first-time setup (these required password; removed from `pat configure`'s scope). Future work could re-introduce as separate `pat` subcommands; not in this patch.

## 3. Design

### 3.1 Architecture overview

```
┌───────────────────────────────────────────────────────────────────────┐
│  cameronzucker/tuxlink (Rust/Tauri)                                   │
│                                                                       │
│  Wizard (Task 9, tuxlink-ko0 — NOT in this patch)                     │
│     │                                                                 │
│     │ 1. collect callsign + password from user                        │
│     │ 2. Rust `keyring` crate:                                        │
│     │      Entry::new("tuxlink-pat", callsign).set_password(&pw)?     │
│     │ 3. write ~/.config/pat/config.json (no password field)          │
│     │                                                                 │
└────────────────────────────────────┬──────────────────────────────────┘
                                     │ writes (service, account, pw) →
                                     ▼
                              ┌─────────────────────┐
                              │  OS keyring         │
                              │  ("tuxlink-pat",    │
                              │   "<callsign>")     │
                              │     → password      │
                              └──────────┬──────────┘
                                         │ ← reads (service, account)
                                         │
┌────────────────────────────────────────┴──────────────────────────────┐
│  cameronzucker/tuxlink-pat (Go; this patch)                           │
│                                                                       │
│  internal/credstore/credstore.go (NEW):                               │
│      const ServiceName = "tuxlink-pat"                                │
│      func Get(callsign string) (pw string, found bool, err error)     │
│                                                                       │
│  app/exchange.go — SetSecureLoginHandleFunc callback:                 │
│      Pat needs password for CMS-bound B2F secure-login                │
│        ├── credstore.Get(addr.Addr) → hit: use silently               │
│        ├── miss & addr is AuxAddr → credstore.Get(cfg.MyCall)         │
│        │   (preserves existing AuxAddr→primary fallback semantic)     │
│        └── still miss → promptHub.Prompt(PromptKindPassword)          │
│              (60s prompt; Pat's existing behavior, unchanged)         │
│                                                                       │
│  cfg/config.go:                                                       │
│      DELETE: SecureLoginPassword string `json:"secure_login_password"`│
│      DELETE: AuxAddr.Password *string + Address:Password marshal      │
│                                                                       │
│  cli/init.go:                                                         │
│      REPLACE password-write block with brief-redirect message         │
│                                                                       │
└───────────────────────────────────────────────────────────────────────┘
```

### 3.2 Component inventory

| Component | Owned by | What it does |
|---|---|---|
| `internal/credstore/credstore.go` | This patch (NEW) | Pkg-level Go module wrapping `github.com/zalando/go-keyring`. Exports: `Get(callsign string) (pw string, found bool, err error)`. Normalizes `keyring.ErrNotFound` → `found=false, err=nil`. Constant `ServiceName = "tuxlink-pat"`. ~50-80 LoC. |
| `internal/credstore/credstore_test.go` | This patch (NEW) | Unit tests via `keyring.MockInit()`. Cases: hit, miss, NotFound-is-miss, ServiceName-constant. ~80-120 LoC. Runs cross-platform on any CI. |
| `internal/credstore/credstore_integration_test.go` | This patch (NEW) | Build-tagged `//go:build integration`. Cases: real-keyring round-trip, locked-keyring soft-error. Runs only with `go test -tags=integration`. ~60-100 LoC. |
| `cfg/config.go` | This patch (MODIFY) | Delete `SecureLoginPassword` field (line 53). Delete `AuxAddr.Password` field (line 23) + custom MarshalJSON/UnmarshalJSON parsing `Address:Password` form (lines 26-43). `AuxAddr` becomes a struct with just `Address string`. |
| `app/exchange.go` | This patch (MODIFY) | Rewrite `SetSecureLoginHandleFunc` callback (lines 175-192): credstore-first, AuxAddr-fallback-to-primary, promptHub-as-last-resort. ~25 LoC change. Keyring lookups consistently key by `addr.Addr` (bare callsign of the incoming fbb.Address); `aux.Address` is no longer the keyring account string. |
| `app/app.go` | This patch (MODIFY) | Delete lines 230-233 (the `if !strings.EqualFold(a.options.MyCall, a.config.MyCall) { a.config.SecureLoginPassword = "" }` block — was clearing the in-memory password when CLI `-mycall` differed from config; obsolete because `cfg.SecureLoginPassword` no longer exists and keyring lookups are naturally keyed by the actual callsign in use). |
| `app/winlink_api.go` | This patch (MODIFY) | Rewrite line 72: replace `a.Config().SecureLoginPassword` with `credstore.Get(a.Options().MyCall)`. If keyring miss + promptHub-style fallback not available in this code path, return error (password-recovery requires a known password). |
| `api/winlink_account.go` | This patch (MODIFY) | Rewrite line 65: replace `password = h.Config().SecureLoginPassword` with `password, _, _ = credstore.Get(...)`. Lines 45 (length validation) + 49 (`cmsapi.AccountAdd`) unchanged — they validate + consume the password var. |
| `api/api.go` | This patch (MODIFY) | Delete lines 414-416 (RedactedPassword set on read) + lines 435-436 (RedactedPassword-to-real swap on write). The field these redacted no longer exists; the redaction machinery has no other consumer in this codebase (verified by grep). |
| `cli/init.go` | This patch (MODIFY) | Lines 193-258 deleted/replaced. Print brief-redirect: `"Skipping password — use the tuxlink wizard to set Winlink credentials. For standalone Pat usage, use upstream la5nta/pat which retains config.json passwords. See: https://github.com/cameronzucker/tuxlink-pat#credentials"`. `validatePassword` + `getPasswordRecoveryEmail` + `cmsapi.PasswordRecoveryEmailSet` calls REMOVED. Other configure steps (callsign, locator, mailbox path) proceed unchanged. |
| `cli/account.go` | This patch (MODIFY) | `getPasswordForCallsign` helper: replace `SecureLoginPassword`-first lookup with `credstore.Get`-first lookup. promptHub fallback unchanged. |
| `cli/prompter.go` | UNCHANGED | `case app.PromptKindPassword` (terminal-prompt handler) stays as-is. It consumes promptHub events; the promptHub call sites are what move. |
| `go.mod` / `go.sum` | This patch (MODIFY) | + `github.com/zalando/go-keyring vX.Y.Z` and transitive deps. |
| `README.md` (tuxlink-pat) | This patch (MODIFY) | + new "## Credentials" section: tuxlink wizard is the credentials entry point; for standalone Pat usage, use upstream la5nta/pat; explain the `(service="tuxlink-pat", account="<callsign>")` keyring scheme briefly; note Linux is the v0.0.1 tested platform. |
| `.github/workflows/test.yml` or equivalent | This patch (MODIFY or CREATE) | Add integration-test job: `apt install libsecret-1-dev`, `dbus-launch -- gnome-keyring-daemon --start ... && go test -tags=integration ./internal/credstore/`. ~40 lines of YAML. |

### 3.3 Data flow — keyring read path

Keyring entries are keyed by the **bare callsign string** (`addr.Addr` for any session; `cfg.MyCall` for the primary-fallback path). The custom `aux.Address` string (which may carry a host suffix like `CALL@host`) is NOT used as the keyring account — using `addr.Addr` consistently gives one keyring entry per callsign regardless of how `aux.Address` was written in config.

For each CMS-bound B2F secure-login event:

1. fbb session needs password for `fbb.Address addr`.
2. Pat's `SetSecureLoginHandleFunc` callback is invoked.
3. **Step 1:** `pw, found, err := credstore.Get(addr.Addr)`.
   - If `found && err == nil` → return `pw` (silent; no log line).
   - If `err != nil`:
     - Soft error (locked) → `log.Warn(...)` with structured fields → continue to Step 2.
     - Hard error (D-Bus unreachable, no secret-service) → `log.Error(...)` with structured fields → continue to Step 2.
4. **Step 2 (AuxAddr fallback to primary):** if `addr.Addr != cfg.MyCall` (i.e., this is an AuxAddr session) AND the AuxAddrs list contains a matching `aux.Address` (via the existing `addr.EqualString(aux.Address)` check), retry: `pw, found, err = credstore.Get(cfg.MyCall)`.
   - If `found && err == nil` → return `pw` (preserves existing semantic where AuxAddr.Password == nil falls to cfg.SecureLoginPassword).
   - Otherwise → continue to Step 3.
5. **Step 3 (promptHub):** `resp := <-promptHub.Prompt(ctx, time.Minute, PromptKindPassword, "Enter secure login password for "+addr.String())`. Return `resp.Value, resp.Err`. Behavior unchanged from Pat 1.0.0.

**Note on v0.0.1 AuxAddr usage:** the v0.0.1 tuxlink wizard does NOT write AuxAddr keyring entries (wizard handles a single callsign only). Multi-account power-users may manually populate `(service="tuxlink-pat", account=AUXCALLSIGN)` keyring entries via OS tools (`secret-tool`, Seahorse) or via a future tuxlink multi-account UX. v0.0.1's Step 1 lookup for AuxAddrs typically misses → Step 2 fallback to primary callsign — same behavior as today's "AuxAddr.Password is nil; use SecureLoginPassword" code path.

### 3.4 Data flow — wizard write path (referenced; OUT OF SCOPE for this patch)

Documented here for completeness (this patch unblocks `tuxlink-ko0`):

1. Operator runs tuxlink. Wizard screen 2 collects callsign + password.
2. Wizard calls Rust `keyring` crate: `keyring::Entry::new("tuxlink-pat", callsign).set_password(&pw)?`.
3. Wizard writes `~/.config/pat/config.json` containing callsign + non-secret config; `secure_login_password` field is absent.
4. Wizard completes; tuxlink spawns Pat for test send; Pat reads keyring via §3.3.

The wizard does NOT use any Pat-side CLI for credential setting. Pat-side `cli/init.go` no longer writes passwords (per this patch).

### 3.5 Error handling

Per §3.3, the keyring read path has 4 outcomes; only "hit" returns silently. The other 3 all degrade gracefully to promptHub.

**Logging policy:**

- **Hit:** no log line (consistent with today's config.json silent-use).
- **Miss:** no log line (consistent with today's `SecureLoginPassword == ""` silent fall-through to promptHub).
- **Soft error** (keyring locked, no password granted): ONE structured `level=warn` log line:
  ```
  level=warn msg="credstore: keyring lookup failed; falling back to prompt"
        callsign=KK6XYZ err="default keyring is locked"
  ```
- **Hard error** (D-Bus unreachable, no secret-service installed): ONE structured `level=error` log line. Same format; different level reflects configuration problem (not transient).

**Explicit NOT-decisions:**

- No retry-loop on keyring lookup failure. One attempt; fall through.
- No auto-prompt to unlock the keyring. Operator uses OS tools (Seahorse, Keychain Access).
- No auto-save of prompted password to keyring. Avoids "did I just save a wrong password?" UX issue; wizard is the sole writer.
- No "first time?" hint suggesting `tuxlink wizard`. Pat shouldn't reference tuxlink explicitly; the README covers it.
- No error-class-specific UX path. Whether keyring is missing, locked, or transient-failed, the user-facing experience is "promptHub fires."

**Per-call-site error handling:**

- `app/exchange.go::SetSecureLoginHandleFunc`: as in §3.3. promptHub fallback always available.
- `app/winlink_api.go::passwordRecoveryEmailSet`: if `credstore.Get` miss → return error to caller (password-recovery flow needs a known password; can't promptHub-fallback here because the caller is an API handler, not an interactive session).
- `api/winlink_account.go`: same as above — return error to API caller on credstore miss.
- `cli/account.go::getPasswordForCallsign`: uses existing promptHub fallback (this helper was designed for the SecureLoginPassword-then-prompt pattern).

### 3.6 Testing

**Layer 1 — Unit tests** (`internal/credstore/credstore_test.go`):
- Uses zalando's `keyring.MockInit()` (built-in test helper; swaps the backend with an in-memory implementation for the test process).
- Test cases:
  - `TestGet_Hit` — Set then Get; verify password matches; verify `found=true, err=nil`.
  - `TestGet_Miss` — Get for an unset callsign; verify `found=false, err=nil` (NOT a hard error).
  - `TestGet_NotFoundIsMiss` — verify `keyring.ErrNotFound` is mapped to `found=false, err=nil` (not propagated as hard error).
  - `TestServiceConstant` — verify `ServiceName == "tuxlink-pat"` (prevents accidental rename).
  - `TestGet_EmptyCallsign` — verify behavior when called with `""`; should return `found=false, err=nil` (no panic; no keyring lookup with empty account).
- Runs on any CI runner cross-platform; no D-Bus, no real keyring required.

**Layer 2 — Integration tests** (`internal/credstore/credstore_integration_test.go`):
- Build-tagged: `//go:build integration`. Only runs with `go test -tags=integration`.
- Test cases:
  - `TestRealKeyring_RoundTrip` — Set then Get against the real OS keyring (whichever backend is available on the test machine).
  - `TestRealKeyring_Cleanup` — verify entries can be deleted after test (avoids polluting the runner's keyring).
- CI: a single Linux job runs the integration tests under `dbus-launch -- gnome-keyring-daemon --start --components=secrets ...`. ~40 lines of YAML; based on aws-vault's pattern.
- macOS / Windows integration runs are NOT in v0.0.1 scope. Credstore code compiles on those platforms (zalando provides the backends); test coverage on those platforms is a future v0.X CI matrix expansion.

**Layer 3 — `app/exchange.go` callback test** (modified existing test if present, else new):
- Uses credstore's `MockInit`-backed test. Test setup: primary callsign + (optionally) an AuxAddr keyring entry, both keyed by bare callsign per §3.3.
- Test cases:
  - `TestSecureLoginCallback_PrimaryHit` — callback receives primary fbb.Address; credstore has entry for the bare callsign; returns password.
  - `TestSecureLoginCallback_PrimaryMiss_PromptHub` — callback receives primary; credstore miss; promptHub test-handler returns sentinel; verify sentinel propagates.
  - `TestSecureLoginCallback_AuxHit` — callback receives AuxAddr's fbb.Address; credstore has an entry for the AuxAddr's bare callsign (manually pre-populated to simulate the future multi-account UX); returns AuxAddr's password.
  - `TestSecureLoginCallback_AuxMiss_PrimaryFallback` — callback receives AuxAddr; credstore miss for AuxAddr's callsign but hit for primary; returns primary password (the v0.0.1 typical case).
  - `TestSecureLoginCallback_AllMiss_PromptHub` — both miss; promptHub fires.
  - `TestSecureLoginCallback_UnknownAddr_PromptHubDirectly` — callback receives an fbb.Address that's neither primary nor in AuxAddrs; skips the fallback path; promptHub fires directly.

**Layer 4 — Config-parse regression tests:**
- `TestConfigParse_NoSecureLoginPassword` — parse a config.json without `secure_login_password` field; verify no error.
- `TestConfigParse_LegacySecureLoginPassword` — parse a config.json WITH `secure_login_password`; verify the field is silently ignored (json.Unmarshal is permissive about unknown fields by default). Documents in README that the field is no longer honored.
- `TestConfigParse_LegacyAuxAddrPassword` — parse a config.json with `auxiliary_addresses: ["CALL:password"]` (the old Address:Password marshal form); verify it parses (without throwing) and the password portion is silently dropped (consequence of removing the custom UnmarshalJSON). Document the format change in README.

**Layer 5 — Brief-redirect message test:**
- `TestPatConfigure_BriefRedirectAtPasswordStep` — invoke `cli/init.go::configureHandle` flow; verify the redirect message is printed to stdout; verify subsequent configure steps proceed.

**Test scope NOT included:**

- End-to-end smoke against live CMS (per RADIO-1; live-CMS is operator-only).
- macOS / Windows keyring tests (future v0.X scope).
- tuxlink-side wizard tests (Task 9's responsibility).

### 3.7 Build / deploy impacts

**tuxlink-pat (Go) side:**
- `go.mod`: + `github.com/zalando/go-keyring vX.Y.Z` (~5-10 transitive deps).
- `make.bash`: unchanged. Go build chain handles new dep via `go build`.
- CI workflow on tuxlink-pat: add integration-test job under `dbus-launch`. ~40 lines YAML.
- No change to tuxlink-pat binary's runtime entry point.

**tuxlink (Rust/Tauri) side:**
- This patch does NOT touch tuxlink. Submodule bump (separate PR against `feat/v0.0.1`) updates the SHA only — no Rust code changes.
- Wizard Rust-side keyring write is `tuxlink-ko0`'s scope.

**AppImage build (CI):**
- `apt install libsecret-1-dev` on CI runners if integration tests run there.
- Runtime AppImage dep (`libsecret-1-0`) → tracked by `tuxlink-gdo`. NOT in this patch.

**Local dev:**
- `docs/development.md` (tuxlink-side) + tuxlink-pat README: "For local dev: ensure `libsecret-1-dev` is installed (Debian/Ubuntu) and `gnome-keyring-daemon` or equivalent is running. macOS/Windows: keyring code compiles but is untested in v0.0.1."

### 3.8 Commit shape

This patch lands as TWO PRs across two repos:

**PR-A (against `cameronzucker/tuxlink-pat/master`):** the cred-handling refactor itself.

- Title: `[shoal-condor-clover] refactor(cred): pat reads WL2K from OS keyring (closes tuxlink-mib partial)`
- Body: cites this spec + the plan + adrev transcripts (gitignored).
- Scope: all changes in §3.2 component inventory.
- Branch: `bd-tuxlink-mib/mib-cred-keyring` on the FORK (tuxlink-pat repo, not tuxlink).
- Branch retained (not deleted) per fork-setup spec §3.2 — preserves cherry-pick for future upstream PR.

**PR-B (against `cameronzucker/tuxlink/feat/v0.0.1`):** the submodule pin bump.

- Title: `[shoal-condor-clover] build(pat): bump tuxlink-pat submodule to include cred-refactor (closes tuxlink-mib)`
- Body: cites the merged PR-A.
- Scope: `external/tuxlink-pat` submodule SHA bump only.
- Branch: a new tuxlink-side branch (e.g., `bd-tuxlink-mib/submodule-bump`).
- Branch deleted on merge (tuxlink convention).

Sequencing: PR-A merges first (after operator review). Agent then opens PR-B referencing the new tuxlink-pat SHA. Operator reviews + merges PR-B. `tuxlink-mib` closes on PR-B merge.

Both commits include `Agent: shoal-condor-clover` + `Co-Authored-By:` trailers. Heredoc commit-message syntax per CLAUDE.md.

## 4. Decisions captured during brainstorm

### 4.1 Scope: v0.0.1-tuxlink-only

**Decision:** Design the patch for v0.0.1 tuxlink consumption only. Keyring-only steady state; no migration tool; no backward compat with existing config.json passwords (no existing installs). Upstream PR comes LATER as a separate, more-conservative design.

**Reasoning:** v0.0.1 tuxlink has no existing installs. The wizard (Task 9) is the sole keyring writer. The simpler scope minimizes the patch surface, the test matrix, and the adrev surface. The upstream PR variant would need additive feature flags, migration tools, and explicit backward-compat — none of which v0.0.1 tuxlink requires. Per ADR 0011 §4, upstream contribution is pursued AFTER the fork-side patch ships; the fork can be opinionated.

**Alternatives considered + rejected:**
- *Upstream-PR-ready from day one:* would design fork-side patch to also serve as the upstream PR candidate. Larger surface; longer pipeline; first iteration is also the upstream pitch. Rejected: complicates the scope; over-thinking upstream acceptance before fork-side ships proves the design.
- *Hybrid (silent auto-migration of config.json passwords):* Pat detects config.json password on startup, writes to keyring, blanks the field, continues. Rejected: silent rewrite of user's config file is surprising; no current users to migrate.

### 4.2 Keyring naming: `service="tuxlink-pat"`, `account="<callsign>"`

**Decision:** Hardcoded service name `"tuxlink-pat"`; account is the bare callsign for primary, or the literal `AuxAddr.Address` for aux accounts.

**Reasoning:** Researched OSS prior art 2026-05-18 (8 production CLIs examined). The dominant pattern is **service = hardcoded tool name** (often with `:`-joined sub-namespace; e.g., GitHub CLI's `gh:<hostname>`, aws-vault's `aws-vault` + profile, HashiCorp Vault's `Vault-token: <addr>`), **account = user-identifying string** (username, profile name, server+user tuple). `account="default"` is unprecedented in the corpus.

Option A (`"tuxlink-pat"` + callsign):
- ✅ Matches GitHub CLI / aws-vault / HashiCorp Vault convention.
- ✅ Multi-account-ready (one entry per callsign; AuxAddrs each get own entries).
- ✅ Distinct namespace from any hypothetical upstream Pat keyring impl.
- ✅ User-visible in keyring UI (Seahorse, Keychain Access) as `tuxlink-pat` entries.

**Alternatives considered + rejected:**
- *`service="pat"`*: matches the binary name but risks namespace collision if upstream Pat ever adds keyring auth with different schema. Inferior unless we commit to upstream-PR-as-primary (we didn't, per §4.1).
- *`service="tuxlink-pat"`, `account="default"` (single-account-only)*: simpler but unprecedented; forecloses multi-account; would require schema change later.

**Prior-art sources** (inline summary; full per-tool detail in §6 References):
- [cli/cli internal/config/config.go (keyringServiceName)](https://github.com/cli/cli/blob/trunk/internal/config/config.go) — `service = "gh:" + hostname`
- [99designs/aws-vault cli/global.go (keyringConfigDefaults)](https://github.com/99designs/aws-vault/blob/master/cli/global.go) — `service = "aws-vault"`, account = AWS profile name
- [docker/docker-credential-helpers credentials.CredsLabel](https://github.com/docker/docker-credential-helpers/blob/master/credentials/credentials.go) — `schema = "io.docker.Credentials"`, multi-attribute tuple
- [git-credential-libsecret schema](https://github.com/git/git/blob/master/contrib/credential/libsecret/git-credential-libsecret.c) — `schema = "org.git.Password"`
- [joemiller/vault-token-helper store](https://github.com/joemiller/vault-token-helper/blob/master/pkg/store/store.go) — `label = "Vault-token: " + addr`

### 4.3 Architecture: keyring as cache + promptHub fallback

**Decision:** The keyring is a SECURE CACHE for the password that the wizard (or one-shot prompt) captured — NOT a hard startup gate. Pat's existing `promptHub.Prompt(PromptKindPassword)` (app/exchange.go:188) is the universal graceful-degradation fallback. Pat falls through to promptHub for: missing keyring entry (normal first-run), locked keyring (soft error), D-Bus unreachable (hard error).

**Reasoning:** Pat's existing code at `app/exchange.go:176-189` already handles "no password configured" by falling through to a 60-second promptHub prompt. P2P operations don't read the password at all. CMS auth happens server-side (Pat doesn't validate locally). The EmComm stand-up scenario (operator boots fresh laptop at Emergency Operations Center, needs to operate radio quickly, may or may not have CMS access) is already handled by Pat's existing architecture — the keyring refactor preserves it.

This is the architectural insight that simplified the design: the keyring layer is a SOURCE for the password Pat needs, not a GATE on Pat's operation. Failure to read the keyring is equivalent to today's "user didn't configure a password" state — promptHub handles it.

**Alternatives considered + rejected:**
- *Stricter: no promptHub fallback when keyring is the configured source.* Pat refuses to prompt when keyring lookup fails. Forces operators to use the wizard or `pat configure`. Rejected: breaks EmComm stand-up (operator at EOC without time to set up keyring).
- *Add `level=warn` log when keyring entry is missing (vs. legitimate-first-run vs. drift).* Rejected: noise; the structured warn-on-soft-error + error-on-hard-error logging already provides drift visibility.

### 4.4 Pat-CLI surface: wizard-only writer

**Decision:** The tuxlink wizard is the SOLE keyring writer in v0.0.1. `cli/init.go`'s `pat configure` password-write block is removed. `pat configure` (if invoked standalone) skips the password step and prints a brief redirect message pointing to the tuxlink wizard or upstream `la5nta/pat`.

**Reasoning:** tuxlink-pat is positioned as tuxlink's engine, NOT a standalone Pat replacement. Users who need standalone-CLI Pat usage should use upstream `la5nta/pat` (which retains config.json passwords and the existing `pat configure` UX). Removing the Pat-CLI cred-setting path:
- Simplifies the patch (smaller diff; tighter test surface).
- Eliminates two-writer ambiguity (wizard vs. `pat configure`).
- Reinforces the "tuxlink-pat is tuxlink's engine" framing.
- Reduces upstream-merge friction (less Pat-side code to maintain that differs from upstream).

The upstream PR variant (later, separate design per §4.1) re-introduces `pat configure` keyring writes for that audience.

**Alternatives considered + rejected:**
- *Both writers (`pat configure` + wizard):* larger scope; supports the hypothetical "users adopt tuxlink-pat for the security improvement, run Pat standalone" audience. Rejected: that audience doesn't exist for v0.0.1; if it emerges, the upstream-PR variant serves it better.
- *Silent skip (no redirect message):* cleaner CLI output but unhelpful for confused users. Rejected: brief message costs nothing and serves the operator.

### 4.5 Go library: `github.com/zalando/go-keyring`

**Decision:** Use `github.com/zalando/go-keyring`.

**Reasoning:**
- Pure Go for Linux secret-service (no CGO needed); uses Apple `security` CLI for macOS; wincred for Windows.
- Simple API: `keyring.Set(service, account, password)` / `keyring.Get(service, account)`.
- Narrow scope: OS-only backends, no file-fallback (matches Cameron's "no disk creds" memory).
- Built-in `keyring.MockInit()` for unit tests.
- ~30 transitive deps; light footprint.
- Maintained; common choice for small/medium Go CLIs.
- License: MIT (compatible with Pat's MIT).

**Alternatives considered + rejected:**
- *`github.com/99designs/keyring`:* used by aws-vault; multi-backend (incl. file-encrypted, KWallet, pass). More elaborate API. Rejected: the file-encrypted backend is a "disk creds" opt-in that must be explicitly disabled; risk of accidental enablement. zalando's narrower scope is the safer default.
- *Defer library choice to impl-time research.* Rejected: spec self-review would flag the unresolved decision; choosing now lets the adrev attack the specific library.

### 4.6 Platform scope: Linux-only for v0.0.1 (cross-platform code compiles)

**Decision:** v0.0.1 commits to Linux as the tested + supported platform. The credstore code uses zalando/go-keyring's cross-platform API; it compiles + runs on macOS and Windows but is NOT tested by v0.0.1's CI. If/when tuxlink expands to those platforms (future v0.X), the cred-refactor design needs no changes — only CI matrix + docs grow.

**Reasoning:** tuxlink v0.0.1's deliverable is the Linux AppImage (per fork-setup spec + plan). macOS and Windows builds aren't in v0.0.1 scope. The keyring library happens to support all 3 platforms; we don't need to actively foreclose that. Per the YAGNI principle: design for the present commitment without foreclosing the plausible future.

This is honest about the v0.0.1 commitment (Linux-tested + supported) without committing scope we won't deliver (macOS / Windows tests + support).

**Alternatives considered + rejected:**
- *Stricter Linux-only via build tags:* exclude macOS/Windows code paths via Go build tags. Forces a conscious "we're adding macOS support" code change at platform expansion time. Rejected: zalando/go-keyring's cross-platform is a no-cost feature; foreclosing it adds work without benefit.
- *Commit to cross-platform-soon timeline in this spec:* premature; v0.0.1 hasn't shipped. Rejected.

## 5. Risks and watched failure modes

### 5.1 Build-time failures (fail at setup; loud)

- **`go mod tidy` fails fetching zalando/go-keyring:** standard Go module proxy issue; user sees clear error.
- **`libsecret-1-dev` missing on CI runner:** integration tests fail with linker error; CI workflow's setup step installs it; loud failure.
- **`dbus-launch` unavailable in CI:** integration test job errors out; standard CI dep issue; documented in workflow file.
- **Removed `RedactedPassword` machinery had other consumers:** would cause compile failure. Mitigation: verify by grep before patch (`grep -rn "RedactedPassword"` in tuxlink-pat); if other consumers exist, redesign that aspect.

### 5.2 Runtime failures (fail at use; gracefully degrade)

- **Keyring locked at session start:** `credstore.Get` → soft-error; `log.Warn`; promptHub fires; one-shot password.
- **D-Bus unreachable / no secret-service installed:** `credstore.Get` → hard-error; `log.Error`; promptHub fires.
- **Keyring entry has wrong/old password:** CMS rejects auth (existing Pat behavior); user re-runs wizard.
- **OS-keyring data corruption:** rare; `credstore.Get` → error; promptHub fires.

### 5.3 Rot-quietly-over-time failures

- **zalando/go-keyring deprecates API:** surfaced at next `go mod tidy`; standard dep-bump cycle. Mitigated by integration tests on every PR.
- **OS-vendor changes secret-service implementation:** zalando handles standard D-Bus interface; should be transparent. If not: fork-side patches credstore package; tuxlink-side unchanged.
- **Pat's promptHub mechanism changes upstream:** fork-side merge conflict at opportunistic-sync. Per fork-setup spec §3.3 step 4, resolved per-conflict during merge.
- **AppImage's bundled libsecret divergence from host:** ABI surprises if AppImage bundles older libsecret than host's daemon. Mitigation: pin libsecret version at AppImage build; `tuxlink-gdo` documents runtime dep.

### 5.4 Security failures

- **Memory dump of running Pat reveals plaintext password:** Pat holds password in memory during CMS session (same as today's config.json model — no change). Standard process-memory exposure; not mitigated by keyring layer.
- **`/proc/<pid>/environ` exposure:** No env var read by this design — no exposure risk.
- **Core dump leaks password:** same memory-exposure surface as above; not changed by this design.
- **Keyring backup leaks:** OS's responsibility (Gnome backup, macOS Time Machine, Windows backup). Not a tuxlink concern.
- **Log line leaks password:** verified by code review; no log statement includes the password value (only callsign + error context).

### 5.5 Cross-task transition risks

- **Existing `~/.config/pat/config.json` with `secure_login_password` set:** this patch removes the field from the Pat config struct. Existing config files deserialize successfully (json.Unmarshal ignores unknown fields by default), but the password value is no longer read. Operator is silently in "no creds set" state; promptHub fires on next CMS session. **Acceptable for v0.0.1** (no existing tuxlink-pat users). **Flagged** for the future upstream-PR variant.
- **Existing `auxiliary_addresses: ["CALL:password"]` (Address:Password marshal form):** custom UnmarshalJSON is removed; the colon-separated form is parsed as a literal Address string including the colon and password. Tests verify graceful handling. **Acceptable for v0.0.1; flagged for upstream PR.**
- **Task 6 (live-CMS smoke) resume:** requires this patch shipped. After PR-B merges, `tuxlink-nk7` unblocks; smoke binary uses the same credstore package.
- **Task 9 (wizard) resume:** requires this patch shipped. Wizard's Rust-side keyring write must use the same `(service="tuxlink-pat", account=callsign)` convention.

### 5.6 What's NOT a risk

- **End-user installation:** tuxlink AppImage is the user-facing artifact; keyring is OS-provided; no extra install for the user on Gnome/KDE distros.
- **Multi-process keyring access:** zalando uses per-call lookups (no held-open file handle); concurrent Pat + tuxlink processes coexist.
- **Long-running Pat process keyring re-reads:** Pat reads on each CMS-session start; no in-process caching beyond the session. Wizard updates take effect on next Pat session start.
- **macOS Keychain prompts on first access:** not a v0.0.1 concern (macOS not in scope; documented in §4.6 as future).
- **Windows CredentialManager UX surprises:** same as above.

## 6. References

- [ADR 0011 — Fork Pat as `tuxlink-pat`](../../adr/0011-fork-pat-for-tuxlink.md) — the strategic decision this patch operationalizes
- [Fork-setup spec](./2026-05-18-fork-setup-design.md) — the predecessor task that established the submodule + Go-build integration
- [Fork-setup plan](../../plans/2026-05-18-fork-setup-plan.md) — the predecessor task's implementation plan; the per-patch workflow (§3.3) is the operational template for this patch
- [ADR 0003 — Pat owns the mailbox](../../adr/0003-no-sqlite-pat-owns-mailbox.md) — Pat is the authoritative source of mailbox state (still holds; this patch doesn't change that)
- [ADR 0008 — Worktrees mandatory under bd-issue ownership](../../adr/0008-worktrees-mandatory-under-bd-issue-ownership.md) — applies to per-patch branches on tuxlink-pat (per fork-setup spec §5.5)
- [ADR 0010 — No-squash merge](../../adr/0010-no-squash-merge.md) — applies to tuxlink-pat's PR model
- [`docs/live-cms-testing-policy.md`](../../live-cms-testing-policy.md) — relevant context for `tuxlink-nk7` (Task 6 live-CMS smoke), which unblocks on this patch
- `bd show tuxlink-mib` — this task's bd record (claimed by shoal-condor-clover 2026-05-18)
- `bd show tuxlink-ko0` / `tuxlink-nk7` / `tuxlink-gdo` / `tuxlink-54p` — downstream tasks unblocked by this patch's PR-B merge
- Pat source-code references (in submodule at `external/tuxlink-pat/`):
  - [`app/exchange.go:173-191`](../../../external/tuxlink-pat/app/exchange.go) — `SetSecureLoginHandleFunc` callback (the primary keyring-read site)
  - [`cfg/config.go:53`](../../../external/tuxlink-pat/cfg/config.go) — `SecureLoginPassword` field (DELETE)
  - [`cfg/config.go:23-43`](../../../external/tuxlink-pat/cfg/config.go) — `AuxAddr.Password` + MarshalJSON/UnmarshalJSON (DELETE)
  - [`cli/init.go:193-258`](../../../external/tuxlink-pat/cli/init.go) — password-write block in `pat configure` (REPLACE)
  - [`api/api.go:414-436`](../../../external/tuxlink-pat/api/api.go) — RedactedPassword machinery (DELETE)
- [`github.com/zalando/go-keyring`](https://github.com/zalando/go-keyring) — the Go keyring library
- [`keyring` crate (Rust, for wizard's Task 9)](https://crates.io/crates/keyring) — for tuxlink-side write
- Adrev transcripts (forthcoming; gitignored per CLAUDE.md): `dev/adversarial/2026-05-18-cred-handling-adrev-R{1..5}.md`

## 7. Adrev disposition summary

*To be populated after 5-round adversarial review (≥1 cross-provider Codex round) per ADR 0011 §3.*
