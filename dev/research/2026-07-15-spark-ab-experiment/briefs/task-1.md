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

