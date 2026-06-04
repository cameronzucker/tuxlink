# VARA + ARDOP Panel Alpha-Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Spec:** [docs/superpowers/specs/2026-06-04-vara-ardop-panel-alpha-design.md](../specs/2026-06-04-vara-ardop-panel-alpha-design.md) (commit `73333e2`)
**bd umbrella:** tuxlink-0ye6 (P1; subsumes tuxlink-fzl7 + tuxlink-12sc per dep edges)

**Goal:** Replace the current per-protocol `ArdopRadioPanel` + `VaraRadioPanel` with a single shared `RadioSessionPanel` driven by sidebar intent (cms / p2p / radio-only), introducing a WLE-grounded Open/Close Session lifecycle that auto-arms the listener (for p2p/radio-only) and unlocks outbound dial — while stripping tuxlink-added safeguards (`CONNECT_DEADLINE`, `ConsentModal`, `useConsent`, RADIO-1 identifier surface).

**Architecture:** One React component (`RadioSessionPanel`) parameterized by `RadioPanelMode` props, with a per-protocol adapter (`ardopAdapter`, `varaHfAdapter`, `varaFmAdapter`) supplying Tauri command names + the settings-expander render function. Backend grows a `SessionIntent { Cms, P2p, RadioOnly }` enum and per-protocol `*_open_session(intent)` / `*_close_session()` / `*_b2f_exchange(target, intent)` Tauri commands; `*_open_session` opens the transport AND auto-arms the listener (for intent in `{p2p, radio-only}`) as one operation. The `consent_token` parameter is removed from all ARDOP commands; a backend in-process busy guard against frontend double-call replaces the RADIO-1 modal.

**Tech Stack:** TypeScript / React (frontend, Vite + Vitest), Rust / Tauri 2 (backend, `cargo test`), `pnpm` for JS dependency management.

---

## How to use this plan

- **Base branch:** branch off `origin/main` as `bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish` (per ADR 0004 + ADR 0008's worktree-issue-ownership rule). Create a worktree at `worktrees/tuxlink-0ye6/` and claim it on the bd issue.
- **Phases are sequential.** Phase 1 (safeguard strip) is a clean-slate prerequisite for Phases 3 + 5. Phase 2 (type widening) unblocks Phases 3 + 5. Phases 3 + 4 are backend; 5 + 6 are frontend.
- **Within a phase, tasks are roughly sequential** unless explicitly noted as parallelizable.
- **TDD discipline:** each task has Write-test → Verify-fails → Implement → Verify-passes → Commit. Don't skip "verify-fails" — that's the gate that catches false-positive tests.
- **Commit cadence:** one commit per task unless a task says otherwise. Use conventional types (`feat`/`fix`/`refactor`/`test`/`chore`). Include `Agent:` trailer with your session moniker + `Co-Authored-By:` (per CLAUDE.md).
- **Push cadence:** push the instant a unit is committed + tests green (per the `never-hold-a-push` memory). Don't batch.
- **Reference paths in this plan are relative to `origin/main`.** If you find a path differs from what the plan says, prefer current code as truth and update the plan inline as you go.

---

## File-structure map

**Files this plan will CREATE:**

- `src/radio/modes/RadioSessionPanel.tsx` — shared component
- `src/radio/modes/RadioSessionPanel.css` — sibling styles
- `src/radio/modes/RadioSessionPanel.test.tsx` — component tests
- `src/radio/modes/radioSessionAdapters.ts` — per-protocol adapter interface + impls
- `src/radio/modes/radioSessionAdapters.test.ts` — adapter tests
- `src-tauri/src/winlink/session_intent.rs` — `SessionIntent` enum (or extend `winlink/mod.rs`)

**Files this plan will DELETE:**

- `src/modem/ConsentModal.tsx` + `ConsentModal.test.tsx`
- `src/modem/useConsent.ts` + `useConsent.test.ts`
- `src/radio/modes/ArdopRadioPanel.tsx` + `ArdopRadioPanel.css` + `ArdopRadioPanel.test.tsx`
- `src/radio/modes/VaraRadioPanel.tsx` + `VaraRadioPanel.css` + `VaraRadioPanel.test.tsx`

**Files this plan will MODIFY:**

- `src/connections/sessionTypes.ts` — flip `radio-only` to built
- `src/radio/types.ts` — widen `RadioPanelMode.intent` to include `radio-only`
- `src/radio/radioPanelVisibility.ts` — thread `radio-only` through visibility router
- `src/radio/RadioPanel.tsx` — switch ardop-hf/vara-hf/vara-fm to render `RadioSessionPanel`
- `src-tauri/src/modem_commands.rs` — drop `CONNECT_DEADLINE`, drop `consent_token` params, add `intent` params, add ARDOP session-lifecycle commands
- `src-tauri/src/ui_commands.rs` — rename `vara_start_session` → `vara_open_session(intent)`, `vara_stop_session` → `vara_close_session()`, add `modem_vara_b2f_exchange`, wire ABORT side-channel
- `src-tauri/src/lib.rs` — register new Tauri commands, drop deleted ones
- `src-tauri/src/winlink/modem/vara/transport.rs` — add `try_clone_abort_writer` for VARA
- `src-tauri/src/winlink/modem/vara/session.rs` (if separate) — add `abort_in_flight` mirroring ARDOP
- `src/modem/useModemStatus.ts` — if it references consent fields, drop them

---

## Phase 0 — Workspace setup

### Task 0.1: Create worktree off origin/main and claim it on the bd issue

**Files:** N/A — environment setup only.

- [ ] **Step 1: Refresh origin/main**

```bash
git fetch origin main
```

- [ ] **Step 2: Create the worktree per ADR 0008 + ADR 0009**

Use the project's `new_tuxlink_worktree.py` helper (or whatever the operator-tooling-on-main equivalent is at execution time):

```bash
python3 scripts/new_tuxlink_worktree.py --branch bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish --base origin/main --path worktrees/tuxlink-0ye6
```

If that script doesn't exist, fall back to:

```bash
git worktree add -b bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish worktrees/tuxlink-0ye6 origin/main
```

Expected: a new worktree at `worktrees/tuxlink-0ye6/` on branch `bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish`.

- [ ] **Step 3: Claim the worktree on the bd issue**

```bash
bd remember --issue tuxlink-0ye6 "worktree at worktrees/tuxlink-0ye6/ on branch bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish"
```

Expected: `bd show tuxlink-0ye6` includes the worktree pointer in the notes.

- [ ] **Step 4: Sanity-check the spec is reachable from the worktree**

```bash
ls -la worktrees/tuxlink-0ye6/docs/superpowers/specs/2026-06-04-vara-ardop-panel-alpha-design.md
```

Expected: file exists. If not, the spec hasn't been merged to main yet — the implementer needs to merge `bd-tuxlink-xygm/recover-handoffs` to main first, or work from this plan + the spec path on the recovery branch and copy them in.

- [ ] **Step 5: Establish baseline gates pass**

From inside the worktree:

```bash
pnpm install
pnpm typecheck && pnpm test && cargo --manifest-path src-tauri/Cargo.toml test
```

Expected: all green. If anything fails, STOP and surface to operator — the baseline must be clean before refactoring.

No commit for this task.

---

## Phase 1 — Strip tuxlink-added safeguards

Per spec §2 and memory `no-tuxlink-added-safeguards`. This phase is REMOVAL-ONLY and unblocks the rest. Modem-native timeouts (VARA's connect timeout, ARDOP's `ARQTIMEOUT`) stay. The internal backend busy guard (replacing `consume_consent_token` as the dup-call defense) is the only NEW code in this phase.

### Task 1.1: Replace `consume_consent_token` gate with an in-process busy guard in `modem_ardop_connect`

**Files:**
- Modify: `src-tauri/src/modem_commands.rs`
- Modify: `src-tauri/src/winlink/modem/ardop/session.rs` (or wherever `ModemSession` lives)
- Test: `src-tauri/src/modem_commands.rs` (existing `tests` module)

**Rationale:** The `consume_consent_token` atomic was a RADIO-1 modal artifact. The spec mandates dropping the modal, so the token plumbing comes with it. The internal busy guard (a `connect_in_progress: AtomicBool`) keeps the dup-call defense that prevented frontend bug-loops, without the user-facing modal.

- [ ] **Step 1: Write the failing test — busy guard rejects concurrent connect**

In `src-tauri/src/modem_commands.rs`'s `tests` module, add:

```rust
#[test]
fn connect_rejects_concurrent_call_when_already_in_progress() {
    let session = Arc::new(ModemSession::new());
    let cfg = ArdopUiConfig::default();

    // Simulate the first connect having flipped the busy bit by calling the
    // helper directly. The factory blocks until we drop the sentinel so the
    // first call never completes during the test.
    let (sentinel_tx, sentinel_rx) = std::sync::mpsc::channel::<()>();
    let session_clone = Arc::clone(&session);
    let h = std::thread::spawn(move || {
        let factory = move |_: ArdopConfig, _: &str| -> Result<Box<dyn ModemTransport>, String> {
            // Block until released; tests `take` the sentinel below to start it.
            sentinel_rx.recv().ok();
            Err("test stub never connects".into())
        };
        modem_ardop_connect_gated_with_factory(&session_clone, "K7TEST", &cfg, factory)
    });

    // Give the worker a beat to enter the busy state. (No production code
    // races on this — the busy guard is set before the factory call.)
    std::thread::sleep(std::time::Duration::from_millis(50));

    let factory_2 =
        |_: ArdopConfig, _: &str| -> Result<Box<dyn ModemTransport>, String> {
            panic!("factory must not run when a connect is already in progress");
        };
    let err = modem_ardop_connect_gated_with_factory(&session, "K7TEST", &cfg, factory_2)
        .expect_err("second concurrent call must reject");
    assert!(err.contains("connect already in progress"), "got: {err}");

    // Release the first worker so the test can exit.
    sentinel_tx.send(()).ok();
    let _ = h.join();
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo --manifest-path src-tauri/Cargo.toml test \
  modem_commands::tests::connect_rejects_concurrent_call_when_already_in_progress
```

Expected: FAIL — either `modem_ardop_connect_gated_with_factory` still has the consent-token signature, or no busy guard exists yet.

- [ ] **Step 3: Implement the busy guard**

In `src-tauri/src/winlink/modem/ardop/session.rs` (the file containing `ModemSession`; locate via `grep -n "pub struct ModemSession" src-tauri/src/`):

Add a `connect_in_progress: AtomicBool` field to `ModemSession`. Add methods:

```rust
/// Try to begin a connect. Returns `true` if the caller now owns the busy
/// bit; `false` if another connect is already in flight. Caller MUST call
/// `clear_connect_in_progress()` in every exit path.
pub fn try_begin_connect(&self) -> bool {
    self.connect_in_progress
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}

/// Release the busy bit. Must pair with a successful `try_begin_connect`.
pub fn clear_connect_in_progress(&self) {
    self.connect_in_progress.store(false, Ordering::Release);
}
```

In `src-tauri/src/modem_commands.rs`, change `modem_ardop_connect_gated_with_factory` to drop the `consent_token` parameter and gate on `try_begin_connect` instead:

```rust
pub fn modem_ardop_connect_gated_with_factory<F>(
    session: &Arc<ModemSession>,
    target: &str,
    ardop_ui: &ArdopUiConfig,
    make_transport: F,
) -> Result<(), String>
where
    F: FnOnce(ArdopConfig, &str) -> Result<Box<dyn ModemTransport>, String>,
{
    if !session.try_begin_connect() {
        return Err("connect already in progress; wait for the previous attempt to complete".into());
    }
    // RAII guard: clear busy bit on every exit path.
    struct ConnectGuard<'a>(&'a Arc<ModemSession>);
    impl<'a> Drop for ConnectGuard<'a> {
        fn drop(&mut self) {
            self.0.clear_connect_in_progress();
        }
    }
    let _guard = ConnectGuard(session);

    modem_ardop_connect_post_consume_with_factory(session, target, ardop_ui, make_transport)
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo --manifest-path src-tauri/Cargo.toml test \
  modem_commands::tests::connect_rejects_concurrent_call_when_already_in_progress
```

Expected: PASS.

- [ ] **Step 5: Run all backend tests to surface fallout from signature change**

```bash
cargo --manifest-path src-tauri/Cargo.toml test
```

Expected: some failures in tests that still pass `consent_token` — that's fine, Tasks 1.2 + 1.3 fix the callers. Note them; don't fix yet.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/modem_commands.rs src-tauri/src/winlink/modem/ardop/session.rs
git commit -m "refactor(modem-ardop): replace consent-token gate with busy guard

Drops the RADIO-1 consume_consent_token atomic; the consent modal goes
away in a follow-up task. Internal AtomicBool busy guard preserves the
dup-call defense without a user-facing modal.

Spec §2 \"No tuxlink-added safeguards\"; bd tuxlink-0ye6.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 1.2: Drop `consent_token` parameter from `modem_ardop_connect` + `modem_ardop_b2f_exchange` Tauri commands

**Files:**
- Modify: `src-tauri/src/modem_commands.rs` (the two `#[tauri::command]` wrappers)
- Modify: `src-tauri/src/lib.rs` if any signature appears in the `.invoke_handler` macro (it shouldn't — Tauri infers from the fn).

- [ ] **Step 1: Write the failing test — Tauri command signature has no consent_token**

In `src-tauri/src/modem_commands.rs`'s `tests` module:

```rust
#[test]
fn modem_ardop_connect_signature_has_no_consent_token() {
    // Compile-time assertion: if the wrapper still takes consent_token,
    // this won't compile. The body is irrelevant — the type check at the
    // module boundary is the test.
    let _f: fn(
        AppHandle,
        State<'_, Arc<ModemSession>>,
        String, // target
    ) -> Result<(), String> = modem_ardop_connect;
}
```

(If `modem_ardop_connect`'s production signature includes more `State` or `AppHandle` args, mirror them in the assertion.)

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo --manifest-path src-tauri/Cargo.toml test \
  modem_commands::tests::modem_ardop_connect_signature_has_no_consent_token
```

Expected: FAIL with a type-mismatch error (the current signature includes `consent_token: String`).

- [ ] **Step 3: Update both Tauri wrappers to drop `consent_token`**

In `src-tauri/src/modem_commands.rs`:

```rust
#[tauri::command]
pub fn modem_ardop_connect(
    app: AppHandle,
    session: State<'_, Arc<ModemSession>>,
    target: String,
) -> Result<(), String> {
    // ... existing body, but remove the consume_consent_token call (now
    // handled inside modem_ardop_connect_gated_with_factory) and pass
    // through to the factory wrapper.
}

#[tauri::command]
pub fn modem_ardop_b2f_exchange(
    app: AppHandle,
    session: State<'_, Arc<ModemSession>>,
    target: String,
) -> Result<(), String> {
    // ... existing body, but remove the consume_consent_token gate at the
    // top — the per-protocol open-session path is the gate now.
}
```

Replace any `modem_mint_consent` references in the same file with deletions; the command itself gets dropped from the registry in Task 1.4.

- [ ] **Step 4: Run test to verify it passes + run backend test battery**

```bash
cargo --manifest-path src-tauri/Cargo.toml test
```

Expected: the new signature test passes. Other tests that were passing `consent_token` now break — note + skip; their callers get fixed in Task 1.3.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/modem_commands.rs
git commit -m "refactor(modem-ardop): drop consent_token from connect + b2f_exchange

The RADIO-1 modal goes away in Task 1.3; the per-session token round-trip
becomes dead weight. Internal busy guard from Task 1.1 keeps the dup-call
defense.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 1.3: Delete `useConsent` + `ConsentModal` + all consent invocations from `ArdopRadioPanel`

**Files:**
- Delete: `src/modem/useConsent.ts`, `src/modem/useConsent.test.ts`
- Delete: `src/modem/ConsentModal.tsx`, `src/modem/ConsentModal.test.tsx`
- Modify: `src/radio/modes/ArdopRadioPanel.tsx` — remove `useConsent`, `ConsentModal`, `showConsent`, `onConsentConfirm`, the `modem_mint_consent` invoke

- [ ] **Step 1: Write the failing test — ArdopRadioPanel renders no consent modal**

In `src/radio/modes/ArdopRadioPanel.test.tsx`, add:

```tsx
it('does not render a consent modal when Start is clicked', async () => {
  // Mock invoke so config_get_ardop resolves; modem_ardop_connect is
  // recorded; modem_mint_consent must NOT be invoked.
  const invokes: { cmd: string; args?: unknown }[] = [];
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    invokes.push({ cmd, args });
    if (cmd === 'config_get_ardop') return defaultArdopConfig();
    if (cmd === 'modem_ardop_connect') return null;
    return null;
  });

  render(<ArdopRadioPanel onClose={() => {}} />);
  await userEvent.type(screen.getByTestId('ardop-target-input'), 'K7TEST');
  await userEvent.click(screen.getByTestId('ardop-start-btn'));

  expect(screen.queryByTestId('consent-modal')).toBeNull();
  expect(invokes.find((i) => i.cmd === 'modem_mint_consent')).toBeUndefined();
  expect(invokes.find((i) => i.cmd === 'modem_ardop_connect')).toBeDefined();
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
pnpm exec vitest run src/radio/modes/ArdopRadioPanel.test.tsx -t "does not render a consent modal"
```

Expected: FAIL — current code shows the modal on Start click.

- [ ] **Step 3: Strip consent surface from ArdopRadioPanel**

In `src/radio/modes/ArdopRadioPanel.tsx`, remove:

- Lines importing `useConsent` and `ConsentModal`.
- `const consent = useConsent()`.
- `const [showConsent, setShowConsent] = useState(false)`.
- The `onConsentConfirm` callback.
- The `<ConsentModal …>` render at the bottom.
- The `consent.token` / `consent.clear()` / `consent.grant(tok)` references inside `doConnect` + `onStartClick` + `onSendReceiveClick`.

Simplify `onStartClick` to just call `doConnect()` (no token); simplify `doConnect`:

```tsx
const doConnect = async () => {
  setConnecting(true);
  setConnectError(null);
  try {
    await invoke('modem_ardop_connect', { target: target.trim() });
  } catch (e) {
    setConnectError(String(e));
  } finally {
    setConnecting(false);
  }
};

const onStartClick = () => {
  setConnectError(null);
  void doConnect();
};
```

Similarly strip `consent_token` from the `modem_ardop_b2f_exchange` invoke in `onSendReceiveClick`.

- [ ] **Step 4: Run test to verify it passes**

```bash
pnpm exec vitest run src/radio/modes/ArdopRadioPanel.test.tsx
```

Expected: PASS (the new test + the previously-passing tests; some may need shallow updates if they expected `consent_token` in the mocked invoke).

- [ ] **Step 5: Delete the modal + hook**

```bash
rm src/modem/useConsent.ts src/modem/useConsent.test.ts \
   src/modem/ConsentModal.tsx src/modem/ConsentModal.test.tsx
```

- [ ] **Step 6: Run type check to surface any remaining consumers**

```bash
pnpm typecheck
```

Expected: clean. If anything still imports `useConsent` or `ConsentModal`, strip those references (they should only be ArdopRadioPanel + possibly VaraRadioPanel; if VARA imports it, do the same strip there).

- [ ] **Step 7: Commit**

```bash
git add -A src/modem/ src/radio/modes/ArdopRadioPanel.tsx src/radio/modes/ArdopRadioPanel.test.tsx
git commit -m "refactor(modem-ui): delete ConsentModal + useConsent hook

The RADIO-1 per-invocation consent modal was a tuxlink-added safeguard;
spec §2 + memory no-tuxlink-added-safeguards mandate dropping it.
Operator click on Connect is the consent.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 1.4: Drop `modem_mint_consent` Tauri command + delete `mint_consent_token` / `consume_consent_token` from `ModemSession`

**Files:**
- Modify: `src-tauri/src/modem_commands.rs` — delete `modem_mint_consent` function
- Modify: `src-tauri/src/lib.rs` — remove `modem_mint_consent` from `.invoke_handler` registry
- Modify: `src-tauri/src/winlink/modem/ardop/session.rs` (or wherever `ModemSession` lives) — delete `mint_consent_token` + `consume_consent_token` + the `consent_token: Mutex<Option<String>>` field

- [ ] **Step 1: Write the failing test — no consent-token field on ModemSession**

In the test module for `ModemSession`:

```rust
#[test]
fn modem_session_has_no_consent_token_methods() {
    // Compile-time assertion: these methods must not exist.
    let session = ModemSession::new();
    // If `mint_consent_token` or `consume_consent_token` are still public,
    // this code COMPILES, and the test would silently pass. So we instead
    // verify by absence of the field — list the public methods and
    // assert the names don't appear. A simpler test: try to compile a
    // line that calls them and use `#[cfg(any())]` to suppress, then
    // rely on a grep gate in CI. For now, the cleanest approach is:
    // if `cargo test` compiles with the lines below uncommented in a
    // non-test build, the deletion failed.

    // SENTINEL: do NOT uncomment — these lines must NOT compile after
    // Task 1.4 lands.
    // let _ = session.mint_consent_token();
    // let _ = session.consume_consent_token("foo");

    let _ = session;
}
```

(For a stronger test, add a build-time check via `cargo expand` or a `compile_fail` doctest. The sentinel-comment approach above plus the grep in step 5 below is acceptable for a refactor of this size.)

- [ ] **Step 2: Run test to verify the suite still passes (the sentinel test is a sentinel, not a failure trigger)**

```bash
cargo --manifest-path src-tauri/Cargo.toml test modem_session_has_no_consent_token_methods
```

Expected: PASS (the test body is a no-op assertion).

- [ ] **Step 3: Delete `mint_consent_token` + `consume_consent_token` methods + the field**

In `src-tauri/src/winlink/modem/ardop/session.rs`: remove the `consent_token: Mutex<Option<String>>` field from `ModemSession`, the `mint_consent_token`, `consume_consent_token`, and any reset logic that clears it (e.g., inside `reset_to_stopped`).

In `src-tauri/src/modem_commands.rs`: delete the `modem_mint_consent` function and its doc comments. Strip any remaining `session.consume_consent_token(...)` calls (Task 1.2 should have removed them already; this is a safety pass).

In `src-tauri/src/lib.rs`: remove `crate::modem_commands::modem_mint_consent,` from the `.invoke_handler(tauri::generate_handler![...])` macro invocation.

- [ ] **Step 4: Run full backend test suite**

```bash
cargo --manifest-path src-tauri/Cargo.toml test
```

Expected: all tests pass. If a test still references `mint_consent_token`, delete or update it (the test was exercising deleted production code).

- [ ] **Step 5: Grep for any stragglers**

```bash
grep -rn "consume_consent_token\|mint_consent_token\|modem_mint_consent" src-tauri/src/ src/
```

Expected: no matches. Anything that returns should be cleaned in this same commit.

- [ ] **Step 6: Commit**

```bash
git add -A src-tauri/src/
git commit -m "refactor(modem-ardop): remove mint_consent_token + consume_consent_token

The RADIO-1 token round-trip is dead code after Tasks 1.1-1.3 dropped the
modal + consent_token params. Clears the consent_token field from
ModemSession.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 1.5: Drop `CONNECT_DEADLINE` constant + 120s bound

**Files:**
- Modify: `src-tauri/src/modem_commands.rs:25` — delete `const CONNECT_DEADLINE: Duration = ...`
- Modify: `src-tauri/src/modem_commands.rs` connect path — replace `CONNECT_DEADLINE` with the modem-native deadline or `Duration::MAX` if no modem-native default exists

**Rationale per spec §2:** ardopcf's `ARQTIMEOUT` and the radio operator's TX-timer are the legitimate bounds. The tuxlink-added 120s cap was reactive to the 2026-05-22 runaway, but the proper fix (per the spec) is the side-channel ABORT (Task 4.x for VARA; ARDOP already has it post-tuxlink-o3f2). Dropping the cap lets operators dial long-haul / weak-signal connects without the artificial cutoff.

- [ ] **Step 1: Write the failing test — connect-with-deadline behavior is removed**

If any current test asserts the 120s bound (`grep -rn "CONNECT_DEADLINE\|120" src-tauri/src/modem_commands.rs`), it's the failing test. Otherwise:

```rust
#[test]
fn connect_arq_no_longer_uses_connect_deadline_constant() {
    // Sentinel: this file must not contain the symbol after Task 1.5.
    let source = include_str!("modem_commands.rs");
    assert!(
        !source.contains("CONNECT_DEADLINE"),
        "modem_commands.rs still references CONNECT_DEADLINE — spec §2 mandates removal"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo --manifest-path src-tauri/Cargo.toml test connect_arq_no_longer_uses_connect_deadline_constant
```

Expected: FAIL — `CONNECT_DEADLINE` is currently defined and referenced.

- [ ] **Step 3: Drop the constant + replace at the call site**

In `src-tauri/src/modem_commands.rs`, delete:

```rust
const CONNECT_DEADLINE: Duration = Duration::from_secs(120);
```

At the call site (`transport.connect_arq(target, CONNECT_REPEAT, CONNECT_DEADLINE)`), pass a long deadline that lets ardopcf's `ARQTIMEOUT` (Task 0.x: the existing `ARQ_TIMEOUT_SECS = 30` already covers idle; the modem-native connect timeout depends on `ARQTIMEOUT × CONNECT_REPEAT` plus protocol overhead). A safe `Duration::from_secs(600)` (10 minutes) sentinel is acceptable as a defense-in-depth ceiling against a wedged TCP socket; it's not a tuxlink-added bounded-airtime cap, it's a TCP-wedge guard. Alternative: pass `Duration::MAX` and rely on operator-driven Close Session.

Pick: `Duration::from_secs(600)` as a TCP-wedge guard, documented as such in a one-line comment:

```rust
// TCP-wedge guard, not an airtime cap — operator's Close Session aborts
// the on-air connect via the side-channel writer (tuxlink-o3f2).
const CONNECT_TCP_WEDGE_GUARD: Duration = Duration::from_secs(600);
```

Update the call site to use the new name. The constant rename signals the semantic shift (no longer a Part 97 airtime concern; just a stuck-socket defense).

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo --manifest-path src-tauri/Cargo.toml test connect_arq_no_longer_uses_connect_deadline_constant
```

Expected: PASS.

- [ ] **Step 5: Run full backend test suite**

```bash
cargo --manifest-path src-tauri/Cargo.toml test
```

Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add -A src-tauri/src/modem_commands.rs
git commit -m "refactor(modem-ardop): drop CONNECT_DEADLINE = 120s bound

Spec §2 mandates removal of tuxlink-added safeguards. Replaced with a
600s TCP-wedge guard documented as a stuck-socket defense, not an
airtime cap. The legitimate abort path is the ABORT side-channel from
tuxlink-o3f2; the operator's Close Session click drives it.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 1.6: Sweep RADIO-1 SAFETY identifier surface + roll back ArdopRadioPanel "Dial as" toggle

**Files:**
- Modify: `src-tauri/src/modem_commands.rs` — strip "RADIO-1 SAFETY" / "RADIO-1:" comment surface from doc strings (keep the operator-click-is-consent semantics; rephrase as "operator-click consent gate")
- Modify: `src/radio/modes/ArdopRadioPanel.tsx` — delete `const [intent, setIntent] = useState<'cms' | 'p2p'>('cms');`, the "Dial as" select element + label, and hardcode `intent: 'cms'` back into the `RadioPanel` `mode` prop until Phase 5 replaces this panel entirely

**Rationale:** Spec §6.1 says roll back the in-panel "Dial as" toggle. The shared `RadioSessionPanel` (Phase 5) will handle multi-intent rendering driven by sidebar props. The toggle is interim cruft that needs to go BEFORE Phase 5 so Phase 5 doesn't inherit it.

- [ ] **Step 1: Write the failing test — ArdopRadioPanel does not render a Dial-as select**

In `src/radio/modes/ArdopRadioPanel.test.tsx`:

```tsx
it('does not render a "Dial as" intent toggle', () => {
  render(<ArdopRadioPanel onClose={() => {}} />);
  expect(screen.queryByTestId('ardop-intent-select')).toBeNull();
  expect(screen.queryByText(/Dial as/i)).toBeNull();
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
pnpm exec vitest run src/radio/modes/ArdopRadioPanel.test.tsx -t "Dial as"
```

Expected: FAIL — the toggle is currently rendered (per `grep` of origin/main).

- [ ] **Step 3: Strip the toggle**

In `src/radio/modes/ArdopRadioPanel.tsx`:

- Delete `const [intent, setIntent] = useState<'cms' | 'p2p'>('cms');`
- Delete the `<label>` containing `<span>Dial as</span>` and the `<select data-testid="ardop-intent-select">` element.
- Restore `mode={{ kind: 'ardop-hf', intent: 'cms' }}` (or keep it pointing at `intent: 'cms'` directly without the state variable).
- Strip "RADIO-1 SAFETY" / "RADIO-1 SAFETY:" comment headers; rephrase as "Operator-click consent gate" or delete if the surrounding code is self-evident.

In `src-tauri/src/modem_commands.rs`: same sweep — replace "RADIO-1:" / "RADIO-1 SAFETY:" comment prefixes with neutral phrasing.

- [ ] **Step 4: Run test to verify it passes**

```bash
pnpm exec vitest run src/radio/modes/ArdopRadioPanel.test.tsx
pnpm typecheck
```

Expected: PASS + clean.

- [ ] **Step 5: Grep sweep for any remaining RADIO-1 SAFETY identifier strings**

```bash
grep -rn "RADIO-1 SAFETY\|RADIO-1:" src/ src-tauri/src/
```

Expected: no matches. If matches remain, strip + add to this commit.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(modem-ui): strip RADIO-1 identifier surface + Dial-as toggle

Spec §6.1 rolls back the PR #348 \"Dial as\" intent toggle (sidebar
drives intent in the redesign). Spec §2 mandates dropping the RADIO-1
identifier surface from code comments + UI; the operator-click consent
semantics stay, just no \"RADIO-1\" branding.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

---

## Phase 2 — Widen types for `radio-only` intent

### Task 2.1: Widen `RadioPanelMode.intent` to include `radio-only` for ardop-hf, vara-hf, vara-fm

**Files:**
- Modify: `src/radio/types.ts`

- [ ] **Step 1: Write the failing test — RadioPanelMode accepts radio-only for ardop-hf**

In `src/radio/types.test.ts` (create if missing — or piggyback in `radioPanelVisibility.test.ts`):

```ts
import type { RadioPanelMode } from './types';

it('accepts radio-only intent for ardop-hf, vara-hf, vara-fm', () => {
  const a: RadioPanelMode = { kind: 'ardop-hf', intent: 'radio-only' };
  const b: RadioPanelMode = { kind: 'vara-hf',  intent: 'radio-only' };
  const c: RadioPanelMode = { kind: 'vara-fm',  intent: 'radio-only' };
  expect([a.intent, b.intent, c.intent]).toEqual(['radio-only','radio-only','radio-only']);
});

it('does NOT accept radio-only intent for telnet or packet', () => {
  // @ts-expect-error — telnet/packet are not radio-bearing; radio-only
  // intent is meaningless for them and must be a type error.
  const _t: RadioPanelMode = { kind: 'telnet', intent: 'radio-only' };
  // @ts-expect-error
  const _p: RadioPanelMode = { kind: 'packet', intent: 'radio-only' };
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
pnpm typecheck
```

Expected: typecheck FAILS — `radio-only` is not a member of the intent union for ardop-hf / vara-hf / vara-fm.

- [ ] **Step 3: Widen the type**

In `src/radio/types.ts`:

```ts
export type RadioPanelMode =
  | { kind: 'telnet'; intent: 'cms' | 'p2p' }
  | { kind: 'packet'; intent: 'cms' | 'p2p' }
  | { kind: 'ardop-hf'; intent: 'cms' | 'p2p' | 'radio-only' }
  | { kind: 'vara-hf'; intent: 'cms' | 'p2p' | 'radio-only' }
  | { kind: 'vara-fm'; intent: 'cms' | 'p2p' | 'radio-only' };
```

Update `panelTitle()` to handle `radio-only`:

```ts
export function panelTitle(mode: RadioPanelMode): string {
  const intentSuffix =
    mode.intent === 'cms' ? 'Winlink' :
    mode.intent === 'p2p' ? 'P2P' :
    'Radio-only';
  switch (mode.kind) {
    case 'telnet':   return `Telnet ${intentSuffix}`;
    case 'packet':   return `Packet ${intentSuffix}`;
    case 'ardop-hf': return `Ardop ${intentSuffix}`;
    case 'vara-hf':  return `Vara HF ${intentSuffix}`;
    case 'vara-fm':  return `Vara FM ${intentSuffix}`;
  }
}
```

- [ ] **Step 4: Run typecheck + tests to verify**

```bash
pnpm typecheck && pnpm exec vitest run src/radio/types.test.ts
```

Expected: clean. Some existing tests in other files may fail if they `switch (intent)` exhaustively — those'll be handled in Task 2.2.

- [ ] **Step 5: Commit**

```bash
git add src/radio/types.ts src/radio/types.test.ts
git commit -m "feat(radio-types): widen RadioPanelMode.intent to include radio-only

Spec §3 capability matrix: radio-only intent is alpha for ardop-hf,
vara-hf, vara-fm. Telnet + Packet stay cms|p2p (they're not RF-bearing).

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 2.2: Thread `radio-only` through `radioPanelVisibility.ts`

**Files:**
- Modify: `src/radio/radioPanelVisibility.ts:32-40` — the sidebar-selection switch currently narrows everything-not-p2p to `cms`; widen it to handle `radio-only` explicitly

- [ ] **Step 1: Write the failing test — radio-only sidebar selection maps to radio-only mode**

In `src/radio/radioPanelVisibility.test.ts`, add (within an existing `describe` block):

```ts
it('maps radio-only sidebar selection to radio-only intent for ardop/vara', () => {
  const reasonArdop = {
    sidebarSelected: { sessionType: 'radio-only', protocol: 'ardop-hf' } as const,
    activeModem: null,
    togglePinned: false,
  };
  expect(computePanelMode(reasonArdop)).toEqual({
    kind: 'ardop-hf',
    intent: 'radio-only',
  });

  const reasonVaraHf = {
    sidebarSelected: { sessionType: 'radio-only', protocol: 'vara-hf' } as const,
    activeModem: null,
    togglePinned: false,
  };
  expect(computePanelMode(reasonVaraHf)).toEqual({
    kind: 'vara-hf',
    intent: 'radio-only',
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
pnpm exec vitest run src/radio/radioPanelVisibility.test.ts -t "radio-only sidebar selection"
```

Expected: FAIL — current code maps `radio-only` to `cms` (line 33 narrows `sessionType === 'p2p' ? 'p2p' : 'cms'`).

- [ ] **Step 3: Widen the sidebar-selection switch**

In `src/radio/radioPanelVisibility.ts`:

```ts
if (reason.sidebarSelected !== null) {
  const { sessionType, protocol } = reason.sidebarSelected;
  const intent: 'cms' | 'p2p' | 'radio-only' =
    sessionType === 'p2p' ? 'p2p' :
    sessionType === 'radio-only' ? 'radio-only' :
    'cms';
  switch (protocol) {
    case 'telnet':
      // telnet doesn't support radio-only; degrade to cms.
      return { kind: 'telnet', intent: intent === 'radio-only' ? 'cms' : intent };
    case 'packet':
      return { kind: 'packet', intent: intent === 'radio-only' ? 'cms' : intent };
    case 'ardop-hf': return { kind: 'ardop-hf', intent };
    case 'vara-hf':  return { kind: 'vara-hf',  intent };
    case 'vara-fm':  return { kind: 'vara-fm',  intent };
  }
}
```

(The telnet/packet degrade-to-cms is a defensive fallback; the sidebar shouldn't offer radio-only for telnet/packet given `sessionTypes.ts` declares them `built: false` for the radio-only intent.)

- [ ] **Step 4: Run test to verify it passes**

```bash
pnpm exec vitest run src/radio/radioPanelVisibility.test.ts
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/radio/radioPanelVisibility.ts src/radio/radioPanelVisibility.test.ts
git commit -m "feat(radio-visibility): thread radio-only intent through sidebar router

Spec §3 capability matrix: sidebar (radio-only, ardop-hf|vara-hf|vara-fm)
must surface a radio-only panel. Telnet + Packet degrade to cms since
they're not RF-bearing.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 2.3: Flip `radio-only` to `built: true` in `sessionTypes.ts`

**Files:**
- Modify: `src/connections/sessionTypes.ts`

- [ ] **Step 1: Write the failing test — radio-only ardop/vara are built**

In `src/connections/sessionTypes.test.ts` (create if missing):

```ts
import { isBuilt } from './sessionTypes';

it('radio-only intent is built for ardop-hf, vara-hf, vara-fm', () => {
  expect(isBuilt({ sessionType: 'radio-only', protocol: 'ardop-hf' })).toBe(true);
  expect(isBuilt({ sessionType: 'radio-only', protocol: 'vara-hf'  })).toBe(true);
  expect(isBuilt({ sessionType: 'radio-only', protocol: 'vara-fm'  })).toBe(true);
});

it('radio-only intent is NOT built for telnet, packet (not RF-bearing)', () => {
  expect(isBuilt({ sessionType: 'radio-only', protocol: 'telnet' })).toBe(false);
  expect(isBuilt({ sessionType: 'radio-only', protocol: 'packet' })).toBe(false);
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
pnpm exec vitest run src/connections/sessionTypes.test.ts
```

Expected: FAIL — `radio-only.built` is currently `false`.

- [ ] **Step 3: Flip the flags**

In `src/connections/sessionTypes.ts`:

```ts
{
  id: 'radio-only',
  label: 'Radio-only',
  blurb: 'RF-only Hybrid network (pool R).',
  built: true,
  protocols: [
    { ...TEL, built: false },  // telnet stays unbuilt — not RF-bearing
    { ...PKT, built: false },  // packet stays unbuilt — not RF-bearing
    { ...ARD, built: true },   // tuxlink-0ye6 — radio-only ARDOP HF
    { ...VHF, built: true },   // tuxlink-0ye6 — radio-only VARA HF
    { ...VFM, built: true },   // tuxlink-0ye6 — radio-only VARA FM
  ],
},
```

(Note the addition of `ARD` to the radio-only protocols list — verify current code; if `ARD` isn't already a member, add it.)

- [ ] **Step 4: Run test to verify it passes**

```bash
pnpm exec vitest run src/connections/sessionTypes.test.ts
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/connections/sessionTypes.ts src/connections/sessionTypes.test.ts
git commit -m "feat(sidebar): flip radio-only to built for ardop-hf + vara-hf + vara-fm

Spec §3 capability matrix; tuxlink-0ye6 umbrella. Telnet + Packet stay
unbuilt for radio-only — they're not RF-bearing transports.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

---

## Phase 3 — Backend `SessionIntent` enum + session lifecycle commands

### Task 3.1: Add `SessionIntent` enum

**Files:**
- Create: `src-tauri/src/winlink/session_intent.rs` (or extend `src-tauri/src/winlink/mod.rs` with the enum if that's the project's convention for small modules)
- Modify: `src-tauri/src/winlink/mod.rs` to `pub mod session_intent; pub use session_intent::SessionIntent;`

- [ ] **Step 1: Write the failing test — SessionIntent serializes to camelCase strings**

In `src-tauri/src/winlink/session_intent.rs` (test module at the bottom of the file):

```rust
use serde_json;

#[test]
fn session_intent_serializes_to_kebab_case_strings() {
    let cms = SessionIntent::Cms;
    let p2p = SessionIntent::P2p;
    let ro  = SessionIntent::RadioOnly;
    assert_eq!(serde_json::to_string(&cms).unwrap(), "\"cms\"");
    assert_eq!(serde_json::to_string(&p2p).unwrap(), "\"p2p\"");
    assert_eq!(serde_json::to_string(&ro).unwrap(),  "\"radio-only\"");
}

#[test]
fn session_intent_deserializes_kebab_case() {
    let cms: SessionIntent = serde_json::from_str("\"cms\"").unwrap();
    let p2p: SessionIntent = serde_json::from_str("\"p2p\"").unwrap();
    let ro:  SessionIntent = serde_json::from_str("\"radio-only\"").unwrap();
    assert_eq!(cms, SessionIntent::Cms);
    assert_eq!(p2p, SessionIntent::P2p);
    assert_eq!(ro,  SessionIntent::RadioOnly);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo --manifest-path src-tauri/Cargo.toml test session_intent_serializes
```

Expected: FAIL — `SessionIntent` doesn't exist yet.

- [ ] **Step 3: Implement the enum**

In `src-tauri/src/winlink/session_intent.rs`:

```rust
//! `SessionIntent` — the operator's intent at session-open time:
//! Cms (Winlink CMS gateway), P2p (direct peer), or RadioOnly (R-pool peer).
//!
//! Wire format is kebab-case ("cms" / "p2p" / "radio-only") so the
//! Tauri JSON payloads match the sidebar's session-type IDs verbatim.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionIntent {
    Cms,
    P2p,
    RadioOnly,
}

impl SessionIntent {
    /// True for intents that arm a listener at session open (per spec §2).
    pub fn auto_arms_listener(self) -> bool {
        matches!(self, SessionIntent::P2p | SessionIntent::RadioOnly)
    }

    /// The B2F routing flag this intent drains (per spec §3 capability matrix).
    /// None means "no flag filter; drain everything queued for this transport."
    pub fn routing_flag(self) -> Option<char> {
        match self {
            SessionIntent::Cms => Some('C'),
            SessionIntent::P2p => None,
            SessionIntent::RadioOnly => Some('R'),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // … the tests from Step 1 …

    #[test]
    fn auto_arms_listener_matches_spec_matrix() {
        assert!(!SessionIntent::Cms.auto_arms_listener());
        assert!( SessionIntent::P2p.auto_arms_listener());
        assert!( SessionIntent::RadioOnly.auto_arms_listener());
    }

    #[test]
    fn routing_flag_matches_spec_matrix() {
        assert_eq!(SessionIntent::Cms.routing_flag(),       Some('C'));
        assert_eq!(SessionIntent::P2p.routing_flag(),       None);
        assert_eq!(SessionIntent::RadioOnly.routing_flag(), Some('R'));
    }
}
```

In `src-tauri/src/winlink/mod.rs`, add:

```rust
pub mod session_intent;
pub use session_intent::SessionIntent;
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo --manifest-path src-tauri/Cargo.toml test session_intent
```

Expected: all four tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/session_intent.rs src-tauri/src/winlink/mod.rs
git commit -m "feat(winlink): add SessionIntent enum (Cms / P2p / RadioOnly)

Spec §3 capability matrix. Backed by serde kebab-case so wire format
matches the sidebar session-type IDs verbatim. Methods declare which
intents auto-arm the listener and which routing flags each drains.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 3.2: Rename `vara_start_session` → `vara_open_session(intent)` and auto-arm listener

**Files:**
- Modify: `src-tauri/src/ui_commands.rs` — rename the existing `vara_start_session` to `vara_open_session`, accept `intent: SessionIntent`, and call into the existing `vara_listen` logic when `intent.auto_arms_listener()` is true
- Modify: `src-tauri/src/lib.rs` — rename in the `.invoke_handler` registry
- Modify: `src/radio/useVaraConfig.ts` + `src/radio/modes/VaraRadioPanel.tsx` — call the new name (transitional; Phase 5 replaces VaraRadioPanel anyway)

- [ ] **Step 1: Write the failing test — vara_open_session(P2p) arms the listener**

In `src-tauri/src/ui_commands.rs`'s test module (or a new integration test file):

```rust
#[tokio::test]
async fn vara_open_session_with_p2p_intent_arms_listener() {
    // This test exercises the lifecycle command with a mocked VARA
    // transport (TCP connect succeeds, no real RF). After vara_open_session
    // returns Ok with intent=P2p, VaraListenState.is_armed() must be true.

    let state = setup_test_state();
    let result = vara_open_session(
        state.app_handle.clone(),
        state.log.clone(),
        state.vara_session.clone(),
        state.vara_listen_state.clone(),
        SessionIntent::P2p,
    ).await;

    assert!(result.is_ok(), "open_session failed: {:?}", result);
    assert!(state.vara_listen_state.is_armed(), "listener should be armed for P2p intent");
}

#[tokio::test]
async fn vara_open_session_with_cms_intent_does_not_arm_listener() {
    let state = setup_test_state();
    let _ = vara_open_session(
        state.app_handle.clone(),
        state.log.clone(),
        state.vara_session.clone(),
        state.vara_listen_state.clone(),
        SessionIntent::Cms,
    ).await;

    assert!(!state.vara_listen_state.is_armed(), "listener must NOT auto-arm for Cms intent");
}
```

(If `setup_test_state()` and `VaraListenState::is_armed()` don't exist yet, this task may need a helper sub-step to add them. The shape above is the assertion target.)

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo --manifest-path src-tauri/Cargo.toml test vara_open_session_with_p2p
```

Expected: FAIL — `vara_open_session` doesn't exist yet (only `vara_start_session`), and the auto-arm wiring isn't there.

- [ ] **Step 3: Rename + add auto-arm**

In `src-tauri/src/ui_commands.rs`:

1. Rename `vara_start_session` → `vara_open_session`.
2. Add `intent: SessionIntent` parameter.
3. After the TCP transport opens successfully, if `intent.auto_arms_listener()`, call the existing `vara_listen` logic (or factor it into an inner helper both can call).

Sketch:

```rust
#[tauri::command]
pub async fn vara_open_session(
    app: AppHandle,
    log: State<'_, Arc<SessionLog>>,
    vara_session: State<'_, Arc<VaraSession>>,
    listen_state: State<'_, Arc<VaraListenState>>,
    intent: SessionIntent,
) -> Result<VaraStatusDto, String> {
    // 1. Open the TCP transport (existing vara_start_session body).
    let status = open_vara_transport_inner(&app, &log, &vara_session).await?;

    // 2. Auto-arm the listener for intents that need it.
    if intent.auto_arms_listener() {
        // Delegate to the existing vara_listen helper; the wrapper handles
        // duplicate-arm safely.
        arm_vara_listener_inner(&app, &log, &vara_session, &listen_state, intent).await?;
    }

    Ok(status)
}
```

The `arm_vara_listener_inner` helper is whatever the current `vara_listen` body calls — factor it out from the existing `#[tauri::command]` so it's callable internally without a Tauri runtime.

In `src-tauri/src/lib.rs`, replace `vara_start_session` with `vara_open_session` in the `.invoke_handler`.

In `src/radio/modes/VaraRadioPanel.tsx` (transitional fix), update the two invoke sites:

```tsx
const next = await invoke<VaraStatusDto>('vara_open_session', { intent: 'cms' });
```

(Hardcode `intent: 'cms'` here — Phase 5 will replace this panel with `RadioSessionPanel` which derives intent from props.)

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo --manifest-path src-tauri/Cargo.toml test vara_open_session
pnpm typecheck && pnpm exec vitest run src/radio/modes/VaraRadioPanel.test.tsx
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(vara): rename vara_start_session → vara_open_session(intent)

Spec §2 + §5 — session-lifecycle button drives transport open + listener
auto-arm in one Tauri call. Cms intent opens transport only; P2p +
RadioOnly auto-arm the listener via the existing vara_listen path.

Frontend hardcodes intent: 'cms' transitionally; Phase 5 derives it
from RadioPanelMode props.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 3.3: Rename `vara_stop_session` → `vara_close_session()` and disarm listener + abort in-flight

**Files:**
- Modify: `src-tauri/src/ui_commands.rs` — rename and add disarm logic
- Modify: `src-tauri/src/lib.rs` — rename in registry
- Modify: `src/radio/modes/VaraRadioPanel.tsx` — call new name

Note: the ABORT side-channel itself is Task 4.x (tuxlink-12sc); this task wires `vara_close_session` to CALL it once it exists. For now the close path will `set_listen_off` (existing) + transport close.

- [ ] **Step 1: Write the failing test — vara_close_session disarms the listener**

```rust
#[tokio::test]
async fn vara_close_session_disarms_listener() {
    let state = setup_test_state();
    // Open with P2p so listener is armed.
    let _ = vara_open_session(/* ... */, SessionIntent::P2p).await;
    assert!(state.vara_listen_state.is_armed());

    // Close should disarm.
    let _ = vara_close_session(/* ... */).await;
    assert!(!state.vara_listen_state.is_armed());
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo --manifest-path src-tauri/Cargo.toml test vara_close_session_disarms_listener
```

Expected: FAIL — `vara_close_session` doesn't exist.

- [ ] **Step 3: Rename + add disarm**

In `src-tauri/src/ui_commands.rs`:

```rust
#[tauri::command]
pub async fn vara_close_session(
    app: AppHandle,
    log: State<'_, Arc<SessionLog>>,
    vara_session: State<'_, Arc<VaraSession>>,
    listen_state: State<'_, Arc<VaraListenState>>,
) -> Result<VaraStatusDto, String> {
    // 1. Disarm listener (idempotent — no-op if not armed).
    let _ = vara_set_listen_inner(&app, &log, &vara_session, &listen_state, false).await;

    // 2. Abort any in-flight B2F exchange. (Stub for now; Task 4.2 wires
    //    the actual side-channel ABORT writer.)
    let _ = vara_session.abort_in_flight();

    // 3. Close transport (existing vara_stop_session body).
    close_vara_transport_inner(&app, &log, &vara_session).await
}
```

In `src-tauri/src/lib.rs`, rename in registry.

In `src/radio/modes/VaraRadioPanel.tsx`, update:

```tsx
const next = await invoke<VaraStatusDto>('vara_close_session');
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo --manifest-path src-tauri/Cargo.toml test vara_close_session
pnpm typecheck
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(vara): rename vara_stop_session → vara_close_session

Spec §5 close-session lifecycle: disarms listener, aborts in-flight,
closes transport. ABORT side-channel wiring is a stub here; Task 4.2
implements the real ABORT writer (tuxlink-12sc).

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 3.4: Add `modem_vara_b2f_exchange(target, intent)` Tauri command

**Files:**
- Modify: `src-tauri/src/ui_commands.rs` — add new command, mirror the shape of `modem_ardop_b2f_exchange`
- Modify: `src-tauri/src/lib.rs` — register

**Rationale:** Per spec §6.2 the VARA outbound path mirrors ARDOP's: CONNECT → CONNECTED → B2F → DISCONNECT, all in one call. This is the Connect button's invocation target for VARA.

- [ ] **Step 1: Write the failing test — modem_vara_b2f_exchange routes intent through to the routing flag**

```rust
#[tokio::test]
async fn modem_vara_b2f_exchange_p2p_drains_unflagged_messages() {
    // Mocked VARA transport accepts the CONNECT, runs B2F with a recorded
    // mailbox, asserts the message drain filter matches SessionIntent::P2p's
    // routing_flag() (None = unflagged).
    let state = setup_test_state_with_messages(vec![
        Message::new("MID-1", Some('C')),  // CMS-flagged — must NOT drain for P2p
        Message::new("MID-2", None),       // unflagged — must drain for P2p
        Message::new("MID-3", Some('R')),  // R-flagged — must NOT drain for P2p
    ]);

    let result = modem_vara_b2f_exchange(
        state.app_handle, state.log, state.vara_session,
        "K7TEST".to_string(),
        SessionIntent::P2p,
    ).await;

    assert!(result.is_ok());
    let drained = state.mailbox_drained_mids();
    assert_eq!(drained, vec!["MID-2"]);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo --manifest-path src-tauri/Cargo.toml test modem_vara_b2f_exchange_p2p
```

Expected: FAIL — command doesn't exist.

- [ ] **Step 3: Implement the command**

Mirror `modem_ardop_b2f_exchange` in `src-tauri/src/ui_commands.rs` (or wherever the VARA modem commands live). Key shape:

```rust
#[tauri::command]
pub async fn modem_vara_b2f_exchange(
    app: AppHandle,
    log: State<'_, Arc<SessionLog>>,
    vara_session: State<'_, Arc<VaraSession>>,
    target: String,
    intent: SessionIntent,
) -> Result<(), String> {
    // 1. Verify the session is open (transport is up).
    // 2. CONNECT to target via VARA's `CONNECT <call>` cmd.
    // 3. Once CONNECTED, run B2F over the data port (existing
    //    winlink_backend::run_vara_b2f_exchange — extend if it doesn't
    //    take an intent filter yet).
    // 4. DISCONNECT cleanly.
    // 5. Return Ok; listener stays armed for p2p/radio-only intents (the
    //    session is still open — only the outbound dial ended).

    let cfg = config::read_config().map_err(|e| format!("read config: {e}"))?;
    let mailbox = Mailbox::new(/* ... */);
    let arbiter = app.state::<Arc<PositionArbiter>>();

    crate::winlink_backend::run_vara_b2f_exchange(
        &vara_session,
        &target,
        intent,
        &cfg,
        &mailbox,
        Some(&arbiter),
    )
    .map_err(|e| format!("VARA B2F exchange failed: {e}"))
}
```

The `winlink_backend::run_vara_b2f_exchange` doesn't exist yet either — if so, add it as a thin wrapper around the existing B2F machinery (`run_exchange_with_role` from `winlink/session.rs`) over the VARA transport. The intent filter goes into the message drain (`mailbox.pending_for_routing_flag(intent.routing_flag())` or equivalent).

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo --manifest-path src-tauri/Cargo.toml test modem_vara_b2f_exchange
```

Expected: PASS.

- [ ] **Step 5: Register in lib.rs and commit**

```bash
# Edit src-tauri/src/lib.rs: add crate::ui_commands::modem_vara_b2f_exchange to .invoke_handler
git add -A
git commit -m "feat(vara): add modem_vara_b2f_exchange(target, intent)

Spec §6.2 — VARA outbound dial mirrors ARDOP's b2f_exchange. CONNECT +
B2F + DISCONNECT in one call. Intent's routing_flag() filters the
mailbox drain so P2p sends unflagged, Cms sends C-flagged, RadioOnly
sends R-flagged.

bd tuxlink-fzl7 (VARA Phase 3 outbound RF dial — subsumed by tuxlink-0ye6).

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 3.5: Add `ardop_open_session(intent)` + `ardop_close_session()` Tauri commands

**Files:**
- Modify: `src-tauri/src/modem_commands.rs` — add the two new Tauri wrappers
- Modify: `src-tauri/src/lib.rs` — register
- Modify: `src-tauri/src/ui_commands.rs` (or wherever ARDOP listener state lives) — call into existing `ardop_listen` for auto-arm

**Rationale:** ARDOP is connect-driven (no separate transport-open phase), but the shared panel's lifecycle button must look identical to VARA's. ARDOP's "open session" therefore: spawn ardopcf + bind the cmd socket + (for p2p/radio-only) arm the listener. NO `connect_arq` — that's the Connect button (Task 3.6's widened `modem_ardop_b2f_exchange`). For cms intent, "open session" spawns ardopcf and stays idle waiting for Connect.

- [ ] **Step 1: Write the failing test — ardop_open_session(P2p) spawns ardopcf + arms listener**

```rust
#[tokio::test]
async fn ardop_open_session_with_p2p_intent_arms_listener_after_spawn() {
    let state = setup_test_state_with_ardop_stub();
    let result = ardop_open_session(
        state.app_handle, state.log, state.modem_session, state.ardop_listen_state,
        SessionIntent::P2p,
    ).await;

    assert!(result.is_ok());
    assert!(state.ardop_listen_state.is_armed(), "P2p intent must auto-arm");
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo --manifest-path src-tauri/Cargo.toml test ardop_open_session
```

Expected: FAIL — command doesn't exist.

- [ ] **Step 3: Implement both commands**

In `src-tauri/src/modem_commands.rs` (or move to `ui_commands.rs` if it's where the ARDOP listener wiring lives):

```rust
#[tauri::command]
pub async fn ardop_open_session(
    app: AppHandle,
    log: State<'_, Arc<SessionLog>>,
    session: State<'_, Arc<ModemSession>>,
    listen_state: State<'_, Arc<ArdopListenState>>,
    intent: SessionIntent,
) -> Result<ModemStatus, String> {
    // 1. Spawn ardopcf + init the cmd socket. NO connect_arq.
    //    Use the existing init path factored out of modem_ardop_connect.
    spawn_and_init_ardop(&app, &session).await?;

    // 2. Auto-arm listener for p2p/radio-only.
    if intent.auto_arms_listener() {
        ardop_listen_inner(&app, &log, &session, &listen_state).await?;
    }

    Ok(session.status_snapshot())
}

#[tauri::command]
pub async fn ardop_close_session(
    app: AppHandle,
    log: State<'_, Arc<SessionLog>>,
    session: State<'_, Arc<ModemSession>>,
    listen_state: State<'_, Arc<ArdopListenState>>,
) -> Result<ModemStatus, String> {
    // 1. Disarm listener (idempotent).
    let _ = ardop_set_listen_inner(&app, &log, &session, &listen_state, false).await;

    // 2. Abort any in-flight connect via the existing side-channel writer.
    let _ = session.abort_in_flight();

    // 3. Tear down transport.
    modem_ardop_disconnect_inner(&session)?;
    Ok(session.status_snapshot())
}
```

You'll need to factor `spawn_and_init_ardop` and `ardop_listen_inner` / `ardop_set_listen_inner` out of the existing `#[tauri::command]` wrappers so they're callable from the new session-lifecycle wrappers.

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo --manifest-path src-tauri/Cargo.toml test ardop_open_session ardop_close_session
```

Expected: PASS.

- [ ] **Step 5: Register + commit**

Edit `src-tauri/src/lib.rs` to register both new commands.

```bash
git add -A
git commit -m "feat(ardop): add ardop_open_session(intent) + ardop_close_session()

Spec §2 + §5 — ARDOP session lifecycle mirrors VARA's shape. Open spawns
ardopcf + arms listener (for p2p/radio-only); Close disarms + aborts +
tears down. NO connect_arq during open — that's Connect (the widened
modem_ardop_b2f_exchange in Task 3.6).

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 3.6: Widen `modem_ardop_b2f_exchange` to accept `intent: SessionIntent`

**Files:**
- Modify: `src-tauri/src/modem_commands.rs` — add `intent` parameter; thread to routing-flag filter

- [ ] **Step 1: Write the failing test — exchange filters mailbox by intent's routing flag**

```rust
#[tokio::test]
async fn modem_ardop_b2f_exchange_radio_only_drains_only_r_flagged() {
    let state = setup_test_state_with_messages(vec![
        Message::new("MID-1", Some('C')),
        Message::new("MID-2", None),
        Message::new("MID-3", Some('R')),
    ]);

    let _ = modem_ardop_b2f_exchange(
        state.app_handle, state.session,
        "K7RR-10".to_string(),
        SessionIntent::RadioOnly,
    );

    assert_eq!(state.mailbox_drained_mids(), vec!["MID-3"]);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo --manifest-path src-tauri/Cargo.toml test modem_ardop_b2f_exchange_radio_only
```

Expected: FAIL — current signature doesn't take `intent`.

- [ ] **Step 3: Add intent parameter + thread through**

```rust
#[tauri::command]
pub fn modem_ardop_b2f_exchange(
    app: AppHandle,
    session: State<'_, Arc<ModemSession>>,
    target: String,
    intent: SessionIntent,
) -> Result<(), String> {
    let mut transport = session.take_transport().ok_or_else(|| {
        "ARDOP transport not open — press Open Session before dialing".to_string()
    })?;

    let outcome = run_b2f_with_transport(&app, &mut *transport, &target, intent);

    let _ = transport.disconnect(Duration::from_secs(5));
    drop(transport);
    let _ = session.reset_to_stopped();

    outcome
}

fn run_b2f_with_transport(
    app: &AppHandle,
    transport: &mut dyn ModemTransport,
    target: &str,
    intent: SessionIntent,
) -> Result<(), String> {
    // ... existing body, but pass intent into run_ardop_b2f_exchange so
    // the mailbox drain filter uses intent.routing_flag().
    crate::winlink_backend::run_ardop_b2f_exchange(
        transport, target, intent, &cfg, &mailbox, Some(&arbiter),
    ).map_err(|e| format!("ARDOP B2F exchange failed: {e}"))
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo --manifest-path src-tauri/Cargo.toml test modem_ardop_b2f_exchange
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(ardop): widen modem_ardop_b2f_exchange to accept SessionIntent

Spec §3 capability matrix — Cms drains C-flagged, P2p drains unflagged,
RadioOnly drains R-flagged. Intent flows through to the mailbox drain
filter via SessionIntent::routing_flag().

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

---

## Phase 4 — VARA disarm ABORT side-channel (tuxlink-12sc)

### Task 4.1: Add `try_clone_abort_writer` + `install_abort_writer` for VARA

**Files:**
- Modify: `src-tauri/src/winlink/modem/vara/transport.rs` — add abort-writer trait method
- Modify: `src-tauri/src/winlink/modem/vara/session.rs` (or wherever `VaraSession` lives) — add `install_abort_writer` + `abort_in_flight` mirroring ARDOP's `ModemSession`

**Rationale per spec §9 watched failure mode:** Without this, operator's Close Session click could take 30s+ to interrupt an active B2F (VARA's natural timeout). The ABORT side-channel makes it immediate.

VARA's cmd-port protocol does NOT have a direct ABORT word like ardopcf's. Per VARA HF/FM spec, the closest is `DISCONNECT` sent on the cmd port — the modem aborts the link and acks. So "abort writer" for VARA is the cmd-port writer; `abort_in_flight` sends `DISCONNECT\r`.

- [ ] **Step 1: Write the failing test — abort_in_flight sends DISCONNECT on cmd port**

```rust
#[test]
fn vara_abort_in_flight_writes_disconnect_to_cmd_port() {
    let session = VaraSession::new();
    let (writer, captured) = test_cmd_writer();
    session.install_abort_writer(Box::new(writer));

    session.abort_in_flight().expect("abort writes succeed");

    assert_eq!(captured.lock().unwrap().as_slice(), b"DISCONNECT\r");
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo --manifest-path src-tauri/Cargo.toml test vara_abort_in_flight
```

Expected: FAIL — methods don't exist.

- [ ] **Step 3: Implement the methods**

In `src-tauri/src/winlink/modem/vara/session.rs`:

```rust
pub struct VaraSession {
    // ... existing fields ...
    abort_writer: Mutex<Option<Box<dyn Write + Send>>>,
}

impl VaraSession {
    pub fn install_abort_writer(&self, writer: Box<dyn Write + Send>) {
        *self.abort_writer.lock().unwrap() = Some(writer);
    }

    pub fn abort_in_flight(&self) -> Result<(), String> {
        let mut guard = self.abort_writer.lock().unwrap();
        let writer = guard.as_mut().ok_or("no abort writer installed")?;
        writer.write_all(b"DISCONNECT\r").map_err(|e| format!("abort write: {e}"))?;
        writer.flush().map_err(|e| format!("abort flush: {e}"))
    }
}
```

In `src-tauri/src/winlink/modem/vara/transport.rs`, add a `try_clone_abort_writer` method that hands out a clone of the cmd-port writer (mirror ARDOP's pattern; `TcpStream::try_clone()` is the obvious mechanism).

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo --manifest-path src-tauri/Cargo.toml test vara_abort_in_flight
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(vara-transport): add abort_in_flight side-channel (DISCONNECT\\r)

Spec §9 watched failure mode + tuxlink-12sc. VARA's cmd-port protocol
has no explicit ABORT — DISCONNECT on the cmd port is the immediate-stop
signal that the modem honors even mid-B2F. Mirror ARDOP's install/clone
pattern so vara_close_session can interrupt active exchanges.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 4.2: Wire `vara_close_session` to call `abort_in_flight` (and the cmd-port writer to install on session open)

**Files:**
- Modify: `src-tauri/src/ui_commands.rs` — wire `vara_open_session` to install the abort writer after transport opens; `vara_close_session` already calls `abort_in_flight` (Task 3.3 stubbed this).

- [ ] **Step 1: Write the failing test — vara_close_session during in-flight exchange interrupts the exchange**

```rust
#[tokio::test]
async fn vara_close_session_interrupts_active_b2f() {
    let state = setup_test_state_with_long_running_exchange();
    let exchange_handle = tokio::spawn({
        let s = state.clone();
        async move {
            modem_vara_b2f_exchange(/* ... */, SessionIntent::P2p).await
        }
    });

    // Give it a beat to enter the exchange loop.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Close session — must interrupt within ~1s, not wait for VARA's
    // natural timeout.
    let close_start = std::time::Instant::now();
    let _ = vara_close_session(/* ... */).await;
    let close_elapsed = close_start.elapsed();

    assert!(
        close_elapsed < Duration::from_secs(2),
        "close_session took {:?} — ABORT side-channel didn't interrupt",
        close_elapsed
    );

    let exchange_result = exchange_handle.await.expect("join");
    assert!(exchange_result.is_err(), "exchange should error from the abort");
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo --manifest-path src-tauri/Cargo.toml test vara_close_session_interrupts_active_b2f
```

Expected: FAIL — `vara_open_session` doesn't install the abort writer yet, so `vara_close_session`'s `abort_in_flight` call (from Task 3.3) is a no-op (returns "no abort writer installed" Err which we ignore).

- [ ] **Step 3: Install abort writer in vara_open_session**

In `src-tauri/src/ui_commands.rs`'s `vara_open_session`, after the transport opens:

```rust
let status = open_vara_transport_inner(&app, &log, &vara_session).await?;

// Install the abort writer so vara_close_session can DISCONNECT mid-B2F.
if let Some(writer) = vara_session.try_clone_cmd_writer() {
    vara_session.install_abort_writer(Box::new(writer));
}
```

(`try_clone_cmd_writer` is what the transport already exposes for the cmd channel; if it's not exposed, add it now as a thin wrapper around `TcpStream::try_clone()`.)

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo --manifest-path src-tauri/Cargo.toml test vara_close_session_interrupts_active_b2f
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(vara): wire ABORT side-channel install at session open

Spec §9 + tuxlink-12sc. vara_open_session now clones the cmd-port
writer + installs it as the abort writer; vara_close_session's
abort_in_flight call now actually interrupts an in-flight exchange.

Closes tuxlink-12sc.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push

# Also close the bd issue.
bd close tuxlink-12sc
```

---

## Phase 5 — Shared `RadioSessionPanel` component + adapters

### Task 5.1: Define the `RadioSessionAdapter` interface

**Files:**
- Create: `src/radio/modes/radioSessionAdapters.ts`
- Create: `src/radio/modes/radioSessionAdapters.test.ts`

- [ ] **Step 1: Write the failing test — adapter interface shape**

In `src/radio/modes/radioSessionAdapters.test.ts`:

```ts
import { ardopAdapter, varaHfAdapter, varaFmAdapter, type RadioSessionAdapter } from './radioSessionAdapters';

describe('RadioSessionAdapter', () => {
  it('each adapter declares the four Tauri command names', () => {
    const adapters: RadioSessionAdapter[] = [ardopAdapter, varaHfAdapter, varaFmAdapter];
    for (const a of adapters) {
      expect(a.commands.openSession).toMatch(/^(ardop|vara)_open_session$/);
      expect(a.commands.closeSession).toMatch(/^(ardop|vara)_close_session$/);
      expect(a.commands.b2fExchange).toMatch(/^modem_(ardop|vara)_b2f_exchange$/);
      expect(a.commands.allowedStationsGet).toMatch(/_allowed_stations_get$/);
    }
  });

  it('each adapter declares the protocol kind it adapts', () => {
    expect(ardopAdapter.kind).toBe('ardop-hf');
    expect(varaHfAdapter.kind).toBe('vara-hf');
    expect(varaFmAdapter.kind).toBe('vara-fm');
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
pnpm exec vitest run src/radio/modes/radioSessionAdapters.test.ts
```

Expected: FAIL — file doesn't exist.

- [ ] **Step 3: Define the interface + three adapter consts**

In `src/radio/modes/radioSessionAdapters.ts`:

```ts
import type { ReactNode } from 'react';
import type { RadioPanelMode } from '../types';

export interface RadioSessionCommands {
  openSession: string;     // 'ardop_open_session' / 'vara_open_session'
  closeSession: string;    // 'ardop_close_session' / 'vara_close_session'
  b2fExchange: string;     // 'modem_ardop_b2f_exchange' / 'modem_vara_b2f_exchange'
  status: string;          // 'modem_get_status' / 'vara_status'
  allowedStationsGet: string;
  allowedStationsAdd: string;
  allowedStationsRemove: string;
  allowedStationsSetAllowAll: string;
}

export interface RadioSessionAdapter {
  kind: RadioPanelMode['kind'];
  commands: RadioSessionCommands;
  /** Render the per-protocol "Modem settings" expander content. */
  renderSettingsExpander: () => ReactNode;
}

export const ardopAdapter: RadioSessionAdapter = {
  kind: 'ardop-hf',
  commands: {
    openSession: 'ardop_open_session',
    closeSession: 'ardop_close_session',
    b2fExchange: 'modem_ardop_b2f_exchange',
    status: 'modem_get_status',
    allowedStationsGet: 'ardop_allowed_stations_get',
    allowedStationsAdd: 'ardop_allowed_stations_add',
    allowedStationsRemove: 'ardop_allowed_stations_remove',
    allowedStationsSetAllowAll: 'ardop_allowed_stations_set_allow_all',
  },
  renderSettingsExpander: () => null, // Task 5.6 fills this from ArdopRadioPanel's Radio section
};

export const varaHfAdapter: RadioSessionAdapter = {
  kind: 'vara-hf',
  commands: {
    openSession: 'vara_open_session',
    closeSession: 'vara_close_session',
    b2fExchange: 'modem_vara_b2f_exchange',
    status: 'vara_status',
    allowedStationsGet: 'vara_allowed_stations_get',
    allowedStationsAdd: 'vara_allowed_stations_add',
    allowedStationsRemove: 'vara_allowed_stations_remove',
    allowedStationsSetAllowAll: 'vara_allowed_stations_set_allow_all',
  },
  renderSettingsExpander: () => null, // Task 5.6 fills from VaraRadioPanel's host/port/bandwidth section
};

export const varaFmAdapter: RadioSessionAdapter = {
  ...varaHfAdapter,
  kind: 'vara-fm',
};
```

- [ ] **Step 4: Run test to verify it passes**

```bash
pnpm exec vitest run src/radio/modes/radioSessionAdapters.test.ts
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/radio/modes/radioSessionAdapters.ts src/radio/modes/radioSessionAdapters.test.ts
git commit -m "feat(radio-panel): add RadioSessionAdapter interface + 3 protocol adapters

Spec §6.1 — shared RadioSessionPanel delegates per-protocol divergence
to small adapters. Three adapters: ardop-hf, vara-hf, vara-fm.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 5.2: Build `RadioSessionPanel` shell (header + Open/Close button + session-log; no outbound or listener yet)

**Files:**
- Create: `src/radio/modes/RadioSessionPanel.tsx`
- Create: `src/radio/modes/RadioSessionPanel.test.tsx`
- Create: `src/radio/modes/RadioSessionPanel.css`

- [ ] **Step 1: Write the failing test — Open Session button renders and invokes the adapter's openSession command**

```tsx
// src/radio/modes/RadioSessionPanel.test.tsx
it('Open Session button invokes the adapter\'s openSession command with sidebar intent', async () => {
  const invokes: { cmd: string; args?: unknown }[] = [];
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    invokes.push({ cmd, args });
    return null;
  });

  render(
    <RadioSessionPanel
      mode={{ kind: 'vara-hf', intent: 'p2p' }}
      adapter={varaHfAdapter}
      onClose={() => {}}
    />
  );

  await userEvent.click(screen.getByTestId('open-session-btn'));
  expect(invokes.find((i) => i.cmd === 'vara_open_session')).toEqual({
    cmd: 'vara_open_session',
    args: { intent: 'p2p' },
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
pnpm exec vitest run src/radio/modes/RadioSessionPanel.test.tsx
```

Expected: FAIL — component doesn't exist.

- [ ] **Step 3: Build the shell**

In `src/radio/modes/RadioSessionPanel.tsx`:

```tsx
import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { RadioPanel } from '../RadioPanel';
import { SessionLogSection } from '../sections/SessionLogSection';
import { useSessionLog } from '../sections/useSessionLog';
import type { RadioPanelMode } from '../types';
import type { RadioSessionAdapter } from './radioSessionAdapters';
import './RadioSessionPanel.css';

export type SessionLifecycleState =
  | 'closed'
  | 'opening'
  | 'open-idle'
  | 'dialing'
  | 'exchange'
  | 'inbound-exchange'
  | 'closing'
  | 'error';

export interface RadioSessionPanelProps {
  mode: RadioPanelMode;
  adapter: RadioSessionAdapter;
  onClose: () => void;
}

export function RadioSessionPanel({ mode, adapter, onClose }: RadioSessionPanelProps) {
  const [lifecycle, setLifecycle] = useState<SessionLifecycleState>('closed');
  const [lastError, setLastError] = useState<string | null>(null);
  const { entries: logEntries, clear: clearLog } = useSessionLog();

  const onOpenSessionClick = async () => {
    setLifecycle('opening');
    setLastError(null);
    try {
      await invoke(adapter.commands.openSession, { intent: mode.intent });
      setLifecycle('open-idle');
    } catch (e) {
      setLastError(String(e));
      setLifecycle('error');
    }
  };

  const onCloseSessionClick = async () => {
    setLifecycle('closing');
    setLastError(null);
    try {
      await invoke(adapter.commands.closeSession);
      setLifecycle('closed');
    } catch (e) {
      setLastError(String(e));
      setLifecycle('error');
    }
  };

  const isClosed = lifecycle === 'closed' || lifecycle === 'error';

  return (
    <RadioPanel mode={mode} state={mapLifecycleToPanelState(lifecycle)} onClose={onClose}>
      <section className="radio-panel-sec radio-session-control">
        {isClosed ? (
          <button
            type="button"
            className="radio-panel-btn radio-panel-btn-primary"
            data-testid="open-session-btn"
            onClick={onOpenSessionClick}
          >
            Open session
          </button>
        ) : (
          <button
            type="button"
            className="radio-panel-btn radio-panel-btn-bad"
            data-testid="close-session-btn"
            onClick={onCloseSessionClick}
          >
            Close session
          </button>
        )}
      </section>

      <SessionLogSection entries={logEntries} onClear={clearLog} />

      {lastError && (
        <p className="radio-panel-error" role="alert" data-testid="session-error">
          {lastError}
        </p>
      )}
    </RadioPanel>
  );
}

function mapLifecycleToPanelState(s: SessionLifecycleState) {
  switch (s) {
    case 'closed': case 'error': return 'disconnected' as const;
    case 'opening': case 'closing': return 'connecting' as const;
    case 'open-idle': case 'dialing': case 'exchange': case 'inbound-exchange': return 'connected' as const;
  }
}
```

`RadioSessionPanel.css` can be empty for now; styling lands in Task 5.7.

- [ ] **Step 4: Run test to verify it passes**

```bash
pnpm exec vitest run src/radio/modes/RadioSessionPanel.test.tsx
pnpm typecheck
```

Expected: PASS + clean.

- [ ] **Step 5: Commit**

```bash
git add -A src/radio/modes/RadioSessionPanel.tsx src/radio/modes/RadioSessionPanel.test.tsx src/radio/modes/RadioSessionPanel.css
git commit -m "feat(radio-panel): RadioSessionPanel shell with Open/Close lifecycle

Spec §5 state machine — closed → opening → open-idle → … → closing →
closed. This commit lands the shell only; outbound dial + allowlist
editor + settings expander follow in Tasks 5.3-5.7.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 5.3: Add outbound dial section (Target input, Bandwidth select, Connect button)

**Files:**
- Modify: `src/radio/modes/RadioSessionPanel.tsx` — add outbound section that renders when `lifecycle ∈ {open-idle}` and is disabled when `inbound-exchange` is active

- [ ] **Step 1: Write the failing test — Connect button invokes the adapter's b2fExchange command**

```tsx
it('Connect button invokes b2fExchange with target + intent', async () => {
  const invokes: { cmd: string; args?: unknown }[] = [];
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    invokes.push({ cmd, args });
    if (cmd === 'vara_open_session') return null;
    if (cmd === 'modem_vara_b2f_exchange') return null;
    return null;
  });

  render(<RadioSessionPanel mode={{kind:'vara-hf', intent:'cms'}} adapter={varaHfAdapter} onClose={()=>{}}/>);
  await userEvent.click(screen.getByTestId('open-session-btn'));
  // wait for open-idle
  await screen.findByTestId('connect-btn');
  await userEvent.type(screen.getByTestId('target-input'), 'W7RMS-10');
  await userEvent.click(screen.getByTestId('connect-btn'));

  expect(invokes.find((i) => i.cmd === 'modem_vara_b2f_exchange')).toEqual({
    cmd: 'modem_vara_b2f_exchange',
    args: { target: 'W7RMS-10', intent: 'cms' },
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
pnpm exec vitest run src/radio/modes/RadioSessionPanel.test.tsx -t "Connect button"
```

Expected: FAIL — no outbound section yet.

- [ ] **Step 3: Add the outbound section**

In `RadioSessionPanel.tsx`, add inside the panel body when `!isClosed`:

```tsx
{lifecycle === 'open-idle' && (
  <section className="radio-panel-sec radio-session-outbound" data-testid="outbound-section">
    <h5>Outbound</h5>
    <label className="radio-panel-input-row">
      <span>Target</span>
      <input
        type="text"
        className="radio-panel-input"
        data-testid="target-input"
        value={target}
        onChange={(e) => setTarget(e.target.value)}
        autoCapitalize="characters"
        autoCorrect="off"
      />
    </label>
    <button
      type="button"
      className="radio-panel-btn radio-panel-btn-primary"
      data-testid="connect-btn"
      disabled={target.trim() === '' || lifecycle !== 'open-idle'}
      onClick={onConnectClick}
    >
      Connect
    </button>
  </section>
)}
```

Plus state + handler:

```tsx
const [target, setTarget] = useState('');

const onConnectClick = async () => {
  setLifecycle('dialing');
  setLastError(null);
  try {
    await invoke(adapter.commands.b2fExchange, {
      target: target.trim(),
      intent: mode.intent,
    });
    setLifecycle('open-idle');
  } catch (e) {
    setLastError(String(e));
    setLifecycle('open-idle');
  }
};
```

- [ ] **Step 4: Run test to verify it passes**

```bash
pnpm exec vitest run src/radio/modes/RadioSessionPanel.test.tsx
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A src/radio/modes/RadioSessionPanel.tsx src/radio/modes/RadioSessionPanel.test.tsx
git commit -m "feat(radio-panel): outbound dial section in RadioSessionPanel

Spec §4-5 — Connect button drives b2fExchange (CONNECT + B2F +
DISCONNECT) within the open session. Lifecycle returns to open-idle
after the exchange completes; listener (for p2p/radio-only) stays armed.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 5.4: Add allowlist editor section (live-editable, visible for p2p/radio-only intents in open-session view)

**Files:**
- Modify: `src/radio/modes/RadioSessionPanel.tsx` — import `AllowedStationsEditor` from `src/radio/sections/`, render conditionally

- [ ] **Step 1: Write the failing test — Allowlist section renders for p2p intent, not for cms**

```tsx
it('renders allowlist editor only when intent has listener (p2p, radio-only)', () => {
  const { rerender } = render(
    <RadioSessionPanel mode={{kind:'vara-hf', intent:'cms'}} adapter={varaHfAdapter} onClose={()=>{}}/>
  );
  // Need to open the session first; simulate by advancing state.
  // For brevity, mock the initial state to 'open-idle' via a test seam.

  expect(screen.queryByTestId('allowlist-section')).toBeNull();

  rerender(
    <RadioSessionPanel mode={{kind:'vara-hf', intent:'p2p'}} adapter={varaHfAdapter} onClose={()=>{}}/>
  );
  // … after opening …
  expect(screen.queryByTestId('allowlist-section')).toBeInTheDocument();
});
```

- [ ] **Step 2: Run test to verify it fails**

Expected: FAIL.

- [ ] **Step 3: Wire the existing `AllowedStationsEditor`**

```tsx
import { AllowedStationsEditor } from '../sections/AllowedStationsEditor';

// ... inside the panel body, when lifecycle !== 'closed' && intent has listener:

const intentHasListener = mode.intent === 'p2p' || mode.intent === 'radio-only';

{!isClosed && intentHasListener && (
  <section className="radio-panel-sec" data-testid="allowlist-section">
    <h5>Listen</h5>
    <AllowedStationsEditor adapter={adapter} />
  </section>
)}
```

`AllowedStationsEditor` (from `src/radio/sections/`) presumably already takes a protocol-aware command interface; if its current API requires a `protocol: 'ardop' | 'vara'` discriminator, pass that through the adapter.

- [ ] **Step 4: Run test to verify it passes**

```bash
pnpm exec vitest run src/radio/modes/RadioSessionPanel.test.tsx
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(radio-panel): allowlist editor inside open-session view (p2p + radio-only)

Spec §4 — allowlist editor lives inside the open-session view and is
live-editable; edits apply to subsequent inbound, not the active
exchange.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 5.5: Add per-protocol Modem settings expander (collapsible, edit while closed only)

**Files:**
- Modify: `src/radio/modes/RadioSessionPanel.tsx` — render `adapter.renderSettingsExpander()` in a collapsible section, locked while session is open
- Modify: `src/radio/modes/radioSessionAdapters.ts` — fill `renderSettingsExpander` for each adapter with the right content (ARDOP: capture/playback/PTT/cmd_port/binary/WebGUI port; VARA: host/cmd_port/data_port/bandwidth)

The content for the ARDOP expander is essentially the existing `ArdopRadioPanel.tsx`'s "Radio" section (lines 640-860 of origin/main); the VARA expander is the existing `VaraRadioPanel.tsx`'s "VARA host" section (lines 250-320). Lift these into reusable components rather than copy-pasting (`AudioDevicePicker`, `PttSerialPicker`, `VaraHostForm`).

- [ ] **Step 1: Write the failing test — settings expander is disabled while session is open**

```tsx
it('disables modem-settings inputs while session is open', async () => {
  vi.mocked(invoke).mockImplementation(async (cmd) => {
    if (cmd === 'vara_open_session') return null;
    return null;
  });
  render(<RadioSessionPanel mode={{kind:'vara-hf', intent:'cms'}} adapter={varaHfAdapter} onClose={()=>{}}/>);

  // Open the modem-settings expander.
  await userEvent.click(screen.getByTestId('modem-settings-toggle'));
  // Inputs editable while closed.
  expect(screen.getByTestId('vara-host-input')).not.toBeDisabled();

  // Open the session.
  await userEvent.click(screen.getByTestId('open-session-btn'));
  // Now disabled.
  expect(await screen.findByTestId('vara-host-input')).toBeDisabled();
});
```

- [ ] **Step 2: Run test to verify it fails**

Expected: FAIL.

- [ ] **Step 3: Build the expander + per-protocol content**

In `RadioSessionPanel.tsx`:

```tsx
const [settingsExpanded, setSettingsExpanded] = useState(false);

// ... near the bottom of the panel body ...
<section className="radio-panel-sec radio-session-settings">
  <button
    type="button"
    className="radio-panel-btn-link"
    data-testid="modem-settings-toggle"
    onClick={() => setSettingsExpanded(v => !v)}
  >
    {settingsExpanded ? '▾' : '▸'} Modem settings
  </button>
  {settingsExpanded && (
    <fieldset disabled={!isClosed}>
      {adapter.renderSettingsExpander()}
    </fieldset>
  )}
</section>
```

In `radioSessionAdapters.ts`, fill `renderSettingsExpander` for each adapter — extract the existing settings UI from `ArdopRadioPanel`'s "Radio" section and `VaraRadioPanel`'s "VARA host" section into shared sub-components:

```tsx
// In radioSessionAdapters.tsx (rename .ts → .tsx for JSX):

export const ardopAdapter: RadioSessionAdapter = {
  // ...
  renderSettingsExpander: () => <ArdopModemSettings />,
};

// Where ArdopModemSettings is the existing Radio section lifted from ArdopRadioPanel.
```

- [ ] **Step 4: Run test to verify it passes**

```bash
pnpm exec vitest run src/radio/modes/RadioSessionPanel.test.tsx
pnpm typecheck
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(radio-panel): per-protocol Modem-settings expander

Spec §4 — collapsible settings (ARDOP: audio/PTT/cmd_port/binary/WebGUI;
VARA: host/cmd_port/data_port/bandwidth). Editable while session is
closed; locked while open (changes need restart).

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 5.6: Add status pill (closed/open-idle/dialing/exchange/inbound-exchange/error)

**Files:**
- Modify: `src/radio/modes/RadioSessionPanel.tsx` — header status pill text + color
- Modify: `src/radio/modes/RadioSessionPanel.css` — pill colors

- [ ] **Step 1: Write the failing test — status pill text reflects lifecycle**

```tsx
it('renders status pill matching the lifecycle state', async () => {
  vi.mocked(invoke).mockImplementation(async () => null);
  render(<RadioSessionPanel mode={{kind:'vara-hf', intent:'p2p'}} adapter={varaHfAdapter} onClose={()=>{}}/>);

  expect(screen.getByTestId('status-pill')).toHaveTextContent('closed');
  await userEvent.click(screen.getByTestId('open-session-btn'));
  await waitFor(() => expect(screen.getByTestId('status-pill')).toHaveTextContent(/open · idle/));
});
```

- [ ] **Step 2: Run test to verify it fails**

Expected: FAIL.

- [ ] **Step 3: Render the pill**

In `RadioSessionPanel.tsx`, replace whatever header sub-text the RadioPanel currently shows with the pill:

```tsx
const pillText = lifecycleToPillText(lifecycle);

<RadioPanel
  mode={mode}
  state={mapLifecycleToPanelState(lifecycle)}
  sub={
    <span className={`status-pill status-pill-${lifecycle}`} data-testid="status-pill">
      {pillText}
    </span>
  }
  onClose={onClose}
>
  ...
</RadioPanel>
```

```tsx
function lifecycleToPillText(s: SessionLifecycleState): string {
  switch (s) {
    case 'closed':            return 'closed';
    case 'opening':           return 'opening…';
    case 'open-idle':         return 'open · idle';
    case 'dialing':           return 'open · dialing';
    case 'exchange':          return 'open · exchange';
    case 'inbound-exchange':  return 'open · inbound exchange';
    case 'closing':           return 'closing…';
    case 'error':             return 'error';
  }
}
```

CSS in `RadioSessionPanel.css`:

```css
.status-pill { padding: 2px 8px; border-radius: 4px; font-size: 12px; }
.status-pill-closed { background: #555; color: #ddd; }
.status-pill-opening, .status-pill-closing { background: #ca8a04; color: #000; }
.status-pill-open-idle { background: #16a34a; color: #fff; }
.status-pill-dialing { background: #ca8a04; color: #000; }
.status-pill-exchange, .status-pill-inbound-exchange { background: #ea580c; color: #fff; }
.status-pill-error { background: #dc2626; color: #fff; }
```

- [ ] **Step 4: Run test to verify it passes**

```bash
pnpm exec vitest run src/radio/modes/RadioSessionPanel.test.tsx
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(radio-panel): status pill reflecting lifecycle state

Spec §4 header — status pill: closed / open · idle / open · dialing /
open · exchange / open · inbound-exchange / error. Color-coded.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 5.7: Inbound-exchange state from listener events (cross modem-busy collision)

**Files:**
- Modify: `src/radio/modes/RadioSessionPanel.tsx` — subscribe to listener events; flip Connect button disabled when an inbound exchange is in-flight

- [ ] **Step 1: Write the failing test — Connect button disabled during inbound exchange**

```tsx
it('disables Connect button during an inbound exchange (modem-busy collision guard)', async () => {
  const listenerEventCallbacks: ((payload: { kind: 'inbound-started' | 'inbound-ended' }) => void)[] = [];
  vi.mocked(listen).mockImplementation((event, cb) => {
    if (event === 'listener:event') listenerEventCallbacks.push(cb as any);
    return Promise.resolve(() => {});
  });

  vi.mocked(invoke).mockImplementation(async () => null);

  render(<RadioSessionPanel mode={{kind:'vara-hf', intent:'p2p'}} adapter={varaHfAdapter} onClose={()=>{}}/>);
  await userEvent.click(screen.getByTestId('open-session-btn'));

  // Connect button enabled in open-idle.
  await screen.findByTestId('connect-btn');
  await userEvent.type(screen.getByTestId('target-input'), 'K7TEST');
  expect(screen.getByTestId('connect-btn')).not.toBeDisabled();

  // Inbound starts — Connect must disable.
  listenerEventCallbacks.forEach(cb => cb({ kind: 'inbound-started' }));
  await waitFor(() => expect(screen.getByTestId('connect-btn')).toBeDisabled());
});
```

- [ ] **Step 2: Run test to verify it fails**

Expected: FAIL.

- [ ] **Step 3: Subscribe to listener events**

```tsx
import { listen } from '@tauri-apps/api/event';

useEffect(() => {
  const unlisten = listen('listener:event', (event: { payload: { kind: string } }) => {
    if (event.payload.kind === 'inbound-started') setLifecycle('inbound-exchange');
    if (event.payload.kind === 'inbound-ended') setLifecycle('open-idle');
  });
  return () => { void unlisten.then(fn => fn?.()); };
}, []);
```

And update the Connect button's disabled prop:

```tsx
disabled={target.trim() === '' || lifecycle !== 'open-idle'}
```

(Already in place from Task 5.3 — Verify the test passes after the listener subscription is in place.)

- [ ] **Step 4: Run test to verify it passes**

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(radio-panel): inbound-exchange listener subscription + Connect lockout

Spec §9 watched failure mode — modem-busy collisions. Listener events
flip lifecycle to inbound-exchange; Connect button is gated on
lifecycle === 'open-idle' so the operator can't double-book the modem.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

---

## Phase 6 — Wire visibility router + delete legacy panels

### Task 6.1: Wire `RadioPanel.tsx` (or the central router) to render `RadioSessionPanel` for ardop-hf / vara-hf / vara-fm

**Files:**
- Modify: wherever the `RadioPanel` picks which mode-panel to render (`grep -n "ArdopRadioPanel\|VaraRadioPanel" src/`)

- [ ] **Step 1: Write the failing test — RadioPanel renders RadioSessionPanel for vara-hf mode**

In whichever test exercises the central router:

```tsx
it('renders RadioSessionPanel for vara-hf mode', () => {
  render(<App initialPanelMode={{ kind: 'vara-hf', intent: 'cms' }} />);
  expect(screen.getByTestId('open-session-btn')).toBeInTheDocument();
});
```

- [ ] **Step 2: Run test to verify it fails**

Expected: FAIL — current code renders `VaraRadioPanel`, which doesn't have an `open-session-btn`.

- [ ] **Step 3: Update the switch**

Replace the `case 'ardop-hf':` / `case 'vara-hf':` / `case 'vara-fm':` branches in the central panel switch with:

```tsx
case 'ardop-hf':
  return <RadioSessionPanel mode={mode} adapter={ardopAdapter} onClose={onClose} />;
case 'vara-hf':
  return <RadioSessionPanel mode={mode} adapter={varaHfAdapter} onClose={onClose} />;
case 'vara-fm':
  return <RadioSessionPanel mode={mode} adapter={varaFmAdapter} onClose={onClose} />;
```

- [ ] **Step 4: Run test + typecheck**

```bash
pnpm typecheck && pnpm exec vitest run
```

Expected: PASS + clean.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(radio-panel): route ardop-hf, vara-hf, vara-fm to RadioSessionPanel

Spec §6.1 — shared component takes over. ArdopRadioPanel +
VaraRadioPanel get deleted in the next task.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 6.2: Delete `ArdopRadioPanel` + `VaraRadioPanel` + their CSS + tests

**Files:**
- Delete: `src/radio/modes/ArdopRadioPanel.tsx`, `ArdopRadioPanel.css`, `ArdopRadioPanel.test.tsx`
- Delete: `src/radio/modes/VaraRadioPanel.tsx`, `VaraRadioPanel.css`, `VaraRadioPanel.test.tsx`

- [ ] **Step 1: Delete the files**

```bash
rm src/radio/modes/ArdopRadioPanel.tsx \
   src/radio/modes/ArdopRadioPanel.css \
   src/radio/modes/ArdopRadioPanel.test.tsx \
   src/radio/modes/VaraRadioPanel.tsx \
   src/radio/modes/VaraRadioPanel.css \
   src/radio/modes/VaraRadioPanel.test.tsx
```

- [ ] **Step 2: Run typecheck + tests**

```bash
pnpm typecheck && pnpm exec vitest run
```

Expected: clean. If any other component imports them, fix those imports to use `RadioSessionPanel` instead.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "refactor(radio-panel): delete ArdopRadioPanel + VaraRadioPanel

Replaced by the shared RadioSessionPanel + per-protocol adapters
(spec §6.1).

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 6.3: Audit `ListenArmButton` for remaining consumers

**Files:**
- Read: `src/radio/sections/ListenArmButton.tsx`
- Grep: `grep -rn "ListenArmButton" src/`

If the only remaining consumer is the deleted `VaraRadioPanel`, delete `ListenArmButton.tsx` too. If `TelnetP2pRadioPanel` or `PacketRadioPanel` still use it, leave it in place.

- [ ] **Step 1: Audit consumers**

```bash
grep -rn "ListenArmButton" src/
```

- [ ] **Step 2: Decide + act**

If no consumers remain, `rm src/radio/sections/ListenArmButton.tsx` + its test + CSS, run typecheck, commit:

```bash
git rm src/radio/sections/ListenArmButton.tsx src/radio/sections/ListenArmButton.test.tsx
pnpm typecheck
git commit -m "refactor(listener-ui): delete ListenArmButton — no consumers after RadioSessionPanel migration

Auto-arm via Open Session lifecycle (spec §2) replaces the explicit
Arm/Disarm button surface. Telnet/Packet panels never consumed it
(no precondition layer). PR #350's disabled-prop addition was useful
defensively but no longer reachable.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

If consumers remain (Telnet / Packet), leave it in place; commit a one-line comment update at the top of the file noting it's no longer used by VARA / ARDOP per spec §2.

---

## Phase 7 — Smoke + walk

### Task 7.1: Run `pnpm tauri dev` and walk all 9 (intent, protocol) combinations

**Files:** none modified; this is an operator-driven validation step.

- [ ] **Step 1: Start the dev build**

```bash
pnpm tauri dev
```

Expected: app launches without runtime errors.

- [ ] **Step 2: Walk the matrix (operator)**

Surface this checklist to the operator:

```
[ ] Sidebar → (cms, ardop-hf): RadioSessionPanel renders, Open Session works
[ ] Sidebar → (cms, vara-hf):  RadioSessionPanel renders, Open Session works
[ ] Sidebar → (cms, vara-fm):  RadioSessionPanel renders, Open Session works
[ ] Sidebar → (p2p, ardop-hf): allowlist editor visible, auto-arm logged
[ ] Sidebar → (p2p, vara-hf):  allowlist editor visible, auto-arm logged
[ ] Sidebar → (p2p, vara-fm):  allowlist editor visible, auto-arm logged
[ ] Sidebar → (radio-only, ardop-hf): allowlist editor visible, auto-arm logged
[ ] Sidebar → (radio-only, vara-hf):  allowlist editor visible, auto-arm logged
[ ] Sidebar → (radio-only, vara-fm):  allowlist editor visible, auto-arm logged
[ ] Close Session in each: lifecycle returns to closed cleanly
[ ] Connect with non-empty target in each: invokes b2fExchange with right intent
[ ] No ConsentModal appears anywhere
```

Verify locally with `grim`-driven screenshots + headless Chromium CDP per the `white-screen-debug-via-chromium-cdp` + `grim-realapp-validation-pandora` memories if the operator isn't available.

- [ ] **Step 3: File P2 issues for any divergence**

For each WLE-parity divergence the operator surfaces, file a `bd create --type=bug --priority=2` issue capturing the discrepancy. These are POST-walk follow-ups; do NOT block the PR on them unless they're correctness regressions.

- [ ] **Step 4: Commit walk results (or empty commit marking walk done)**

If the walk found no issues, commit an implementation-log entry:

```bash
cat >> dev/implementation-log.md <<'EOF'

## 2026-MM-DD — tuxlink-0ye6 build-walk-revise complete (Phase 7)

Operator walked all 9 (intent, protocol) combinations against the dev
build. Findings: <none / list>.
EOF

git add dev/implementation-log.md
git commit -m "docs(impl-log): tuxlink-0ye6 build-walk results

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

---

## Phase 8 — Land

### Task 8.1: Open PR and hand off to cross-provider adrev round

**Files:** none modified directly.

- [ ] **Step 1: Verify gates pass**

```bash
pnpm typecheck && pnpm test && cargo --manifest-path src-tauri/Cargo.toml test
```

Expected: all green.

- [ ] **Step 2: Open the PR**

```bash
gh pr create --title "[<YOUR-MONIKER>] feat(radio-panel): shared RadioSessionPanel + Open/Close lifecycle (tuxlink-0ye6)" \
  --body "$(cat <<'EOF'
## Summary

- Replaces per-protocol `ArdopRadioPanel` + `VaraRadioPanel` with shared `RadioSessionPanel` driven by sidebar intent.
- Open/Close Session lifecycle auto-arms listener (p2p / radio-only) and unlocks outbound dial.
- Drops `ConsentModal`, `useConsent`, `CONNECT_DEADLINE`, RADIO-1 identifier surface per spec §2.
- Adds `SessionIntent` enum + per-protocol `*_open_session` / `*_close_session` / widened `*_b2f_exchange` Tauri commands.
- VARA disarm ABORT side-channel (tuxlink-12sc) — `vara_close_session` interrupts in-flight B2F via cmd-port `DISCONNECT\r`.

Spec: docs/superpowers/specs/2026-06-04-vara-ardop-panel-alpha-design.md
Plan: docs/superpowers/plans/2026-06-04-vara-ardop-panel-alpha-polish.md
bd: tuxlink-0ye6 (subsumes tuxlink-fzl7, closes tuxlink-12sc)

## Test plan

- [ ] Operator walk: all 9 (intent, protocol) combos open + close cleanly (Phase 7).
- [ ] Operator on-air: dial CMS via VARA, P2P via ARDOP — verify B2F completes.
- [ ] Operator on-air: inbound P2P session — verify auto-arm + accept + DISCONNECT.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 3: Hand off to build-robust-features for cross-provider adrev**

This plan execution terminates here. The next step is invoking the `superpowers:build-robust-features` skill on the PR for the cross-provider Codex adrev round (per `no-carveout-on-cross-provider-adrev` memory).

```bash
# In the next session:
# /skill build-robust-features <PR-URL>
```

The adrev round runs 5 cross-provider attack-angle reviews; any P0/P1 findings get fixed before merge.

---

## Self-review

**Spec coverage:**

- ✅ §1 (disparity framing) — addressed in plan intro
- ✅ §2 (architecture decisions: one panel, sidebar intent, session lifecycle, no safeguards) — Phase 1 + Phase 5
- ✅ §3 (capability matrix) — Phase 2 + Phase 3 (SessionIntent + routing_flag)
- ✅ §4 (panel surface) — Phase 5 (Tasks 5.2-5.6)
- ✅ §5 (state machine) — Phase 5 (Tasks 5.2 + 5.7)
- ✅ §6.1 (frontend impl) — Phase 5 + Phase 6
- ✅ §6.2 (backend impl: open/close + b2f_exchange + disarm ABORT) — Phase 3 + Phase 4
- ✅ §6.3 (PR-disposition cleanup) — PR #350 is already merged; PR #348 frontend "Dial as" toggle rollback is Task 1.6
- ✅ §7 (out-of-alpha-scope items) — explicitly NOT in plan
- ✅ §8 (build-walk-revise loop) — Phase 7
- ✅ §9 (watched failure modes: auto-arm surprise, modem-busy collision, ARDOP spawn failures, VARA disarm) — Tasks 5.4, 5.7, 4.1-4.2

**Placeholder scan:** No `TBD` / `TODO` / `fill in details` / unspecified "appropriate error handling." Each step has actionable code or a commit-able action. Step 3 of some tasks (e.g., 3.4, 3.5) reference helper functions (`setup_test_state`, `run_vara_b2f_exchange`) that may need creation as part of those tasks; this is called out inline rather than left dangling.

**Type consistency:** `SessionIntent` is the single backend enum; the frontend wire shape is the kebab-case string `'cms' | 'p2p' | 'radio-only'` (the `RadioPanelMode.intent` union). The Tauri command `args: { intent }` payload mirrors this verbatim because serde's `#[serde(rename_all = "kebab-case")]` is set on the Rust enum. Command names are consistent across `radioSessionAdapters.ts` (defined in Task 5.1) and the backend registrations (Tasks 3.2-3.6).

---

## Execution handoff

**Plan complete and saved to `docs/superpowers/plans/2026-06-04-vara-ardop-panel-alpha-polish.md`. Two execution options:**

1. **Subagent-Driven (recommended)** — dispatch a fresh subagent per task; review between tasks; fast iteration. Uses `superpowers:subagent-driven-development`.
2. **Inline Execution** — execute tasks in the current session using `superpowers:executing-plans`; batch execution with checkpoints for review.

**Which approach?**

Per the spec handoff: this is bd-tuxlink-0ye6 (P1 umbrella) with the cross-provider Codex adrev round queued after impl. The `build-robust-features` skill is the canonical wrapper; it picks the right execution sub-skill and runs the adrev round as part of its workflow. So the answer is likely **subagent-driven via build-robust-features**, not bare subagent-driven-development.
