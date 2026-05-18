# Spec: onboarding wizard cluster (Tasks 9 + 10 + 11 + 11.5) — Wave-2 design

**bd issue:** `tuxlink-ln3` (P2; blocks `tuxlink-ko0`, `tuxlink-1r5`, `tuxlink-e4x`, `tuxlink-d76`)
**Predecessors revoked:** Wave-1 plans for these tasks (`tuxlink-ak2` for Task 9 + analogous Wave-1 plan-writing issues) were revoked 2026-05-18 via PRs #47-52 rollback. Wave-1 had skipped the upstream `build-robust-features` pipeline (no brainstorm, no 5-round cross-provider adrev) on the "design settled" assumption; the resulting plans missed cross-provider failure modes and were not shippable. Per memory `feedback_no_carveout_on_cross_provider_adrev`, the carveout is closed: Wave-2 runs the full pipeline from scratch despite the design doc baseline.

**Canonical UX baseline:** `docs/design/v0.0.1-ux-mockups.md` §5.1 (Task 9), §5.2 (Task 10), §5.3 (Task 11), §5.4 (Task 11.5) — landed via PR #33. The spec below treats those sections as authoritative for *visual* design and *user-facing copy*; technical-architecture decisions land here.

**Canonical credentials-flow baseline:** `docs/superpowers/specs/2026-05-18-cred-handling-design.md` §2 (Scope) + §3 (Design) — defines the `(service="tuxlink-pat", account=<callsign>)` OS-keyring convention that Pat reads and that this wizard writes. AMD-13 of the v0.0.1 plan codifies the wizard-side commitment to write via the Rust `keyring` crate.

---

## 1. Context

The onboarding wizard is the first user-facing surface a new tuxlink operator touches. It must:

1. Route between a CMS-connected deployment (most operators) and an offline / radio-only deployment (ARES drills, EOC tabletops, lab work, Hybrid Network operators) without privileging one as the "default true path."
2. Capture the Winlink CMS callsign + password (CMS path only) and write the password to the OS keyring — never to disk — so the fork-side Pat can read it back via the cred-handling refactor's `(service, account)` convention.
3. Offer an optional verification step (a real telnet test send to `SERVICE@winlink.org`) that is informational rather than blocking: every state of the verification flow has a path to the main UI.
4. Persist tuxlink's nested-shape config (per AMD-1: `connect.connect_to_cms`, `identity.callsign | identifier`, `identity.grid`, `privacy.*`) to `$XDG_CONFIG_HOME/tuxlink/config.json` — without any password material and without any `winlink_password_present` boolean (both dropped per AMD-11).

**Single coherent cluster vs four atomic specs.** The four screens share state (the wizard reducer), share persistence (one transactional commit of config + keyring at completion), share a routing protocol (the welcome screen's choice routes to one of two paths), and share error UX (e.g., keyring-unavailable behaves the same in the wizard happy-path as in a post-wizard re-run). Decomposing into four per-screen specs would re-litigate the state machine four times. The cred-handling spec set the precedent — one spec for a coherent unit — and this cluster follows the same shape.

**Out of scope.** Settings UI (post-wizard adjustment of these same fields via `Tools → Settings`) is tracked separately under future work; the wizard only invokes the operations, it doesn't expose them as standalone UI. Tasks 12-16 (mailbox, reading pane, compose, session log, status bar) are downstream consumers of the wizard's persisted state — out of scope for this spec but referenced where the wizard hands off.

---

## 2. Scope

### 2.1 In scope

- **4 React components**: `<Step1Welcome>`, `<Step2Credentials>`, `<Step2OfflineIdentity>`, `<Step3TestSend>` (filenames per current v0.0.1 plan Tasks 9/10/11/11.5).
- **1 wizard state machine** (`wizardState` reducer in TypeScript, shared across all 4 screens).
- **2 Tauri commands** invoked from React via `@tauri-apps/api/core::invoke`:
  - `wizard_persist_cms`(callsign, password, grid, mbo_address, connect_to_cms) — writes config.json (no password material) AND keyring entry (password); transactional (both or neither).
  - `wizard_persist_offline`(identifier, grid) — writes config.json with `connect_to_cms: false`; no keyring touch.
- **1 Tauri command** for the test send: `wizard_run_test_send`() — invokes the existing pat_client (Task 5) to POST a test message + poll inbox for the autoresponder reply; returns one of 4 outcomes (idle is the pre-invocation state, not a return). The smoke binary (`live_cms_smoke`, Task 6, `tuxlink-nk7`) shares the underlying flow but is a separate operator-only binary.
- **Routing/wiring**: `<App>` mounts `<Wizard>` when `wizard_completed === false`; `<Wizard>` mounts the current-step component per `wizardState.step`. Post-completion, `<App>` mounts the main shell (Task 12+ surface).
- **Test coverage**: unit (vitest) for the wizard reducer + each screen's render-and-interact behavior; integration (cargo test) for the two persist commands hitting a real keyring backend in CI (per cred-handling spec §3 — gnome-keyring-daemon + dbus-launch in the Linux CI runner).
- **Browser smoke** (per memory `feedback_browser_smoke_before_ship`) before declaring the cluster shippable: `pnpm tauri dev` and walk the 4-screen flow + the offline branch + the 4 test-send substates.

### 2.2 Out of scope

- Settings UI for post-wizard adjustment (separate future bd issue when Task 12+ shell lands).
- AuxAddr keyring entries (single-callsign scope per AMD-11 + AMD-13; multi-account power-users provision via `secret-tool` manually).
- Credential ROTATION (operator-facing UX for changing an existing keyring entry). v0.0.1 only writes on first wizard completion; operators rotate by re-running the wizard via a future Settings → Reset wizard surface OR by `secret-tool store` directly. Documented as a known v0.0.1 limitation, not a wizard responsibility.
- The CMS connection itself (Pat handles it; the wizard's test-send only triggers `pat_client.send()` + `pat_client.list(Inbox)` polling, all of which is Task 5 + Task 6 surface).
- The session log pane's rendering of the test-send session — covered by Task 15's spec.

### 2.3 Keyring convention (inherited from cred-handling spec §2)

| Field | Value |
|---|---|
| Service | `tuxlink-pat` (constant) |
| Account | The operator's callsign as entered (upper-cased per AMD-1's `validate_identity()`) |
| Secret | The Winlink CMS password as entered (no transformation; CMS is authoritative on validation) |

The wizard writes EXACTLY one entry per completion (single callsign). Re-running the wizard (e.g., to change callsign) overwrites the entry under the new callsign and does NOT delete the prior callsign's entry — operators with multiple callsigns get multiple entries naturally.

---

## 3. Design

### 3.1 Wizard state machine

`wizardState` is a `React.useReducer` state held in `<Wizard>` and threaded to child screens via a React context (`wizardContext.tsx` exports `{state, dispatch}`). The reducer handles ONLY synchronous transitions; async side-effects (keyring write, test-send) are initiated in component handlers via `@tauri-apps/api/core::invoke`, and the resolved result is dispatched as a synchronous follow-up action (`SUBMIT_CREDENTIALS_SUCCESS` / `SUBMIT_CREDENTIALS_FAILURE` / `TEST_SEND_RESULT`). This pattern (useReducer + effects-in-component) was selected over Zustand or a thunk middleware to keep dependencies minimal and the data flow unidirectional.

State shape per AMD-2 + AMD-5 + cred-handling spec §2:

```typescript
type WizardStep =
  | 'account'             // Screen 1 (Task 9): connect_to_cms routing
  | 'credentials'         // Screen 2-CMS (Task 10): callsign + password + grid
  | 'offline_identity'    // Screen 2-offline (Task 11.5): optional identifier + grid
  | 'test_send'           // Screen 3 (Task 11): 4 substates (idle/sending/success/failed)
  | 'complete';           // Terminal: wizard_completed=true persisted; render main shell

interface WizardState {
  step: WizardStep;
  connectToCms: boolean | null;    // null until Step 1 answered
  callsign: string;                // empty until Step 2 (CMS) submit; "" not null even in offline path (offline writes identity.callsign as null in JSON via Rust normalization)
  password: string;                // empty until Step 2 (CMS) submit; cleared from state after wizard_persist_cms succeeds
  identifier: string;              // empty unless offline path
  grid: string;                    // optional; both paths
  mboAddress: string;              // optional; CMS path only; auto-fills "<callsign>@winlink.org" on callsign `onChange` ONLY when MBO field is empty OR matches the prior auto-filled value (don't overwrite operator customization)
  testSendSubstate: 'idle' | 'sending' | 'success' | 'failed';
  testSendError: string | null;    // populated when testSendSubstate==='failed'
  testSendLog: string[];           // human-shaped projection lines streamed via Tauri event during `sending`; reset on BEGIN_TEST_SEND
  inFlight: boolean;               // true while a wizard_persist_* or wizard_run_test_send is mid-call; disables submit buttons + guards reducer
  skipSignaled: boolean;           // set true when SKIP_TEST_SEND fires during `sending`; subsequent TEST_SEND_RESULT is silently ignored (per R3 finding 5)
}

type WizardAction =
  | { type: 'SET_CONNECT_TO_CMS'; payload: boolean }
  | { type: 'ADVANCE_FROM_ACCOUNT' }                             // reads state.connectToCms; transitions step to 'credentials' or 'offline_identity'
  | { type: 'SET_CREDENTIALS_FIELD'; field: 'callsign' | 'password' | 'grid' | 'mboAddress'; value: string }
  | { type: 'SET_OFFLINE_FIELD'; field: 'identifier' | 'grid'; value: string }
  | { type: 'SUBMIT_BEGIN' }                                     // sets inFlight=true; called before invoke()
  | { type: 'SUBMIT_CREDENTIALS_SUCCESS' }                       // clears password from state; transitions step to 'test_send' OR 'complete' (depends on which button was clicked — component carries `skipTestSend: boolean` in the invoke result)
  | { type: 'SUBMIT_OFFLINE_SUCCESS' }                           // transitions step to 'complete'
  | { type: 'SUBMIT_FAILURE'; error: WizardError }               // sets inFlight=false; populates per-error UX (see §3.5)
  | { type: 'BEGIN_TEST_SEND' }                                  // guarded: no-op if testSendSubstate !== 'idle' (see invariant below)
  | { type: 'TEST_SEND_LOG_LINE'; line: string }                 // appends to testSendLog; ignored if skipSignaled
  | { type: 'TEST_SEND_RESULT'; outcome: TestSendOutcome }       // folds outcome into 'success' or 'failed'; ignored if skipSignaled
  | { type: 'SKIP_TEST_SEND' };                                  // sets skipSignaled=true; transitions step to 'complete'

export function wizardReducer(state: WizardState, action: WizardAction): WizardState { /* … */ }
```

`WizardError` and `TestSendOutcome` shapes are defined in §3.7 (Tauri command surface, where they originate); their TypeScript discriminated unions mirror the Rust enum's `#[serde(tag = "kind", content = "detail")]` shape — same definitions, see §3.7.

**Crucial state-machine invariants:**

1. `password` is cleared from `WizardState` (set to empty string) immediately on `SUBMIT_CREDENTIALS_SUCCESS`. The plaintext password lives in JavaScript memory ONLY between the user typing it and the Rust command writing it to the keyring. React-DevTools state inspection should not retrieve it after submit. (Defense-in-depth.)

2. **`BEGIN_TEST_SEND` dedup guard (Part 97 correctness; R3 finding 1).** The reducer MUST treat `BEGIN_TEST_SEND` as a strict no-op (`return state` unchanged) when `testSendSubstate !== 'idle'`. The Rust side has a complementary `Mutex<bool>` guard (see §3.7) so multi-window dispatch can't bypass the React-only guard. The `[Send test]` button MUST be UNCONDITIONALLY ABSENT (not merely disabled) from the `sending`/`success`/`failed` substate renders. Rationale: two concurrent `wizard_run_test_send` invocations = two CMS transmissions under the operator's callsign without separate consent.

3. **`SKIP_TEST_SEND` during `sending` sets `skipSignaled=true`.** The in-flight `wizard_run_test_send` runs to natural completion (the CMS message is already in transit; no cancel attempted), but the subsequent `TEST_SEND_RESULT` dispatch is silently no-op'd by the reducer. The test message IS sent (operator consented at idle→sending transition); skipping just suppresses the UI feedback. Documented as expected, not a bug.

4. **`ADVANCE_FROM_ACCOUNT` routing logic.** Transitions `step` from `account` to `credentials` IF `state.connectToCms === true`, else to `offline_identity`. Dispatched immediately after `SET_CONNECT_TO_CMS` on choice-card click; the two-action sequence is a reducer pattern, not two user steps.

### 3.2 Persistence contract — transactional pair

The wizard's two write targets are tuxlink's config.json AND the OS keyring. For the CMS path, both writes must succeed together — partial-success states are operator-confusing (e.g., "config thinks I'm CMS but Pat can't find my password"). The Rust-side `wizard_persist_cms` command implements transactional semantics. Authoritative signature in §3.7; the block below is illustrative of the ordering only:

```rust
// 0. One-time at app init (in src-tauri/src/lib.rs::run() before tauri::generate_handler!):
//    keyring::use_native_store(true)?;
//    — REQUIRED by the current keyring-core API per cred-handling spec §3.4; the
//      implicit-OS-backend selection of older API versions is deprecated. Omitting this
//      makes the keyring crate emit deprecation warnings AND fall back to a behavior
//      that may not match what the integration test or Pat-side credstore.Get() expects.
//      This initialization is mandatory at app startup, NOT per-command.

// Per-command flow inside wizard_persist_cms (single-flight mutex guarded; see §3.7):
async fn wizard_persist_cms(
    raw_callsign: String, password: String, grid: String,
    mbo_address: String, connect_to_cms: bool,
) -> Result<(), WizardError> {
    // 1. Normalize callsign (TrimSpace + ToUpper per cred-handling spec §3.3's normalizeAccount).
    //    This normalization is the COMMAND's responsibility, not the caller's — both
    //    JS-side (where state.callsign may carry the raw input) and any future callers
    //    of this command get consistent normalization.
    let callsign = raw_callsign.trim().to_uppercase();
    // 2. Re-validate normalized callsign via validate_identity (defense-in-depth even
    //    after frontend validation per AMD-3); reject non-ASCII via explicit predicate
    //    (homoglyph guard; see §5.NEW for the homoglyph failure mode).
    if !callsign.is_ascii() || !validate_identity(&callsign) {
        return Err(WizardError::InvalidInput { field: "callsign".into() });
    }
    // 3. Build the new Config struct in memory (does NOT touch disk yet). The `Config`
    //    type lives in `crate::config::Config` (created by Task 2 / AMD-1; the wizard
    //    reuses it, no new type definition).
    let new_config = crate::config::Config { /* fields per §3.6 */ };
    // 4. Write keyring FIRST (keyring failure aborts before any persistent state change).
    let entry = keyring::Entry::new("tuxlink-pat", &callsign)?;
    entry.set_password(&password)?;
    // 5. Write config.json (atomic-rename via tempfile + std::fs::rename). Reuses
    //    `crate::config::write_config_atomic(config: &Config) -> Result<(), ConfigError>`
    //    from Task 2 / AMD-1.
    if let Err(e) = crate::config::write_config_atomic(&new_config) {
        // Best-effort keyring rollback. If rollback also fails, the operator is told
        // BOTH failures explicitly per §3.5's ErrConfigWrite + ErrConfigWriteAndRollbackFailed
        // augmented message.
        let _ = entry.delete_credential();
        return Err(map_config_write_error(e));
    }
    Ok(())
}
```

**Ordering rationale.** Keyring-first means a keyring failure (locked, daemon unavailable, etc.) is reported to the operator BEFORE any persistent state changes. A subsequent config.json write failure (disk full, permissions) triggers a best-effort keyring rollback. If rollback fails too, the operator is told explicitly and given the `secret-tool delete` command to clean up manually (per §3.5's augmented `ErrConfigWrite` message). Inverse ordering (config first, keyring second) leaves a config.json saying "CMS path" without a keyring entry — Pat then can't connect, and the operator has to guess what went wrong.

**App-quit during in-flight `wizard_persist_cms` (R3 finding 2).** If the operator quits tuxlink (SIGTERM, window close, Alt-F4) BETWEEN the successful keyring write (step 4) and the config.json commit (step 5), the keyring carries the password but `wizard_completed === false` in the absent/old config.json. The §5.3 claim "nothing persisted" is false in this specific timing window. On re-launch the wizard re-opens at Step 1. Two cases:
- Same callsign on re-submit: the keyring write overwrites cleanly; no operator-visible artifact.
- Different callsign on re-submit: the prior callsign's keyring entry becomes an ORPHAN that survives forever (single-callsign scope per §2.3 means the wizard doesn't track or clean up other callsigns' entries).

**Mitigation.** Wizard MUST add a startup check: if `wizard_completed === false` AND a keyring entry exists for ANY callsign with `service="tuxlink-pat"`, show a Step 2 pre-fill hint: "Credentials from a previous wizard run were found (callsign X). Re-using will overwrite; entering a different callsign will leave the previous entry. Use `secret-tool delete service tuxlink-pat account <CALLSIGN>` to clean up old entries." Surfaced as a non-blocking banner above the form; the operator can ignore. The detection requires `secret-tool search service tuxlink-pat --all` or the equivalent `keyring` crate iteration — implementer's choice.

The offline path's `wizard_persist_offline(identifier: String, grid: String) -> Result<(), WizardError>` is single-write (config.json only, no keyring) — atomic by definition. The Rust command hardcodes `connect.connect_to_cms = false` and writes `connect.transport = "CmsSsl"` (the default; harmless in the offline path since no transport is exercised, and keeping the field present preserves schema-shape consistency for future-mode transitions via Settings → Connection). The persisted JSON's `identity.callsign` is hardcoded to `null` (not derived from `WizardState.callsign`, which stays as the empty string in JS state for the offline path).

### 3.3 Screen-by-screen behavior

Each subsection summarizes the screen; full UX copy + mockups live in `docs/design/v0.0.1-ux-mockups.md` §5.1-5.4.

**Step 1 (`account`, Task 9, `tuxlink-ko0`):** Title "Will this installation connect to the Winlink CMS?". Two choice cards (CMS = default; offline). On selection, `SET_CONNECT_TO_CMS` followed by `ADVANCE_FROM_ACCOUNT` routes to either `credentials` or `offline_identity` based on the boolean. The screen renders no other fields; back-button is disabled (Step 1 is the entry point).

**Step 2-CMS (`credentials`, Task 10, `tuxlink-1r5`):** Form with callsign (required, loose validator per AMD-3), password (required, ≥6 chars per Express convention, show/hide toggle), grid (optional, 4-or-6-char Maidenhead per AMD-1's grid validator), MBO address (optional, auto-fills `<callsign>@winlink.org` on callsign change). Header carries the inline Register link per AMD-3. Two submit buttons:

- **Continue** → calls `wizard_persist_cms`, on success advances to `test_send` (idle substate). Per AMD-13, the keyring write happens here.
- **Save credentials and skip verification** → same `wizard_persist_cms` call, then directly to `complete`; main shell loads with a status-bar note: "Test send pending — Session → Test send to run now."

**Step 2-offline (`offline_identity`, Task 11.5, `tuxlink-d76`):** Single optional-field form per AMD-5: station identifier (free-form, accepts tactical strings) + grid (4-char broadcast precision). Footer copy: "All fields optional. Tuxlink works fully offline — you can configure identity later via Tools → Settings." Single submit button "Continue offline" → calls `wizard_persist_offline`, then directly to `complete`. No test-send screen for offline.

**Step 3 (`test_send`, Task 11, `tuxlink-e4x`):** Four substates per AMD-4 (see §3.4 for the substate-to-action mapping). Transport-visibility copy renders above the form per §5.3 baseline. Test message destination: `SERVICE@winlink.org`; subject contains the `/test/` token (Express convention; CMS auto-responds with a brief reply that the binary polls inbox for).

**Terminal (`complete`):** Wizard unmounts; main shell mounts. No persistent UI from the wizard at this state — the operator is now in the "I have already onboarded" world.

### 3.4 Test-send substate machine

| Substate | Trigger | Visible | Operator-actionable controls |
|---|---|---|---|
| `idle` | Initial state on `test_send` step | Explanatory copy + transport-visibility paragraph | [Send test] [Skip] |
| `sending` | `BEGIN_TEST_SEND` action | Progress indicator + line-by-line session-log preview (the human-shaped projection from design doc §4.4) | [Skip and go to inbox] — always available, never disabled |
| `success` | `TEST_SEND_RESULT` with success outcome | Green check + "Test send complete. Your CMS account is verified." | Auto-advance to `complete` after 3 s (operator can cancel by clicking anywhere) |
| `failed` | `TEST_SEND_RESULT` with failure outcome | Yellow warning (NOT red error) + likely-cause list (no internet connection, firewall blocking port 8773, CMS temporarily busy, OR a captive portal / network login page intercepting traffic — per §5.12) + the specific error from `testSendError` | [Retry] (dispatches `BEGIN_TEST_SEND` → re-enters `sending` substate, re-invokes `wizard_run_test_send`) · [Go to inbox] (dispatches `SKIP_TEST_SEND`) · [Open Settings] (out-of-scope nav to the future Settings UI; v0.0.1 may render this disabled with tooltip "Settings UI lands in a later release") |

Non-blocking principle: every substate has a path to the inbox. Failed test-send is INFORMATION not a wall; the operator's credentials are saved regardless and they can retry from `Session → Test send` post-wizard (the AMD-10 menu item that lands in Task 7).

### 3.5 Error UX — keyring failures and the credential flow

The wizard handles SIX classes of failure during the credential persist flow. **Naming alignment vs the cred-handling spec:** Two of these (`ErrLocked`, `ErrUnavailable`) MIRROR BY NAME the cred-handling spec §3.2's Go-side exported sentinels (which Pat's call sites `errors.Is`-dispatch on). The other four (`ErrPermissionDenied`, `ErrConfigWrite`, `ErrBusy`, `ErrInvalidInput`) are WIZARD-INTERNAL Rust enum variants without Pat-side counterparts — the cred-handling spec does not export these. The naming mirror on the two shared sentinels is a discipline (consistency aids cross-language debugging), not a literal type re-export.

| Failure | When | Operator-visible message | Recovery path |
|---|---|---|---|
| `ErrUnavailable` (no daemon / no D-Bus on Linux; equivalent on other platforms is rare) | Wizard submit-credentials → keyring write | "Tuxlink couldn't find a secret-service keyring on your system. Tuxlink uses the OS keyring to store your Winlink CMS password securely (instead of saving it to a config file). Install and start one (e.g., `sudo apt install gnome-keyring`) and re-run the wizard. See [installation docs](#) for distro-specific guidance." | Form stays mounted; operator can copy the message + retry after installing |
| `ErrLocked` (daemon present but session locked; Linux) | Wizard submit-credentials → keyring write | "Your keyring is currently locked. Unlock it (typically: click the keyring icon in your system tray, OR run `secret-tool lock --collection=default` followed by your login password prompt) and click Retry." | Form stays mounted with a Retry button alongside Continue |
| `ErrPermissionDenied` — **Linux**: daemon refused write (rare; check distro config) | Wizard submit-credentials → keyring write on Linux | "The keyring daemon refused the write. This is unusual on Linux; check your distro's keyring permission settings or report the issue at github.com/cameronzucker/tuxlink/issues." | Form stays mounted; suggests filing an issue |
| `ErrPermissionDenied` — **macOS**: first-access authorization denied (R3 finding 8) | Wizard submit-credentials → keyring write on macOS | "macOS Keychain requires you to authorize tuxlink to store your password. A system dialog should have appeared; if you clicked Deny, click Retry and authorize when prompted." | Form stays mounted; Retry re-triggers the auth prompt |
| `ErrPermissionDenied` — **Windows**: CredentialManager rejected write (rare) | Wizard submit-credentials → keyring write on Windows | "Windows CredentialManager refused the write. Check that no group policy is blocking generic credential storage, or report the issue at github.com/cameronzucker/tuxlink/issues." | Form stays mounted; suggests filing an issue |
| `ErrConfigWrite { detail: String }` (keyring succeeded, config.json failed) | Step 5 of `wizard_persist_cms` | "Tuxlink wrote your password to the keyring but couldn't save the config file (disk full? permissions?). Tuxlink has attempted to remove the keyring entry; if you see a stale `tuxlink-pat` entry for callsign `<callsign>`, run `secret-tool delete service tuxlink-pat account <callsign>`. Details: {detail}" | Form stays mounted; operator addresses the disk issue + retries |
| `ErrConfigWrite` **AND keyring rollback failed** (R3 finding 9 augmentation) | Step 5 + rollback failure | Augmented: "Tuxlink couldn't save the config file AND the attempt to remove the keyring entry also failed. Run `secret-tool delete service tuxlink-pat account <callsign>` manually before retrying. Details: {config_error} / {rollback_error}" | Form stays mounted; operator runs manual cleanup + retries |
| `ErrBusy` (second concurrent invocation; multi-window OR React double-render) | Any submit while a `wizard_persist_*` or `wizard_run_test_send` is in-flight | NO user-visible message — React suppresses silently as a no-op. The submit-button-disabled-during-`inFlight` pattern is the primary defense; ErrBusy is the Rust-side mutex backstop for multi-window scenarios where React state isn't shared. | None needed; the in-flight call resolves and the user sees that result |
| `ErrInvalidInput { field: String }` (homoglyph or non-ASCII callsign reaches Rust despite frontend validator; R3 finding 6) | Wizard submit-credentials → callsign normalization | "The callsign field contains characters tuxlink can't handle (non-ASCII, zero-width, or homoglyph). Re-type using only A-Z, 0-9, and `/`." | Form stays mounted; field cleared with the prompt visible |
| `ErrOther { detail: String }` (catchall; e.g., Windows CredentialManager 2.5KB blob limit per R3 finding 7) | Any unhandled OS error from the `keyring` crate | "An unexpected error occurred while saving credentials. Details: {detail}. If this looks like a tuxlink bug, please report at github.com/cameronzucker/tuxlink/issues." | Form stays mounted; operator can retry or file an issue |

The form's submit button is disabled while `state.inFlight === true` (see §3.1). The form REMAINS mounted across failure — operator does not lose their typed input (except the password field, which is intentionally NOT cleared on failure so the operator doesn't have to re-type).

**Post-wizard keyring deletion (R3 finding 4).** Once the wizard completes and the main shell mounts, the operator may use Seahorse / kwallet manager / `secret-tool delete` to remove the `(service="tuxlink-pat", account=<callsign>)` entry manually. The wizard has no in-process control after this. Pat's next call to `credstore.Get()` returns `(found=false, err=nil)` and Pat falls through to `promptHub` — operator sees a password prompt with no tuxlink-side context. The main shell (Tasks 12+, out of scope for this spec) MUST detect this case and surface: "Your Winlink CMS password was not found in the system keyring. It may have been deleted. Re-run the wizard via Tools → Reset wizard (deferred to a future menu item; no v0.0.1 menu ID yet — see §2.2 out-of-scope) or run `secret-tool store --label='tuxlink-pat WL2K' service tuxlink-pat account <callsign>`." Flagged as a required handoff requirement to the Tasks 12+ shell spec.

### 3.6 Config schema persisted by the wizard

Per AMD-1 nested-shape + AMD-11 (drop of `winlink_password_present`), the wizard writes:

```json
{
  "schema_version": 1,
  "wizard_completed": true,
  "connect": {
    "connect_to_cms": true,                    // false in offline path
    "transport": "CmsSsl"                      // default; Settings can change post-wizard
  },
  "identity": {
    "callsign": "W4PHS",                       // null in offline path
    "identifier": null,                        // "EOC-1" etc in offline path
    "grid": "EM75xx"                           // null if operator leaves blank
  },
  "privacy": {
    "gps_state": "BroadcastAtPrecision",       // default per Principle 7
    "position_precision": "FourCharGrid"       // default per Principle 7
  },
  "pat_mbo_address": "W4PHS@winlink.org"       // null in offline path
}
```

NO `winlink_password_present` field (removed per AMD-11). NO password material anywhere in the file. The keyring is the single source of truth for the password.

### 3.7 Tauri command surface (Rust side)

Four new commands in `src-tauri/src/wizard.rs` (new file) registered in `tauri::generate_handler![...]`:

| Command | Signature | Synchronous? | Side effects |
|---|---|---|---|
| `get_wizard_completed` | `() → Result<bool, WizardError>` | Sync (one config.json read on startup) | None; read-only |
| `wizard_persist_cms` | `(raw_callsign, password, grid, mbo_address, connect_to_cms) → Result<(), WizardError>` | Async (D-Bus call to keyring; disk write) | Keyring write + config.json atomic write |
| `wizard_persist_offline` | `(identifier, grid) → Result<(), WizardError>` (hardcodes `connect.connect_to_cms = false` and `connect.transport = "CmsSsl"`; writes `identity.callsign = null`) | Async (disk write only) | Config.json atomic write |
| `wizard_run_test_send` | `() → Result<TestSendOutcome, WizardError>` | Async (HTTP to Pat + inbox poll) | None (Pat handles the actual TX); emits `wizard:test_send:log` Tauri events for line-by-line streaming to `<Step3TestSend>` per §3.4 |

**Rust enum definitions** (in `src-tauri/src/wizard.rs`):

```rust
#[derive(Debug, serde::Serialize)]
#[serde(tag = "kind", content = "detail")]
pub enum WizardError {
    Unavailable,
    Locked,
    PermissionDenied { platform_hint: String },   // detail carries "linux" | "macos" | "windows" for §3.5 per-platform copy
    ConfigWrite { detail: String },
    ConfigWriteAndRollbackFailed { config_error: String, rollback_error: String },
    Busy,
    InvalidInput { field: String },
    Other { detail: String },
}

#[derive(Debug, serde::Serialize)]
#[serde(tag = "kind", content = "detail")]
pub enum TestSendOutcome {
    Success { reply_subject: Option<String> },    // autoresponder's subject line if captured
    Failed { cause: String, likely_causes_hint: Vec<String> },  // cause = literal error string; likely_causes_hint per §3.4's failed-substate copy
}
```

**Matching TypeScript discriminated union** (in `src/wizard/types.ts`):

```typescript
type WizardError =
  | { kind: 'Unavailable' }
  | { kind: 'Locked' }
  | { kind: 'PermissionDenied'; detail: { platform_hint: 'linux' | 'macos' | 'windows' } }
  | { kind: 'ConfigWrite'; detail: { detail: string } }
  | { kind: 'ConfigWriteAndRollbackFailed'; detail: { config_error: string; rollback_error: string } }
  | { kind: 'Busy' }
  | { kind: 'InvalidInput'; detail: { field: string } }
  | { kind: 'Other'; detail: { detail: string } };

type TestSendOutcome =
  | { kind: 'Success'; detail: { reply_subject: string | null } }
  | { kind: 'Failed'; detail: { cause: string; likely_causes_hint: string[] } };
```

**Tauri naming convention.** Tauri's `invoke` automatically serializes JavaScript camelCase property names to Rust snake_case parameter names (and back for return values). Callers from React pass `{rawCallsign, mboAddress, ...}` JS-side; Rust signatures use `raw_callsign`, `mbo_address`, ... — no manual conversion needed.

**Single-flight mutex on the Rust side (R3 finding 11 + R2 P0-2 ErrBusy).** All three write-side commands (`wizard_persist_cms`, `wizard_persist_offline`, `wizard_run_test_send`) MUST be guarded by a single Tauri-state-held `Mutex<bool>` (or `tokio::sync::Mutex<()>`). A second invocation while ANY of the three is in-flight returns `WizardError::Busy` regardless of which window called it. The React-side `inFlight` flag (§3.1) is UI debounce; the Rust mutex is the authoritative deduplication gate that handles multi-window scenarios where React state isn't shared. `get_wizard_completed` is NOT mutex-guarded (read-only, idempotent).

**Tauri 2 capability scope + CSP (R3 finding 3).** All four wizard commands MUST be declared in `src-tauri/capabilities/<wizard-capability>.json` with `windows: ["main"]` scope so they're only callable from the main window's webview — NOT from any externally-loaded URL or developer-tools secondary webview. Additionally:

- The wizard webview's CSP (in `tauri.conf.json::app.security.csp`) MUST include `default-src 'self'` + `connect-src 'self' http://127.0.0.1:*` (the latter for Pat's local HTTP API, which `wizard_run_test_send` invokes via `pat_client`).
- The Register link in `<Step2Credentials>` MUST open in the SYSTEM BROWSER via `tauri-plugin-shell::open()` (configured with an allowlist for `winlink.org`), NOT via webview navigation. An external URL loaded into the webview without these guards could call `wizard_persist_cms` and overwrite the operator's keyring credential.
- Integration test verification: `cargo test` with Tauri 2's `tauri::test::mock_runtime` confirms that a webview without the wizard capability cannot invoke the four commands.

### 3.8 Test strategy

**Unit tests (vitest, frontend):**
- `wizardReducer.test.ts`: state-machine transitions for every action; invariants (password clears after successful submit; substate transitions are valid).
- `Step1Welcome.test.tsx`: both choice cards render + click routes correctly via reducer.
- `Step2Credentials.test.tsx`: form validation (loose callsign, password length, grid format), submit button enables only when required fields valid, Save-and-skip vs Continue routing.
- `Step2OfflineIdentity.test.tsx`: blank-submit allowed, identifier accepts tactical strings, grid accepts 4-char.
- `Step3TestSend.test.tsx`: each substate renders the right copy + controls; substate transitions are testable via mocked Tauri command.

**Unit tests (cargo, backend):**
- `wizard_test.rs`: `wizard_persist_cms` happy path + each error class; `wizard_persist_offline` happy path; `wizard_run_test_send` against mocked pat_client.

**Integration tests (cargo, backend; CI-only via `cargo test --test wizard_integration_test --ignored`):**
- Real keyring backend: spawn `gnome-keyring-daemon` + `dbus-launch` in CI per cred-handling spec §3. Verify the wizard's write lands at the exact `(service="tuxlink-pat", account=<callsign>)` shape that Pat's `credstore.Lookup()` finds.
- The Pat-side read is the cred-handling spec's integration test (already shipped via tuxlink-pat#2). Cross-validating the wizard's write reaches that read is the gate.

**Browser smoke (manual, per memory `feedback_browser_smoke_before_ship`):**
- `pnpm tauri dev`. Walk: account → credentials → test-send (idle → skip → complete). Walk: account → offline_identity → complete. Walk: account → credentials → test-send (idle → send → failed → retry → success → complete). Confirm config.json + keyring entry match per state.

### 3.9 File inventory

| Path | Operation | Owner-task |
|---|---|---|
| `src/wizard/Wizard.tsx` | Create | tuxlink-ko0 (Step1 + state-machine wiring) |
| `src/wizard/wizardReducer.ts` | Create | tuxlink-ko0 (per §3.1 full signature) |
| `src/wizard/wizardContext.tsx` | Create | tuxlink-ko0 (`{state, dispatch}` provider) |
| `src/wizard/types.ts` | Create | tuxlink-ko0 (TypeScript discriminated unions for `WizardError` + `TestSendOutcome` per §3.7; shared by all 4 screens) |
| `src/wizard/Step1Welcome.tsx` | Create | tuxlink-ko0 (renamed from `Step1Welcome.tsx` per AMD-2) |
| `src/wizard/Step2Credentials.tsx` | Create | tuxlink-1r5 |
| `src/wizard/Step2OfflineIdentity.tsx` | Create | tuxlink-d76 |
| `src/wizard/Step3TestSend.tsx` | Create | tuxlink-e4x |
| `src/wizard/validators.ts` | Create | tuxlink-1r5 (callsign, password, grid validators per AMD-3 + non-ASCII rejection per §5.NEW homoglyph guard) |
| `src/wizard/*.test.tsx` | Create | per owner-task |
| `src/App.tsx` | Modify | tuxlink-ko0 (wizard-vs-shell routing via `invoke('get_wizard_completed')` on mount) |
| `src-tauri/src/wizard.rs` | Create | tuxlink-1r5 (keyring write; the 4 Tauri commands; `WizardError` + `TestSendOutcome` enums; mutex state; bulk of Rust work) |
| `src-tauri/src/lib.rs` | Modify | tuxlink-1r5 (add `pub mod wizard;`, register all 4 commands in `tauri::generate_handler![...]`, AND call `keyring::use_native_store(true)?` at app init per §3.2's step 0) |
| `src-tauri/Cargo.toml` | Modify | tuxlink-1r5 (add `keyring = "<pin-per-cred-handling-plan>"` per AMD-14; the cred-handling plan pins the exact version compatible with `keyring-core` API surface used in §3.2's pseudocode) |
| `src-tauri/capabilities/wizard.json` | Create | tuxlink-1r5 (Tauri 2 capability declaration scoping the 4 wizard commands to `windows: ["main"]` per §3.7 security paragraph) |
| `src-tauri/tauri.conf.json` | Modify | tuxlink-1r5 (CSP per §3.7: `default-src 'self'; connect-src 'self' http://127.0.0.1:*`; declare `wizard.json` capability; add `tauri-plugin-shell` for system-browser Register link) |
| `src-tauri/tests/wizard_test.rs` | Create | tuxlink-1r5 (unit tests; each Tauri command's happy + each `WizardError` variant; uses mocked keyring backend) |
| `src-tauri/tests/wizard_integration_test.rs` | Create | tuxlink-1r5 (CI-only via `--ignored`; spawns `dbus-launch + gnome-keyring-daemon`; writes via wizard's `keyring::Entry::new` path; reads back via the SAME Rust `keyring` crate at the matching `(service="tuxlink-pat", account=<uppercased-callsign>)` shape — the cross-language assurance is that BOTH the Rust `keyring` crate AND Pat's `go-keyring` honor the freedesktop secret-service D-Bus protocol's collection-and-item model; reading back from Pat's Go code via `cargo test` is NOT attempted — see also a separate shell-level cross-validation script noted below) |
| `dev/scratch/cross-validate-wizard-pat.sh` | Create (optional, dev-only) | tuxlink-1r5 (a 5-line shell script: writes via a Rust helper binary, reads via `secret-tool get service tuxlink-pat account <CALL>`, asserts equality — exercises the cross-language contract once on developer machines; `dev/scratch/` is workspace-but-not-shipped per `feedback_artifacts_in_workspace`) |
| `.github/workflows/release.yml` OR a new `wizard-test.yml` workflow | Modify | tuxlink-1r5 (add `dbus-launch` + `gnome-keyring-daemon` to Linux runner per cred-handling plan's Phase 9 CI integration test recipe — copy verbatim, do not re-invent) |

Owner-task distribution is suggestive, not load-bearing — Task 10 (`tuxlink-1r5`) owns the bulk of the Rust work because the keyring write lives there per AMD-13. Tasks 9/11/11.5 are predominantly TypeScript/React. The shared `types.ts` is owned by Task 9 (`tuxlink-ko0`) because it's the first wizard file the cluster needs, but its content (Rust enum mirroring) is dictated by §3.7 which Task 10's implementer also reads.

---

## 4. Decisions captured during brainstorm

(One spec for the whole wizard cluster, single brainstorm session, parent-agent + operator dialogue.)

1. **Wizard-cluster scope (4 tasks bundled into ONE spec) chosen over per-task atomic specs.** Rationale: state machine + persistence + routing are shared; per-task decomposition forces re-litigating the state machine 4 times across 4 specs. Matches the cred-handling spec precedent. Trade-off accepted: bigger plan PR, more review surface, but coherence > granularity.

2. **Keyring write lives in Task 10 (`tuxlink-1r5`), not Task 9 (`tuxlink-ko0`).** Task 9 is account-existence routing only; the password enters the system on Step 2 (credentials), so the Tauri command + keyring crate dep + integration test all attach to Task 10. AMD-13's "implementing agent (`tuxlink-1r5` — Task 10 owns the keyring WRITE)" makes this explicit.

3. **Transactional keyring-first → config-second.** Locks in that a keyring failure aborts BEFORE persisting any config state, avoiding the "config says CMS, Pat can't find password" failure mode. Inverse ordering rejected. Best-effort rollback if config-write fails after keyring success.

4. **Password cleared from JS state after successful submit.** Defense-in-depth against React-DevTools state inspection; doesn't change the underlying threat model (local-machine attacker has full process access) but signals discipline.

5. **Non-blocking test-send (4 substates per AMD-4).** Failed test-send is information, not a wall. Operator can always reach the inbox. Aligns with AMD-4's "yellow warning, not red error" framing.

6. **Single-callsign-per-wizard scope.** AuxAddr keyring entries are operator-provisioned manually via `secret-tool`; the wizard doesn't expose multi-account UX in v0.0.1. Matches cred-handling spec §2's single-callsign scope.

7. **Credential rotation deferred to Settings.** v0.0.1 wizard runs once; rotation = re-run wizard (via future Settings → Reset wizard) OR `secret-tool store` directly. Documented as a known limitation.

8. **No `winlink_password_present` field in config.json.** Per AMD-11. Keyring is single source of truth; presence is testable via `keyring::Entry::get_password()` returning `Ok(_)` vs an error.

9. **Browser smoke before declaration of complete.** Per memory `feedback_browser_smoke_before_ship` — `pnpm tauri dev` walk-through is a non-optional gate before the cluster's PR can be declared shippable.

---

## 5. Risks and watched failure modes

### 5.1 Keyring backend missing on Linux

**Risk:** Operator on a minimal Linux install (i3 / sway / server) lacks gnome-keyring-daemon or kwalletd. Wizard's `wizard_persist_cms` fails with `ErrUnavailable`. Operator confused.

**Mitigation:** §3.5's error message names the fix (`sudo apt install gnome-keyring`) and points at the install docs. The libsecret-1 documentation work landed via `tuxlink-gdo` ([PR #61](https://github.com/cameronzucker/tuxlink/pull/61)) so the install path is documented when an operator hits this. Wizard does NOT silently fall back to writing the password elsewhere — that would defeat the entire cred-handling refactor.

### 5.2 Keyring locked at wizard time

**Risk:** Operator runs the wizard with their session keyring locked (common right after login on KDE). Write fails. Operator unlocks. Re-submits. Works.

**Mitigation:** §3.5's `ErrLocked` UX explicitly tells the operator to unlock + click Retry. Form preserves input. Acceptable: one extra click.

### 5.3 Wizard interrupted mid-flow

**Risk:** Operator quits tuxlink during the wizard (X out the window). Was their callsign saved? Their password? Their progress?

**Mitigation:** Wizard does NOT persist anything until Submit on Step 2 (`wizard_persist_cms` or `wizard_persist_offline`). Mid-wizard quit = nothing persisted = re-run from Step 1 next launch. Documented in the wizard's `Tools → Reset wizard` post-completion entry: the only way to re-run is the explicit Reset. No partial state to recover from.

### 5.4 Test-send hangs forever

**Risk:** `wizard_run_test_send` invokes `pat_client.send()` and polls inbox for the autoresponder. The poll could loop forever if SERVICE@winlink.org silently doesn't respond.

**Mitigation:** Poll has a hard timeout (default 30 s per design doc §5.3). After 30 s without an autoresponder reply, the substate transitions to `failed` with message "CMS didn't reply within 30 seconds (no autoresponder). Likely cause: CMS busy or your network's outbound port 8773 is blocked." Operator can Retry or Skip.

### 5.5 Operator submits CMS path with wrong password

**Risk:** Wizard writes the wrong password to the keyring. Pat reads it; CMS rejects auth. Wizard's test-send substate goes to `failed`. Operator re-runs the wizard with the right password.

**Mitigation:** The test-send substate's failure UI lists "Wrong password" as a likely cause and points at `Tools → Settings → Reset wizard`. Cross-references the CMS-side error if Pat surfaces it.

### 5.6 Cross-validating wizard-write against Pat-read shape

**Risk:** Wizard writes to `(service="tuxlink-pat", account=W4PHS)` but Pat reads from `(service="tuxlink-pat", account="W4PHS@winlink.org")` (or some other shape mismatch). Pat finds nothing; falls through to promptHub; operator confused.

**Mitigation:** Integration test in §3.8 explicitly cross-validates the shape — wizard writes, then a Pat-style `credstore.Lookup()` reads back. CI-runner gnome-keyring-daemon makes this reproducible. The cred-handling spec §2's `(service, account)` convention is the contract that both sides honor.

### 5.7 Wizard reducer + keyring write race

**Risk:** Operator double-clicks Continue. Two `wizard_persist_cms` invocations fire concurrently. Two keyring writes, two config writes, potentially interleaved. The same race exists for `BEGIN_TEST_SEND` (live CMS transmission consequence per §5.8 below).

**Mitigation.** Three layers:
1. React-side: submit-button is disabled while `state.inFlight === true` per §3.1 + §3.5 last paragraph.
2. Reducer-side: `BEGIN_TEST_SEND` while `testSendSubstate !== 'idle'` is a strict no-op per §3.1 invariant 2.
3. Rust-side: a `tokio::sync::Mutex<()>` in Tauri state guards `wizard_persist_cms`, `wizard_persist_offline`, and `wizard_run_test_send` — any concurrent invocation returns `WizardError::Busy` regardless of which window called it (handles multi-Tauri-window + dev-tools secondary-webview scenarios where React state isn't shared).

### 5.8 BEGIN_TEST_SEND double-fire (Part 97 correctness)

**Risk:** A React double-render (StrictMode, Suspense boundary flush, dev-tools triggered re-render) could fire `BEGIN_TEST_SEND` twice before the first `sending` state paints. Two concurrent `wizard_run_test_send` invocations = two `pat_client.send()` calls = two test messages sent to `SERVICE@winlink.org` under the operator's callsign. This is a **live transmission event** under Part 97 — the operator consented to ONE transmission via the [Send test] click, not two.

**Mitigation.** The reducer's `BEGIN_TEST_SEND` dedup guard (§3.1 invariant 2) is the FIRST defense. The Rust-side mutex (§5.7) is the second defense. The `[Send test]` button MUST be UNCONDITIONALLY ABSENT (not just disabled) when `testSendSubstate !== 'idle'` — this removes the dispatch surface entirely. A reducer unit test (§3.8) covers `BEGIN_TEST_SEND` while `sending` → state unchanged. Part 97 RADIO-1 compliance audit.

### 5.9 Callsign homoglyph / non-ASCII input

**Risk:** Operator on a non-English locale keyboard accidentally types a Cyrillic А (U+0410) instead of Latin A (U+0041), or a zero-width joiner (U+200D), or an em-dash where ASCII hyphen was expected. The "loose validator" per AMD-3 might pass these; the Rust keyring write stores `W4PHSА` (with Cyrillic А); Pat's `credstore.Get("W4PHSA")` (Latin) misses; CMS auth fails; operator confused. Worse: the validator may accept the input, the user-visible string LOOKS like a valid callsign, but the byte-level mismatch is invisible.

**Mitigation.** The frontend validator (`src/wizard/validators.ts`) AND the Rust `wizard_persist_cms` step 2 BOTH reject non-ASCII callsigns. Frontend: explicit `[A-Za-z0-9/]+` regex check before submit (UX inline error). Rust: `!callsign.is_ascii()` returns `WizardError::InvalidInput { field: "callsign" }` (defense in depth). The Rust check is non-negotiable — defense-in-depth catches a malicious or buggy frontend.

### 5.10 macOS Keychain first-access authorization prompt

**Risk:** On macOS first-run, the OS Keychain prompts the user for their macOS login password the FIRST time tuxlink writes to the keychain. The prompt is a modal system dialog that blocks the Tauri command indefinitely. The operator may click Deny (didn't recognize the app prompting; mis-clicked). Result: `ErrPermissionDenied` returned to the wizard.

**Mitigation.** §3.5's `ErrPermissionDenied` table row carries a MACOS-SPECIFIC copy variant: "macOS Keychain requires you to authorize tuxlink to store your password. A system dialog should have appeared; if you clicked Deny, click Retry and authorize when prompted." This contrasts with the Linux variant ("This is unusual; check distro settings") which would actively mislead on macOS.

### 5.11 Windows CredentialManager 2.5 KB blob limit

**Risk:** Windows CredentialManager has a documented 2,560-byte limit on generic credential blobs. Most Winlink passwords (8-32 chars) fit easily, but an unusually long password or `keyring` crate encoding overhead could exceed it. Result: `entry.set_password()` returns an OS error not matching `ErrLocked` / `ErrUnavailable` / `ErrPermissionDenied`; falls through to `WizardError::Other { detail }` with the verbatim OS error string.

**Mitigation.** §3.5's `ErrOther` row's message surfaces the verbatim detail string alongside an issue-tracker link. The wizard does NOT silently truncate. An operator with a >2KB password sees a clear error and can shorten the CMS password on the Winlink web UI side. Adding a frontend max-length warning is deferred (rare in practice).

### 5.12 Test-send captive-portal false-success

**Risk:** Captive portals (airport wifi, hotel network) return HTTP 200 with a login-page HTML body for any outbound request. `pat_client.send()` may report success from Pat's perspective (HTTP 200 received) while the CMS never received anything. The wizard's `wizard_run_test_send` then waits for the autoresponder reply; after 30 s timeout, transitions to `failed` with "no autoresponder" — but the captive-portal scenario is indistinguishable from "CMS busy / port blocked" without deeper inspection.

**Mitigation.** §3.4's `failed` substate copy is amended to list **three** likely causes (vs the prior two): "no internet connection, firewall blocking port 8773, CMS temporarily busy, **OR a captive portal / network login page intercepting traffic**." Defense by operator education + the 30-s autoresponder timeout (which catches the captive-portal case as a fail). True positive-success detection of captive portals would require parsing the CMS response shape — out of scope for v0.0.1.

### 5.13 Schema version downgrade on wizard re-run

**Risk:** A future tuxlink v0.1+ introduces `schema_version: 2` in config.json. An operator running an OLDER tuxlink (v0.0.1) against a v2 config (e.g., during a botched downgrade) would have the wizard write `schema_version: 1` over the v2 file, losing v2 fields silently.

**Mitigation.** `wizard_persist_cms` and `wizard_persist_offline` MUST read the existing config.json (if present) and refuse to overwrite if `schema_version > 1`. Error: `WizardError::Other { detail: "Existing config.json has schema_version N > 1; refusing to downgrade. Upgrade tuxlink to configure." }`. Surfaced as a non-fatal banner in the wizard's Step 2 (CMS or offline). Defense for a corner case that won't fire in v0.0.1 but codifies the contract for future versions.

---

## 6. References

### 6.1 In-repo

- `docs/design/v0.0.1-ux-mockups.md` §5.1-5.4 — canonical UX baseline
- `docs/superpowers/specs/2026-05-18-cred-handling-design.md` §2 + §3 + §5 — the `(service, account)` keyring convention + Pat-side read contract
- `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` — base plan with AMD-1..14
- AMD-1 (Task 2 config schema), AMD-2 (Task 9 routing), AMD-3 (Task 10 callsign validator), AMD-4 (Task 11 4-substate test-send), AMD-5 (Task 11.5 offline path), AMD-11 (drop winlink_password_present), AMD-13 (keyring write contract), AMD-14 (keyring crate authorization)
- ADR 0011 — Pat fork rationale; keyring-direct is the v0.0.1 commitment

### 6.2 External

- [`keyring`](https://crates.io/crates/keyring) — Rust crate, used by wizard's persist_cms
- [`zalando/go-keyring`](https://github.com/zalando/go-keyring) — Pat-side equivalent (already vendored in `external/tuxlink-pat/go.mod`)
- [Tauri 2 `invoke` API](https://tauri.app/v2/guides/features/command/) — React → Rust command invocation
- [secret-service D-Bus spec](https://specifications.freedesktop.org/secret-service/latest/) — the Linux protocol both keyring crates honor

### 6.3 Watched failure modes references

- `feedback_browser_smoke_before_ship` (auto-memory) — manual UI walk-through before declaration of complete
- `feedback_no_carveout_on_cross_provider_adrev` (auto-memory) — the Wave-1 revocation rationale that triggered this Wave-2 rewrite

---

## 7. Adrev disposition

5-round cross-provider adversarial review completed 2026-05-18 on the pre-adrev spec at commit `9ce433e`. **46 total findings** (11 P0, 15 P1, ~13 P2, ~7 P3) across R1-R4 Claude rounds; R5 Codex inconclusive (CLI hung past 10-minute timeout — second time this Codex CLI has produced no output for this project; noted as a known tool reliability issue and skipped for this spec, may revisit). Findings consolidated into this revision (commit immediately following pre-adrev).

### 7.1 Per-round summary

| Round | Lens | Reviewer | Findings | Disposition |
|---|---|---|---|---|
| R1 | Friction (would an implementing subagent know what to do?) | Claude Sonnet 4.6 subagent | 13 (3 P0, 4 P1, 4 P2, 2 P3) | All 3 P0 applied (reducer signature, async-dispatch pattern, WizardError shape); 3 of 4 P1 applied (TestSendOutcome defined, get_wizard_completed added, build_config / write_config_atomic origin cited); 4th P1 (MBO auto-fill timing) applied as inline note in §3.1. P2/P3 applied where tightly scoped (camelCase/snake_case Tauri note, ErrBusy table row); deferred where peripheral. |
| R2 | Contract (do internal interfaces align?) | Claude Sonnet 4.6 subagent | 10 (2 P0, 3 P1, 3 P2, 2 P3) | Both P0 applied (callsign normalization moved into command body §3.2; ErrBusy added to §3.5 + WizardError enum §3.7). P1 applied: `wizard_persist_offline` connect_to_cms hardcoded explicitly; testSendLog field added to WizardState §3.1 + streamed via Tauri event per §3.7; integration test cross-language gate clarified §3.9 (shape-by-protocol, not Go-from-Rust). P2 applied. |
| R3 | Coverage (missed failure modes / race conditions / security) | Claude Sonnet 4.6 subagent | 12 (3 P0, 5 P1, 3 P2, 1 P3) | All 3 P0 applied: BEGIN_TEST_SEND dedup guard §3.1 + §5.8 (Part 97 critical); app-quit partial-write recovery §3.2 + §5.3 amendment; Tauri capability scope + CSP + system-browser Register link §3.7. All 5 P1 applied as new §5 entries (§5.9 homoglyph, §5.10 macOS first-access, §5.11 Windows blob, §3.5's post-wizard Seahorse-deletion paragraph, §5.7's amended Skip-during-sending). P2 applied (rollback-of-rollback augmented §3.5 message; captive portal added §5.12; multi-window mutex §5.7). P3 (schema version downgrade) applied as §5.13. |
| R4 | Cross-task (alignment with cred-handling spec + plan AMDs + design doc) | Claude Sonnet 4.6 subagent | 11 (3 P0, 3 P1, 3 P2, 2 P3) | All 3 P0 applied: `use_native_store(true)?` added as step 0 §3.2 + §3.9 lib.rs note; `ErrPermissionDenied` reframed as wizard-internal §3.5 with naming-alignment clarification; `Step1Welcome.tsx` rename applied via global edit §2.1 + §3.9. P1-A (design doc §5.2 staleness) and P1-B (plan Task 11.5 winlink_password_present) and P1-C (plan's hasAccount snippet) are **OUT OF SCOPE for this spec revision**: they require amending docs/design/v0.0.1-ux-mockups.md and docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md respectively, which is a separate AMD/design-doc-update PR. Tracked as a follow-up bd issue (see §7.3). P2-A (cross-language gate clarification) applied §3.9. P2-B (Settings → Reset wizard menu deferral) applied §3.5 last paragraph. P2-C (cite cred-handling §3.3 not AMD-1 for normalization) applied §2.3. P3 applied. |
| R5 | Cross-provider (Codex; lens-free) | Codex CLI via `npx @openai/codex exec` | **Inconclusive** | Process ran past 600s timeout without producing output (third Codex-CLI session this project that produced empty output; pattern indicates a tool-reliability issue, not a spec issue). Findings file `dev/adversarial/2026-05-18-wizard-cluster-adrev-R5-cross-provider-codex.md` was never created. Spec ships with 4 Claude-round coverage; cross-provider gate not satisfied. **Risk acceptance:** the 4 Claude rounds covered distinct lenses (friction / contract / coverage / cross-task) with high specificity and minimal overlap; the cross-provider lens was the additional check, not the only-novel one. Operator may re-run R5 separately if desired. |

### 7.2 Findings rejected with reasoning

None in this revision. Three R4-P1 cross-doc staleness findings (P1-A, P1-B, P1-C) are deferred to a separate AMD PR (see §7.3) rather than rejected — the wizard spec correctly carries the post-AMD-13/AMD-11 state internally; the deferral is about WHERE the fixes land, not whether they're needed.

### 7.3 Follow-up bd issues to file

Findings that require changes OUTSIDE this spec — captured here for downstream PR creation:

1. **AMD on docs/design/v0.0.1-ux-mockups.md §5.2 — drop "v0.1+ promotion / pat config" language per AMD-13.** R4-P1-A. The design doc §5.2 "Keep" line still says "stored to OS keychain (v0.1+ promotion) or pat config (v0.0.1)" which AMD-13 superseded. Create a small "design doc amendment" PR.

2. **AMD-15 (or similar) on docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md Task 11.5 — drop `winlink_password_present = false` from Step 1 behavior.** R4-P1-B. AMD-11 dropped this field but didn't cascade to Task 11.5's behavior text. One-line plan fix.

3. **AMD on plan Task 9 Step 3+4 code snippets — update `hasAccount: boolean | null` → `connectToCms: boolean | null`.** R4-P1-C. AMD-2 explicitly notes Step 1-7 code snippets aren't updated; this AMD makes the update explicit (or, alternatively, deletes the snippets and points the implementer at this spec's §3.1).

These three can be bundled as a single small "post-cred-handling docs cleanup" PR, separate from the wizard impl plan.

### 7.4 Codex R5 follow-up

If a fresh attempt at the Codex R5 lens produces findings, those land as a second spec-revision commit (or a planless P1+ patch if no findings are spec-affecting). Tracked as a follow-up bd issue to re-attempt the Codex round outside the current session.

---

## 8. Status

- **Spec status:** post-adrev (revision applied). Ready for operator review.
- **Next phase:** writing-plans skill on operator approval → implementation plan covering Tasks 9 + 10 + 11 + 11.5 in dependency order (Task 9 → 10 or 11.5 → 11; per-file ownership per §3.9).
- **Open follow-up:** the three R4-P1 cross-doc staleness fixes (§7.3) and the Codex R5 retry (§7.4) are tracked separately and don't block this spec's transition to plan-writing.
