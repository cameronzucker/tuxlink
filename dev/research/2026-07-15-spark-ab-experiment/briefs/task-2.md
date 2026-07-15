# Worker brief — journal `StateChanged` step/rig enrichment (one task of three)

You are implementing ONE task of a three-task feature in the tuxlink repository.
Your working directory is the repository root of a dedicated worktree; work only
there.

## Repo context (all you need; do not explore beyond it unless a step says to)

- Tuxlink is a Tauri 2.x Linux desktop app: Rust backend under `src-tauri/`,
  React 18 + TypeScript frontend under `src/` (Vite, vitest).
- `src-tauri/tuxlink-routines/` is a self-contained Rust engine crate (its own
  Cargo.toml; serde + tokio only). It journals every run state transition to a
  JSONL write-ahead file (`src/journal.rs`) from the executor
  (`src/executor.rs`).
- The frontend run monitor (`src/routines/designer/RunsTab.tsx`) renders that
  journal as a Gantt; `src/routines/routinesApi.ts` mirrors the wire types.
- The feature (bd tuxlink-xvd1i): `state_changed` journal entries gain optional
  `step`/`rig` context so the monitor attributes parked intervals exactly
  instead of via an adjacent-step heuristic. Old journals must keep parsing and
  rendering exactly as before.

## Global constraints (binding)

- MSRV is Rust 1.75; clippy denies `incompatible_msrv` — no APIs stabilized in
  1.76+ (e.g. `Result::inspect_err` is banned).
- CI runs `cargo clippy --all-targets -- -D warnings`: arm against common traps
  — inline format args (`format!("{x}")`, not `format!("{}", x)`), no needless
  `.clone()`/borrows, `matches!` for single-pattern booleans.
- No new dependencies. No files touched beyond the task's **Files** list.
- Back-compat both directions is a hard requirement (the task's tests pin it).
- Comments: match the surrounding density and voice — this codebase writes
  load-bearing doc comments; write yours to state constraints, not narration.
- TDD: execute the steps IN ORDER, run every listed command, and capture real
  output. Do not skip the "verify it fails" steps.
- Do NOT run any `git` command. Do not commit. The orchestrator commits.

## Completion report (your final message)

1. Files touched (paths).
2. Test names added/modified.
3. Verbatim final output lines of every gate command the task lists.
4. Any deviation from the brief, with the reason (deviating without reporting
   is a defect).

---
### Task 2: Executor emission — populate `step`/`rig` at the four sites

**Files:**
- Modify: `src-tauri/tuxlink-routines/src/executor.rs:240-271` (consent park), `:458-474` (delay)
- Test: `src-tauri/tuxlink-routines/src/executor.rs` (tests module — extend `delay_step_sleeps_virtual_time_and_journals_waiting` at line ~1050, extend `park_time_is_not_charged_against_the_step_timeout` at line ~1378, add one new test)

**Interfaces:**
- Consumes: Task 1's `RunEvent::StateChanged { state, step: Option<StepId>, rig: Option<String> }`.
- Produces: journal semantics Task 3 relies on —
  - consent park: `AwaitingConsent { step: Some(<transmit step id>), rig: <verbatim "rig" param when present> }`, resume `Running { step: Some(<same id>), rig: None }`;
  - delay: `Waiting { step: Some(<delay control step id>), rig: None }`, resume `Running { step: Some(<same id>), rig: None }`.

- [ ] **Step 1: Extend the two existing tests to assert the context fields (failing first).**

In `delay_step_sleeps_virtual_time_and_journals_waiting` (line ~1050), replace the two trailing `assert!(matches!(...))` blocks with exact-equality asserts:

```rust
        let entries = read_journal(&jpath).unwrap();
        assert_eq!(
            entries[0].event,
            RunEvent::StateChanged {
                state: RunState::Waiting,
                step: Some(StepId("d1".into())),
                rig: None,
            },
            "the Waiting entry must name the delay control step"
        );
        assert_eq!(
            entries[1].event,
            RunEvent::StateChanged {
                state: RunState::Running,
                step: Some(StepId("d1".into())),
                rig: None,
            },
            "the Running resume must name the same step, closing the window exactly"
        );
```

In `park_time_is_not_charged_against_the_step_timeout` (line ~1378), the fixture step's params are `json!({})` — change them to `json!({"rig": "g90"})` and replace the `states` assert (lines ~1411-1422) with:

```rust
        let states: Vec<(RunState, Option<StepId>, Option<String>)> = entries
            .iter()
            .filter_map(|e| match &e.event {
                RunEvent::StateChanged { state, step, rig } => {
                    Some((*state, step.clone(), rig.clone()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            states,
            vec![
                (
                    RunState::AwaitingConsent,
                    Some(StepId("s1".into())),
                    Some("g90".into())
                ),
                (RunState::Running, Some(StepId("s1".into())), None),
            ],
            "the park must name the transmit step and its verbatim rig param; \
             the resume names the step only"
        );
```

(Keep that test's trailing `kinds` ordering assert as-is — its patterns already
end with `..` after Task 1.)

- [ ] **Step 2: Add one new test — no `"rig"` param means `rig: None` (no fabrication):**

```rust
    /// tuxlink-xvd1i: a transmit step whose params carry no "rig" key journals
    /// rig: None — the engine records verbatim data only; the "default" rig is
    /// an app/frontend display concern (`rig_id_from_params`), never invented
    /// by the executor.
    #[tokio::test(start_paused = true)]
    async fn consent_park_without_rig_param_journals_rig_none() {
        let tx = Arc::new(
            FakeAction::new("radio.tx")
                .with_capabilities(true, true, false)
                .ok(json!({"sent": true})),
        );
        let mut reg = ActionRegistry::default();
        reg.register(tx);
        let dir = tempfile::tempdir().unwrap();
        let (mut ctx, jpath) = ctx(reg, dir.path());
        ctx.attended = true;
        ctx.consent = Some(Arc::new(crate::fakes::FakeConsent::granting_after(
            Duration::from_secs(1),
        )));
        let track = Track {
            name: "t".into(),
            steps: vec![action("s1", "radio.tx", json!({"to": "W7ABC"}))],
        };
        let mut vars = RunVars::default();
        run_track(&track, &mut vars, &ctx).await.unwrap();

        let entries = read_journal(&jpath).unwrap();
        let parked = entries
            .iter()
            .find_map(|e| match &e.event {
                RunEvent::StateChanged {
                    state: RunState::AwaitingConsent,
                    step,
                    rig,
                } => Some((step.clone(), rig.clone())),
                _ => None,
            })
            .expect("an AwaitingConsent entry must exist");
        assert_eq!(parked, (Some(StepId("s1".into())), None));
    }
```

- [ ] **Step 3: Run the three tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/tuxlink-routines/Cargo.toml delay_step_sleeps 2>&1 | tail -5` (and the other two by name)
Expected: FAIL — journaled entries carry `step: None` (Task 1's mechanical placeholders).

- [ ] **Step 4: Populate the consent-park site.** In `run_action_step_shared` (executor.rs:240-271), the resolved params and the step are both in scope. Replace the consent block's two journal calls:

```rust
    if ctx.attended && action.descriptor().transmits {
        if let Some(consent) = &ctx.consent {
            // Verbatim context for the monitor (tuxlink-xvd1i): the step that
            // is parking, and its literal "rig" param when it has one. No
            // defaulting here — the engine has no rig semantics (app layer's
            // `rig_id_from_params` and the frontend own the "default"
            // fallback); absent means absent.
            let rig = resolved
                .get("rig")
                .and_then(|v| v.as_str())
                .map(String::from);
            journal(
                ctx,
                RunEvent::StateChanged {
                    state: RunState::AwaitingConsent,
                    step: Some(step.id.clone()),
                    rig,
                },
            );
            let parked = tokio::select! {
                r = consent.park(&ctx.run_id, &step.id.0) => r,
                _ = ctx.cancel.cancelled() => Err(StepError::Cancelled),
            };
            match parked {
                Ok(()) => journal(
                    ctx,
                    RunEvent::StateChanged {
                        state: RunState::Running,
                        step: Some(step.id.clone()),
                        rig: None,
                    },
                ),
```

(The `Err` arm of `parked` is unchanged.)

- [ ] **Step 5: Populate the delay site.** In the `Control::Delay` arm (executor.rs:458-474):

```rust
                        journal(
                            ctx,
                            RunEvent::StateChanged {
                                state: RunState::Waiting,
                                // The delay control step's own id (control
                                // steps have ids and appear in the snapshot's
                                // tracks) — this is what lets the monitor draw
                                // the delay bar on an exact lane instead of
                                // attributing it to an adjacent step
                                // (tuxlink-xvd1i).
                                step: Some(c.id.clone()),
                                rig: None,
                            },
                        );
                        tokio::select! {
                            _ = tokio::time::sleep(dur) => {}
                            _ = ctx.cancel.cancelled() => return Err(StepError::Cancelled),
                        }
                        journal(
                            ctx,
                            RunEvent::StateChanged {
                                state: RunState::Running,
                                step: Some(c.id.clone()),
                                rig: None,
                            },
                        );
```

- [ ] **Step 6: Run the full crate test suite**

Run: `cargo test --manifest-path src-tauri/tuxlink-routines/Cargo.toml 2>&1 | tail -5`
Expected: PASS — the three Step-1/2 tests now pass; nothing else regressed.

- [ ] **Step 7: Report completion** (files touched, test names, suite summary). Do not commit.

