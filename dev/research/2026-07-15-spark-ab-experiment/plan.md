# Journal `StateChanged` step/rig enrichment — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Experiment note (orchestrator-only, not part of any worker brief):** this plan is the
> shared artifact for bd `tuxlink-c5ckf`'s A/B experiment. Both arms execute it from
> identical per-task briefs (`briefs/task-N.md`). The vehicle is real backlog:
> bd `tuxlink-xvd1i`.

**Goal:** `RunEvent::StateChanged` journal entries carry the step (and, for parked
states, the rig) they pertain to, so the Runs monitor attributes parked intervals and
the awaiting-radio rig exactly instead of via the adjacent-step heuristic.

**Architecture:** Additive wire-format change following the crate's established
`dry_run` pattern — two `Option` fields with `#[serde(default)]` +
`skip_serializing_if`, populated at the four executor emission sites where the step is
already in scope, consumed by `ganttModel`/`radioAwaitRig` with exact-field-first and
the existing heuristic retained as the legacy-journal fallback.

**Tech Stack:** Rust (serde, tokio) in the `tuxlink-routines` engine crate; TypeScript
(React 18, vitest) in `src/routines/`.

## Global Constraints

- **MSRV 1.75** (`src-tauri/Cargo.toml` `rust-version`); clippy denies
  `incompatible_msrv` — no APIs stabilized in 1.76+ (e.g. `Result::inspect_err` is
  banned; use `map_err` with a side effect or `if let`).
- CI gates: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked
  -- -D warnings`, `cargo test --manifest-path src-tauri/Cargo.toml --locked`,
  `pnpm typecheck`, `pnpm vitest run`, `pnpm build`.
- **No new dependencies** in any Cargo.toml or package.json.
- **Back-compat is a hard requirement, both directions:** an old journal line without
  the new fields MUST parse (fields default to `None`), and a new entry with `None`
  fields MUST serialize to the exact legacy JSON shape (no `"step"`/`"rig"` keys).
  Old readers tolerate new fields (the enum does not use `deny_unknown_fields`) —
  pin that with a test.
- **Crate boundary:** `tuxlink-routines` is a generic engine crate with no rig
  semantics (`rig_id_from_params` lives in the app layer,
  `src-tauri/src/routines/actions/mod.rs:462`). The executor journals the verbatim
  `"rig"` string param when present — `Option<String>`, no defaulting, no app-layer
  imports. The frontend keeps its `'default'` display fallback.
- Comments follow the surrounding density/idiom (this codebase writes load-bearing
  module and field docs; match them).
- Rust test cycle: `cargo test --manifest-path src-tauri/tuxlink-routines/Cargo.toml
  <filter>` — the leaf crate compiles locally; the FIRST build compiles deps
  (several minutes on this machine), subsequent cycles are fast. Frontend:
  `pnpm vitest run <file>` and `pnpm typecheck` are fast.
- Workers do NOT run git commands. Code + tests + a completion report; the
  orchestrator commits.

## File Structure

| File | Responsibility | Task |
|---|---|---|
| `src-tauri/tuxlink-routines/src/journal.rs` | `RunEvent::StateChanged` gains `step`/`rig`; back-compat pins | 1 |
| `src-tauri/tuxlink-routines/src/executor.rs` | 4 emission sites populate context; compile-fix pattern sites | 1 (mechanical), 2 (populate) |
| `src/routines/routinesApi.ts` | TS wire type mirrors the enum | 3 |
| `src/routines/designer/RunsTab.tsx` | `ganttModel` exact attribution + `radioAwaitRig` exact rig; module-doc rewrite | 3 |
| `src/routines/designer/RunsTab.test.tsx` | exact-path + legacy-fallback tests | 3 |

---

### Task 1: Wire format — optional `step` + `rig` on `RunEvent::StateChanged`

**Files:**
- Modify: `src-tauri/tuxlink-routines/src/journal.rs:49-51` (enum variant), `:326-329` (test constructor)
- Modify: `src-tauri/tuxlink-routines/src/executor.rs:244-247, 255-257, 460-462, 470-472` (constructors — mechanical `None`s), `:1075-1077, 1081-1083, 1414, 1428-1429, 1458` (patterns — add `..`)
- Test: `src-tauri/tuxlink-routines/src/journal.rs` (tests module)

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: `RunEvent::StateChanged { state: RunState, step: Option<StepId>, rig: Option<String> }` — Task 2 populates the fields; Task 3 mirrors the JSON shape (`step` serializes as a bare string because `StepId` is a serde newtype, e.g. `{"type":"state_changed","state":"waiting","step":"d1"}`).

- [ ] **Step 1: Write the failing back-compat tests** — append to the `tests` module in `journal.rs`:

```rust
    #[test]
    fn state_changed_without_context_fields_parses_as_none() {
        // A pre-enrichment journal line, byte-for-byte: parsing it is the
        // back-compat contract for every .jsonl already on disk.
        let legacy = r#"{"ts_unix":1752400000,"run_id":"run-1","seq":0,"event":{"type":"state_changed","state":"waiting"}}"#;
        let entry: JournalEntry = serde_json::from_str(legacy).unwrap();
        match &entry.event {
            RunEvent::StateChanged { state, step, rig } => {
                assert_eq!(*state, RunState::Waiting);
                assert!(step.is_none());
                assert!(rig.is_none());
            }
            other => panic!("expected StateChanged, got {other:?}"),
        }
    }

    #[test]
    fn state_changed_none_fields_serialize_to_legacy_shape() {
        // skip_serializing_if keeps the None-shape byte-identical to the
        // legacy wire format — an enriched build writing un-enriched entries
        // produces journals older readers have always seen.
        let event = RunEvent::StateChanged {
            state: RunState::Waiting,
            step: None,
            rig: None,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "state_changed", "state": "waiting"})
        );
    }

    #[test]
    fn state_changed_context_fields_round_trip() {
        let event = RunEvent::StateChanged {
            state: RunState::AwaitingConsent,
            step: Some(StepId("s2".into())),
            rig: Some("g90".into()),
        };
        let json = serde_json::to_value(&event).unwrap();
        // StepId is a serde newtype: it rides the wire as a bare string,
        // matching step_intent's existing "step" field shape.
        assert_eq!(
            json,
            serde_json::json!({
                "type": "state_changed",
                "state": "awaiting_consent",
                "step": "s2",
                "rig": "g90"
            })
        );
        let back: RunEvent = serde_json::from_value(json).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn state_changed_tolerates_unknown_future_fields() {
        // Pins the absence of deny_unknown_fields: an OLD build reading a
        // journal written by a NEWER build (with fields this build doesn't
        // know) must not fail the parse.
        let future = r#"{"type":"state_changed","state":"waiting","step":"d1","some_future_field":42}"#;
        let event: RunEvent = serde_json::from_str(future).unwrap();
        match event {
            RunEvent::StateChanged { state, step, .. } => {
                assert_eq!(state, RunState::Waiting);
                assert_eq!(step, Some(StepId("d1".into())));
            }
            other => panic!("expected StateChanged, got {other:?}"),
        }
    }
```

- [ ] **Step 2: Run the new tests to verify they fail to compile** (the variant has no such fields yet)

Run: `cargo test --manifest-path src-tauri/tuxlink-routines/Cargo.toml state_changed_ 2>&1 | tail -20`
Expected: compile error — `variant RunEvent::StateChanged does not have a field named step`.

- [ ] **Step 3: Add the fields to the variant** — in `journal.rs`, replace lines 49-51:

```rust
    StateChanged {
        state: RunState,
        /// The step this transition pertains to, when the emitter has one in
        /// scope: the transmit step whose consent park begins/ends, the delay
        /// control step whose wait begins/ends. Additive (`#[serde(default)]`,
        /// same pattern as `RunStarted::dry_run`) — journals written before
        /// this field parse as `None`, and `None` serializes to the legacy
        /// shape (no key). The run monitor prefers this over its
        /// adjacent-step-intent heuristic (RunsTab.tsx `ganttModel`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<StepId>,
        /// The verbatim `"rig"` string param of the step, populated only on
        /// parked-state entries (`AwaitingConsent` now; `AwaitingRadio` when
        /// an emitter exists) and only when the resolved params carry one.
        /// The engine crate has no rig semantics — no defaulting here; the
        /// frontend supplies the `"default"` display fallback, mirroring the
        /// app layer's `rig_id_from_params`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rig: Option<String>,
    },
```

- [ ] **Step 4: Mechanically fix every construction/pattern site so the crate compiles.** Constructors get `step: None, rig: None` (Task 2 replaces the executor `None`s with real values); patterns get `..`:

In `journal.rs` tests (line ~326):

```rust
        w.append(RunEvent::StateChanged {
            state: RunState::AwaitingRadio,
            step: None,
            rig: None,
        })
```

In `executor.rs` — the four constructors (lines 244-247, 255-257, 460-462, 470-472), e.g.:

```rust
                RunEvent::StateChanged {
                    state: RunState::AwaitingConsent,
                    step: None,
                    rig: None,
                },
```

In `executor.rs` test patterns — add `..` (lines 1075-1077, 1081-1083, 1414, 1428-1429, 1458), e.g.:

```rust
            RunEvent::StateChanged { state, .. } => Some(*state),
```

```rust
            RunEvent::StateChanged { state: RunState::Waiting, .. }
```

Verify no site is missed: `grep -rn "StateChanged" src-tauri/tuxlink-routines/src/` — every constructor names all three fields, every pattern either names them or ends with `..`.

- [ ] **Step 5: Run the crate tests to verify everything passes**

Run: `cargo test --manifest-path src-tauri/tuxlink-routines/Cargo.toml 2>&1 | tail -5`
Expected: PASS, including the 4 new `state_changed_*` tests and every pre-existing test (no behavior changed — the executor still journals `None`s).

- [ ] **Step 6: Report completion** (files touched, test names added, full-crate test summary line). Do not commit — the orchestrator commits.

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

---

### Task 3: Frontend exactness — `ganttModel` + `radioAwaitRig` prefer the exact fields

**Files:**
- Modify: `src/routines/routinesApi.ts:161` (wire type)
- Modify: `src/routines/designer/RunsTab.tsx:14-63` (module doc), `:164-274` (`ganttModel`), `:281-293` (`radioAwaitRig`)
- Test: `src/routines/designer/RunsTab.test.tsx`

**Interfaces:**
- Consumes: Task 2's journal semantics — `state_changed` may carry `step?: string` (bare string on the wire) and `rig?: string`; legacy journals carry neither.
- Produces: UI behavior only (no downstream task).

- [ ] **Step 1: Write the failing tests.** Add to the `ganttModel (a)` describe block in `RunsTab.test.tsx`. The existing `SNAPSHOT` has no control step — these tests use a local snapshot that includes one (control steps carry a top-level `id` on the wire, so `extractTracks` lane-maps them already):

```tsx
  it('attributes a parked window to the exact step named on state_changed, not the heuristic', () => {
    const snapshot = {
      tracks: [
        {
          name: 'net-control',
          steps: [
            { id: 's1', action: 'cat.apply_preset' },
            { id: 'd1', control: 'delay', delay: '+5m' },
            { id: 's2', action: 'radio.connect' },
          ],
        },
      ],
    };
    const journal: JournalEntry[] = [
      { ts_unix: T, run_id: 'run-e', seq: 0, event: { type: 'run_started', routine: 'r', snapshot } },
      { ts_unix: T + 1, run_id: 'run-e', seq: 1, event: { type: 'step_intent', step: 's1', action: 'cat.apply_preset', resolved_params: {} } },
      { ts_unix: T + 2, run_id: 'run-e', seq: 2, event: { type: 'step_ok', step: 's1', output: {} } },
      // Enriched delay park: names d1. The legacy heuristic would have
      // attributed this window to s1 (most recently CLOSED step).
      { ts_unix: T + 3, run_id: 'run-e', seq: 3, event: { type: 'state_changed', state: 'waiting', step: 'd1' } },
      { ts_unix: T + 303, run_id: 'run-e', seq: 4, event: { type: 'state_changed', state: 'running', step: 'd1' } },
      { ts_unix: T + 400, run_id: 'run-e', seq: 5, event: { type: 'run_finished', state: 'completed', reason: null } },
    ];
    const model = ganttModel(journal);
    const bars = model.lanes[0]!.bars;
    const delayBar = bars.find((b) => b.kind === 'delay');
    expect(delayBar).toBeDefined();
    expect(delayBar!.stepId).toBe('d1');
    expect(delayBar!.t0).toBe(T + 3);
    expect(delayBar!.t1).toBe(T + 303);
    // Exactly one bar for the window — no duplicate heuristic attribution.
    expect(bars.filter((b) => b.kind === 'delay')).toHaveLength(1);
  });

  it('falls back to the legacy heuristic when state_changed carries no step (old journals)', () => {
    const snapshot = {
      tracks: [
        { name: 'net-control', steps: [{ id: 's1', action: 'cat.apply_preset' }] },
      ],
    };
    const journal: JournalEntry[] = [
      { ts_unix: T, run_id: 'run-l', seq: 0, event: { type: 'run_started', routine: 'r', snapshot } },
      { ts_unix: T + 1, run_id: 'run-l', seq: 1, event: { type: 'step_intent', step: 's1', action: 'cat.apply_preset', resolved_params: {} } },
      { ts_unix: T + 2, run_id: 'run-l', seq: 2, event: { type: 'step_ok', step: 's1', output: {} } },
      { ts_unix: T + 3, run_id: 'run-l', seq: 3, event: { type: 'state_changed', state: 'waiting' } },
      { ts_unix: T + 63, run_id: 'run-l', seq: 4, event: { type: 'state_changed', state: 'running' } },
      { ts_unix: T + 70, run_id: 'run-l', seq: 5, event: { type: 'run_finished', state: 'completed', reason: null } },
    ];
    const model = ganttModel(journal);
    const delayBar = model.lanes[0]!.bars.find((b) => b.kind === 'delay');
    // Legacy behavior preserved verbatim: attributed to the most recently
    // closed step.
    expect(delayBar).toBeDefined();
    expect(delayBar!.stepId).toBe('s1');
  });
```

And to the `radioAwaitRig` describe block:

```tsx
  it('prefers the rig named on a state_changed entry over the adjacent-intent heuristic', () => {
    const journal: JournalEntry[] = [
      { ts_unix: T, run_id: 'run-r', seq: 0, event: { type: 'step_intent', step: 's1', action: 'radio.connect', resolved_params: { rig: 'ft710' } } },
      { ts_unix: T + 1, run_id: 'run-r', seq: 1, event: { type: 'state_changed', state: 'awaiting_radio', step: 's1', rig: 'g90' } },
    ];
    // The exact field wins over the (different) rig on the adjacent intent.
    expect(radioAwaitRig(journal)).toBe('g90');
  });

  it('a rig-less state_changed does not shadow the intent fallback', () => {
    const journal: JournalEntry[] = [
      { ts_unix: T, run_id: 'run-r2', seq: 0, event: { type: 'step_intent', step: 's1', action: 'radio.connect', resolved_params: { rig: 'ft710' } } },
      { ts_unix: T + 1, run_id: 'run-r2', seq: 1, event: { type: 'state_changed', state: 'running', step: 's1' } },
    ];
    expect(radioAwaitRig(journal)).toBe('ft710');
  });
```

- [ ] **Step 2: Run the tests to verify the new ones fail**

Run: `pnpm vitest run src/routines/designer/RunsTab.test.tsx 2>&1 | tail -15`
Expected: the two `ganttModel` tests fail (exact test: bar lands on lane via `lastClosed`, `stepId` is `'s1'` not `'d1'`; the T+3 window in the first test is attributed to s1); the first `radioAwaitRig` test fails (`'ft710'` returned). The legacy-fallback tests pass already (they pin current behavior). Type errors on `step`/`rig` in fixtures are expected until Step 3.

- [ ] **Step 3: Update the wire type.** In `routinesApi.ts`, replace line 161:

```ts
  | { type: 'state_changed'; state: RunState; step?: string; rig?: string }
```

- [ ] **Step 4: Implement exact-first attribution in `ganttModel`.** In `RunsTab.tsx`:

The `parked` accumulator (line ~177) gains the exact step:

```ts
  let parked: {
    state: RunState;
    ts: number;
    openSnapshot: Map<string, JournalEntry>;
    exactStep?: string;
  } | null = null;
```

`closeParked` (line ~184) tries the exact step FIRST, before the two heuristic branches (which remain verbatim for legacy journals):

```ts
  const closeParked = (ts: number) => {
    if (!parked) return;
    const { state, ts: t0, openSnapshot, exactStep } = parked;
    const kind: GanttBar['kind'] = state === 'waiting' ? 'delay' : 'consent';
    if (exactStep !== undefined) {
      // Enriched journal (tuxlink-xvd1i): the state_changed entry names the
      // step itself — exact attribution, no heuristic. The intent entry is
      // attached when the parked step has an open intent (a consent park);
      // a bare delay control step has none.
      const intentEntry = openSnapshot.get(exactStep);
      const fields = intentEntry ? stepIntentFields(intentEntry.event) : null;
      pushBar(
        { kind, parkedState: state, stepId: exactStep, action: fields?.action ?? undefined, t0, t1: ts, intentEntry },
        exactStep,
      );
    } else if (openSnapshot.size > 0) {
      for (const [stepId, intentEntry] of openSnapshot) {
        const fields = stepIntentFields(intentEntry.event);
        pushBar(
          { kind, parkedState: state, stepId, action: fields?.action, t0, t1: ts, intentEntry },
          stepId,
        );
      }
    } else if (lastClosed) {
      pushBar({ kind, parkedState: state, stepId: lastClosed.stepId, t0, t1: ts }, lastClosed.stepId);
    }
    parked = null;
  };
```

The `state_changed` case (line ~231) captures it:

```ts
      case 'state_changed':
        if (ev.state === 'waiting' || ev.state === 'awaiting_consent' || ev.state === 'awaiting_radio') {
          parked = { state: ev.state, ts: entry.ts_unix, openSnapshot: new Map(openByStep), exactStep: ev.step };
        } else if (parked) {
          closeParked(entry.ts_unix);
        }
        break;
```

- [ ] **Step 5: Implement exact-first rig in `radioAwaitRig`.** Replace the function body (the doc comment updates with it):

```ts
/** The rig a currently-parked `awaiting_radio` state pertains to. An enriched
 *  journal (tuxlink-xvd1i) names it on the `state_changed` entry itself —
 *  that wins. Legacy journals carry no `rig` field there, so the pre-existing
 *  fallback remains: the `rig` param off the most recent `step_intent`,
 *  mirroring the Rust side's own default (`actions/mod.rs`'s
 *  `rig_id_from_params`, `DEFAULT_RIG_ID = "default"`). */
export function radioAwaitRig(entries: JournalEntry[]): string {
  for (let i = entries.length - 1; i >= 0; i--) {
    const ev = entries[i]!.event;
    if (ev.type === 'state_changed' && typeof ev.rig === 'string') {
      return ev.rig;
    }
    if (ev.type === 'step_intent') {
      const params = ev.resolved_params;
      if (params && typeof params === 'object' && typeof (params as { rig?: unknown }).rig === 'string') {
        return (params as { rig: string }).rig;
      }
      return 'default';
    }
  }
  return 'default';
}
```

- [ ] **Step 6: Rewrite the module doc's stale claims.** The RunsTab.tsx header (lines 14-63, "what the journal ACTUALLY carries") states `StateChanged` is `{ state }` ONLY. Rewrite that section to describe the enriched reality — keep its structure and voice:

```
 * ---- ganttModel: what the journal carries (read before editing) ----
 *
 * `RunEvent::StateChanged` (journal.rs) is `{ state, step?, rig? }`:
 * `step`/`rig` are additive optional fields (tuxlink-xvd1i) an enriched
 * engine populates — the transmit step whose consent park begins/ends, the
 * delay control step whose wait begins/ends, and (parked states only) the
 * step's verbatim "rig" param. Journals written BEFORE the enrichment carry
 * neither field, and this module must render those exactly as it always has,
 * so BOTH paths below are load-bearing:
 *
 * 1. Exact path: a parked `state_changed` naming a `step` attributes the
 *    parked interval to that step's lane directly (control steps have ids
 *    and appear in the snapshot's tracks, so a delay bar lands on the delay
 *    step's own lane). `radioAwaitRig` likewise returns the `rig` named on
 *    the parked entry when present.
 *
 * 2. Legacy fallback (pre-enrichment journals): `closeParkedWindow()`'s
 *    original mechanism, verbatim — attribute to whichever step intents were
 *    OPEN when parking began; if none, to the most recently CLOSED step; if
 *    neither, drop the interval rather than invent a lane. `radioAwaitRig`
 *    falls back to the `rig` param off the most recent `step_intent`
 *    (`actions/mod.rs`'s `rig_id_from_params`, defaulting `"default"`).
```

(Preserve the module doc's third open-intent paragraph — the open-ended-bar
flush — unchanged; that behavior is untouched.)

- [ ] **Step 7: Run the frontend gates**

Run: `pnpm vitest run src/routines/designer/RunsTab.test.tsx 2>&1 | tail -8`
Expected: PASS — all new and all pre-existing tests (legacy fixtures never carry the new fields, so the fallback paths stay covered).

Run: `pnpm typecheck 2>&1 | tail -3`
Expected: clean exit.

- [ ] **Step 8: Report completion** (files touched, test names, vitest + typecheck summary lines). Do not commit.

---

## Self-Review (performed at plan-authoring time)

1. **Spec coverage** — bd `tuxlink-xvd1i` asks for: optional step/rig on `StateChanged`
   (Task 1), backend enrichment at emission sites (Task 2), exact monitor attribution
   (Task 3), wire-format versioning/back-compat minded (Task 1's four pins + Task 3's
   legacy-fallback tests). The issue's alternative design (a dedicated park-context
   event) was considered and rejected: a new event type is a bigger wire-format
   surface, needs its own back-compat story, and the `dry_run` additive-field
   precedent already establishes the cheaper pattern. `AwaitingRadio` has no emitter
   today (issue: "no v1 executor path journals AwaitingRadio"); the enrichment covers
   it structurally (same fields) without inventing an emitter — in-scope emitters are
   consent + delay only.
2. **Placeholder scan** — no TBDs; every code step carries the actual code.
3. **Type consistency** — `step` is `Option<StepId>` in Rust, bare-string `step?:
   string` on the wire (StepId is a serde newtype; Task 1 Step 3's round-trip test
   pins it), `exactStep?: string` in ganttModel. `rig` is `Option<String>` /
   `rig?: string` throughout.
