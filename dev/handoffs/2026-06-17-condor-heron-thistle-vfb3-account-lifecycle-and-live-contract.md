# Handoff — tuxlink-vfb3 CMS account lifecycle: wired the update slice, found the real API contract + auth blocker

**Agent:** condor-heron-thistle · **Date:** 2026-06-17
**Branch:** `bd-tuxlink-vfb3/cms-password-change` (PR #787, **draft** — do NOT mark ready)
**Worktree:** `worktrees/bd-tuxlink-vfb3-cms-password-change`
**bd:** `tuxlink-vfb3` (in_progress) · blocker `tuxlink-lu7t` (the access key) · spec `docs/superpowers/specs/2026-06-17-cms-account-api-command-layer-design.md` (v2)

## TL;DR

What looked like "finish wiring the password-change control" turned into two findings that reshape the feature:

1. **Wire-walk fired.** The operator's real definition-of-done is the **full account lifecycle** (create/read/update/delete + recovery; in-app account creation in the wizard with a **mandatory recovery email**; forgot-password recovery from the **status-bar identity dialog**) — gating production CMS approval. vfb3's password-change-only slice satisfies none of those flows end-to-end. Operator approved expanding scope.
2. **The wire contract was wrong — including in already-merged code.** Codex adrev pushed me to check the live `api.winlink.org`. The WLE 1.8.2.0 decompile the backend was built from is **stale**, and the shared WLE access code is **rejected** by the current server. The feature is **auth-blocked** until Tuxlink has its own Winlink-issued key.

## Working-tree / branch state

- Working tree **clean** (all committed + pushed). Branch up to date with `origin`.
- **CI is GREEN** on head `7ec3638a` (verify + build-linux, both arches pass) — first green on this branch. Sub-project 0 (below) is built + verified offline-correct.
- **Root cause of the previously-red CI (found + fixed this session):** reqwest 0.13 moved `RequestBuilder::form()` behind an opt-in `"form"` feature (default in 0.12); this crate never enabled it, so **the original vfb3 backend never compiled**. The author couldn't cold-build on the Pi, "Codex adrev clean" reviewed the wire format (not compilation), and the draft PR's red CI went unnoticed. Fixed by adding `"form"` to the reqwest features (+ a one-line `serde_urlencoded` Cargo.lock edge, resolution-only via `cargo metadata`). **Lesson: `gh pr checks` green is a real gate; "Codex adrev clean" ≠ "compiles".**
- This handoff is committed on the **feature branch** (main is lease-locked); relocate per your normal flow if desired.

## What shipped this session (all committed + pushed to #787)

1. **`CredentialFields`** ([src/wizard/CredentialFields.tsx](../../src/wizard/CredentialFields.tsx)) — shared callsign+password fields (show/hide) extracted from `Step2Credentials`, which now reuses it. TDD; wizard ids preserved.
2. **`WinlinkAccountSettings`** ([src/shell/WinlinkAccountSettings.tsx](../../src/shell/WinlinkAccountSettings.tsx)) — Settings "Winlink Account" section hosting `CmsPasswordChange` + a **keyring-only** re-enter recovery. Re-enter uses `credentials_write_password` (NOT `wizard_persist_cms`, which rebuilds config.json from scratch and would wipe grid/MBO/modem/APRS/favorites — a data-loss trap caught + avoided this session).
3. **Menu wiring** — `menu:tools:settings_account` → `openWinlinkAccount` → `SettingsPanel initialSection='account'` ([menuModel](../../src/shell/chrome/menuModel.ts) / [dispatchMenuAction](../../src/shell/chrome/dispatchMenuAction.ts) / [AppShell](../../src/shell/AppShell.tsx)).
4. **`cms_password_change` corrected to the live contract** ([src-tauri/src/winlink/cms_account.rs](../../src-tauri/src/winlink/cms_account.rs)) — see below.
5. **Spec v2** + this handoff.

Frontend vitest + typecheck were green locally throughout. Rust is CI-compiled.

## The verified live contract (use THIS, not the decompile)

Probed `api.winlink.org` (CMS v5.0.9649), 2026-06-17:

- **Auth param is `Key`** — uniform across every account op. (The decompile's per-endpoint `WebServiceAccesscode`/`WebServiceAccessCode` casing distinction is **moot**.)
- **ServiceStack envelope:** payload fields **top-level** (e.g. `CallsignExists`, `Blocked`); errors in **`ResponseStatus { ErrorCode, Message, Errors[] }}`**; **no `HasError`**; error text is **`Message`** (not `ErrorMessage`); **HTTP 400** on error, 200 on success (success = `ResponseStatus` absent or empty `ErrorCode`).
- **`/account/add` takes `RecoveryEmail` directly** → account creation is one atomic call (satisfies mandatory-recovery-email with no partial state).
- **`account_remove` is privilege-gated:** `/account/remove` exists (decompile) but its live metadata returns **403** (vs `AccountTacticalRemove` = 200). The client key may not be authorized — **do not wire delete to UI until an operator live-test proves the issued key can invoke it.**

### THE BLOCKER (tuxlink-lu7t, P1)

The shared WLE 1.8.2.0 access code (`C6B6…`) returns **`InvalidAccessKey` (HTTP 400)** against the current server — verified with the real value. **Every account-API call (incl. the shipped `cms_password_change`) is non-functional live until Tuxlink holds its own Winlink-issued `Key`.** Sanctioned path: a per-application key from a Winlink administrator — the same CMS team gating prod approval. **Operator action:** obtain the key, set `TUXLINK_WINLINK_ACCESS_CODE`, and confirm it's a *static per-application code* (current architecture assumes this) vs a *per-session token* (would change sub-project 0's shape). Nothing is user-visible-broken today (access-code-gated, never live-exercised).

## Plan: full account lifecycle (decomposed, operator-approved)

Build order **0 → 1 → 2 → 3**; live validation deferred to when the key exists.

- **0 · Backend account-API command layer — ✅ BUILT + CI-GREEN** ([v2 spec](../../docs/superpowers/specs/2026-06-17-cms-account-api-command-layer-design.md)). Shipped `post_account_form` helper + `AccountApiError` + `account_create` (`/account/add`, `RecoveryEmail` a direct param → single atomic create) / `account_exists` (fails closed if `CallsignExists` missing) / `account_set_recovery_email` / `account_send_recovery` / `account_remove` (deletes keyring entry on success) + corrected `change_password`, all on a **global mutation lock**; `normalize_account_callsign` rejects tactical inputs; `credentials::delete_password` added; 5 Tauri commands registered; unit tests for form builders / envelope parse / normalization. **Remaining for 0:** (a) `account_validate_password` was **deferred** — its response *payload* field (the validation code) wasn't verified against the live server; verify it before building. (b) `account_remove` `UnknownOutcome`/timeout reconciliation is specced but not implemented (basic version only). (c) **Run the second Codex round on the corrected encoding** (build-robust-features). All LIVE exercise blocked on the key (`tuxlink-lu7t`).
- **1 · Wizard account creation** (mockups) — fork after Step1Welcome's "Yes, CMS": have-account vs create; create collects callsign+password+confirm+**mandatory recovery email** → `account_create` → existing `cms_verify → location → complete`.
- **2 · Status-bar identity dialog recovery** (mockups) — flow-3 entry on `IdentitySwitcher`: forgot-password (`account_send_recovery`) + change-password. Note the reality: forgot-password "reset" = the server **emails the existing password** (requires a recovery email on file) — there is no arbitrary set-new-without-old.
- **3 · Settings → Winlink Account management** — set recovery email, account status, **delete behind a typed-confirmation gate** (`account_remove`, only once live-proven).

## Decisions locked this session

- DELETE → implement + expose behind a heavy **typed-confirmation** gate.
- Recovery email → **MANDATORY** at creation (hamexandria: missing recovery email = ~20–30% of support posts; the S1 cleartext-storage concern is mitigated by the "use a unique, non-reused password" guidance).
- Re-enter recovery → keyring-only (`credentials_write_password`), never `wizard_persist_cms`.

## Watch-outs

- Don't trust the private decompile design note's endpoint table — it **omitted `/account/remove`** and its contract is stale. The **live server is the source of truth**.
- The access code is an operator-managed secret; the agent is (correctly) blocked from handling it — live probes/tests are operator-run.
- `account_remove` 403 → don't ship delete UI until live-proven.
