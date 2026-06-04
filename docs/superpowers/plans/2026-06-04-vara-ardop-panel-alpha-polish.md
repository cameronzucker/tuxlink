# VARA + ARDOP Panel Alpha-Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Spec:** [docs/superpowers/specs/2026-06-04-vara-ardop-panel-alpha-design.md](../specs/2026-06-04-vara-ardop-panel-alpha-design.md)
**bd umbrella:** tuxlink-0ye6 (P1; subsumes tuxlink-fzl7 + tuxlink-12sc per dep edges)

**Revision history:**
- 2026-06-04 v1 — mink-harrier-cardinal initial draft (PR #360 merged spec + plan + mocks on main).
- 2026-06-04 v2 — cypress-glade-peregrine plan-fix bundle (tuxlink-fl6e). Applies operator decisions on bd tuxlink-8gq3 (drop backend consent gate), tuxlink-qtgg (modem-native airtime bound; no replacement cap), tuxlink-d8bq (ship radio-only listener as designed) AND the Codex Round 1 P1+P2 fixes (existing SessionIntent — extend not duplicate; ABORT cmd not DISCONNECT for VARA; ARDOP connect_arq inside b2f_exchange not at open; backend session arbiter for transport ownership; backend-status-driven lifecycle; per-protocol transportKind on adapters). Codex rounds 2-5 pending.

**Goal:** Replace the current per-protocol `ArdopRadioPanel` + `VaraRadioPanel` with a single shared `RadioSessionPanel` driven by sidebar intent (cms / p2p / radio-only), introducing a WLE-shaped Open/Close Session lifecycle that auto-arms the listener (for p2p/radio-only) and unlocks outbound dial — while fully removing tuxlink-added safeguards (`CONNECT_DEADLINE`, `ConsentModal`, `useConsent`, backend `mint_consent_token` / `consume_consent_token` round-trip, RADIO-1 identifier surface). The Part 97 consent is the operator's click on the lifecycle / Connect / Send/Receive button; no in-process token round-trip is added.

**Architecture:** One React component (`RadioSessionPanel`) parameterized by `RadioPanelMode` props, with a per-protocol adapter (`ardopAdapter`, `varaHfAdapter`, `varaFmAdapter`) supplying Tauri command names + the per-protocol `transportKind` value (so VARA FM is not silently logged as VARA HF) + the settings-expander render function. Lifecycle state derives from the backend status snapshot (`modem_get_status` / `vara_status`) via subscription — local React `useState` is for transient UI affordances only, never the lifecycle source of truth. Backend extends the EXISTING `SessionIntent` enum at `src-tauri/src/winlink/session.rs:109` with serde derives + an `auto_arms_listener` method (Codex Round 1 P1 #6: the enum already exists; the earlier plan would have created a duplicate). New per-protocol `*_open_session(intent, transport_kind)` / `*_close_session()` Tauri commands open the transport + arm the listener (for intent in `{p2p, radio-only}`); `*_open_session` does NOT call `connect_arq` / `CONNECT` (Codex Round 1 P1 #1: that happens inside the Connect-button command `modem_*_b2f_exchange`). VARA's `abort_in_flight` sends `ABORT\r` (Codex Round 1 P1 #4: VARA's codec models Abort separately from Disconnect; ABORT is hard tear-down within ~2s, DISCONNECT can wait for the current burst). A backend session arbiter (Phase 4 Task 4.3) serializes transport ownership between the listener consumer task and outbound dial (Codex Round 1 P1 #5: without it the spec's "listener stays armed while operator dials outbound" is architecturally undefined). The `CONNECT_DEADLINE = 120s` constant is dropped entirely with no replacement wall-clock cap — bound is ardopcf's `ARQTIMEOUT` plus the operator's ABORT button (operator decision bd tuxlink-qtgg).

**Tech Stack:** TypeScript / React (frontend, Vite + Vitest), Rust / Tauri 2 (backend, `cargo test`), `pnpm` for JS dependency management.

---

## How to use this plan

- **Base branch:** branch off `origin/main` as `bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish-v2` (or a similar new slug — the original PR #360 worktree is merged-dead; the impl run needs a fresh worktree off current main). Per ADR 0004 + ADR 0008's worktree-issue-ownership rule. Create a worktree at `worktrees/tuxlink-0ye6-v2/` and claim it on the bd issue.
- **Phase order matters.** Per Codex Round 1 P2 #11: each phase MUST end with the test suite green. Phase 1 (safeguard strip) intentionally goes FIRST — that's the operator's directive (drop the modal + token round-trip; modem-native airtime bound). It still ends green: each task's "expect FAIL → make it green" cycle holds within the task, and no task pushes interim broken state. Phase 4 (ABORT side-channel + session arbiter) is a **precondition for Phase 1.5** (the actual `Duration::MAX` substitution in `connect_arq`) — Task 1.5 references this dependency and gates on it; in practice this means working Phase 4.1 before Task 1.5's Step 3.
- **Within a phase, tasks are sequential** unless explicitly noted as parallelizable. The dependency edges below override "naive" task ordering:
  - Task 1.5 (drop CONNECT_DEADLINE) depends on Task 4.1 (VARA ABORT) landing first.
  - Task 3.6 (ARDOP b2f_exchange does `connect_arq`) depends on Task 3.5 (ardop_open_session spawns ardopcf without `connect_arq`).
  - Task 4.3 (session arbiter) is a precondition for Phase 5 (shared panel).
  - Tasks 3.2 / 3.3 / 3.4 / 3.5 / 3.6 accept `transport_kind` per Task 5.1's amendment — work it in when implementing each, not as a separate sweep.
- **No "expect broken tests" framing.** Codex Round 1 P2 #11 explicitly flagged any "don't fix yet" / "tests stay red until next task" framing as unacceptable. If a task description below contains such language, treat it as a defect and ensure the task lands green; flag it in commit body so the issue is traceable.
- **TDD discipline:** each task has Write-test → Verify-fails → Implement → Verify-passes → Commit. Don't skip "verify-fails" — that's the gate that catches false-positive tests.
- **Commit cadence:** one commit per task unless a task says otherwise. Use conventional types (`feat`/`fix`/`refactor`/`test`/`chore`). Include `Agent:` trailer with your session moniker + `Co-Authored-By:` (per CLAUDE.md).
- **Push cadence:** push the instant a unit is committed + tests green (per the `never-hold-a-push` memory). Don't batch.
- **Reference paths in this plan are relative to `origin/main`.** If you find a path differs from what the plan says, prefer current code as truth and update the plan inline as you go.

---

## File-structure map

**Files this plan will CREATE:**

- `src/radio/modes/RadioSessionPanel.tsx` — shared component (backend-status-driven lifecycle)
- `src/radio/modes/RadioSessionPanel.css` — sibling styles
- `src/radio/modes/RadioSessionPanel.test.tsx` — component tests
- `src/radio/modes/radioSessionAdapters.ts` — per-protocol adapter (carries `transportKind`)
- `src/radio/modes/radioSessionAdapters.test.ts` — adapter tests
- `src/radio/modes/useRadioSessionLifecycle.ts` — hook deriving lifecycle from backend status snapshot

**Files this plan will NOT create** (Codex Round 1 P1 #6):

- ~~`src-tauri/src/winlink/session_intent.rs`~~ — `SessionIntent` already exists at `src-tauri/src/winlink/session.rs:109`; we EXTEND it (add serde derives + `auto_arms_listener` method), we do not duplicate it.

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
- `src-tauri/src/modem_commands.rs` — drop `CONNECT_DEADLINE` (no replacement), drop `consent_token` params, add `intent` + `transport_kind` params, add ARDOP session-lifecycle commands, add `connect_arq` step to `modem_ardop_b2f_exchange`
- `src-tauri/src/ui_commands.rs` — rename `vara_start_session` → `vara_open_session(intent, transport_kind)`, `vara_stop_session` → `vara_close_session()`, add `modem_vara_b2f_exchange(target, intent, transport_kind)`, wire ABORT side-channel
- `src-tauri/src/lib.rs` — register new Tauri commands, drop deleted ones (`modem_mint_consent` goes; `vara_start_session` / `vara_stop_session` get renamed)
- `src-tauri/src/winlink/session.rs` — add serde derives + `auto_arms_listener` to existing `SessionIntent`; add backend session arbiter on `ModemSession` (Task 4.3)
- `src-tauri/src/winlink/modem/vara/transport.rs` — add `try_clone_abort_writer` (ABORT side-channel)
- `src-tauri/src/winlink/modem/vara/session.rs` — add `abort_in_flight` (sends `ABORT\r`, not `DISCONNECT\r`) + arbiter on `VaraSession`
- `src-tauri/src/winlink/modem/vara/listener.rs` — yield + reclaim transport via arbiter (Task 4.3)
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

- [ ] **Step 5: Run all backend tests; fix every breakage from the signature change in THIS commit**

```bash
cargo --manifest-path src-tauri/Cargo.toml test
```

Expected: PASS for everything. **Codex Round 1 P2 #11 + operator commit-discipline:** the earlier draft permitted fallout from this task to stay red until Tasks 1.2 + 1.3. That's a phase-greenness defect. Instead: inside THIS task, update every caller of `modem_ardop_connect` to pass the new signature (no `consent_token`), and update every test that asserted on the old signature. If that means Tasks 1.2 + 1.3 partially collapse into this one — that's fine, a green phase is the priority.

If touching the callers makes Task 1.1 too large, split Task 1.1 itself into 1.1a + 1.1b at a green-test boundary. Don't push a red suite.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/modem_commands.rs src-tauri/src/winlink/modem/ardop/session.rs
# Plus whichever caller / test files needed updates to keep the suite green.
git commit -m "refactor(modem-ardop): replace consent-token gate with busy guard

Drops the RADIO-1 consume_consent_token atomic; the consent modal goes
away in Task 1.3. Internal AtomicBool busy guard preserves the
dup-call defense without a user-facing modal.

Per Codex Round 1 P2 #11 + operator decision bd tuxlink-8gq3: this
task keeps the full test suite green by updating every direct caller
of the old signature in the same commit. If callers collapse with
Tasks 1.2/1.3 work, that's intentional.

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

### Task 1.5: Drop `CONNECT_DEADLINE` constant + 120s bound (no replacement)

**Files:**
- Modify: `src-tauri/src/modem_commands.rs:25` — delete `const CONNECT_DEADLINE: Duration = ...`
- Modify: `src-tauri/src/modem_commands.rs` connect path — pass `Duration::MAX` (or equivalent — see Step 3) so the modem-native `ARQTIMEOUT` is the wall-clock bound. **Do NOT introduce a `CONNECT_TCP_WEDGE_GUARD` or any other tuxlink-added wall-clock cap.**

**Rationale per spec §2 + operator decision 2026-06-04 (bd tuxlink-qtgg):** ardopcf's `ARQTIMEOUT` (configurable; default 30s for idle, but bounds keyed airtime via `ARQTIMEOUT × CONNECT_REPEAT` cycles) and the operator's ABORT-button click are the legitimate bounds. The tuxlink-added 120s cap was reactive to the 2026-05-22 runaway; the proper fix is the side-channel ABORT (ARDOP already has it post-tuxlink-o3f2; VARA gets it in revised Phase 4). An earlier draft of this plan introduced a 600s "TCP-wedge guard" as a replacement — Codex Round 1 P1 #3 + operator decision bd tuxlink-qtgg both reject this: 600s is real airtime, 5× the original cap, and the operator-click on Open Session / Connect / Send-Receive plus the ABORT button is the bound. Long-haul / weak-signal connects that genuinely run minutes are operator-chosen RF behavior; the agent does not second-guess the operator's airtime budget.

**Precondition:** Task 4.1 (VARA ABORT side-channel via `ABORT\r`) AND Task 4.2 (wire `vara_open_session` to install the abort writer) MUST both land before this task. Per Codex Round 2 P1 #1: Task 4.1 alone only creates the API; without Task 4.2 the abort writer is never installed, so `vara_close_session` can't actually send `ABORT\r`. Both wiring steps are the precondition, not just 4.1. ARDOP's ABORT side-channel is already wired in production (post-tuxlink-o3f2 — verify via Step 1). Both protocols must have a **working end-to-end** ABORT before the 120s cap is dropped.

**Precondition refactor (Codex Round 2 P1 #2 — `Duration::MAX` is unsafe):** the existing `arq_connect` call path flows the deadline into `CmdSocket::recv_event(...)` and `std::sync::mpsc::Receiver::recv_timeout`, where `Duration::MAX` overflows the internal arithmetic (the receiver's deadline is `now + dur`, which panics or wedges). Step 4 below introduces an explicit **no-deadline** path through `connect_arq` — change the signature to accept `Option<Duration>` (or a dedicated `Deadline` enum), and have the inner `recv_timeout` branch on `None` to use `recv()` (no timeout) instead of `recv_timeout(Duration::MAX)`. A grep-sentinel-only test is not enough; Step 6 below adds a behavioral test that exercises the no-deadline branch and confirms it does not panic.

- [ ] **Step 1: Verify ARDOP ABORT path is wired** (sanity check before stripping the cap)

```bash
# Confirm ARDOP's abort path exists and is invokable from Tauri.
grep -rn "abort_in_flight\|install_abort_writer" src-tauri/src/modem_commands.rs src-tauri/src/winlink/modem/ardop/ | head -20
```

Expected: matches for `abort_in_flight` exist on ARDOP's `ModemSession` and are invoked by `modem_ardop_close_session` (or equivalent).

If the ARDOP abort path is NOT wired or is broken, STOP. Fix it before continuing. The 120s cap is the current backstop; we don't drop it without a working replacement.

- [ ] **Step 2: Verify VARA ABORT path is wired end-to-end** (Phase 4.1 + 4.2 preconditions)

```bash
# Phase 4.1 — the ABORT writer API:
grep -rn "abort_in_flight\|install_abort_writer" src-tauri/src/winlink/modem/vara/ src-tauri/src/ui_commands.rs | head -20
# Phase 4.1 — the codec emits ABORT (Codex Round 2 P2: the file is command.rs, NOT codec.rs):
grep -rn 'ABORT' src-tauri/src/winlink/modem/vara/command.rs | head -10
# Phase 4.2 — the writer is installed during vara_open_session, not just declared:
grep -rn "install_abort_writer" src-tauri/src/ui_commands.rs | head -5
```

Expected: matches for `abort_in_flight` exist on VARA's `VaraSession`; `command.rs` emits `"ABORT"` for `Command::Abort`; `vara_open_session` in `ui_commands.rs` calls `install_abort_writer`.

If Phase 4.1 OR Phase 4.2 isn't wired, STOP. **Both** are preconditions per Codex Round 2 P1 #1.

- [ ] **Step 3: Write the failing test — `CONNECT_DEADLINE` symbol is gone (sentinel, source-scan)**

```rust
// IMPORTANT (Codex Round 2 P2 — sentinel-test-passability): the test body itself
// contains the literal "CONNECT_DEADLINE" string, so a naive `source.contains(...)`
// would match the test's own assertion message + variable. Use `concat!` to split
// the sentinel into pieces that the production code never contains as a
// single token — but the assertion text is still readable in failure output.
#[test]
fn modem_commands_source_does_not_define_connect_deadline_symbol() {
    let source = include_str!("modem_commands.rs");
    // Split the sentinel so the test file's own bytes don't match.
    let sentinel = concat!("CONNECT", "_DEADLINE");
    let wedge_sentinel = concat!("CONNECT", "_TCP_WEDGE_GUARD");
    assert!(
        !source.contains(sentinel),
        "modem_commands.rs still defines CONNECT_DEADLINE — spec §2 + operator decision bd tuxlink-qtgg mandate removal"
    );
    assert!(
        !source.contains(wedge_sentinel),
        "modem_commands.rs introduces a CONNECT_TCP_WEDGE_GUARD substitute — Codex Round 1 P1 #3 + operator decision bd tuxlink-qtgg reject any tuxlink-added wall-clock cap"
    );
}
```

- [ ] **Step 4: Run test to verify it fails**

```bash
cargo --manifest-path src-tauri/Cargo.toml test modem_commands_source_does_not_define_connect_deadline_symbol
```

Expected: FAIL — `CONNECT_DEADLINE` is currently defined and referenced.

- [ ] **Step 5: Refactor `connect_arq` to accept an `Option<Duration>` (or `Deadline` enum) and skip the timeout wrapper when None**

The current `connect_arq` signature is `connect_arq(target, repeats, deadline: Duration)`. Per Codex Round 2 P1 #2, change it to `connect_arq(target, repeats, deadline: Option<Duration>)`, and inside the body's `recv_timeout(deadline)` site, branch:

```rust
let cmd_event = match deadline {
    Some(dur) => cmd_socket.recv_event(dur)?,        // existing path
    None      => cmd_socket.recv_event_blocking()?,  // new no-timeout path
};
```

If `CmdSocket::recv_event` is implemented via `Receiver::recv_timeout(dur)`, add a sibling `recv_event_blocking()` that calls `Receiver::recv()` instead. The two-method shape is preferable to `Duration::MAX` because it documents intent at the call site AND it skips the overflow-prone deadline-arithmetic path entirely.

Update every existing `connect_arq` caller to pass `Some(...)` (preserving behavior) — your new Connect-button call site passes `None`.

- [ ] **Step 6: Add behavioral test for the no-deadline path** (Codex Round 2 P1 #2 also asks for this)

```rust
#[test]
fn connect_arq_with_no_deadline_does_not_panic_and_blocks_until_event() {
    let mut transport = StubTransportEmittingDelayedConnected::new(Duration::from_millis(50));
    let outcome = transport.connect_arq("TEST", 1, None);
    // The stub emits CONNECTED after 50ms; the no-deadline path should
    // return Ok without panicking on internal arithmetic or wedging.
    assert!(outcome.is_ok(), "no-deadline connect_arq must complete on event arrival");
}
```

(`StubTransportEmittingDelayedConnected` is whatever modem-transport stub matches the codebase's existing test idiom — see `src-tauri/src/winlink/modem/ardop/tests.rs` or wherever existing modem stubs live; if no such stub exists, add a minimal one. Don't synthesize from scratch without checking — the existing tests almost certainly already have a stub for this.)

- [ ] **Step 5b: Drop the constant + call the new no-deadline path at the Connect site**

In `src-tauri/src/modem_commands.rs`, delete:

```rust
const CONNECT_DEADLINE: Duration = Duration::from_secs(120);
```

At the new call site (inside `modem_ardop_b2f_exchange` per Task 3.6, NOT inside the old `modem_ardop_connect` which Task 1.1-1.4 has already deleted), pass `None`:

```rust
// No tuxlink wall-clock bound on ARQCALL; bound is ardopcf's ARQTIMEOUT
// plus the operator's ABORT-button click (Close Session → abort_in_flight).
// Operator decision 2026-06-04 (bd tuxlink-qtgg) + Codex Round 1 P1 #3.
// `None` (not Duration::MAX) so the inner recv_timeout is skipped entirely
// (Codex Round 2 P1 #2 — Duration::MAX overflows recv_timeout arithmetic).
transport.connect_arq(target, CONNECT_REPEAT, None).await?;
```

- [ ] **Step 6: Run test to verify it passes**

```bash
cargo --manifest-path src-tauri/Cargo.toml test modem_commands_does_not_define_connect_deadline
```

Expected: PASS.

- [ ] **Step 7: Run full backend test suite**

```bash
cargo --manifest-path src-tauri/Cargo.toml test
```

Expected: clean. If any test had asserted "connect-fails-after-120s," replace it with a test that confirms the ABORT path interrupts a long connect (which Phase 4.1 should already cover).

- [ ] **Step 8: Commit**

```bash
git add -A src-tauri/src/modem_commands.rs
git commit -m "refactor(modem-ardop): drop CONNECT_DEADLINE entirely (no replacement)

Operator decision 2026-06-04 (bd tuxlink-qtgg) + Codex Round 1 P1 #3:
no tuxlink-added wall-clock cap on ARQCALL. The bound on keyed airtime
is ardopcf's ARQTIMEOUT × CONNECT_REPEAT plus the operator's ABORT-button
click. Precondition: ABORT side-channel reliable (already on ARDOP via
tuxlink-o3f2; on VARA via revised Phase 4.1).

An earlier draft of this task introduced a 600s 'TCP-wedge guard' as a
defense-in-depth replacement; that approach was rejected by Codex Round 1
(real airtime, 5× the original, conflicts with no-tuxlink-added-safeguards)
and by the operator's direct ratification.

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

## Phase 3 — Backend `SessionIntent` enum + status DTO widening + session lifecycle commands

### Task 3.0: Widen `ModemStatus` + `VaraStatus` DTOs with lifecycle fields (Codex Round 2 P1 #5)

**Codex Round 2 P1 #5 + Task 5.2 amendment precondition:** the revised plan asks `useRadioSessionLifecycle` to derive lifecycle state from `modem_get_status` / `vara_status` snapshots. But the current `ModemStatus` (`src-tauri/src/modem_status.rs:19-`) only exposes `ModemState` (Stopped / Spawning / Initializing / Idle / Connecting / ConnectedIrs / ConnectedIss / Disconnecting / Error) — there's no `listener_armed`, no `exchange_in_flight`, no transport-owner snapshot. `VaraStatus` (`src-tauri/src/winlink/modem/vara/commands.rs:85-`) is similarly thin — `state / last_error / bound_host / bound_cmd_port`. A frontend worker can make a mocked `useRadioSessionLifecycle` test pass while production has no source of the listener-armed / exchange-in-flight data. This task adds the fields.

**Files:**
- Modify: `src-tauri/src/modem_status.rs` — add `listener_armed: bool`, `exchange: Option<ExchangeState>`, `transport_owner: TransportOwner` to `ModemStatus`
- Modify: `src-tauri/src/winlink/modem/vara/commands.rs` (`VaraStatus`) — same additions
- Modify: `src-tauri/src/modem_commands.rs` (`modem_get_status` handler) + `src-tauri/src/ui_commands.rs` (`vara_status` handler) — populate the new fields from `ModemSession` / `VaraSession` state (Task 4.3's arbiter feeds the transport_owner field)

**`ExchangeState` shape:**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExchangeState {
    Dialing,            // connect_arq in flight
    Outbound,           // CONNECTED, running B2F handshake or message drain (outbound dial)
    Inbound,            // listener accepted; running B2F (inbound from peer)
}
```

`None` means "no exchange in flight."

**Codex Round 3 P1 #3 — active session mode in status:** the status DTOs MUST also carry `active_intent: Option<SessionIntent>` and `active_transport_kind: Option<TransportKind>`. Without these, the operator can navigate the sidebar from VARA HF/P2p to VARA FM/Cms while the backend session is still open with the original intent + transport_kind; the UI then renders the new sidebar mode while the listener is still armed for the OLD mode. The frontend uses these fields to detect "sidebar nav drift" and surface a mismatch banner (or block nav with a confirmation), per the navigation-handler decision below.

Add to both DTOs:

```rust
pub active_intent: Option<SessionIntent>,
pub active_transport_kind: Option<TransportKind>,
```

`None` means "no session open."

**Codex Round 3 P1 #3 also requires** that the plan defines sidebar-navigation-while-session-open behavior. Three viable choices (operator decision deferred to next session; for now the plan specifies the safe default):

- **(a) Block nav** — clicking a different sidebar entry while a session is open shows a confirmation modal: "Session is open for {intent}/{transport_kind}; Close Session before switching?" Cancel keeps the operator on the open session's panel. **DEFAULT for the impl** unless a future operator decision overrides.
- (b) Auto-close — clicking a different sidebar entry calls `*_close_session()` automatically.
- (c) Mismatch banner — let the operator navigate; the new panel renders a banner saying "Session for {prev_intent} is still open in {prev_transport_kind} panel" with a quick-return link.

The default (a) is the most conservative; it doesn't risk dropping an active inbound exchange. Default goes into a new spec § "Sidebar navigation while session open" added in this task's spec edit step (see Step 0 below).

**Codex Round 3 P1 #4 — socket liveness:** polling `vara_status` / `modem_get_status` alone cannot detect a dead modem because the status snapshot is cached. Add a backend background task per session that probes the cmd-port:

- VARA: send a benign cmd (e.g., `VERSION\r` if VARA acks it, or `MYCALL\r` which is idempotent) every 5s; if no reply in 3s for 2 consecutive probes → transition `state` to a new `VaraState::SocketLost` variant.
- ARDOP: read from the cmd-port with a 5s heartbeat; if ardopcf process is gone (via `Child::try_wait` returning a non-None status) OR no FAULT/READY/PENDING received in 10s → transition `ModemState::SocketLost`.

Add to `VaraState` and `ModemState`:

```rust
// VaraState (existing variants: Closed / Connecting / Open / Error):
SocketLost,  // cmd-port unresponsive; operator should Close Session to recover

// ModemState (existing): add SocketLost variant similarly
```

The frontend hook (`useRadioSessionLifecycle`, Task 5.2) translates these to its `'crash-recovery'` lifecycle state.

**Spec edit for Task 3.0 (Step 0 below):** add a new section to `docs/superpowers/specs/2026-06-04-vara-ardop-panel-alpha-design.md`: "§2.5 Sidebar navigation while session open" explaining the default (a) behavior, plus add `SocketLost` to the §5 state machine.

- [ ] **Step 0: Edit the spec to add §2.5 (sidebar-nav-while-session-open) + extend §5 state machine with `SocketLost`**

Add to `docs/superpowers/specs/2026-06-04-vara-ardop-panel-alpha-design.md` between §2 and §3:

```markdown
### 2.5 Sidebar navigation while session is open

Operator clicks a different sidebar entry while a session is open. Default behavior (until a future operator decision overrides) is **block nav + confirmation modal**: show "Session open for {intent}/{transport_kind}; Close Session before switching?" Cancel keeps the operator on the open-session panel. This avoids dropping an active inbound exchange. The backend `*_status` DTO exposes `active_intent` + `active_transport_kind` so the frontend can detect this state without holding it in React local state.

Watched failure mode: operator hard-refreshes the dev build (Ctrl+R in `pnpm tauri dev`); React unmounts but the backend session stays open. On remount the panel re-derives lifecycle from the status snapshot (Task 5.2 amendment) and renders the original session. The sidebar selection state may need backend storage to re-route to the right panel on hard-refresh.
```

Append to §5 state machine the `socket-lost` state with transitions: `open · {anything} → socket-lost` (on cmd-port unresponsive / ardopcf exit) → `closed` (on operator Close).

Commit the spec edit separately from the backend changes below for a clean audit trail.

- [ ] **Step 1: Write the failing test — VaraStatus DTO carries listener_armed + exchange + transport_owner**

```rust
#[test]
fn vara_status_dto_includes_lifecycle_fields() {
    let session = VaraSession::new();
    // After Phase 4.3, the session exposes transport_owner; before, this test FAILs.
    let status = session.snapshot();
    let _: bool = status.listener_armed;
    let _: Option<ExchangeState> = status.exchange;
    let _: TransportOwner = status.transport_owner;
    // The point of the test is that these fields compile + populate.
}

#[test]
fn vara_status_serializes_lifecycle_fields_camel_case() {
    let snap = VaraStatus {
        state: VaraState::Open,
        last_error: None,
        bound_host: None,
        bound_cmd_port: None,
        listener_armed: true,
        exchange: Some(ExchangeState::Outbound),
        transport_owner: TransportOwner::Outbound,
    };
    let json = serde_json::to_string(&snap).unwrap();
    assert!(json.contains("\"listenerArmed\":true"));
    assert!(json.contains("\"exchange\":\"outbound\""));
    assert!(json.contains("\"transportOwner\":\"outbound\""));
}
```

Mirror these for `ModemStatus`.

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo --manifest-path src-tauri/Cargo.toml test vara_status_dto_includes_lifecycle_fields modem_status_dto_includes_lifecycle_fields
```

Expected: FAIL — fields don't exist on the DTO.

- [ ] **Step 3: Add fields to both DTOs**

In `src-tauri/src/modem_status.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModemStatus {
    pub state: ModemState,
    pub last_error: Option<String>,
    pub arq_flags: ArqFlags,
    // ... existing fields ...
    // New (Codex Round 2 P1 #5):
    pub listener_armed: bool,
    pub exchange: Option<ExchangeState>,
    pub transport_owner: TransportOwner,
}
```

In `src-tauri/src/winlink/modem/vara/commands.rs`:

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaraStatus {
    pub state: VaraState,
    pub last_error: Option<String>,
    pub bound_host: Option<String>,
    pub bound_cmd_port: Option<u16>,
    // New (Codex Round 2 P1 #5):
    pub listener_armed: bool,
    pub exchange: Option<ExchangeState>,
    pub transport_owner: TransportOwner,
}
```

`ExchangeState` and `TransportOwner` live in a shared module (e.g., `src-tauri/src/winlink/lifecycle.rs` or just inline near `SessionIntent` at `winlink/session.rs`).

- [ ] **Step 4: Wire the status handlers to populate the new fields**

In `modem_get_status` and `vara_status`, populate from `ModemSession::snapshot()` / `VaraSession::snapshot()`. The Session types need to expose:

```rust
impl ModemSession {
    pub fn listener_armed(&self) -> bool { /* from ArdopListenState */ }
    pub fn current_exchange(&self) -> Option<ExchangeState> { /* from arbiter */ }
    // transport_owner already added in Task 4.3
}
```

(Same for `VaraSession`.)

**Sequencing note:** Task 4.3 (arbiter) adds `transport_owner` to both sessions. Task 3.0 should land BEFORE Task 4.3 to define the type, OR Task 4.3 defines the type and Task 3.0 just adds the DTO field referencing it. Pick the second order: Task 3.0 happens AFTER Task 4.3. Update plan front-matter dependency notes accordingly.

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo --manifest-path src-tauri/Cargo.toml test modem_status vara_status
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(modem,vara): widen status DTOs with lifecycle fields

Codex Round 2 P1 #5. The Task 5.2 amendment asks
useRadioSessionLifecycle to derive lifecycle from backend status
snapshots, but the existing ModemStatus + VaraStatus DTOs only carry
coarse transport state — no listener_armed, no exchange, no transport
owner snapshot. Without these fields, the frontend hook is unimplementable
against real production data; a mocked test could pass while production
sees nothing.

Adds listener_armed: bool, exchange: Option<ExchangeState> (Dialing /
Outbound / Inbound), and transport_owner: TransportOwner (from the
Task 4.3 arbiter) to both DTOs. Wire format is camelCase.

Precondition: Task 4.3 (arbiter) must have landed first so the
TransportOwner type exists.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 3.1: Extend the existing `SessionIntent` enum with serde + `auto_arms_listener`

**Codex Round 1 P1 #6 + memory `feedback_verify_handoff_runtime_provenance`:** `SessionIntent` ALREADY EXISTS at `src-tauri/src/winlink/session.rs:109-136` with variants `Cms` (default), `RadioOnly`, `PostOffice`, `Mesh`, `P2p`, and an `impl` block at `:178` exposing `routing_flag(self) -> Option<RoutingFlag>`. Existing derives: `Debug, Clone, Copy, PartialEq, Eq, Default`. An earlier draft of this plan would have created a duplicate enum at `src-tauri/src/winlink/session_intent.rs` — DO NOT DO THAT. Extend the existing one.

**Files:**
- Modify: `src-tauri/src/winlink/session.rs` — add `Serialize, Deserialize` derives + `#[serde(rename_all = "kebab-case")]` + `auto_arms_listener(self) -> bool` method
- Do NOT create `src-tauri/src/winlink/session_intent.rs`. Do NOT touch `src-tauri/src/winlink/mod.rs` for a re-export — `SessionIntent` is already exposed via the existing module structure (verify with `grep -rn "use.*SessionIntent" src-tauri/src/`).

**Pre-flight check before writing tests** (Codex Round 2 P2 — use a relative path so subagents working in a different worktree see THEIR own code, not the review worktree's):

```bash
# Confirm the enum + impl are where Codex said they are.
# Run from the active worktree root (e.g. `cd worktrees/<your-slug>` first).
grep -n "pub enum SessionIntent\|impl SessionIntent\|fn routing_flag" src-tauri/src/winlink/session.rs | head
```

Expected: matches at `:109`, `:178`, `:182` (or close — file may have drifted by a few lines since 2026-06-04).

- [ ] **Step 1: Write the failing tests — serde round-trip + `auto_arms_listener`**

Add to the existing `mod tests` block in `src-tauri/src/winlink/session.rs`:

```rust
#[test]
fn session_intent_serializes_kebab_case() {
    use serde_json;
    assert_eq!(serde_json::to_string(&SessionIntent::Cms).unwrap(),         "\"cms\"");
    assert_eq!(serde_json::to_string(&SessionIntent::P2p).unwrap(),         "\"p2p\"");
    assert_eq!(serde_json::to_string(&SessionIntent::RadioOnly).unwrap(),   "\"radio-only\"");
    assert_eq!(serde_json::to_string(&SessionIntent::PostOffice).unwrap(),  "\"post-office\"");
    assert_eq!(serde_json::to_string(&SessionIntent::Mesh).unwrap(),        "\"mesh\"");
}

#[test]
fn session_intent_deserializes_kebab_case() {
    use serde_json;
    let cms: SessionIntent = serde_json::from_str("\"cms\"").unwrap();
    let p2p: SessionIntent = serde_json::from_str("\"p2p\"").unwrap();
    let ro:  SessionIntent = serde_json::from_str("\"radio-only\"").unwrap();
    let po:  SessionIntent = serde_json::from_str("\"post-office\"").unwrap();
    let me:  SessionIntent = serde_json::from_str("\"mesh\"").unwrap();
    assert_eq!(cms, SessionIntent::Cms);
    assert_eq!(p2p, SessionIntent::P2p);
    assert_eq!(ro,  SessionIntent::RadioOnly);
    assert_eq!(po,  SessionIntent::PostOffice);
    assert_eq!(me,  SessionIntent::Mesh);
}

#[test]
fn auto_arms_listener_matches_spec_matrix() {
    // Per spec §3 capability matrix — only intents that accept inbound auto-arm.
    assert!(!SessionIntent::Cms.auto_arms_listener());
    assert!( SessionIntent::P2p.auto_arms_listener());
    assert!( SessionIntent::RadioOnly.auto_arms_listener());
    // PostOffice + Mesh are out of alpha scope (sessionTypes.ts `built: false`); their
    // auto-arm behavior is defined-but-unused. Codify the current intent so a future
    // change is a deliberate decision, not a silent flip.
    assert!(!SessionIntent::PostOffice.auto_arms_listener());
    assert!(!SessionIntent::Mesh.auto_arms_listener());
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo --manifest-path src-tauri/Cargo.toml test --package tuxlink-tauri \
  session_intent_serializes_kebab_case \
  session_intent_deserializes_kebab_case \
  auto_arms_listener_matches_spec_matrix
```

Expected: FAIL — `Serialize` / `Deserialize` not derived; `auto_arms_listener` not defined.

- [ ] **Step 3: Add derives + method**

In `src-tauri/src/winlink/session.rs`, at the `#[derive(...)]` line (currently `:109`):

```rust
// BEFORE:
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]

// AFTER:
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
```

(If `serde` isn't already a project dependency for this file's enclosing crate, verify with `cargo --manifest-path src-tauri/Cargo.toml tree | grep serde`. It's used elsewhere in the project so should be present; if not, `serde = { workspace = true, features = ["derive"] }` in `src-tauri/Cargo.toml`.)

In the existing `impl SessionIntent` block (currently around `:178`), add:

```rust
/// True for intents that auto-arm a listener at Open Session (per spec §2 + §3).
/// Driven by whether the intent has an inbound side: P2p (any peer) and RadioOnly
/// (R-pool peer) yes; Cms (CMS gateway is outbound-only from the client's view),
/// PostOffice and Mesh (out of alpha scope) no.
pub fn auto_arms_listener(self) -> bool {
    matches!(self, SessionIntent::P2p | SessionIntent::RadioOnly)
}
```

**Do NOT** change `routing_flag(self) -> Option<RoutingFlag>` — the existing signature is correct. An earlier draft would have added a parallel `routing_flag(self) -> Option<char>` returning a raw `char`; that would have created a redundant API surface. Keep the existing `RoutingFlag`-returning method; callers that need the `char` go through `RoutingFlag::as_char()`.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo --manifest-path src-tauri/Cargo.toml test --package tuxlink-tauri \
  session_intent_serializes_kebab_case \
  session_intent_deserializes_kebab_case \
  auto_arms_listener_matches_spec_matrix
# Plus the full file's existing tests, to confirm we didn't break the routing_flag suite.
cargo --manifest-path src-tauri/Cargo.toml test --package tuxlink-tauri winlink::session::tests
```

Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/session.rs
git commit -m "feat(winlink): extend SessionIntent with serde + auto_arms_listener

Codex Round 1 P1 #6: SessionIntent already exists at winlink/session.rs
with Cms/RadioOnly/PostOffice/Mesh/P2p variants and a routing_flag method.
Earlier plan would have created a duplicate; this commit extends in place.

Adds serde Serialize/Deserialize with kebab-case (so the wire format
matches the sidebar's session-type IDs verbatim) and an auto_arms_listener
method matching spec §2's capability matrix (P2p + RadioOnly auto-arm;
Cms doesn't; PostOffice + Mesh are out of alpha and don't either).

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

### Task 3.2: Rename `vara_start_session` → `vara_open_session(intent, transport_kind)` and auto-arm listener

**Codex Round 2 P2 amendment** (load-bearing for Phase 3 tasks 3.2 / 3.3 / 3.4 / 3.5 / 3.6): every command signature accepts `transport_kind: TransportKind` (matches the backend enum used for arm-records + reject-forensics). The earlier sketch below shows `intent` only — when implementing, ADD `transport_kind` to the signature, the test invocations, and the `.invoke_handler` registration. The frontend adapter (Task 5.1) sends `{ intent, transportKind }` already; without the backend accepting `transportKind`, the IPC fails at deserialization. The Step 1 test below MUST also assert on the transport_kind being recorded.

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

### Task 3.6: Widen `modem_ardop_b2f_exchange` to perform `connect_arq` + B2F + DISCONNECT, accepting `intent: SessionIntent`

**Codex Round 1 P1 #1:** the current `modem_ardop_b2f_exchange` assumes `modem_ardop_connect` already brought the ARQ link up. After Phase 3, the only way to open the transport is `ardop_open_session`, which (per Task 3.5) does NOT call `connect_arq`. If `modem_ardop_b2f_exchange` runs as written, it sends the B2F handshake over an unconnected data stream → silent failure / on-air weirdness. Fix: Connect button's command (`modem_ardop_b2f_exchange`) performs `connect_arq(target, ...)` BEFORE the B2F handshake, with a test asserting ARQCALL is sent before any B2F byte.

**Files:**
- Modify: `src-tauri/src/modem_commands.rs` — add `connect_arq` step + `intent` parameter; thread intent to routing-flag filter

- [ ] **Step 1: Write the failing test — ARQCALL is sent before B2F handshake**

```rust
#[tokio::test]
async fn modem_ardop_b2f_exchange_sends_arqcall_before_b2f() {
    let state = setup_test_state_with_ardop_stub_capturing_cmds();
    let _ = modem_ardop_b2f_exchange(
        state.app_handle, state.session,
        "K7LED-7".to_string(),
        SessionIntent::P2p,
    );

    let cmds = state.captured_cmd_sequence();
    let arqcall_idx = cmds.iter().position(|c| c.starts_with("ARQCALL"))
        .expect("ARDOP b2f exchange must send ARQCALL");
    let b2f_idx = cmds.iter().position(|c| c == "B2F_HANDSHAKE_FIRST_BYTE_SENTINEL")
        .expect("ARDOP b2f exchange must send B2F handshake bytes");
    assert!(arqcall_idx < b2f_idx, "ARQCALL must precede B2F handshake (Codex Round 1 P1 #1)");
}
```

If the captured-cmd-sequence stub doesn't exist, factor a small helper out of the existing modem-stub harness so commands sent through the transport's writer are recorded in order. The B2F sentinel is whatever marker the test stub uses to identify B2F vs cmd-port bytes.

- [ ] **Step 2: Write the failing test — exchange filters mailbox by intent's routing flag**

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

- [ ] **Step 3: Run tests to verify they fail**

```bash
cargo --manifest-path src-tauri/Cargo.toml test modem_ardop_b2f_exchange
```

Expected: FAIL — current code doesn't `connect_arq` first, and signature doesn't take `intent`.

- [ ] **Step 4: Add `connect_arq` step + `intent` parameter**

```rust
#[tauri::command]
pub async fn modem_ardop_b2f_exchange(
    app: AppHandle,
    session: State<'_, Arc<ModemSession>>,
    target: String,
    intent: SessionIntent,
    transport_kind: TransportKind,
) -> Result<(), String> {
    // Codex Round 2 P1 #3: this command MUST preserve the open-session lifecycle.
    // Earlier draft called `transport.disconnect()` + `session.reset_to_stopped()`
    // after the exchange — which closes the Open Session window and disarms the
    // listener. The spec (§2 "Outbound dial is within-session") requires return
    // to `open · idle` so the operator can retry / continue with the listener
    // still armed. Take the transport via the arbiter; on success OR failure,
    // return it via the arbiter — do NOT call reset_to_stopped.
    let mut transport = session.take_transport_for_outbound().await
        .map_err(|e| format!("ARDOP cannot take transport for outbound: {e}"))?;

    // Bring the ARQ link up. Connect button = Connect + B2F + DISCONNECT-the-link
    // (NOT Disconnect-the-session). Codex Round 1 P1 #1: ardop_open_session does
    // not call connect_arq, so we must do it here.
    //
    // No tuxlink wall-clock cap on ARQCALL (operator decision bd tuxlink-qtgg).
    // Pass `None` (Codex Round 2 P1 #2 — Duration::MAX overflows recv_timeout):
    let outcome = (|| async {
        if let Err(e) = transport.connect_arq(&target, CONNECT_REPEAT, None).await {
            return Err(format!("ARDOP connect failed: {e}"));
        }
        let b2f = run_b2f_with_transport(&app, &mut *transport, &target, intent).await;
        // ARQ link disconnect — this is the link-level DISCONNECT, NOT the session
        // tear-down. The transport stays usable for the next outbound dial.
        let _ = transport.disconnect_arq_link(Duration::from_secs(5)).await;
        b2f
    })().await;

    // Return transport to the session/arbiter. This is the load-bearing change:
    // session stays in `Open`; listener arbiter re-arms; operator can retry.
    session.return_transport_from_outbound(transport);

    outcome
}

async fn run_b2f_with_transport(
    app: &AppHandle,
    transport: &mut dyn ModemTransport,
    target: &str,
    intent: SessionIntent,
) -> Result<(), String> {
    // Existing body — pass intent through to the mailbox-drain filter via
    // intent.routing_flag().
    crate::winlink_backend::run_ardop_b2f_exchange(
        transport, target, intent, &cfg, &mailbox, Some(&arbiter),
    ).await.map_err(|e| format!("ARDOP B2F exchange failed: {e}"))
}
```

**Failure-path note (Codex Round 2 P1 #3 amended):** the session remains in `Open` state on BOTH success and failure of the exchange. The arbiter receives the transport back via `return_transport_from_outbound`, re-arms the listener consumer task (if intent auto-arms), and the operator can retry Connect or click Close Session. The transport stays alive (TCP cmd-port still bound; ardopcf still spawned). Only the ARQ LINK is torn down via `disconnect_arq_link` — NOT the whole session. If `disconnect_arq_link` fails, surface as a warning but still return the transport (we'd rather have a flaky link state than a leaked transport handle).

**Disconnect-vs-disconnect-arq distinction.** `transport.disconnect(...)` previously did "close the TCP cmd-port and end the session." This task introduces `disconnect_arq_link` as the inner-link teardown (sends `DISCONNECT` on the cmd-port to release the keyed link, but keeps the cmd-port alive). If the codebase's existing `ModemTransport` trait doesn't expose a link-only-disconnect, add it as part of this task. Naming: `disconnect_arq_link` for the new method (link-level); leave `disconnect` for the full transport tear-down (which is now called by `ardop_close_session` ONLY).

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo --manifest-path src-tauri/Cargo.toml test modem_ardop_b2f_exchange
```

Expected: PASS for both tests.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(ardop): connect_arq inside modem_ardop_b2f_exchange + intent parameter

Codex Round 1 P1 #1: ardop_open_session (Task 3.5) spawns ardopcf only;
the Connect button's command must bring the ARQ link up itself, run B2F,
and disconnect. Earlier draft would have run B2F over an unconnected
stream after Phase 3 landed.

Also widens the command to accept SessionIntent so the mailbox drain
filter routes by spec §3 capability matrix (Cms→C, P2p→none, RadioOnly→R).

Operator decision bd tuxlink-qtgg + Codex Round 1 P1 #3: no tuxlink
wall-clock cap on the connect_arq step — bound is ardopcf's ARQTIMEOUT
plus operator ABORT.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

---

## Phase 4 — VARA ABORT side-channel + backend session arbiter (tuxlink-12sc + Codex P1 #5)

**Phase contract:** by the end of Phase 4, the backend can (a) interrupt a VARA exchange within ~2s via `ABORT\r` (tuxlink-12sc / Codex P1 #4) and (b) atomically arbitrate transport ownership between the auto-armed listener consumer task and the outbound dial command (Codex P1 #5). Both are preconditions for Phase 5's shared panel — without (b) the auto-arm-vs-outbound race makes the shared panel architecturally undefined.

### Task 4.1: Add `try_clone_abort_writer` + `install_abort_writer` for VARA (sends `ABORT\r`, not `DISCONNECT\r`)

**Files:**
- Modify: `src-tauri/src/winlink/modem/vara/transport.rs` — add abort-writer trait method
- Modify: `src-tauri/src/winlink/modem/vara/session.rs` (or wherever `VaraSession` lives) — add `install_abort_writer` + `abort_in_flight` mirroring ARDOP's `ModemSession`

**Rationale per spec §9 watched failure mode + Codex Round 1 P1 #4:** Without this, operator's Close Session click could take 30s+ to interrupt an active B2F (VARA's natural timeout). The ABORT side-channel makes it immediate (~2s per spec §2 contract).

**An earlier draft of this task wired the abort writer as `DISCONNECT\r`** on the cmd port, on the (wrong) assumption that VARA's cmd protocol has no ABORT word. **Codex Round 1 corrected this:** the VARA command codec at `src-tauri/src/winlink/modem/vara/command.rs:62-65` ALREADY models `Command::Abort` (`"ABORT"`) distinct from `Command::Disconnect` (`"DISCONNECT"`). The VARA HF/FM spec treats them differently — `ABORT` is hard tear-down (interrupts in-flight TX); `DISCONNECT` is graceful (waits for the current burst, can be slow on weak signal modes). The "must interrupt within ~2s" spec requirement is met by `ABORT`, not by `DISCONNECT`.

**Verify before writing tests:**

```bash
# Run from the active worktree root.
grep -n "Abort\|ABORT" src-tauri/src/winlink/modem/vara/command.rs | head
```

Expected: `Abort` variant + `Self::Abort => "ABORT".into()` line + `"ABORT" => Self::Abort` parser line.

- [ ] **Step 1: Write the failing test — abort_in_flight sends `ABORT\r` first on cmd port**

```rust
#[test]
fn vara_abort_in_flight_writes_abort_as_first_command() {
    let session = VaraSession::new();
    let (writer, captured) = test_cmd_writer();
    session.install_abort_writer(Box::new(writer));

    session.abort_in_flight().expect("abort writes succeed");

    let bytes = captured.lock().unwrap().clone();
    // ABORT\r must be the first command on the wire.
    assert!(
        bytes.starts_with(b"ABORT\r"),
        "Codex Round 1 P1 #4: ABORT must be sent FIRST (got {:?}). DISCONNECT can wait for the current burst.",
        String::from_utf8_lossy(&bytes),
    );
    // DISCONNECT may optionally follow to release the slot cleanly; that's fine,
    // but if it appears it must be AFTER ABORT, not before.
    if let Some(disc_idx) = bytes.windows(b"DISCONNECT\r".len()).position(|w| w == b"DISCONNECT\r") {
        let abort_idx = bytes.windows(b"ABORT\r".len()).position(|w| w == b"ABORT\r").unwrap();
        assert!(abort_idx < disc_idx, "ABORT must precede any DISCONNECT");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo --manifest-path src-tauri/Cargo.toml test vara_abort_in_flight_writes_abort_as_first_command
```

Expected: FAIL — methods don't exist.

- [ ] **Step 3: Implement the methods (send `ABORT\r`, optionally followed by `DISCONNECT\r`)**

In `src-tauri/src/winlink/modem/vara/session.rs`:

**Codex Round 3 P1 #1 amendment** (load-bearing): the synchronous `write_all` + `flush` path below can block Close Session if the VARA process is alive but no longer draining the cmd socket. The spec's ~2s interrupt guarantee is NOT met if `write_all` blocks for the OS TCP retransmission timeout (typically tens of seconds). Two fixes — apply BOTH:

1. **Set a write deadline on the cloned cmd-port writer.** When `try_clone_abort_writer` returns the writer, call `TcpStream::set_write_timeout(Some(Duration::from_millis(1500)))` on it before boxing. This makes `write_all` return `Err(WouldBlock)` instead of blocking past the deadline.

2. **`abort_in_flight` interprets the write error as "modem unresponsive"** — still hard-close the underlying TCP connection in that case (a non-graceful socket close forces the modem to notice and stop TX even when it's not draining the cmd channel).

```rust
pub struct VaraSession {
    // ... existing fields ...
    abort_writer: Mutex<Option<Box<dyn Write + Send>>>,
    // Codex Round 3 P1 #1: keep a separate handle for the hard-close fallback path.
    abort_stream: Mutex<Option<Box<dyn ShutdownableStream + Send>>>,
}

impl VaraSession {
    /// `writer` MUST have a write_timeout (≤1500ms) set so this stays bounded.
    /// `stream` is the underlying TcpStream-like handle used for the hard-close
    /// fallback when the cooperative write fails.
    pub fn install_abort_writer(
        &self,
        writer: Box<dyn Write + Send>,
        stream: Box<dyn ShutdownableStream + Send>,
    ) {
        *self.abort_writer.lock().unwrap() = Some(writer);
        *self.abort_stream.lock().unwrap() = Some(stream);
    }

    /// Hard-tear-down the current VARA ARQ link. Sends `ABORT\r` first.
    /// If the cooperative write fails (modem not draining cmd port), falls
    /// back to a hard TCP shutdown so the modem notices and stops TX.
    /// Codex Round 1 P1 #4 (ABORT cmd) + Codex Round 3 P1 #1 (bounded write).
    pub fn abort_in_flight(&self) -> Result<(), String> {
        // Phase 1: cooperative ABORT via the writer (bounded by its write_timeout).
        let cooperative = {
            let mut guard = self.abort_writer.lock().unwrap();
            let writer = guard.as_mut().ok_or("no abort writer installed")?;
            writer.write_all(b"ABORT\r")
                .and_then(|_| {
                    // Best-effort follow-up DISCONNECT for clean slot release.
                    let _ = writer.write_all(b"DISCONNECT\r");
                    writer.flush()
                })
        };

        match cooperative {
            Ok(()) => Ok(()),
            Err(_) => {
                // Codex Round 3 P1 #1: cooperative path failed — modem is wedged
                // or not draining cmd. Hard-close the underlying stream to force
                // the modem to notice (TCP RST → modem aborts TX on its end).
                let mut guard = self.abort_stream.lock().unwrap();
                if let Some(stream) = guard.as_mut() {
                    let _ = stream.shutdown_both();
                }
                Err("VARA cmd port unresponsive; hard-closed".into())
            }
        }
    }
}

/// Trait the VARA transport's cmd-port stream implements so the session can
/// hard-close it from the abort path without holding the full TcpStream type.
pub trait ShutdownableStream {
    fn shutdown_both(&mut self) -> std::io::Result<()>;
}
```

**Tests for the failure path** (REQUIRED — extends Step 1's test list):

```rust
#[test]
fn vara_abort_in_flight_completes_within_2s_when_peer_does_not_drain() {
    // Set up a TCP listener that ACCEPTs but never recv()s — simulates the
    // wedged VARA process case.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let _peer = std::thread::spawn(move || {
        let (_s, _) = listener.accept().unwrap();
        std::thread::sleep(Duration::from_secs(30));  // never drain
    });
    let stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.set_write_timeout(Some(Duration::from_millis(1500))).unwrap();
    let writer = Box::new(stream.try_clone().unwrap()) as Box<dyn Write + Send>;
    let shutdown = Box::new(stream) as Box<dyn ShutdownableStream + Send>;
    let session = VaraSession::new();
    session.install_abort_writer(writer, shutdown);

    let start = std::time::Instant::now();
    let _ = session.abort_in_flight();  // Cooperative write times out → hard-close fallback.
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(2),
            "abort_in_flight took {:?} — Codex Round 3 P1 #1 bound is 2s",
            elapsed);
}
```

In `src-tauri/src/winlink/modem/vara/transport.rs`, add `try_clone_abort_writer` that:
1. Calls `TcpStream::try_clone()` to clone the cmd-port stream.
2. Calls `set_write_timeout(Some(Duration::from_millis(1500)))` on the clone.
3. Returns both the boxed writer AND the shutdown handle (or a tuple) for `install_abort_writer`.

Mirror ARDOP's pattern; ARDOP's abort path also needs the same write-deadline treatment as part of this task's amendment (Codex Round 3 P1 #1 doesn't only affect VARA — both protocols' cooperative abort writes need bounded write timeouts + hard-close fallback).

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo --manifest-path src-tauri/Cargo.toml test vara_abort_in_flight_writes_abort_as_first_command
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(vara-transport): add abort_in_flight side-channel (ABORT\\r, not DISCONNECT)

Spec §9 watched failure mode + tuxlink-12sc + Codex Round 1 P1 #4.

VARA's cmd-port codec already models Command::Abort separately from
Command::Disconnect (command.rs:62-65). ABORT is hard tear-down — halts
TX within ~2s, satisfying the spec's interrupt requirement. DISCONNECT
is graceful — waits for the current burst, can be slow on weak-signal
modes, and does NOT satisfy the spec.

An earlier draft of this task wired DISCONNECT on the assumption that
VARA had no ABORT word; that assumption was wrong and was caught by
the Codex Round 1 cross-provider review.

Implementation sends ABORT first, optionally followed by DISCONNECT to
release the slot cleanly. Test asserts ABORT precedes any DISCONNECT.

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

### Task 4.3: Backend session arbiter — serialize transport ownership between listener consumer + outbound dial

**Codex Round 1 P1 #5:** the existing ARDOP and VARA listener consumer tasks take ownership of the transport for the entire armed window via `take_transport` / `return_transport` on `ModemSession` / `VaraSession`. The spec's "listener stays armed while operator can redial outbound" model is racy without coordination: outbound either finds no transport to dial with (consumer holds it) or pulls it out from under the consumer (silently disarms the listener). Phase 5's shared panel assumes both can coexist within the armed window — that assumption is only true if the backend arbiter sequences them.

**Files:**
- Modify: `src-tauri/src/winlink/session.rs` (or wherever `ModemSession` lives) — add `take_transport_for_outbound` / `return_transport_from_outbound` pair distinct from the listener's `take_transport` / `return_transport`
- Modify: `src-tauri/src/winlink/modem/vara/session.rs` — same pair on `VaraSession`
- Modify: listener consumer tasks (`src-tauri/src/winlink/modem/vara/listener.rs`, ARDOP equivalent) to yield + reclaim the transport when the arbiter signals outbound starts/ends
- Modify: `modem_ardop_b2f_exchange` (Task 3.6) + `modem_vara_b2f_exchange` (Task 3.4) — call `take_transport_for_outbound` instead of bare `take_transport`; return via `return_transport_from_outbound`

**Arbiter contract** (the load-bearing semantics; tests assert against these):

1. At any moment, AT MOST ONE of `{listener-consumer, outbound-exchange}` owns the transport.
2. If the listener is armed but idle (no inbound in flight), outbound can request the transport: arbiter signals the listener consumer to yield; consumer yields the transport via `return_transport_to_arbiter`; arbiter hands it to outbound via `take_transport_for_outbound`; outbound completes (success or error); outbound returns the transport via `return_transport_from_outbound`; arbiter hands it back to the listener consumer; consumer re-arms and re-enters its accept loop.
3. If an inbound exchange is in flight when outbound requests the transport, the arbiter rejects outbound with a "modem busy — inbound in progress" error. The frontend disables the Connect button while inbound is in flight (Task 5.7).
4. If outbound is in flight and the operator clicks Close Session, the arbiter calls `abort_in_flight` on the outbound side (Task 4.1+4.2) before tearing down the listener; the arbiter's yield-loop unblocks once the outbound side reports done.

The arbiter does NOT need to be a separate type — it can be a small inner state machine on `ModemSession` / `VaraSession` with an enum like `enum TransportOwner { None, ListenerArmed, ListenerInbound, Outbound }` and atomic transitions guarded by the existing mutex.

- [ ] **Step 1: Write the failing test — outbound yields the transport back to the listener after success**

```rust
#[tokio::test]
async fn vara_arbiter_returns_transport_to_listener_after_outbound() {
    let state = setup_test_state();
    // Open with P2p so listener auto-arms.
    let _ = vara_open_session(state.app.clone(), state.log.clone(),
                              state.vara_session.clone(), state.listen_state.clone(),
                              SessionIntent::P2p, TransportKind::VaraHf).await;
    assert_eq!(state.vara_session.transport_owner(), TransportOwner::ListenerArmed);

    // Run an outbound exchange (against a stub that immediately returns Ok).
    let _ = modem_vara_b2f_exchange(state.app.clone(), state.vara_session.clone(),
                                    "K7LED-7".into(), SessionIntent::P2p,
                                    TransportKind::VaraHf).await;

    // Listener should be re-armed; transport should be back with the consumer.
    assert_eq!(state.vara_session.transport_owner(), TransportOwner::ListenerArmed);
    assert!(state.listen_state.is_armed());
}
```

- [ ] **Step 2: Write the failing test — outbound during inbound returns "modem busy"**

```rust
#[tokio::test]
async fn vara_arbiter_rejects_outbound_during_inbound_exchange() {
    let state = setup_test_state_with_listener_accepting();
    // Simulate an inbound exchange holding the transport.
    state.vara_session.simulate_inbound_in_progress();
    assert_eq!(state.vara_session.transport_owner(), TransportOwner::ListenerInbound);

    let result = modem_vara_b2f_exchange(state.app.clone(), state.vara_session.clone(),
                                         "K7LED-7".into(), SessionIntent::P2p,
                                         TransportKind::VaraHf).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("inbound in progress"));
}
```

- [ ] **Step 3: Run tests to verify they fail**

```bash
cargo --manifest-path src-tauri/Cargo.toml test vara_arbiter
```

Expected: FAIL — the arbiter state machine, `take_transport_for_outbound`, and `transport_owner` accessor don't exist.

- [ ] **Step 4: Implement the arbiter on `VaraSession`**

Sketch (adapt to actual VaraSession internals):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportOwner {
    None,                // session closed; no transport
    ListenerArmed,       // listener consumer holds transport, no inbound exchange in flight
    ListenerInbound,     // listener consumer holds transport, inbound exchange in flight
    Outbound,            // outbound exchange holds transport
}

impl VaraSession {
    pub fn transport_owner(&self) -> TransportOwner {
        // Read from inner state
    }

    /// Outbound request: yield from listener if needed, hand transport to outbound.
    /// Returns Err if inbound exchange is in flight.
    ///
    /// Codex Round 2 P1 #4 — **the lock MUST be dropped before awaiting the yield
    /// channel**. The earlier sketch held `guard` across the .await, which (a)
    /// blocks every other thread that needs session state — including the listener
    /// consumer task that's supposed to yield, deadlocking the system — and (b)
    /// doesn't compile with `std::sync::MutexGuard` across an await. The protocol
    /// is: snapshot state under the lock, record the yield request, drop the lock,
    /// then await the yield channel.
    pub async fn take_transport_for_outbound(&self) -> Result<VaraTransport, String> {
        // Phase 1: snapshot + record request under the lock; drop the lock before await.
        {
            let mut guard = self.inner.lock().unwrap(); // sync mutex, not tokio
            match guard.transport_owner {
                TransportOwner::None => return Err("session not open".into()),
                TransportOwner::ListenerInbound => return Err("modem busy — inbound exchange in progress".into()),
                TransportOwner::Outbound => return Err("outbound exchange already in flight".into()),
                TransportOwner::ListenerArmed => {
                    guard.transport_owner = TransportOwner::OutboundPending; // intermediate state
                    // Record the yield request; the consumer task watches this notify.
                    self.transport_yield_request.notify_one();
                }
            }
        } // lock dropped here

        // Phase 2: await the listener consumer's yield (no lock held).
        // Codex Round 3 P1 #2: bounded wait. If the listener consumer task
        // crashed, missed the notify, or is wedged in its accept loop, an
        // unbounded await leaves outbound stuck in OutboundPending forever.
        // After YIELD_TIMEOUT, assume stale consumer; force-clean and surface.
        const YIELD_TIMEOUT: Duration = Duration::from_secs(3);
        let yield_result = tokio::time::timeout(
            YIELD_TIMEOUT,
            self.transport_yield_rx.recv(),
        ).await;

        let transport = match yield_result {
            Ok(Some(t)) => t,
            Ok(None) => {
                // Channel closed — listener task is gone. Reset state, surface error.
                let mut guard = self.inner.lock().unwrap();
                guard.transport_owner = TransportOwner::None;
                return Err("listener consumer task exited; session needs Close + reopen".into());
            }
            Err(_elapsed) => {
                // Timeout — consumer wedged. Reset to None and surface so the
                // operator can Close + reopen. This is a recovery path, not a
                // normal one — log it loudly.
                let mut guard = self.inner.lock().unwrap();
                guard.transport_owner = TransportOwner::None;
                tracing::error!(
                    "arbiter yield wait timed out after {:?}; listener consumer appears wedged",
                    YIELD_TIMEOUT,
                );
                return Err(format!(
                    "modem busy — listener did not yield within {:?}; \
                     Close Session and reopen to recover",
                    YIELD_TIMEOUT,
                ));
            }
        };

        // Phase 3: finalize state under the lock; ownership transfer is atomic
        // w.r.t. other arbiter operations.
        {
            let mut guard = self.inner.lock().unwrap();
            guard.transport_owner = TransportOwner::Outbound;
        }

        Ok(transport)
    }

    /// Outbound completes: hand transport back; arbiter re-arms listener.
    pub fn return_transport_from_outbound(&self, transport: VaraTransport) {
        // Phase 1: state transition under the lock.
        {
            let mut guard = self.inner.lock().unwrap();
            guard.transport_owner = TransportOwner::ListenerArmed;
        } // drop lock before send
        // Phase 2: hand transport back to the listener consumer via its inner channel.
        let _ = self.transport_return_tx.send(transport);
    }
}
```

(The `TransportOwner::OutboundPending` intermediate state lets Close Session distinguish "outbound has the transport" from "outbound is waiting for it" — useful when the operator clicks Close during yield-await.)

The listener consumer task: blocks on `transport_yield_request.notified()` while holding the transport in its accept loop. When notified, it sends its held transport back through `transport_yield_rx` and awaits a fresh transport on `transport_return_rx`. When the fresh transport arrives, it re-enters its accept loop.

ARDOP mirrors the same pattern on `ModemSession`. **Both implementations + their tests are required to complete Task 4.3** (Codex Round 2 P2 — the earlier verification command only ran `vara_arbiter` / `vara` tests; ARDOP equivalents must exist).

- [ ] **Step 5: Wire `modem_vara_b2f_exchange` (Task 3.4) + `modem_ardop_b2f_exchange` (Task 3.6) through the arbiter**

Replace bare `take_transport()` calls with `take_transport_for_outbound().await?`. Replace bare `return_transport(...)` with `return_transport_from_outbound(...)`.

- [ ] **Step 6: Run tests to verify they pass**

```bash
# VARA arbiter tests:
cargo --manifest-path src-tauri/Cargo.toml test vara_arbiter
# ARDOP arbiter tests (Codex Round 2 P2 — Task 4.3 modifies BOTH VaraSession AND
# ModemSession; tests are required for both, not just VARA):
cargo --manifest-path src-tauri/Cargo.toml test ardop_arbiter
# Plus the listener-consumer tests, the outbound-dial tests, and the
# vara_open_session / vara_close_session round-trip tests.
cargo --manifest-path src-tauri/Cargo.toml test vara
cargo --manifest-path src-tauri/Cargo.toml test modem_ardop
```

Expected: all PASS. Both VARA and ARDOP arbiter tests must exist + pass to complete Task 4.3 — Codex Round 2 P2 explicitly called out that a worker can land green VARA arbiter tests while leaving ARDOP on the old bare `take_transport` race. The ARDOP tests mirror the VARA ones (yield/return/inbound-busy rejection) on `ModemSession`.

- [ ] **Step 7: Add a watched-failure-mode integration test — close-during-outbound aborts then re-arms**

```rust
#[tokio::test]
async fn vara_close_session_during_outbound_aborts_outbound_and_disarms_listener() {
    let state = setup_test_state();
    let _ = vara_open_session(/* p2p */).await;
    // Spawn outbound; it'll block on the stub mid-B2F.
    let outbound_handle = tokio::spawn({
        let s = state.clone();
        async move { modem_vara_b2f_exchange(/* p2p */).await }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(state.vara_session.transport_owner(), TransportOwner::Outbound);

    // Close mid-outbound: arbiter must abort outbound first, then disarm listener.
    let close_start = std::time::Instant::now();
    let _ = vara_close_session(/* ... */).await;
    let close_elapsed = close_start.elapsed();

    assert!(close_elapsed < Duration::from_secs(3),
            "close took {:?} — arbiter didn't sequence abort + disarm fast enough",
            close_elapsed);
    let _ = outbound_handle.await;
    assert_eq!(state.vara_session.transport_owner(), TransportOwner::None);
    assert!(!state.listen_state.is_armed());
}
```

Run + verify pass.

- [ ] **Step 7b: Add bounded-yield failure-mode tests** (Codex Round 3 P1 #2)

```rust
#[tokio::test]
async fn arbiter_yield_times_out_when_listener_consumer_wedged() {
    let state = setup_test_state_listener_wedged_no_yield();
    // Listener "armed" but its accept loop blocks forever on a stub.
    let _ = vara_open_session(/* p2p */).await;
    assert_eq!(state.vara_session.transport_owner(), TransportOwner::ListenerArmed);

    let start = std::time::Instant::now();
    let result = modem_vara_b2f_exchange(/* p2p */).await;
    let elapsed = start.elapsed();

    assert!(result.is_err());
    assert!(elapsed >= Duration::from_secs(3) && elapsed < Duration::from_secs(5),
            "yield wait should bound to ~3s; got {:?}", elapsed);
    // After timeout, transport_owner reset to None so a clean reopen can proceed.
    assert_eq!(state.vara_session.transport_owner(), TransportOwner::None);
}

#[tokio::test]
async fn arbiter_yield_handles_listener_consumer_dropping_yield_channel() {
    let state = setup_test_state_listener_drops_channel();
    let _ = vara_open_session(/* p2p */).await;
    let result = modem_vara_b2f_exchange(/* p2p */).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("listener consumer task exited"));
    assert_eq!(state.vara_session.transport_owner(), TransportOwner::None);
}
```

Run + verify both pass. The first test stubs a listener that never yields (consumer wedged); the second drops the channel sender mid-flight (consumer task exited).

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat(vara,ardop): backend session arbiter for transport ownership

Codex Round 1 P1 #5. ListenerArmed + outbound dial both contend for the
single modem transport. Without an arbiter the spec's 'listener stays
armed while operator can redial outbound' is racy — outbound either
finds no transport (consumer holds it) or pulls it out from under the
consumer (silently disarms). Arbiter sequences:

  ListenerArmed → (yield) → Outbound → (return) → ListenerArmed

with strict mutual exclusion and an explicit 'modem busy' error when
outbound requests during an inbound exchange in flight. Close Session
during outbound aborts outbound first via ABORT side-channel, then
disarms listener.

Precondition for Phase 5 (shared panel) — without this, auto-arm +
within-session outbound dial is architecturally undefined.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
git push
```

---

## Phase 5 — Shared `RadioSessionPanel` component + adapters

### Task 5.1: Define the `RadioSessionAdapter` interface (carries `transportKind`; commands invoked with it)

**Files:**
- Create: `src/radio/modes/radioSessionAdapters.ts`
- Create: `src/radio/modes/radioSessionAdapters.test.ts`

**Codex Round 1 P2 #10:** the backend listener layer distinguishes `TransportKind::VaraHf` from `TransportKind::VaraFm` for arms / reject forensics. If the adapter sends only `{ intent }` to `vara_open_session`, VARA FM sessions are silently logged + gated as VARA HF. The adapter MUST carry a `transportKind` value and the Tauri commands MUST accept it. Tests cover both HF and FM paths.

**Codex Round 1 P2 #9:** the shared panel's lifecycle state derives from the backend status snapshot (`modem_get_status` for ARDOP, `vara_status` for VARA) via subscription, NOT from React `useState('closed')` as the source of truth. The adapter exposes the status command name and the type of the status DTO it returns; the panel reads from `useQuery`-shaped data, not local state. Local state is allowed only for transient form values + in-flight button affordances.

- [ ] **Step 1: Write the failing test — adapter interface shape, including `transportKind`**

In `src/radio/modes/radioSessionAdapters.test.ts`:

```ts
import { ardopAdapter, varaHfAdapter, varaFmAdapter, type RadioSessionAdapter } from './radioSessionAdapters';

describe('RadioSessionAdapter', () => {
  it('each adapter declares the Tauri command names', () => {
    const adapters: RadioSessionAdapter[] = [ardopAdapter, varaHfAdapter, varaFmAdapter];
    for (const a of adapters) {
      expect(a.commands.openSession).toMatch(/^(ardop|vara)_open_session$/);
      expect(a.commands.closeSession).toMatch(/^(ardop|vara)_close_session$/);
      expect(a.commands.b2fExchange).toMatch(/^modem_(ardop|vara)_b2f_exchange$/);
      expect(a.commands.status).toMatch(/^(modem_get_status|vara_status)$/);
      expect(a.commands.allowedStationsGet).toMatch(/_allowed_stations_get$/);
    }
  });

  it('each adapter declares the protocol kind it adapts', () => {
    expect(ardopAdapter.kind).toBe('ardop-hf');
    expect(varaHfAdapter.kind).toBe('vara-hf');
    expect(varaFmAdapter.kind).toBe('vara-fm');
  });

  // Codex Round 1 P2 #10 — VARA HF + FM must distinguish at the backend
  // listener layer, so the adapter MUST carry a transportKind and the
  // Tauri commands MUST accept it.
  it('VARA HF and FM adapters carry distinct transportKind values', () => {
    expect(varaHfAdapter.transportKind).toBe('vara-hf');
    expect(varaFmAdapter.transportKind).toBe('vara-fm');
    expect(ardopAdapter.transportKind).toBe('ardop');
  });

  // Sanity: VaraFmAdapter must NOT be `{ ...varaHfAdapter, kind: 'vara-fm' }`
  // because that would share transportKind too — silently logging FM as HF.
  it('varaFmAdapter is not a spread of varaHfAdapter with only kind overridden', () => {
    expect(varaFmAdapter.transportKind).not.toBe(varaHfAdapter.transportKind);
  });
});
```

Also add a backend-side test in `src-tauri/src/ui_commands.rs` tests (or wherever VARA listener arms are recorded):

```rust
#[tokio::test]
async fn vara_open_session_records_transport_kind_for_vara_fm() {
    let state = setup_test_state();
    let _ = vara_open_session(state.app.clone(), state.log.clone(),
                              state.vara_session.clone(), state.listen_state.clone(),
                              SessionIntent::P2p, TransportKind::VaraFm).await;
    let last_arm = state.listen_state.last_arm_record().unwrap();
    assert_eq!(last_arm.transport_kind, TransportKind::VaraFm,
               "Codex Round 1 P2 #10: VARA FM session must be logged as VaraFm, not VaraHf");
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
pnpm exec vitest run src/radio/modes/radioSessionAdapters.test.ts
cargo --manifest-path src-tauri/Cargo.toml test vara_open_session_records_transport_kind_for_vara_fm
```

Expected: FAIL — adapter file doesn't exist, `vara_open_session` doesn't take `transport_kind`.

- [ ] **Step 3: Define the interface + three adapter consts**

In `src/radio/modes/radioSessionAdapters.ts`:

```ts
import type { ReactNode } from 'react';
import type { RadioPanelMode } from '../types';

/** Matches the backend's `TransportKind` discriminator. Kebab-case wire format
 *  per the SessionIntent serde convention. */
export type TransportKind = 'ardop' | 'vara-hf' | 'vara-fm';

export interface RadioSessionCommands {
  openSession: string;     // 'ardop_open_session' / 'vara_open_session'
  closeSession: string;    // 'ardop_close_session' / 'vara_close_session'
  b2fExchange: string;     // 'modem_ardop_b2f_exchange' / 'modem_vara_b2f_exchange'
  /** Status command name. The shared panel subscribes to this (poll or event)
   *  and derives lifecycle state from the returned DTO — NOT from local
   *  useState (Codex Round 1 P2 #9). */
  status: string;          // 'modem_get_status' / 'vara_status'
  allowedStationsGet: string;
  allowedStationsAdd: string;
  allowedStationsRemove: string;
  allowedStationsSetAllowAll: string;
}

export interface RadioSessionAdapter {
  kind: RadioPanelMode['kind'];
  /** Sent as part of every command payload so the backend's listener layer
   *  distinguishes ARDOP / VARA HF / VARA FM (Codex Round 1 P2 #10). */
  transportKind: TransportKind;
  commands: RadioSessionCommands;
  /** Render the per-protocol "Modem settings" expander content. */
  renderSettingsExpander: () => ReactNode;
}

export const ardopAdapter: RadioSessionAdapter = {
  kind: 'ardop-hf',
  transportKind: 'ardop',
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
  renderSettingsExpander: () => null, // Task 5.6 fills from ArdopRadioPanel's Radio section
};

export const varaHfAdapter: RadioSessionAdapter = {
  kind: 'vara-hf',
  transportKind: 'vara-hf',
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

// EXPLICIT object literal, NOT a spread of varaHfAdapter — Codex Round 1 P2 #10:
// a spread that overrides only `kind` would silently share transportKind, causing
// VARA FM sessions to be logged + gated as VARA HF at the backend listener layer.
export const varaFmAdapter: RadioSessionAdapter = {
  kind: 'vara-fm',
  transportKind: 'vara-fm',
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
  renderSettingsExpander: () => null,
};
```

**On the backend side**, widen `vara_open_session` / `vara_close_session` / `modem_vara_b2f_exchange` to accept `transport_kind: TransportKind` (matching the existing Rust enum at `src-tauri/src/connections/...`). Thread it to the listener-record write path so VARA FM is distinguishable in arm logs + reject forensics. Tasks 3.2 / 3.3 / 3.4 / 3.5 / 3.6 above need amendment — when working those tasks, include the `transport_kind` parameter even though Task 5.1 is where the surface-level shared-panel work lives. Tests there must cover both HF and FM.

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

### Task 5.2: Build `RadioSessionPanel` shell (header + Open/Close button + session-log; backend-status-driven lifecycle)

**Files:**
- Create: `src/radio/modes/RadioSessionPanel.tsx`
- Create: `src/radio/modes/RadioSessionPanel.test.tsx`
- Create: `src/radio/modes/RadioSessionPanel.css`

**Codex Round 1 P2 #9 amendment** (load-bearing):

The earlier sketch below stored `lifecycle` purely in React `useState`. Local state is NOT the source of truth — derive lifecycle from the backend status snapshot the adapter exposes (`adapter.commands.status`).

Pattern:

- A `useRadioSessionLifecycle(adapter, mode)` hook polls or subscribes to the backend status command on a tight interval (e.g. 500ms while UI is mounted, or events if the backend emits them). It derives `lifecycle: SessionLifecycleState` from the DTO fields (transport-open vs closed, listener armed vs not, exchange in flight vs not, last error).
- The component renders from the hook's output. Open/Close buttons disable while `lifecycle === 'opening' | 'closing'`. Local `useState` is allowed ONLY for transient affordances (pending-spinner ticks, form field values, error banner dismissal) — never for "is the session open".
- Hot reload, window remount, ardopcf crash, VARA socket drop, rapid Open/Close all recover correctly because the hook re-reads truth from the backend on next tick.
- The hook MUST handle status-call errors: if the backend status command returns an error, that's an `'error'` lifecycle state with the error text exposed via `lastError`. If the call itself fails (Tauri IPC disconnect), surface as a `'crash-recovery'` state distinct from `'error'`.

Add `'opening' | 'closing' | 'crash-recovery'` as lifecycle states; the sketch below already has the first two but not the third.

The local-state sketch in Step 3 below is RETAINED for reference but the worker MUST replace `useState<SessionLifecycleState>` with the `useRadioSessionLifecycle` hook before commit. A passing test for "lifecycle recovers from a simulated backend crash" is required in Step 1.

- [ ] **Step 1: Write the failing test — Open Session button renders and invokes the adapter's openSession command (with `transportKind`)**

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
    args: { intent: 'p2p', transportKind: 'vara-hf' },  // Codex P2 #10 — transportKind required
  });
});

it('lifecycle recovers from backend status crash by reading next snapshot', async () => {
  // Simulate: status call returns crashed → recovered.
  let statusCallCount = 0;
  vi.mocked(invoke).mockImplementation(async (cmd) => {
    if (cmd === 'vara_status') {
      statusCallCount += 1;
      if (statusCallCount <= 2) throw new Error('socket closed');
      // Codex Round 3 P1 #5: production VARA status wire format is camelCase
      // (Task 3.0 specifies `#[serde(rename_all = "camelCase")]` on VaraStatus).
      // The mock MUST match the production wire shape or the hook will pass
      // tests while reading the wrong field name in production.
      return { state: 'open', listenerArmed: true, exchange: null, transportOwner: 'listenerArmed' };
    }
    return null;
  });

  render(
    <RadioSessionPanel
      mode={{ kind: 'vara-hf', intent: 'p2p' }}
      adapter={varaHfAdapter}
      onClose={() => {}}
    />
  );

  // First two ticks: crash-recovery; third: open-idle.
  await screen.findByText(/recovering|reconnecting/i);
  await screen.findByTestId('lifecycle-open-idle', undefined, { timeout: 2000 });
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
