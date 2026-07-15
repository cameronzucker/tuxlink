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

