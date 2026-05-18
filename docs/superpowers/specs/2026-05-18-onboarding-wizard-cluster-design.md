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

- **4 React components**: `<Step1Account>`, `<Step2Credentials>`, `<Step2OfflineIdentity>`, `<Step3TestSend>` (filenames per current v0.0.1 plan Tasks 9/10/11/11.5).
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

`wizardState` is a TypeScript reducer-shaped state held in `<Wizard>` and threaded to child screens via context. The shape per AMD-2 + AMD-5:

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
  callsign: string;                // empty until Step 2 (CMS) submit
  password: string;                // empty until Step 2 (CMS) submit; cleared from state after wizard_persist_cms succeeds
  identifier: string;              // empty unless offline path
  grid: string;                    // optional; both paths
  mboAddress: string;              // optional; CMS path only; defaults to "<callsign>@winlink.org" on callsign change
  testSendSubstate: 'idle' | 'sending' | 'success' | 'failed';
  testSendError: string | null;    // populated when testSendSubstate==='failed'
}
```

Reducer actions: `SET_CONNECT_TO_CMS`, `ADVANCE_FROM_ACCOUNT`, `SET_CREDENTIALS_FIELD`, `SUBMIT_CREDENTIALS` (async; calls `wizard_persist_cms`), `SET_OFFLINE_FIELD`, `SUBMIT_OFFLINE` (async; calls `wizard_persist_offline`), `BEGIN_TEST_SEND` (sets substate to 'sending'; spawns `wizard_run_test_send`), `TEST_SEND_RESULT` (folds into 'success' or 'failed' with error message), `SKIP_TEST_SEND` (sets `step: 'complete'`).

**Crucial state-machine invariant:** `password` is cleared from `WizardState` (set to empty string) immediately after `wizard_persist_cms` returns success. The plaintext password lives in JavaScript memory ONLY between the user typing it and the Rust command writing it to the keyring. Browser dev-tools console / React-DevTools state inspector should not be able to retrieve it after submit. (Defense-in-depth, not a serious threat model — JS memory was always exposed to a local-machine attacker — but worth doing.)

### 3.2 Persistence contract — transactional pair

The wizard's two write targets are tuxlink's config.json AND the OS keyring. For the CMS path, both writes must succeed together — partial-success states are operator-confusing (e.g., "config thinks I'm CMS but Pat can't find my password"). The Rust-side `wizard_persist_cms` command implements transactional semantics:

```rust
#[tauri::command]
async fn wizard_persist_cms(
    callsign: String, password: String, grid: String,
    mbo_address: String, connect_to_cms: bool,
) -> Result<(), WizardError> {
    // 1. Validate inputs (callsign via validate_identity, grid via grid validator)
    // 2. Build the new Config struct in memory (does NOT touch disk yet)
    let new_config = build_config(/* … */)?;
    // 3. Write keyring FIRST (so a keyring failure aborts before disk write)
    let entry = keyring::Entry::new("tuxlink-pat", &callsign)?;
    entry.set_password(&password)?;
    // 4. Write config.json (atomic-rename via tempfile + std::fs::rename)
    write_config_atomic(&new_config)?;
    // 5. If step 4 fails, roll back step 3 (best-effort delete keyring entry)
    //    — covered in §3.5 error UX
    Ok(())
}
```

Ordering rationale: keyring-first means a keyring failure (locked, daemon unavailable, etc.) is reported to the operator BEFORE any persistent state changes. A subsequent config.json write failure (disk full, permissions) triggers a best-effort keyring rollback — but if rollback fails too, the operator is told explicitly and given the `secret-tool delete` command to clean up manually. Inverse ordering (config first, keyring second) leaves a config.json saying "CMS path" without a keyring entry — Pat then can't connect, and the operator has to guess what went wrong.

The offline path's `wizard_persist_offline` is single-write (config.json only, no keyring) — atomic by definition.

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
| `failed` | `TEST_SEND_RESULT` with failure outcome | Yellow warning (NOT red error) + likely-cause list (no internet, firewall, CMS busy) + the specific error from `testSendError` | [Retry] [Go to inbox] [Open Settings] |

Non-blocking principle: every substate has a path to the inbox. Failed test-send is INFORMATION not a wall; the operator's credentials are saved regardless and they can retry from `Session → Test send` post-wizard (the AMD-10 menu item that lands in Task 7).

### 3.5 Error UX — keyring failures and the credential flow

The wizard handles three classes of keyring failure, mapped to the cred-handling spec's `ErrLocked` / `ErrUnavailable` / `ErrNotFound` sentinels (plus a fourth: `ErrPermissionDenied` for daemon-running-but-rejecting):

| Failure | When | Operator-visible message | Recovery path |
|---|---|---|---|
| `ErrUnavailable` (no daemon / no D-Bus) | Wizard submit-credentials → keyring write | "Tuxlink couldn't find a secret-service keyring on your system. Tuxlink uses the OS keyring to store your Winlink CMS password securely (instead of saving it to a config file). Install and start one (e.g., `sudo apt install gnome-keyring`) and re-run the wizard. See [installation docs](#) for distro-specific guidance." | Form stays mounted; operator can copy the message + retry after installing |
| `ErrLocked` (daemon present but session locked) | Wizard submit-credentials → keyring write | "Your keyring is currently locked. Unlock it (typically: click the keyring icon in your system tray, OR run `secret-tool lock --collection=default` followed by your login password prompt) and click Retry." | Form stays mounted with a Retry button alongside Continue |
| `ErrPermissionDenied` (daemon refused write) | Wizard submit-credentials → keyring write | "The keyring daemon refused the write. This is unusual; check your distro's keyring permission settings or report the issue at github.com/cameronzucker/tuxlink/issues." | Form stays mounted; suggests filing an issue |
| `ErrConfigWrite` (keyring succeeded, config.json failed) | Step 4 of `wizard_persist_cms` | "Tuxlink wrote your password to the keyring but couldn't save the config file (disk full? permissions?). Tuxlink has attempted to remove the keyring entry; if you see a stale `tuxlink-pat` entry for callsign `<callsign>`, run `secret-tool delete service tuxlink-pat account <callsign>`." | Form stays mounted; operator addresses the disk issue + retries |

The form's submit button is disabled during in-flight `wizard_persist_cms`. The form REMAINS mounted across failure — operator does not lose their typed input.

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

Three new commands in `src-tauri/src/wizard.rs` (new file) registered in `tauri::generate_handler![...]`:

| Command | Signature | Synchronous? | Side effects |
|---|---|---|---|
| `wizard_persist_cms` | `(callsign, password, grid, mbo_address, connect_to_cms) → Result<(), WizardError>` | Async (D-Bus call to keyring; disk write) | Keyring write + config.json atomic write |
| `wizard_persist_offline` | `(identifier, grid) → Result<(), WizardError>` | Async (disk write only) | Config.json atomic write |
| `wizard_run_test_send` | `() → Result<TestSendOutcome, WizardError>` | Async (HTTP to Pat + inbox poll) | None (Pat handles the actual TX) |

`WizardError` is an enum with variants for each failure mode in §3.5 plus an `Other(String)` catch-all. Serialized to JSON via `serde::Serialize`; the React side pattern-matches on the variant tag.

### 3.8 Test strategy

**Unit tests (vitest, frontend):**
- `wizardReducer.test.ts`: state-machine transitions for every action; invariants (password clears after successful submit; substate transitions are valid).
- `Step1Account.test.tsx`: both choice cards render + click routes correctly via reducer.
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
| `src/wizard/wizardReducer.ts` | Create | tuxlink-ko0 |
| `src/wizard/wizardContext.tsx` | Create | tuxlink-ko0 |
| `src/wizard/Step1Account.tsx` | Create | tuxlink-ko0 |
| `src/wizard/Step2Credentials.tsx` | Create | tuxlink-1r5 |
| `src/wizard/Step2OfflineIdentity.tsx` | Create | tuxlink-d76 |
| `src/wizard/Step3TestSend.tsx` | Create | tuxlink-e4x |
| `src/wizard/validators.ts` | Create | tuxlink-1r5 (callsign, password, grid validators per AMD-3) |
| `src/wizard/*.test.tsx` | Create | per owner-task |
| `src/App.tsx` | Modify | tuxlink-ko0 (wizard-vs-shell routing) |
| `src-tauri/src/wizard.rs` | Create | tuxlink-1r5 (keyring write; bulk of Rust work) |
| `src-tauri/src/lib.rs` | Modify | tuxlink-1r5 (add `pub mod wizard;` + register commands) |
| `src-tauri/Cargo.toml` | Modify | tuxlink-1r5 (add `keyring` crate per AMD-14) |
| `src-tauri/tests/wizard_test.rs` | Create | tuxlink-1r5 |
| `src-tauri/tests/wizard_integration_test.rs` | Create | tuxlink-1r5 (CI-only) |
| `.github/workflows/release.yml` OR a new wizard-test workflow | Modify | tuxlink-1r5 (add gnome-keyring-daemon + dbus-launch to Linux runner per cred-handling spec §3) |

Owner-task distribution is suggestive, not load-bearing — Task 10 (`tuxlink-1r5`) owns the bulk of the Rust work because the keyring write lives there per AMD-13. Tasks 9/11/11.5 are predominantly TypeScript/React.

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

**Risk:** Operator double-clicks Continue. Two `wizard_persist_cms` invocations fire concurrently. Two keyring writes, two config writes, potentially interleaved.

**Mitigation:** React's submit-button disabled-during-in-flight pattern (§3.5 last paragraph). The Rust side also debounces — a second invocation while the first is in flight returns `ErrBusy` (which the React side suppresses as a no-op since the user clearly meant the same action).

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

_(To be filled in after the 5-round cross-provider adversarial review per `build-robust-features` pipeline.)_

Planned rounds:
- R1 — Claude subagent, friction lens (does an implementing agent know what to do at each step?)
- R2 — Claude subagent, contract lens (does §3.7's Tauri command surface align with §3.2's reducer actions?)
- R3 — Claude subagent, coverage lens (do the §5 risks cover all the failure modes implied by §3.5 error UX?)
- R4 — Claude subagent, cross-task lens (does the spec align with the cred-handling spec §2/§3/§5 and with AMDs 11+13+14?)
- R5 — Codex CLI (`npx --yes @openai/codex exec`), cross-provider lens

Each round writes a punch-list to `dev/adversarial/2026-05-18-wizard-cluster-adrev-R<N>-{provider}.md` (gitignored). Findings consolidated + dispositioned in a spec revision commit.
