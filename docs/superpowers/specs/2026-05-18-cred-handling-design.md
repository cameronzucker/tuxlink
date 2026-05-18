# Spec: cred-handling refactor — Pat reads WL2K from OS keyring (tuxlink-pat patch)

**Date:** 2026-05-18
**Agent:** shoal-condor-clover
**bd issue:** `tuxlink-mib` (P1; cred-handling refactor blocks Task 6 resume `tuxlink-nk7`, Task 9 wizard `tuxlink-ko0`, AppImage dep doc `tuxlink-gdo`, plan amendments `tuxlink-54p`)
**Branch:** `bd-tuxlink-mib/mib-cred-keyring` (worktree at `worktrees/bd-tuxlink-mib-mib-cred-keyring/`)
**Status:** Revised post 5-round adrev (4 Claude subagents + 1 Codex cross-provider; 50 findings; 8 P0 + 15 P1 + 11 P2 applied in this revision; see §7 for full disposition).
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

1. Replace `cfg.SecureLoginPassword` reads in `tuxlink-pat` with OS-keyring reads via a new `internal/credstore` package.
2. Remove `secure_login_password` JSON field from `cfg/Config`. Remove `Password *string` from `cfg/AuxAddr` BUT **preserve the custom MarshalJSON/UnmarshalJSON** — the JSON-string form (`"CALL"`) is the on-the-wire schema; we just need to strip-and-drop any legacy password portion from `"CALL:password"` on parse, never re-emit it on marshal. (R1+R3+R4 caught: removing the methods outright breaks all configs with `auxiliary_addresses`, including valid string-only ones.)
3. Remove the `RedactedPassword` API-redaction machinery from `api/api.go` (the field it protected no longer exists).
4. Update the **web config UI** (`web/src/config.html` + `web/src/js/config.js` + rebuild `web/dist/*`) to (a) remove the `secure_login_password` form field + redirect to wizard, and (b) reject any AuxAddr `"CALL:password"` POST payloads with a clear error. (R3+R4 caught: leaving the form in place after backend field removal silently drops user-saved passwords.)
5. Rewrite `app/exchange.go`'s `SetSecureLoginHandleFunc` to: (a) skip credstore lookup entirely for SMTP-proto addresses (per `fbb.Address` semantics), (b) try credstore via canonical-key normalizer (`normalizeAccount`) for Winlink-namespace addresses, (c) fall through to promptHub on miss/error. **No AuxAddr-fallback-to-primary** (dropped per R2 safety + R5 YAGNI: the fallback served power-users-against-intent and risked auth bypass when `cfg.MyCall` was empty).
6. Rewrite `cli/init.go` to skip BOTH password-write paths: (a) existing-account `pat configure` path (lines 193-258), AND (b) `handleNewAccount` / `promptNewPassword` / `cmsapi.AccountAdd` new-account creation path (cli/init.go:60-188). Both print the brief redirect ("Skipping password — use the tuxlink wizard..."). Other configure steps unchanged. (R4 caught: removing only 193-258 leaves the new-account creation path still asking for and submitting passwords with no keyring write.)
7. Rewrite `api/winlink_account.go` + `app/winlink_api.go` + `cli/account.go` to pull password from credstore where they currently read `cfg.SecureLoginPassword`. Handle the `(found bool, err error)` return values explicitly — never the `_, _ = credstore.Get(...)` discard pattern. On miss in non-interactive (API) contexts, return clear errors with `Cannot perform this operation without a keyring-stored password. Use the tuxlink wizard.` (R4 P2 caught the discard pattern hazard.)
8. Add `internal/credstore/credstore.go` with: `ServiceName = "tuxlink-pat"` constant; `normalizeAccount(callsign) → string` helper (trim + uppercase + reject empty/whitespace); `Get(callsign) → (pw, found, err)` with short-circuit on empty-after-normalize and treat-empty-stored-as-miss; explicit error classification (locked-keyring vs D-Bus-unreachable vs missing-entry).
9. Add `github.com/zalando/go-keyring` to `go.mod`.
10. Update `tuxlink-pat/README.md` with a new "Credentials" section pointing to the tuxlink wizard (no fragile URL anchor — just point at the README path).
11. Update `tuxlink-pat`'s CI to run integration tests via `dbus-run-session -- bash -c "..."` wrapping (per R4 P2: bare `dbus-launch && go test` doesn't carry the session bus into the test process). Linux only in v0.0.1.
12. Add unit tests (`internal/credstore/credstore_test.go`) using zalando's `keyring.MockInit()` — tests serialize (NOT `t.Parallel()`) because MockInit globally mutates package state (R3 F3 caught).
13. Add integration tests (`internal/credstore/credstore_integration_test.go`, build-tagged `integration`).
14. Add ONE config-parse regression test (`TestConfigParse_LegacyAuxAddrPasswordStripped`) — verifies legacy `auxiliary_addresses: ["CALL:password"]` parses without error AND password portion is dropped from in-memory `Address` (NOT exposed). Per R5 YAGNI, the previously-spec'd 3-test legacy-field battery is collapsed; default `json.Unmarshal` permissive behavior is a Go-stdlib guarantee, not test surface.

**Out of scope** (cross-linked to other bd issues):

- Tuxlink wizard's Rust-side keyring write (`tuxlink-ko0`, Task 9 — blocked-on-this).
- Live-CMS smoke binary's keyring-read code (`tuxlink-nk7`, Task 6 — blocked-on-this).
- AppImage `libsecret-1-0` system-package dep documentation (`tuxlink-gdo` — blocked-on-this).
- v0.0.1 plan amendments for Tasks 5/6/9/11 (`tuxlink-54p` — blocked-on-this).
- Upstream PR to `la5nta/pat` (a more-conservative variant; new bd issue post-merge per ADR 0011 §4).
- macOS / Windows CI integration tests for the keyring backend (future v0.X platform expansion; credstore code compiles on those platforms but is untested in v0.0.1).
- Multi-account wizard UX in tuxlink (future tuxlink work; this patch supports Pat-side multi-account via AuxAddrs but the wizard handles single account in v0.0.1).
- Re-introducing the `validatePassword` + `getPasswordRecoveryEmail` + `cmsapi.PasswordRecoveryEmailSet` first-time-setup flow as separate `pat` subcommands. Not in this patch; can be revisited if standalone-Pat usage becomes a real audience for the fork.
- Rebuilding `web/dist/*` as part of this PR — the spec's §3.2 lists the web/src changes; the dist rebuild step is part of the implementation plan, not a design decision. (Mentioned here so reviewers don't ask.)
- Auto-stripping `secure_login_password` from existing user config files (R2 P2 finding: legacy plaintext persists on-disk if config isn't rewritten). v0.0.1 has no existing users; flagged for upstream-PR variant.

## 3. Design

### 3.1 Architecture overview

```
┌───────────────────────────────────────────────────────────────────────┐
│  cameronzucker/tuxlink (Rust/Tauri)                                   │
│                                                                       │
│  Wizard (Task 9, tuxlink-ko0 — NOT in this patch)                     │
│     │                                                                 │
│     │ 1. collect callsign + password from user                        │
│     │ 2. Rust `keyring` crate (current `keyring-core` API):           │
│     │      use_native_store(true)?;                                   │
│     │      let entry = Entry::new("tuxlink-pat",                      │
│     │                              &normalize(callsign))?;            │
│     │      entry.set_password(&pw)?;                                  │
│     │      (normalize = trim + uppercase the bare callsign)           │
│     │ 3. write ~/.config/pat/config.json (no password field)          │
│     │                                                                 │
└────────────────────────────────────┬──────────────────────────────────┘
                                     │ writes (service, account, pw) →
                                     ▼
                              ┌─────────────────────┐
                              │  OS keyring         │
                              │  ("tuxlink-pat",    │
                              │   "<NORMALIZED      │
                              │    BARE CALLSIGN>") │
                              │     → password      │
                              └──────────┬──────────┘
                                         │ ← reads (service, account)
                                         │
┌────────────────────────────────────────┴──────────────────────────────┐
│  cameronzucker/tuxlink-pat (Go; this patch)                           │
│                                                                       │
│  internal/credstore/credstore.go (NEW):                               │
│      const ServiceName = "tuxlink-pat"                                │
│      func normalizeAccount(callsign) → string  (trim+upper)           │
│      func Get(callsign) → (pw, found, err)                            │
│         (short-circuits empty-after-normalize; treats empty-stored    │
│          password as miss; classifies error → soft/hard)              │
│                                                                       │
│  app/exchange.go — SetSecureLoginHandleFunc callback:                 │
│      Pat needs password for CMS-bound B2F secure-login                │
│        ├── if addr.Proto != "" (SMTP-proto, etc.) → skip credstore,   │
│        │   fall straight to promptHub (R3 caught: addr.Addr for       │
│        │   SMTP addresses is the FULL email, not a callsign)          │
│        ├── credstore.Get(normalizeAccount(addr.Addr)) → hit: silent   │
│        └── miss/error → promptHub.Prompt(PromptKindPassword)          │
│              (60s prompt; Pat's existing behavior, unchanged)         │
│                                                                       │
│  cfg/config.go:                                                       │
│      DELETE: SecureLoginPassword string `json:"secure_login_password"`│
│      MODIFY: AuxAddr — drop Password field; KEEP MarshalJSON/         │
│         UnmarshalJSON (string-form schema is the on-the-wire          │
│         contract; legacy "CALL:password" form parses but the password │
│         portion is stripped + dropped, never re-emitted)              │
│                                                                       │
│  cli/init.go:                                                         │
│      REPLACE both password-write paths (existing-account 193-258      │
│      AND new-account handleNewAccount/promptNewPassword/AccountAdd)   │
│      with brief-redirect message                                      │
│                                                                       │
│  web/src/config.html + web/src/js/config.js + web/dist/*:             │
│      REMOVE secure_login_password form field + JS handlers;           │
│      ADD inline "Set Winlink credentials via tuxlink wizard" hint;    │
│      AuxAddr CALL:password POST payloads rejected at API boundary     │
│                                                                       │
└───────────────────────────────────────────────────────────────────────┘
```

### 3.2 Component inventory

| Component | Owned by | What it does |
|---|---|---|
| `internal/credstore/credstore.go` | This patch (NEW) | Pkg-level Go module wrapping `github.com/zalando/go-keyring`. Exports: `const ServiceName = "tuxlink-pat"`; `normalizeAccount(callsign string) (string, bool)` — returns `("CALLSIGN", true)` on a valid callsign (after `TrimSpace` + `ToUpper`), or `("", false)` for empty/whitespace; `Get(callsign string) (pw string, found bool, err error)`. Get short-circuits when `normalizeAccount` returns `false` (no backend call). Get treats empty-stored password as `found=false` (per R3 F4: `Set("")` behavior differs across backends; uniform "treat empty as miss" closes the surface). Get classifies errors: `keyring.ErrNotFound` → `(found=false, err=nil)`; D-Bus-unreachable / no-secret-service → `(found=false, err=ErrUnavailable)`; locked-keyring → `(found=false, err=ErrLocked)`; anything else → `(found=false, err=<raw>)`. Sentinels exported for caller error-classification. ~80-120 LoC. |
| `internal/credstore/credstore_test.go` | This patch (NEW) | Unit tests via `keyring.MockInit()`. Tests serialize (NOT `t.Parallel()`) because MockInit globally mutates package state (R3 F3). Cases: hit, miss, NotFound-is-miss, ServiceName-constant, empty-callsign-short-circuit, whitespace-callsign-short-circuit, empty-stored-treated-as-miss, casing-normalization (write "kk6xyz"; read "KK6XYZ" → same entry). ~150-200 LoC. Runs cross-platform on any CI. |
| `internal/credstore/credstore_integration_test.go` | This patch (NEW) | Build-tagged `//go:build integration`. Cases: real-keyring round-trip; locked-keyring soft-error returning `ErrLocked`; no-secret-service returning `ErrUnavailable` (skip if D-Bus is present). Cleanup via `t.Cleanup(func() { keyring.DeleteAll(ServiceName) })`. Runs only with `go test -tags=integration`. ~80-120 LoC. |
| `cfg/config.go` | This patch (MODIFY) | Delete `SecureLoginPassword` field (line 53). MODIFY `AuxAddr`: drop the `Password *string` field; **PRESERVE** the custom `MarshalJSON`/`UnmarshalJSON` (string-form wire schema) — `MarshalJSON` always emits `a.Address` (never `Address:Password`); `UnmarshalJSON` accepts both `"CALL"` and `"CALL:password"` forms, dropping any colon-suffix portion on parse without populating any password storage. (R1+R3+R4 caught: removing the methods outright breaks all valid `["CALL"]` configs since the wire form is JSON string, not struct.) |
| `app/exchange.go` | This patch (MODIFY) | Rewrite `SetSecureLoginHandleFunc` callback (lines 175-192): (a) `if addr.Proto != "" { return promptHub.Prompt(...) }` — SMTP-proto addresses skip credstore entirely because `fbb.Address.Addr` for SMTP retains the full email, not a callsign (R3 F1); (b) `account, ok := credstore.NormalizeAccount(addr.Addr); if !ok { promptHub }`; (c) `pw, found, err := credstore.Get(account)` — on hit return silently; on miss/error fall through to promptHub; (d) NO AuxAddr-fallback-to-primary (dropped per R2+R5: auth-bypass risk when `cfg.MyCall` is empty; serves no v0.0.1 wizard-writer path). ~30 LoC change. |
| `app/app.go` | This patch (MODIFY) | Delete lines 230-233 (the `if !strings.EqualFold(a.options.MyCall, a.config.MyCall) { a.config.SecureLoginPassword = "" }` block — obsolete because the field is gone and keyring lookups are keyed by the active callsign per §3.3). |
| `app/winlink_api.go` | This patch (MODIFY) | Rewrite line 72: replace `a.Config().SecureLoginPassword` with `pw, found, err := credstore.Get(a.Options().MyCall)`. Handle ALL three return values: on `err != nil` return API error with sentinel-context (Locked/Unavailable/other); on `!found` return clean "no credential" error. Never the discard pattern `_, _, _ = credstore.Get(...)` (R4 P2 caught). |
| `api/winlink_account.go` | This patch (MODIFY) | Rewrite line 65: replace `password = h.Config().SecureLoginPassword` with explicit `pw, found, err := credstore.Get(...)` handling per the §3.5 per-call-site rules. Lines 45 (length validation) + 49 (`cmsapi.AccountAdd`) unchanged. |
| `api/api.go` | This patch (MODIFY) | Delete lines 404 (`const RedactedPassword`), 414-416, 435-436 (RedactedPassword machinery). The field these redacted no longer exists; the redaction machinery has no other Go consumer (verified by grep). The web UI's `[REDACTED]` placeholder semantics MOVE to the web-side update (next row). |
| `cli/init.go` | This patch (MODIFY) | TWO password-touching paths must be addressed (R4 caught: prior spec only addressed one): **(A) Existing-account path** — lines 193-258 replaced with brief redirect message + skip `validatePassword`/`getPasswordRecoveryEmail`/`cmsapi.PasswordRecoveryEmailSet` calls. **(B) New-account-creation path** — `InitHandle` no longer routes nonexistent callsigns into `handleNewAccount` (line 60); the call is replaced with the brief redirect ("Account creation requires the tuxlink wizard; or use upstream la5nta/pat for standalone Pat usage."). `handleNewAccount` + `promptNewPassword` functions are KEPT in the source for future use but become unreferenced — leave with a `// TODO: re-introduce as separate subcommand if standalone-Pat audience emerges (per spec §2)` comment. Other `pat configure` steps (callsign, locator, mailbox path) proceed unchanged. |
| `cli/account.go` | This patch (MODIFY) | `getPasswordForCallsign` helper: replace `SecureLoginPassword`-first lookup with `credstore.Get`-first lookup using `normalizeAccount`. promptHub fallback unchanged. Handle all 3 credstore return values explicitly. |
| `cli/prompter.go` | UNCHANGED | `case app.PromptKindPassword` (terminal-prompt handler) stays as-is. It consumes promptHub events; the promptHub call sites are what move. |
| **`web/src/config.html`** | **This patch (MODIFY) — NEW row** | Remove the `<input id="secure_login_password" ...>` form field (lines 82-85). Replace with a static info block: `Set Winlink credentials via the tuxlink wizard. For standalone Pat usage, use upstream la5nta/pat (see README).` |
| **`web/src/js/config.js`** | **This patch (MODIFY) — NEW row** | Remove all `secure_login_password` references (lines 176, 190, 347, 349, 354, 518). Specifically: line 518's `updatedConfig.secure_login_password = ...` in the POST-payload assembly is removed so the field is never sent to the backend. Add validation step that rejects any AuxAddr `"CALL:password"` form on form submission with an inline error message (defensive). |
| **`web/dist/*`** | **This patch (MODIFY) — NEW row** | Rebuild the prebuilt web assets (the dist directory is a committed build output in Pat's repo). Implementation plan handles the rebuild step; the design records that dist needs regeneration after src changes. |
| `go.mod` / `go.sum` | This patch (MODIFY) | + `github.com/zalando/go-keyring vX.Y.Z` (pinned during impl) and transitive deps. Dep count: small (run `go list -m all` post-add and confirm). |
| `README.md` (tuxlink-pat) | This patch (MODIFY) | + new "## Credentials" section: tuxlink wizard is the credentials entry point; for standalone Pat usage, use upstream la5nta/pat; explain the `(service="tuxlink-pat", account=NORMALIZED-BARE-CALLSIGN)` keyring scheme; note Linux is v0.0.1 tested platform. Plain README path link from `cli/init.go` redirect message (no `#credentials` fragment anchor — fragile to README restructure per R1 F5). |
| `.github/workflows/*.yml` (test or release) | This patch (MODIFY or CREATE) | Add integration-test job wrapped in **`dbus-run-session -- bash -c "..."`** (R4 P2: bare `dbus-launch && go test` doesn't carry session bus env). Specifically: `apt install libsecret-1-dev gnome-keyring`; `dbus-run-session -- bash -c "echo '' \| gnome-keyring-daemon --unlock --replace --daemonize && go test -tags=integration ./internal/credstore/..."`. CI runner image pinned (R1 F4: floating images = test rot). ~50 lines YAML. |

### 3.3 Data flow — keyring read path

**Canonical key.** All keyring read+write paths key by `normalizeAccount(addr.Addr)` which is `strings.ToUpper(strings.TrimSpace(callsign))` — same normalization on both writer and reader sides. Pat's existing `app.go:228`'s `strings.ToUpper(a.options.MyCall)` upper-cases the active callsign; the wizard (per `tuxlink-ko0`, OOS for this patch) must perform the same `TrimSpace + ToUpper` when calling `keyring::Entry::new("tuxlink-pat", &normalize(callsign))?`. The custom `aux.Address` string (which may carry a host suffix like `CALL@host`) is NOT used as the keyring account — only the bare callsign field via `addr.Addr`.

**Per-call-site classification.** The promptHub fallback only applies in **interactive** call sites (the `app/exchange.go::SetSecureLoginHandleFunc` callback runs during a session the user initiated). **API / HTTP-handler** call sites (`api/winlink_account.go`, `app/winlink_api.go::passwordRecoveryEmailSet`) cannot reasonably promptHub — they have no terminal attached; they return clear errors on credstore miss/error. The §3.5 per-call-site rules enumerate.

**Interactive read path (`SetSecureLoginHandleFunc` callback):**

For each CMS-bound B2F secure-login event:

1. fbb session needs password for `fbb.Address addr`.
2. Pat's `SetSecureLoginHandleFunc` callback is invoked.
3. **Pre-check 1 (SMTP-proto skip):** if `addr.Proto != ""` (non-empty Proto indicates SMTP-namespace address per `fbb.AddressFromString`, where `addr.Addr` holds the full email like `someone@example.org`, NOT a callsign), skip credstore entirely and go directly to Step 5 (promptHub). Credstore is for Winlink-namespace callsigns only. (R3 F1 caught.)
4. **Pre-check 2 (canonicalize):** `account, ok := credstore.NormalizeAccount(addr.Addr)`.
   - If `!ok` (empty or whitespace-only after trim) → log structured warn + go to Step 5 (promptHub).
   - Else continue with the normalized `account` string.
5. **Step 1 (credstore lookup):** `pw, found, err := credstore.Get(account)`.
   - If `found && err == nil && pw != ""` → return `pw` (silent; no log line). (Empty-pw case can't actually fire here because credstore.Get treats empty-stored as miss internally per §3.2; reasserted for clarity.)
   - If `err != nil`:
     - Soft error (`errors.Is(err, credstore.ErrLocked)`) → `log.Warn(...)` with structured fields → continue to Step 6.
     - Hard error (`errors.Is(err, credstore.ErrUnavailable)` or other) → `log.Error(...)` with structured fields → continue to Step 6.
   - If `!found && err == nil` (clean miss) → continue to Step 6 (no log line; consistent with today's silent fall-through).
6. **Step 2 (promptHub):** `resp := <-promptHub.Prompt(ctx, time.Minute, PromptKindPassword, "Enter secure login password for "+addr.String())`. Return `resp.Value, resp.Err`. Behavior unchanged from Pat 1.0.0.

**No AuxAddr-fallback-to-primary.** The previously-spec'd "if AuxAddr keyring entry is missing, try the primary callsign's entry" path is **DROPPED** per the cross-round adrev findings (R2 P0 #3 + R5 F1):

- The fallback served only power-users who manually populated `auxiliary_addresses` entries; the v0.0.1 wizard writes a single primary entry. Falling back to primary is *opposite* the operator's intent for multi-account setups (uses the WRONG password for the AuxAddr session).
- The fallback created an auth-bypass surface when `cfg.MyCall` was empty (e.g., `pat http` started with no callsign): an unauthenticated AuxAddr session could match an empty primary callsign and inappropriately receive primary's password.
- Power-users wanting multi-account today have a clean path: manually populate `(service="tuxlink-pat", account=<NORMALIZED-AUX-CALLSIGN>)` via OS keyring tools (Seahorse, `secret-tool`); future tuxlink multi-account wizard UX (post-v0.0.1) will write these entries. Missing AuxAddr entries fall through to promptHub (correct UX).

**API / HTTP-handler read path** (per `api/winlink_account.go` + `app/winlink_api.go::passwordRecoveryEmailSet`):

1. Handler receives request requiring a password for CMS auth (e.g., setting password-recovery email).
2. `pw, found, err := credstore.Get(normalizeAccount(<callsign-from-handler-context>))`.
3. If `err != nil` → return API error: `"Cannot perform this operation: keyring <Locked|Unavailable> (set credentials via the tuxlink wizard)"`.
4. If `!found` → return API error: `"Cannot perform this operation without a keyring-stored password. Use the tuxlink wizard to set credentials."`.
5. Else → proceed with CMS auth call.

NEVER the discard pattern `password, _, _ = credstore.Get(...)` — explicitly enforced (R4 P2 caught the prior spec's lapse).

**Note on v0.0.1 AuxAddr usage:** the v0.0.1 tuxlink wizard does NOT write AuxAddr keyring entries (wizard handles a single callsign only per Task 9 scope). Multi-account power-users today manually populate `(service="tuxlink-pat", account=AUXCALLSIGN)` keyring entries via OS tools (`secret-tool`, Seahorse). Missing AuxAddr entries fall through to promptHub — operator types the AuxAddr password once per CMS session (graceful degradation, no auto-bypass).

### 3.4 Data flow — wizard write path (referenced; OUT OF SCOPE for this patch)

Documented here for completeness (this patch unblocks `tuxlink-ko0`); the wizard's actual implementation is `tuxlink-ko0`'s scope, but it MUST honor the canonical-key normalization (§3.3) and the current `keyring`/`keyring-core` Rust API contract (R3 F5 caught the prior spec's wrong syntax):

1. Operator runs tuxlink. Wizard screen 2 collects callsign + password.
2. Wizard normalizes the callsign: `let account = callsign.trim().to_uppercase();` then validates non-empty.
3. Wizard initializes the keyring store (one-time at app init): `keyring::use_native_store(true)?` — required by the current `keyring-core` API; the implicit-OS-backend selection of older API versions is deprecated.
4. Wizard writes the keyring entry:
   ```rust
   use keyring_core::Entry;
   let entry = Entry::new("tuxlink-pat", &account)?;  // Result<Entry>, must ?
   entry.set_password(&pw)?;
   ```
   (R3 F5 caught: the chained-on-constructor syntax from the prior spec `keyring::Entry::new(...).set_password(&pw)?` does NOT compile against the current API — `Entry::new` returns `Result<Entry, Error>` and must be `?`-unwrapped before `.set_password` is callable.)
5. Wizard writes `~/.config/pat/config.json` containing callsign + non-secret config; `secure_login_password` field is absent; `auxiliary_addresses` (if any) is JSON-string form per AuxAddr's MarshalJSON.
6. Wizard completes; tuxlink spawns Pat for test send; Pat reads keyring via §3.3.

**Wizard does NOT clear passwords via `set_password("")`.** R3 F4 caught: `keyring.Set("")` is undocumented and per-backend (some store empty, some delete, Windows wincred may reject). The wizard's "clear credentials" UX (if added later) MUST call `entry.delete_credential()` explicitly, not `set_password("")`.

The wizard does NOT use any Pat-side CLI for credential setting. Pat-side `cli/init.go` no longer writes passwords (per this patch).

### 3.5 Error handling

Per §3.3, credstore.Get has 4 outcomes; the disposition depends on **call-site classification** (interactive vs API). The credstore package itself classifies errors via exported sentinels (`ErrLocked`, `ErrUnavailable`); callers use `errors.Is` to dispatch.

**Logging policy** (operator-confirmed `warn`-vs-`error` split):

- **Hit:** no log line (consistent with today's config.json silent-use).
- **Miss** (`found=false, err=nil`): no log line in interactive contexts (consistent with today's silent fall-through). API contexts log per their handler-error pattern.
- **Soft error** (`ErrLocked`): ONE structured `level=warn` log line:
  ```
  level=warn msg="credstore: keyring locked; falling back to prompt"
        callsign=KK6XYZ
  ```
- **Hard error** (`ErrUnavailable` or unclassified): ONE structured `level=error` log line. Same format; different level reflects configuration problem (not transient).

**Per-call-site error handling (the authoritative rules):**

| Call site | Class | On miss | On soft error | On hard error |
|---|---|---|---|---|
| `app/exchange.go::SetSecureLoginHandleFunc` callback | Interactive | promptHub | log.Warn + promptHub | log.Error + promptHub |
| `cli/account.go::getPasswordForCallsign` | Interactive (CLI) | promptHub | log.Warn + promptHub | log.Error + promptHub |
| `app/winlink_api.go::passwordRecoveryEmailSet` | API | Return `"missing credential"` error | Return `"keyring locked"` error | Return `"keyring unavailable"` error |
| `api/winlink_account.go::winlinkPasswordRecoveryEmailHandler` | API | Same as above | Same | Same |

API-context error messages MUST be operator-actionable: `"Cannot perform this operation: <reason>. Use the tuxlink wizard to set credentials."` — never just an internal-sounding string.

**Explicit NOT-decisions** (the load-bearing ones; trimmed per R5):

- **No retry-loop on keyring lookup failure.** One attempt; fall through. (Wrong-password retries against CMS = lockout risk.)
- **No auto-save of prompted password to keyring.** Avoids "did I just save a wrong password?" UX issue; wizard is the sole writer.

(R5 F5 trimmed 3 prior NOT-decisions — "auto-prompt to unlock," "first-time hint," "error-class-specific UX" — as defensive scaffolding that duplicates positive decisions elsewhere in the spec.)

### 3.6 Testing

**Layer 1 — Unit tests** (`internal/credstore/credstore_test.go`):
- Uses zalando's `keyring.MockInit()` (test helper; swaps backend with in-memory impl).
- **MockInit is process-global state** (R3 F3 caught): tests serialize — NO `t.Parallel()`. Each test uses `t.Cleanup(func() { keyring.DeleteAll(ServiceName) })` to avoid cross-test pollution.
- Test cases (~10 cases; ~150-200 LoC):
  - `TestGet_Hit` — Set then Get; verify password matches; `found=true, err=nil`.
  - `TestGet_Miss` — Get for an unset account; verify `found=false, err=nil`.
  - `TestGet_NotFoundIsMiss` — verify `keyring.ErrNotFound` maps to `found=false, err=nil`.
  - `TestGet_EmptyStoredTreatedAsMiss` — Set("") then Get; verify `found=false, err=nil` (R3 F4 caught: per-backend Set("") semantics).
  - `TestGet_EmptyCallsign_ShortCircuit` — Get(""); verify `found=false, err=nil` + verify backend NOT invoked.
  - `TestGet_WhitespaceCallsign_ShortCircuit` — Get("   "); same as above.
  - `TestGet_CasingNormalization` — Set("KK6XYZ", "pw"); Get("kk6xyz") returns "pw" (R2 F1 caught: wizard may write lowercase; reader uppercase).
  - `TestNormalizeAccount` — table-driven: `"  kk6xyz  " → "KK6XYZ", ok=true`; `"" → "", ok=false`; `"   " → "", ok=false`.
  - `TestServiceConstant` — verify `ServiceName == "tuxlink-pat"` (rename-protection).
  - `TestGet_ErrLockedClassified` / `TestGet_ErrUnavailableClassified` — using `keyring.MockInitWithError(...)` to inject specific errors; verify they propagate as `ErrLocked` / `ErrUnavailable` sentinels callers can `errors.Is`-dispatch.
- Runs cross-platform on any CI runner; no D-Bus required.

**Layer 2 — Integration tests** (`internal/credstore/credstore_integration_test.go`):
- Build-tagged: `//go:build integration`. Only runs with `go test -tags=integration`.
- Test cases:
  - `TestRealKeyring_RoundTrip` — Set then Get against the real OS keyring.
  - `TestRealKeyring_DeleteCleanup` — verify entries deleted after test.
- **CI invocation** (R4 P2 caught the prior spec's bare-`dbus-launch && go test` bug — daemon launched in new session but tests run in parent shell):
  ```yaml
  - name: install keyring deps
    run: sudo apt install -y libsecret-1-dev gnome-keyring dbus-x11
  - name: run integration tests in D-Bus session
    run: |
      dbus-run-session -- bash -c '
        echo "" | gnome-keyring-daemon --unlock --replace --daemonize
        go test -tags=integration ./internal/credstore/...
      '
  ```
- **CI runner image pinned** (R1 F4): `ubuntu-22.04` (not `ubuntu-latest`). Floating images = silent CI rot when actions or daemons update.
- macOS / Windows integration runs: NOT in v0.0.1 scope.

**Layer 3 — `app/exchange.go` callback test** (modified existing test if present, else new):
- Uses credstore's `MockInit`-backed test (serial, with Cleanup).
- Test cases:
  - `TestSecureLoginCallback_PrimaryHit` — callback receives primary fbb.Address; credstore has entry (keyed by NORMALIZED bare callsign); returns password silently.
  - `TestSecureLoginCallback_PrimaryMiss_PromptHub` — credstore miss; promptHub test-handler returns sentinel; verify propagation.
  - `TestSecureLoginCallback_SmtpProtoSkipsCredstore` — callback receives fbb.Address with `Proto="SMTP"` (e.g., `Addr="someone@example.org"`); verify credstore NOT invoked (MockInit's call counter); promptHub fires directly. (R3 F1 caught.)
  - `TestSecureLoginCallback_EmptyAddrSkipsCredstore` — callback receives fbb.Address with empty `Addr` (or whitespace-only); credstore NOT invoked; promptHub fires.
  - `TestSecureLoginCallback_AuxHit` — callback receives AuxAddr's fbb.Address; credstore has entry for the AuxAddr's NORMALIZED bare callsign (pre-populated); returns password silently. Covers the manual-multi-account power-user path.
  - `TestSecureLoginCallback_AuxMiss_PromptHub_NoFallbackToPrimary` — AuxAddr miss but primary has entry; verify the callback DOES NOT return primary's password (the dropped fallback per §3.3); promptHub fires for the AuxAddr. (Regression test for the dropped fallback per R2+R5.)
  - `TestSecureLoginCallback_KeyringLockedFallsToPrompt` — MockInitWithError(ErrLocked); verify warn-log + promptHub fires.

**Layer 4 — Config-parse regression test** (one test, simplified per R5 F2):
- `TestConfigParse_LegacyAuxAddrPasswordStripped` — parse a config.json with `auxiliary_addresses: ["CALL:password"]`; verify it parses without error; verify the AuxAddr's in-memory `Address` field is `"CALL"` (password stripped on parse via custom UnmarshalJSON); verify re-marshal produces `["CALL"]` form (password NEVER re-emitted). Single test covers the legacy-config compatibility surface.
- (Per R5 F2: the prior 3-test battery for "legacy secure_login_password silently ignored" / "no field" / "with field" was YAGNI — default `json.Unmarshal` permissive behavior is a Go stdlib guarantee, not a test surface. `app/config.go::ReadConfig` uses `json.Unmarshal(data, &config)` without `DisallowUnknownFields` — verified.)

**Layer 5 — DROPPED** (per R5 F3): the previous `TestPatConfigure_BriefRedirectAtPasswordStep` was anti-test territory (verifying a `fmt.Println` executed). Manual smoke via `pat configure` covers it during the implementation plan.

**Test scope NOT included:**
- End-to-end smoke against live CMS (per RADIO-1; operator-only).
- macOS / Windows keyring tests (future v0.X scope).
- tuxlink-side wizard tests (Task 9's responsibility; `tuxlink-ko0`).
- Web UI tests (browser-driven test framework not in tuxlink-pat's CI; manual smoke during implementation).

### 3.7 Build / deploy impacts

**tuxlink-pat (Go) side:**
- `go.mod`: + `github.com/zalando/go-keyring` (version pinned during impl). Dep footprint: small (confirm via `go list -m all` post-add; R1 F9 caught the prior spec's unsourced `~30 transitive deps` figure).
- `make.bash`: unchanged. Go build chain handles new dep via `go build`.
- CI workflow on tuxlink-pat: add integration-test job using `dbus-run-session -- bash -c "..."` wrapping (per R4 P2; details in §3.6 Layer 2). Pinned runner image (`ubuntu-22.04`, not `ubuntu-latest`).
- No change to tuxlink-pat binary's runtime entry point.

**tuxlink (Rust/Tauri) side:**
- This patch does NOT touch tuxlink. Submodule bump (separate PR against `feat/v0.0.1`) updates the SHA only — no Rust code changes.
- Wizard Rust-side keyring write is `tuxlink-ko0`'s scope.

**AppImage build (CI):**
- `apt install libsecret-1-dev gnome-keyring dbus-x11` on CI runners (libsecret-1-dev for build linkage; gnome-keyring + dbus-x11 for integration-test runtime).
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

### 4.7 Drop the AuxAddr-fallback-to-primary path (post-adrev decision)

**Decision:** The previously-spec'd "if AuxAddr keyring entry is missing, try the primary callsign's entry" fallback is REMOVED from the design.

**Reasoning:** Cross-round adrev convergence:
- **R2 P0 #3 (partial-input lens):** when `cfg.MyCall` is empty (legitimate at `pat http` startup with no callsign), the fallback fires `credstore.Get("")`. Combined with R3's normalization gap and the missing empty-check, this becomes an auth-bypass surface where an unauthenticated AuxAddr session could match the empty primary callsign.
- **R5 F1 (YAGNI lens):** the fallback serves only power-users who manually populated AuxAddr keyring entries. For them, falling back to primary is *opposite* their intent (uses the WRONG password for the AuxAddr session). v0.0.1 wizard writes a single primary entry; no v0.0.1 path benefits from the fallback.
- Cleaner v0.0.1 model: each callsign owns its own keyring entry; missing entries fall through to promptHub directly (correct UX). No legacy semantic to preserve.

**Replaces:** the original §4.3 mention that "AuxAddr.Password == nil falls to cfg.SecureLoginPassword" preservation. That preservation is dropped; modern callers (wizard + manual multi-account power-users) populate per-callsign entries.

### 4.8 Canonical key normalization (`normalizeAccount`)

**Decision:** All keyring read+write paths key by `normalizeAccount(callsign)` which is `strings.ToUpper(strings.TrimSpace(callsign))`. The function returns `("", false)` for empty-after-trim inputs; `(NORMALIZED, true)` otherwise. credstore.Get short-circuits to `(found=false, err=nil)` when normalization returns `false` — no backend call.

**Reasoning:** Cross-round adrev convergence:
- **R2 F1 (P0, partial-input):** wizard may write `"kk6xyz"` (operator typed lowercase); Pat reads via `addr.Addr` which is uppercase. libsecret/Keychain do exact-match account lookups; two different entries → silent miss → unexplained promptHub.
- **R3 F1 (P0, dep-contract):** `fbb.Address.Addr` is the bare callsign for Winlink addresses (uppercased), but is the FULL email for SMTP addresses (NOT uppercased). Normalization on the credstore boundary makes the lookup deterministic regardless of caller surface; SMTP-proto is filtered separately via `addr.Proto != ""` (§3.3).
- **R4 P2 #2 (Codex):** convergent finding — define a single trimmed/uppercased bare-callsign helper for every read/write.

**Wizard side responsibility** (per `tuxlink-ko0`): the Rust-side `normalize` helper must produce identical output to Go-side `normalizeAccount`. Implementation plan must verify byte-equivalent normalization with a shared test vector.

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
- **Hardcoded `ServiceName = "tuxlink-pat"` constant** (R1 F1): rots if the fork is ever renamed (e.g., transferred to a `tuxlink-org/tuxlink-pat`). No migration path designed — operators would re-run the wizard to re-populate keyring under the new ServiceName. Acceptable for v0.0.1; flagged for future rename event.
- **Hardcoded `time.Minute` promptHub timeout in `app/exchange.go:190`** (R3 F10): 60 seconds may not be enough for an EmComm operator whose hands are full (radio in progress) or a user looking up a password in a separate password manager. Not changed by this patch (preserves Pat 1.0.0 behavior). Flagged for follow-up to make configurable via Pat config.
- **CI runner image floats** (R1 F4): if the CI workflow uses `runs-on: ubuntu-latest` instead of pinned (e.g., `ubuntu-22.04`), action/dependency updates can silently change behavior of `gnome-keyring-daemon`, `dbus-run-session`, etc. Mitigated: §3.6 Layer 2 explicitly pins runner image.
- **No keyring schema versioning** (R1 F7): if a future tuxlink-pat patch adds a SECOND credential class (e.g., VARA HF station passwords keyed by callsign), the unversioned `(service="tuxlink-pat", account=CALLSIGN)` scheme has no way to coexist with the existing WL2K-password class on the same callsign. Acceptable for v0.0.1 (single class). Flagged for any future multi-class expansion.
- **Web UI / dist drift** (R3 F2 cascade): if a future Pat upstream merge re-introduces a `secure_login_password` form field (e.g., upstream adds a new variant), the fork's `web/dist/*` rebuild must catch it. Mitigated: integration test verifies `web/src/config.html` has no `secure_login_password` references.

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

(Per R5 F6: trimmed to load-bearing items only.)

- **End-user installation:** tuxlink AppImage is the user-facing artifact; keyring is OS-provided; no extra install for the user on Gnome/KDE distros.
- **Long-running Pat process keyring re-reads:** Pat reads on each CMS-session start; no in-process caching beyond the session. Wizard updates take effect on next Pat session start.

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

5-round adversarial review completed 2026-05-18 on commit `26a0ffb` (the pre-adrev spec draft). 4 Claude subagents per-lens (R1 scale, R2 partial-input, R3 dep-contract-drift, R5 YAGNI) + 1 Codex cross-provider (R4). **50 findings total: 8 P0, 20 P1, 17 P2, 5 P3.**

### Cross-provider / cross-round convergence

The strongest signal in adrev is when findings converge across blind spots. This cycle had **three high-value convergences:**

1. **Removing fields has unforeseen consumers** — R1 F2 (P0, scale: AuxAddr colon-form data destruction), R2 F4 (P0, partial-input: legacy AuxAddr password leaks into Address field), R3 F2 (P0, dep-contract: web UI POST silently drops `secure_login_password`), R3 F6 (P1, dep-contract: cmsapi.PasswordRecoveryEmailSet cascade), R4 P1 #1 (Codex cross-provider: AuxAddr unmarshalling), R4 P2 #3 (Codex: web UI), R4 P2 #6 (Codex: handleNewAccount residual flow). **Six findings across four rounds and two providers** point at the same architectural defect: the prior spec was too aggressive removing field-level types without auditing all consumers. Triggered the largest revision: §3.2 web/src additions, AuxAddr MarshalJSON preservation, handleNewAccount handling, explicit credstore return-value handling at API call sites.
2. **Keyring keying is wrong without canonicalization** — R2 F1 (P0, case normalization), R3 F1 (P0, addr.Addr SMTP-proto), R4 P2 #2 (Codex, canonicalization). Triggered §4.8 NEW decision (canonical-key normalizer) + §3.3 SMTP-proto skip.
3. **AuxAddr-fallback path is a real problem** — R2 P0 #3 (empty cfg.MyCall auth-bypass), R5 F1 (P1, YAGNI: serves no v0.0.1 path, opposite power-user intent). Triggered §4.7 NEW decision (drop the fallback).

These convergences are precisely what cross-provider adrev is designed to surface. The cycle earned its keep.

### Findings landed in this revision (all 8 P0 + 15 of 20 P1 + 11 of 17 P2)

| Finding | Round(s) | Severity | Action taken |
|---|---|---|---|
| AuxAddr colon-form removal = data destruction on write + legacy parse breakage | R1 F2, R2 F4, R3 P0 #4 (implicit), R4 P1 #1 | P0 / P0 / P1 | §2.2 changed from "remove custom MarshalJSON/UnmarshalJSON" to "preserve marshaling; drop Password field; strip-and-drop colon-suffix on parse, never re-emit"; §3.2 cfg/config.go row rewritten; §3.6 Layer 4 test `TestConfigParse_LegacyAuxAddrPasswordStripped` |
| Web UI form-POST silently drops secure_login_password after backend removal | R3 P0 #2, R4 P2 #3 | P0 / P2 | §2.4 NEW scope item; §3.2 NEW rows for `web/src/config.html` + `web/src/js/config.js` + `web/dist/*`; §3.1 architecture diagram updated |
| `addr.Addr` is FULL email for SMTP-proto addresses, NOT a callsign | R3 F1 | P0 | §3.3 SMTP-proto skip in callback; §3.1 architecture diagram updated; §3.6 `TestSecureLoginCallback_SmtpProtoSkipsCredstore` |
| Callsign case-normalization gap (wizard lowercase vs Pat uppercase) | R2 F1, R4 P2 #2 | P0 / P2 | §4.8 NEW decision (canonical key); `normalizeAccount` added to §3.2 credstore.go; §3.3 normalize-then-lookup; §3.6 `TestGet_CasingNormalization` |
| Empty/whitespace callsign + AuxAddr-fallback = auth-bypass surface | R2 F2, R2 F3 | P0 / P0 | §4.7 NEW decision (drop AuxAddr fallback); §3.3 step 3 + step 4 short-circuit; §3.6 `TestGet_EmptyCallsign_ShortCircuit` + `TestGet_WhitespaceCallsign_ShortCircuit` |
| `handleNewAccount` / `promptNewPassword` / `cmsapi.AccountAdd` residual flow in `pat configure` | R4 P2 #6 | P2 (Codex; treated as P0 due to behavior-leak risk) | §2.6 expanded to cover BOTH password-touching paths; §3.2 cli/init.go row rewrites both A + B paths |
| cmsapi.PasswordRecoveryEmailSet cascading removal (cli/account.go + winlinkPasswordRecoveryEmailHandler) | R3 F6 | P1 | §3.2 cli/account.go + api/winlink_account.go rows handle credstore (found, err) explicitly; §3.5 per-call-site rules table |
| Rust `keyring` crate API has migrated to `keyring-core`; chained syntax doesn't compile | R3 F5 | P1 | §3.4 wizard write path rewritten with correct `keyring-core` API: `Entry::new(...)?` then `.set_password(...)?`; `use_native_store(true)?` at app init; `entry.delete_credential()` for clear-credential UX (not `set_password("")`) |
| `MockInit` is process-global, unsafe for t.Parallel() | R3 F3 | P1 | §3.2 credstore_test.go row notes serialization + Cleanup; §3.6 Layer 1 explicit "NO t.Parallel()" |
| `keyring.Set("")` per-backend semantics: empty-stored treated as miss | R3 F4 | P1 | §3.2 credstore.go contract: empty-stored = miss; §3.6 `TestGet_EmptyStoredTreatedAsMiss` |
| ErrNotFound mapping not cross-platform contracted; error classification needed | R3 F7 | P1 | §3.2 credstore.go exports `ErrLocked` + `ErrUnavailable` sentinels; §3.5 caller errors.Is dispatch |
| Don't discard credstore lookup errors (`password, _, _ = credstore.Get(...)`) | R4 P2 #4 | P2 (Codex; treated as P1 due to security impact) | §3.2 + §3.5 explicit "never the discard pattern" rule; §3.3 API-call-site read path mandates explicit handling |
| CI integration test bare `dbus-launch && go test` doesn't inherit session bus | R4 P2 #5 | P2 (Codex) | §3.6 Layer 2 CI invocation rewritten with `dbus-run-session -- bash -c "..."` wrapping |
| AuxAddr-fallback-to-primary path serves no v0.0.1 use, creates auth-bypass | R2 F2+F3, R5 F1 | P0 / P0 / P1 | §4.7 NEW decision (dropped); §3.3 no fallback step; §3.6 `TestSecureLoginCallback_AuxMiss_PromptHub_NoFallbackToPrimary` regression test |
| Layer 4 config-parse 3-test battery is YAGNI (json.Unmarshal permissive is stdlib) | R5 F2 | P1 | §3.6 Layer 4 collapsed to single `TestConfigParse_LegacyAuxAddrPasswordStripped` test |
| Layer 5 `TestPatConfigure_BriefRedirectAtPasswordStep` is anti-test (tests fmt.Println) | R5 F3 | P1 | §3.6 Layer 5 DROPPED; manual smoke during implementation |
| §3.5 "Explicit NOT-decisions" 5-bullet list is defensive scaffolding | R5 F5 | P1 | §3.5 trimmed from 5 bullets to 2 (kept load-bearing: no retry, no auto-save) |
| Hardcoded URL anchor `#credentials` decays through README evolution | R1 F5 | P2 | §3.2 README.md row + §2.10 — drop anchor; plain README link |
| Unsourced `~30 transitive deps` figure | R1 F9 | P3 | §3.7 — replaced with "small; confirm via `go list -m all` post-add" |
| `app/app.go:230-233` removal — verify safety | R3 F11 | P2 | §3.2 app/app.go row — added rationale: removal is safe because keyring lookups key by active callsign per §3.3 |
| Hardcoded ServiceName rots on fork rename | R1 F1 | P1 | §5.3 NEW risk row; no migration designed (acceptable v0.0.1) |
| Hardcoded 60s promptHub timeout undermines EmComm framing | R3 F10 | P2 | §5.3 NEW risk row; flagged for follow-up to make configurable |
| CI runner image floats → silent test rot | R1 F4 | P1 | §3.6 Layer 2 + §5.3 — pinned to `ubuntu-22.04` |
| Unversioned keyring schema locks out future multi-class | R1 F7 | P2 | §5.3 NEW risk row; YAGNI for v0.0.1 |
| Web UI/dist drift on upstream merge re-introducing the field | R3 F2 follow-on | structural | §5.3 NEW risk row; integration test verifies |
| §5.6 "NOT a risk" defensive section trimmed | R5 F6 | P2 | §5.6 trimmed from 5 items to 2 |

### Findings rejected (with reasoning)

| Finding | Round | Severity | Why rejected |
|---|---|---|---|
| `json.Unmarshal` default-permissive is unverified assumption | R3 F9 | P2 | **VERIFIED via code read**: `app/config.go::ReadConfig` uses `json.Unmarshal(data, &config)` without `DisallowUnknownFields`. The spec's claim holds. No change needed. |
| Soft-error vs hard-error log-level distinction is unjustified | R5 F4 | P1 | **OPERATOR-CONFIRMED** during brainstorm §3 review: Cameron picked "Approve, but RAISE log level to error on hard failures" as the explicit answer. R5's "collapse to single warn" is overruled by the prior operator decision. |
| No retry-loop on keyring failure conflates transient vs permanent | R1 F3 | P1 | **REJECTED on safety grounds:** retrying CMS-rejected wrong passwords is a lockout risk. Transient failures (locked keyring) ARE distinguished via the `ErrLocked` sentinel classification (§3.5) and the operator can unlock externally — no retry-loop needed. |

### Findings deferred to follow-up tasks

| Finding | Disposition |
|---|---|
| Pat-killed-mid-promptHub context-cancellation edge cases (R2 F12, P3) | Edge case; not exercised in v0.0.1 flows; acceptable as-is. |
| Auto-strip `secure_login_password` from existing user config (R2 F11, P2) | v0.0.1 has no existing tuxlink-pat users; flagged in §2 out-of-scope. Future upstream-PR variant will add explicit migration. |
| Hardcoded promptHub timeout configurable (R3 F10, P2) | Acknowledged in §5.3; separate small refactor; not in cred-refactor scope. |
| Keyring schema versioning (R1 F7, P2) | YAGNI for v0.0.1 single-class; flagged in §5.3 for future multi-class expansion. |

### Findings accepted as P2/P3 not requiring spec changes

(Remaining P2 + P3 findings, ~6 of them.) These were either (a) covered by P0/P1 fixes that addressed the underlying concern, (b) operational details belonging in the implementation plan rather than the spec, or (c) cosmetic/wording suggestions not material to the design. Full per-finding dispositions in the adrev transcripts at `dev/adversarial/2026-05-18-cred-handling-adrev-R{1..5}.md` (gitignored; local-only per CLAUDE.md).

### Per-round finding counts

| Round | Lens | Findings | P0 | P1 | P2 | P3 |
|---|---|---|---|---|---|---|
| R1 | scale (Claude rosella-tussock-meadow) | 9 | 1 | 4 | 3 | 1 |
| R2 | partial-input (Claude juniper-glade-condor) | 12 | 4 | 5 | 2 | 1 |
| R3 | dep-contract-drift (Claude linnet-bracken-shoal) | 12 | 3 | 5 | 3 | 1 |
| R4 | cross-provider (Codex GPT-5.5 xhigh) | 6 | 0 | 1 | 5 | 0 |
| R5 | YAGNI (Claude currant-pelican-thorn) | 11 | 0 | 5 | 4 | 2 |
| **Total** | | **50** | **8** | **20** | **17** | **5** |

Note: Codex (R4) is structurally more conservative on severity than Claude rounds, which is why R4's P2-tagged findings (e.g., the discard-pattern + handleNewAccount residual flow) were elevated by the parent agent during synthesis based on actual impact (security-significant + functional-correctness-significant respectively). This is the disposition discipline — the round's stated severity is input, not output.
