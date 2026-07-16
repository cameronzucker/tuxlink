# Blind eval findings — candidate-1

## Findings

### (1) Wire-format back-compat

No findings.

### (2) Attribution Logic

- **P2 - `src/routines/designer/RunsTab.tsx:160`**: `ganttModel` still tracks only one parked window globally. In a multi-track run, if delay `d1` parks, then delay `d2` parks before `d1` resumes, the second `state_changed` overwrites `parked`; when `d1` emits `running`, `closeParked` closes a bar for `d2` at the wrong time and drops `d1` entirely. Exact `step` context should allow per-step parked windows, but the implementation neither stores them per step nor matches the closing `running.step`.

- **P2 - `src/routines/designer/RunsTab.tsx:285`**: `radioAwaitRig` returns the first backward `state_changed.rig` regardless of state, so an older `awaiting_consent` rig can be treated as the exact rig for a current `awaiting_radio` wait. The fallback below also keeps scanning past a newer rig-less `step_intent`, so a later default-rig radio wait can display a stale earlier rig instead of `"default"`.

### (3) Executor Emission Context

- **P3 - `src/routines/designer/RunsTab.test.tsx:325`**: The frontend exact-rig test fabricates an `awaiting_radio` entry with `rig`, but this diff only enriches executor emissions for `AwaitingConsent` and delay `Waiting`. If the real radio-arbiter wait still emits legacy `state_changed { state: awaiting_radio }`, the production exact path is never exercised and the monitor silently falls back to the heuristic.

### (4) Test Gaps

The P2/P3 findings above are also test gaps: no overlapping exact parked-window case, no stale-rig/default fallback case, and no executor-produced `AwaitingRadio` journal fixture. No additional test-gap findings.

### (5) MSRV 1.75 / Clippy

tokens used
86,479












# Blind eval findings — candidate-2

**Findings**

**(1) Wire-Format Back-Compat**
No findings. The Rust fields are optional with serde defaults and skipped when `None`; the TS type is additive.

**(2) Attribution Logic**
- **P2** `src/routines/designer/RunsTab.tsx:287` - `radioAwaitRig` returns the first `rig` on any prior `state_changed`, regardless of whether that entry is the current `awaiting_radio` park. In a run with an earlier enriched consent park on rig `g90`, followed by a later `awaiting_radio` state that lacks `rig` but whose nearest `step_intent` targets `ft710`, the scan will return stale `g90` before reaching the legacy fallback. The exact path needs to be scoped to the relevant/current parked entry, not any historical state change.

**(3) Executor Emission Context**
No findings in the changed executor emissions visible in this diff. Consent parks name the action step and verbatim rig, resumes name the same step with no rig, and delay parks/resumes name the delay control step.

**(4) Test Gaps**
- **P3** `src/routines/designer/RunsTab.test.tsx:223` - The frontend exact-step coverage only exercises a delay park, where there is no open `step_intent`. It does not cover the consent/radio exact path where `openSnapshot.get(exactStep)` should attach the intent/action and suppress heuristic duplication. A regression could drop `intentEntry`, lose the action label, or double-render a consent bar without these tests failing.
- **P3** `src/routines/designer/RunsTab.test.tsx:289` - The `radioAwaitRig` tests do not cover stale exact `rig` entries before the current park. They prove a current exact rig wins and a rig-less state can fall back to an intent, but not that an older `state_changed.rig` must not override the current awaiting-radio fallback.

**(5) MSRV 1.75 / Clippy**
No findings visible in the diff. I did not have a full checkout here, so I could not run `cargo clippy` or the test suite.
tokens used
64,589






# Blind eval findings — candidate-3

**Findings**

- **P2** `src/routines/designer/RunsTab.tsx:284` - `radioAwaitRig` returns the nearest `rig` on any prior `state_changed`, not the rig on the current `awaiting_radio` state. In a mixed journal where an earlier enriched consent park records `rig: "g90"` after the relevant radio step intent, and the current `awaiting_radio` entry lacks `rig`, the new scan returns stale `"g90"` before reaching the legacy `step_intent` fallback that would have returned the current step’s rig.

- **P3** `src/routines/designer/RunsTab.test.tsx:293` - The `radioAwaitRig` tests cover “exact rig wins” and “rig-less state falls back,” but not stale exact rig interaction. A regression like the P2 above would pass because no test puts an older `state_changed.rig` between the current `awaiting_radio` entry and the relevant legacy `step_intent`.

- **P3** `src/routines/designer/RunsTab.test.tsx:227` - `ganttModel` exact attribution is tested only for a delay control step with no open intent. There is no frontend test for the consent path where `exactStep` should attach the open `step_intent` via `openSnapshot.get(exactStep)`, so the transmit-step exact path could lose `action`/`intentEntry` or fall back to another open step without being caught.

**No Findings**

- Wire-format back-compat: no findings. The Rust fields are additive, defaulted, and skipped when `None`; the TS type adds optional fields.
- Executor emission context: no findings in the changed emissions. Consent and delay begin/end entries carry the expected step context, with rig only on the consent parked entry.
- MSRV 1.75 / clippy `-D warnings`: no findings.
tokens used
74,915






# Blind eval findings — candidate-4

**Findings**

- **P2** `src/routines/designer/RunsTab.tsx:280`  
  `radioAwaitRig` accepts `rig` from any `state_changed` while scanning backward, not just the current/relevant `awaiting_radio` entry, and it does not use an enriched `state_changed.step` to bound the fallback. A new journal like `step_intent s1 rig=g90`, later `step_intent s2` with no rig, then `state_changed awaiting_radio step=s2` should display `"default"` for s2; this code can continue past s2 and return stale `g90` from an earlier `state_changed` or `step_intent`. That breaks the exact/fallback interaction for enriched journals while preserving the old heuristic only where no exact context exists.

- **P3** `src/routines/designer/RunsTab.test.tsx:226`  
  The frontend tests cover exact attribution for delay windows, but not exact attribution for consent windows with an open `step_intent`. That leaves the important `closeParked` branch where `openSnapshot.get(exactStep)` attaches `intentEntry` and `action` unpinned; a regression could render the consent bar on the right lane but lose the action/intent metadata or reintroduce heuristic duplication without a test failing.

- **P3** `src-tauri/tuxlink-routines/src/journal.rs:310`  
  The compatibility tests prove new readers parse old `state_changed` lines and omit `None` fields, but they do not directly model an old reader parsing a new enriched line where `step` and `rig` are unknown. Serde likely ignores them because there is no `deny_unknown_fields`, but the “either direction” requirement would be better pinned with a legacy test enum containing only `state`.

**Category Notes**

- Wire-format back-compat: no concrete runtime break found; only the P3 old-reader test gap above.
- `ganttModel` attribution: no concrete logic break found in the delay exact path or no-step legacy fallback.
- Executor emission context: no concrete wrong consent/delay context found in the shown diff.
- MSRV 1.75 / clippy `-D warnings`: no findings.
tokens used
80,742






# Blind eval findings — candidate-5

## Findings

### (1) Wire-format back-compat

No findings.

### (2) Attribution Logic

- **P2 - `src/routines/designer/RunsTab.tsx:160`**: `ganttModel` still tracks only one parked window globally. In a multi-track run, if delay `d1` parks, then delay `d2` parks before `d1` resumes, the second `state_changed` overwrites `parked`; when `d1` emits `running`, `closeParked` closes a bar for `d2` at the wrong time and drops `d1` entirely. Exact `step` context should allow per-step parked windows, but the implementation neither stores them per step nor matches the closing `running.step`.

- **P2 - `src/routines/designer/RunsTab.tsx:285`**: `radioAwaitRig` returns the first backward `state_changed.rig` regardless of state, so an older `awaiting_consent` rig can be treated as the exact rig for a current `awaiting_radio` wait. The fallback below also keeps scanning past a newer rig-less `step_intent`, so a later default-rig radio wait can display a stale earlier rig instead of `"default"`.

### (3) Executor Emission Context

- **P3 - `src/routines/designer/RunsTab.test.tsx:325`**: The frontend exact-rig test fabricates an `awaiting_radio` entry with `rig`, but this diff only enriches executor emissions for `AwaitingConsent` and delay `Waiting`. If the real radio-arbiter wait still emits legacy `state_changed { state: awaiting_radio }`, the production exact path is never exercised and the monitor silently falls back to the heuristic.

### (4) Test Gaps

The P2/P3 findings above are also test gaps: no overlapping exact parked-window case, no stale-rig/default fallback case, and no executor-produced `AwaitingRadio` journal fixture. No additional test-gap findings.

### (5) MSRV 1.75 / Clippy

tokens used
86,479












codex
I found one substantive frontend issue: the exact step is recorded but the parked-window state is still a single global slot, so concurrent tracks can close each other’s bars. I’m checking whether there are any equally concrete wire or Rust-side issues before I finalize.
exec
/bin/bash -lc 'git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-xvd1i-arm-a show 4f45fe9b:src/routines/designer/RunsTab.test.tsx | rg -n "live|parked|awaiting_consent|awaiting_radio|waiting"' in /tmp/claude-1000/-home-administrator-Code-tuxlink/6d9bafa0-3fb5-4c15-980c-8ae4b6bbc3f7/scratchpad/blind-eval
 succeeded in 0ms:
35:// state_changed -> awaiting_consent, run_finished.
87:  { ts_unix: T + 400, run_id: 'run-1', seq: 5, event: { type: 'state_changed', state: 'awaiting_consent' } },
154:  it('derives one lane per track, bars from step_intent/step_ok/step_err pairs, and a parked interval from state_changed', () => {
159:    expect(model.live).toBe(false);
173:    // The awaiting_consent state_changed anchors to the last CLOSED step
176:    const parked = bars.find((b) => b.kind === 'consent');
177:    expect(parked).toMatchObject({ stepId: 's2', parkedState: 'awaiting_consent', t0: T + 400, t1: T + 560 });
187:  it('extends t1 to `now` and sets live:true when the journal has no run_finished entry yet', () => {
188:    const liveJournal = FIXTURE_JOURNAL.slice(0, 4); // through the open s2 step_intent, no err/finish yet
189:    const model = ganttModel(liveJournal, T + 1000);
190:    expect(model.live).toBe(true);
195:    const liveJournal = FIXTURE_JOURNAL.slice(0, 4); // s1 closed ok, s2 intent open, run still live
196:    const model = ganttModel(liveJournal, T + 1000);
219:    expect(model.live).toBe(false);
227:  it('attributes a parked window to the exact step named on state_changed, not the heuristic', () => {
246:      { ts_unix: T + 3, run_id: 'run-e', seq: 3, event: { type: 'state_changed', state: 'waiting', step: 'd1' } },
271:      { ts_unix: T + 3, run_id: 'run-l', seq: 3, event: { type: 'state_changed', state: 'waiting' } },
296:      { ts_unix: T + 1, run_id: 'run-r', seq: 1, event: { type: 'state_changed', state: 'awaiting_radio', step: 's1', rig: 'g90' } },
425:  it('shows Cancel run for a live run and invokes routines_cancel with {runId}', async () => {

