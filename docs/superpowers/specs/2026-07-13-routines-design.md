# Routines — deterministic, repeatable station automation

- **bd issue:** tuxlink-03d39
- **Status:** design approved in brainstorm (operator + agent butte-arroyo-condor, 2026-07-13); pending written-spec review
- **Naming:** the feature is **Routines**. The words "workflow" and "Workflow" do not appear in UI text, documentation, code symbols, crate/module names, or JSON schema keys.

## 1. Purpose

Routines are the deterministic power-user automation layer for Tuxlink: operators (and agents) compose repeatable station procedures from graphical action steps, save them as portable JSON, and invoke them manually, on a schedule, or from other routines. Routines reduce operator overhead and provide deterministic outcomes without relying on inference — and they let agents *author* deterministic procedures once instead of re-reasoning each run.

Grounding scenarios (operator-supplied):

1. **ICS communications log cycle** — every 2 hours at the top of the hour, attempt a station set on a band set; on contact, send the ICS log, record it, complete; on no contact, take a defined fallback action.
2. **Deployment poll loop** — every 30 minutes, attempt a station list on defined bands. Concurrently, every 6 hours, post space-weather and NWS tabular updates to the CMS through whichever gateway connects; 5 minutes later, re-dial the last-heard gateway to pull the CMS catalog response.
3. **Community prior art** — the KM4ACK/KC2HJU Winlink catalog-request bash script (900+ lines of hand-rolled B2F composition + cron): evidence the community already builds these by hand, per-station, fragile.

### Terminology

| Term | Meaning |
|---|---|
| **Routine** | The saved unit: a JSON definition of triggers, tracks, and steps |
| **Run** | One execution of a routine, journaled from start to terminal state |
| **Step** | A node on the canvas |
| **Action** | What a step does (an entry in the curated action catalog) |
| **Track** | A parallel lane of steps within a routine |
| **Library step** | A reusable saved step: a configured step or a composite step (§7) |

## 2. Scope

### In scope (v1, built whole per ADR 0022)

Engine, scheduler, radio arbiter, validator (all three layers), dry-run, run journal + export bundle, canvas designer, routines dashboard, dockable-surface pop-out capability (Routines, Tac Map, APRS Chat), MCP tool family, the full v1 action catalog (§6), routine composition (§7), library steps, JSON import/export.

### Explicit non-goals (operator-decided, recorded so they are not casually reinvented)

- **Event triggers** (on message received, on station heard, on threshold): deferred until the community demonstrates need. An event bus is a combinatorial surface with unintended-consequence risk on a feature that is already very powerful. Triggers in v1 are manual, schedule, and routine-invokes-routine.
- **Generic shell-command action:** arbitrary exec guts the validation story and is an RCE vector for agent-authored routines. Not offered.
- **Generic HTTP-request action:** every grounding job is RF/local. Internet-touching actions are typed wrappers over existing Tuxlink features (§6), not raw HTTP.
- **Run priorities and run-vs-run preemption:** first-come-first-served within arbiter policy. Priorities are how workflow engines grow unexplainable behavior; additive later if alpha demands.
- **True headless operation:** by existing design, Tuxlink's features are too powerful to run unsupervised and the transmit consent gates are GUI elements. Routines run while the app process runs (minimized to taskbar counts); closing the app pauses schedules.

## 3. Runtime posture — supervised operation

The engine lives in the Tauri backend, in-process with the transports and rig control. No daemon split, no systemd unit. This sidesteps radio-ownership IPC arbitration entirely: the engine and the transports share one process and one arbiter (§9).

- Minimized to taskbar: schedules fire, runs execute.
- App closed: schedules pause; missed fires are recorded visibly (§8).
- A GUI is always reachable to intervene; the operator always wins ties for the radio (§9).

## 4. Transmit consent model

Consent is a per-routine, design-time property, mirroring Part 97's own attended/automatic vocabulary (§97.109, §97.221):

- **`transmit_mode: "automatic"`** — transmit steps fire unattended. Selecting this surfaces a one-time, plainly-worded acknowledgment: automatic transmission under Part 97 is the licensee's responsibility (automatic-control rules, sub-band limits). The acknowledgment (callsign + timestamp) is recorded in the routine definition.
- **`transmit_mode: "attended"`** — every transmit step pauses the run (`awaiting consent`) until the operator confirms in the GUI. The routine becomes a guided sequence — useful in its own right (e.g., a net-opening checklist that tunes and composes but lets the operator key the confirm).

**The design-time acknowledgment is the consent envelope.** After acknowledgment, schedule, human click, agent call, and parent routine are equivalent invokers of an automatic routine. Operators wanting per-invocation control choose attended mode. The acknowledgment itself is operator-only: it is recorded by a UI act, and the MCP surface has no parameter to supply it (§13).

Any routine whose call-graph closure (§10) contains a transmit step is a transmitting routine and must declare a mode; an unacknowledged automatic routine is not an enableable state.

## 5. Canvas — structured flowchart with parallel lanes

The designer is a **structured, auto-laid-out flowchart** (paradigm decision: operator-selected over vertical-list and free-form-canvas alternatives). The operator inserts steps; the engine owns the geometry. Parallel tracks render as lanes. The definition contains only logic — blocks, edges, lanes — never coordinates, so:

- The same definition always renders the same way; the canvas cannot disagree with the JSON.
- Agent-authored routines render identically to hand-built ones.
- Git diffs of routine files read like code review (no coordinate noise) — deliberate, for community routine libraries shared via git.

**Chronological alignment.** Two surfaces, two precisions:

- **Design canvas:** lanes align at *declared temporal anchors* — schedule offsets, delay steps ("+5 min"), synchronization points. Cross-track dependencies (e.g., a track consuming `last_heard_gateway` set by another track) draw an alignment rule across lanes.
- **Run monitor:** timing is exact; lanes render against a true shared timeline (Gantt-like) with each step's actual start/end from the run journal.

## 6. Action catalog (v1)

Curated, typed action registry (Approach A — operator-selected over auto-generating blocks from the ~200-command Tauri surface, which would produce untyped contracts, no transmit metadata, and reference rot on internal refactors). Adding an action later = implementing the action trait + declaring its schema; the registry is the growth path toward broader coverage if alpha demands.

Every action declares capabilities: **`needs_radio`** (takes the rig lease), **`transmits`** (consent-relevant), **`needs_internet`**. The validator uses these for consent closure, capability-vs-station-profile checks, and contention analysis.

### Triggers
| Action | Notes |
|---|---|
| Manual | Run button / MCP invocation, with optional input parameters |
| Schedule | Intervals, top-of-hour/day alignment, time windows ("only 06:00–22:00"), missed-fire policy (§8) |

### Radio actions (`needs_radio: true`)
| Action | TX | Notes |
|---|---|---|
| Connect attempt | ✔ | Station set × band set in order; forwards staged outbox traffic. Outputs `connected`, `station`, `band`, `gateway`, or verbatim failure |
| Send APRS message | ✔ | Position/status/message via the APRS stack |
| Listen | — | Dwell N seconds on the target frequency; outputs `channel_busy` + detector evidence. Also available as a `listen_before_tx_s` pre-flight option on every transmitting action (same detector) |
| Read radio state | — | Freq, mode, power, meters via CAT; outputs a state object |
| Validate radio settings | — | Compare live state against a named **Radio Preset**; outputs `matches` + structured diff |
| Apply preset | — | Write counterpart: set the rig to a named Radio Preset |
| Switch VFO | — | Discrete, individually journaled |
| Tune ATU | ✔ | Keys a carrier; `transmits: true` |
| Update space weather from WWV | — | The shipped off-air decode: tune, capture at :18/:45, STT, restore. RX-only but seizes the rig |

**Radio Preset** is a new first-class named entity (frequency/mode/filter/power/ATU expectations), referenced by `@preset:` tokens and covered by reference validation. The read-state → validate → apply chain is the supported rig pre-flight pattern.

### Internet actions (`needs_internet: true`)
| Action | Notes |
|---|---|
| Update space weather (SWPC) | The online fetch path |
| Update station list | Winlink gateway status API refresh |

### Local actions
| Action | Notes |
|---|---|
| Compose message | Template + routine variables (ICS-213/309, wx tabular) |
| Compose catalog request | Stages the WL2K catalog/inquiry B2F in the outbox. *Sending* is whatever Connect attempt comes next; the response arrives on a later connection (modeled by a subsequent connect, e.g. the "+5 min re-dial") |
| Read data | Inbox, space weather, heard stations, GPS/grid |
| Set identity | Switch to a tactical call for subsequent steps. **Run-scoped**: affects later steps in this run only; never mutates the app's global identity (least-surprise; makes parallel runs with different tactical calls safe) |
| Log entry / Notify | Station log write; desktop notification |

### Control flow
| Action | Notes |
|---|---|
| Branch | Condition on a variable |
| Delay | Relative ("+5 min") or aligned ("next top of hour"). Journaled wake times, not in-memory sleeps |
| Retry | Wrap a step: attempts / backoff / until-success |
| Parallel tracks | The lanes themselves |
| Call routine | Sync (await result) or fire-and-forget, per invocation — fire-and-forget is call-without-await; provenance is identical in both modes |
| End | Explicit terminal: complete / failed, with reason |

## 7. Composition and the library

**Call is the primitive.** "Routine A calls B and uses its result" gives a call stack with intact provenance ("run 47 of B, invoked by run 12 of A, step 3"). Fire-and-forget is the same call without consuming the result — never a provenance-free side channel.

Two library concepts, both built on existing machinery:

- **Configured steps:** any single action with parameters bound and saved under a name ("Try Oregon HF gateways" = Connect attempt + saved station×band config). Editing the saved config updates every instance.
- **Composite steps:** a routine with declared inputs and outputs, flagged as a library step. Placing it on a canvas is a Call-routine step with its own name and icon — one execution mechanism (composition), two presentations. Engine, validator, and journal treat a composite step exactly as a sub-routine call.

**Runs execute a snapshot.** At run start the engine snapshots the fully resolved definition — routine + every referenced library step, preset, station set, template, and called routine, transitively — into the run journal. Editing a library entity mid-run cannot mutate an in-flight run, and an exported run bundle is self-contained: it shows what executed, not what the library says today.

## 8. Execution engine

**Run lifecycle** — explicit states only:

```
pending → running → completed | failed | cancelled | interrupted
                 ↕ waiting (delay) | awaiting consent | awaiting radio
```

Every terminal state carries evidence: `failed` names the step and the verbatim underlying error; `cancelled` names who cancelled; `interrupted` means the process died underneath the run. There is no state meaning "unknown" — the no-silent-death invariant is structural.

- **Execution:** each run is an async task executing its snapshot. Parallel lanes are concurrent futures joined at run end.
- **Step timeouts:** every step has one (per-action defaults, per-step override). A timed-out step fails with the timeout recorded; it never hangs the run. This is also the host-side backstop for wedged transports (e.g., the known ARDOP ARQTimeout-120s wedge): the engine abandons the step, journals the wedge verbatim, releases the radio lease, and the failure branch runs.
- **Persistence:** routine definitions are individual JSON files under the config directory's `routines/`; each run appends to its own journal file — every state transition and every step's resolved inputs/outputs/error, timestamped, written intent-before-effect where possible, so a hard crash leaves a truthful record.
- **Graceful quit** with active runs prompts the operator ("2 routines running — stop them and exit?") and stops them as `cancelled`.
- **Crash/power loss:** on next launch, mid-flight journals are marked `interrupted` at their last journaled step and surfaced in the run monitor.
- **`on_interrupted` policy (per routine, design time):** `"stay"` (default) — interrupted runs are never auto-resumed; the operator re-runs deliberately. `"resume"` — on next launch the run resumes from the interrupted step, executing the *journal's snapshot* (not current library state); a step with journaled intent but no result re-executes from its start (at-least-once, journaled as a re-execution). Choosing `"resume"` on an automatic-transmit routine gets the same plain-words treatment as the transmit mode: the operator is choosing "this may key the radio shortly after boot."
- **Missed-fire policy (per schedule):** `"skip"` (default) or `"run_once_on_launch"` (the anacron pattern, for the deployment Pi that rebooted overnight). Misses are recorded visibly either way.

## 9. Radio arbiter

One arbiter per radio, in the backend beside the transports. Anything seizing the rig holds the **lease** — a routine's radio step or **the human operator's interactive session, modeled as a first-class lease-holder** (clicking Connect in the UI holds the lease exactly as a run does). Every acquisition/release is journaled, so "step 4 waited 90 s: radio held by operator (interactive VARA session)" is an answerable question.

- **Contention policy per radio step:** `wait` (with timeout — default) or `fail` immediately. Either way the journal records who held the lease and for how long.
- **Human asymmetries:** a run never preempts the human (it waits or fails per policy); the operator always has a "take the radio" control that pauses the holding run at its current step boundary (`awaiting radio`).
- **Cross-run ordering:** first-come-first-served. No priorities, no run-vs-run preemption (v1 non-goals).
- **Static half:** the validator flags structurally-concurrent radio demands at design time — parallel lanes in one routine both taking the same rig (warning: they will serialize), and enabled routines whose schedules provably collide (enable-time fleet check, §10).
- Leases are per-radio: multi-rig stations run concurrent routines on different rigs with no new machinery.

Prior art: this is Tuxlink's main-checkout session-lease model applied to the rig — one exclusive resource, multiple writers, one human whose state must never be clobbered.

## 10. Validation

Design doctrine, derived from the publicly documented Laserfiche Workflow failure taxonomy (silent instance death; triggers that lie; late-binding reference rot — empty tokens resolving to their own names; a flaky validator): **defined variables never resolve to their own name, dead runs never go quiet, and failures surface at design time, not at 03:00.**

### Layer 1 — continuous static validation

Runs in the builder on every edit, and identically on any JSON arriving from outside (agent-authored, imported). **One validator, no privileged path.**

- **References resolve:** every `@`-token — station sets, radio presets, identities, templates, library steps, called routines. Rename rot dies at edit time.
- **Type contracts:** each step's inputs satisfiable from upstream outputs and routine parameters. Referencing a variable no path can have set is an error. Runtime rule: an unset variable fails the step verbatim — never resolves to its own name, never silently empty.
- **Structure:** unreachable steps, lanes without terminals, retries without exit conditions, recursive calls (A→B→A).
- **Consent consistency:** transmit steps require a declared mode; automatic requires the recorded acknowledgment. Unacknowledged auto-TX cannot be enabled.
- **Capabilities vs. station profile:** `needs_internet` steps flagged against an off-grid profile; radio steps flagged with no rig configured.
- **Radio contention:** the arbiter's static half (§9).
- **Call-graph closure:** validating routine A resolves its full transitive call graph and validates the composite as one giant routine — contracts across call boundaries (a called routine's declared outputs must exist on *every* path through it; violations surface on the caller, attributed to the callee), consent closure (transmit-ness propagates; a mixed chain — automatic A calling attended B — gets a specific warning: *the unattended 03:00 run will pause for a click nobody is present to give*; silent **stalling** is a failure class alongside silent death), capability and contention closure.

### Layer 2 — enable-time fleet check

Cross-routine facts exist only when routines go live. Any enable/disable/edit of an enabled routine re-validates **all enabled routines together**: schedule collisions on the same rig, aggregate contention windows, same-effect overlaps (two routines refreshing the station list hourly — warning). The arbiter remains the runtime safety net; the fleet check exists so the operator hears about the collision Tuesday afternoon, not during the net.

### Layer 3 — dry-run

Executes the real graph; mocks the boundary. Radio and internet actions simulate; the operator chooses each simulated outcome interactively ("connect succeeds / fails / channel busy") or picks optimistic/pessimistic presets. Variables, branches, delays (compressed), and journaling are real; the journal is stamped `dry-run`. The dry-run mocks implement the same action trait the test suite uses (§14) — one mechanism.

### Severity model

**Errors block enable and run — never save.** A draft is an honest state (blocking save is how Laserfiche makes users fight the validator). Warnings never block anything; they inform the operator, who outranks them.

## 11. Run journal and log export

- Every run writes a structured journal (JSONL): each step's timestamp, resolved inputs, outputs, and the **verbatim** underlying error — the actual VARA disconnect reason, the actual CAT timeout — never "an error occurred." A step ends in `ok(output)` or `err(verbatim cause)`; there is no third state, so journal completeness is structural.
- Machine-readable for agents; **one-click "export run bundle"** for humans filing GitHub issues: definition snapshot + run journal + engine context in one file.
- The on-screen log view shows raw content (existing project convention); **exported bundles pass through the standard redaction sinks** so credentials never land in a GitHub attachment.

## 12. UI

### Placement and dashboard

Top-level **Routines** menu in the existing menu bar (`File · Message · Session · Mailbox · Routines · View · Tools · Help`); the surface renders inline in the main pane (no forced new windows).

Landing view is **dashboard-first** (operator-selected over the mailbox master-detail idiom): the fleet as an ops table — routine, status (`running #47` / `enabled` / `draft · 2 errors`), trigger, last result, next fire, TX mode — with fleet-check warnings in its status bar. Rationale: routines are *operated* daily and *designed* occasionally; the landing surface answers "what is my station doing / did 03:00 fire / why did Tuesday fail" at a glance.

Double-clicking a routine opens the full-pane **designer** with three tabs: **Design** (the canvas, §5, with the always-on validation status bar), **Runs** (journal browser + chronological run monitor), **Settings** (transmit mode + acknowledgment, `on_interrupted`, schedules, missed-fire policy).

### Dockable surfaces (shell capability)

A generic dock/pop-out mechanism, shipped with three wired surfaces: **Routines, Tac Map, APRS Chat** (framework + consumers are one feature, per the completeness doctrine). Supports the second-screen station; the single-laptop bag deployment remains the default.

- **↗ Pop out** in the surface header moves the surface to its own OS window; **⇤ Dock back** in that window's title bar (or closing it) returns it inline. The main window's menu item reads "Routines ↗" while popped and focuses the window instead of swapping the pane.
- **Pure view decision:** the surface renders from the same backend state docked or popped; nothing about runs, leases, or consent changes.
- **Consent cannot hide:** transmit gates surface on whichever window hosts the surface *and* badge the main window — a prompt buried on a powered-off second monitor must not silently stall a run.
- **Persistence with a safety net:** popped/docked choice + window geometry survive restart; a missing monitor at launch falls back to docked (never a window stranded off-screen).
- **Measured cost (2026-07-13, webkit2gtk 2.52.3, Pi 5/16 GB, software GL + DMABUF off, PSS via `smaps_rollup`):** **~30 MiB per additional webview window** with dashboard-grade content (+30.2 / +29.2 MiB for windows 2 and 3). Content-dependent — a popped Tac Map adds Leaflet + tile cache on top; re-measure with real surfaces at implementation (harness: `dev/scratch/measure-webview-marginal-memory.py`, ~15 s). Design rule this validates: windows spawn on operator demand and dock-back reclaims the process — never pre-rendered hidden windows. **User-docs note:** document the number plainly (users will compare against Pat, a headless Go binary; a full graphical station costing ~30 MB per extra window on a 4 GB Pi is <1% and worth stating without defensiveness).

## 13. Agent integration (MCP)

Tool family on the existing MCP surface, visible to both transports (Elmer's in-process invoker; external clients via the UDS shim):

`routines_list · routines_get · routines_validate · routines_save · routines_enable · routines_disable · routines_run · routines_run_status · routines_journal_get · routines_dry_run`

- **Validate is the contract:** `routines_validate` returns the same machine-readable error/warning list the builder renders. Agents iterate against it before the operator looks. `save` always accepts drafts (drafts may carry errors — same rule as humans).
- **No invoker privileges:** the design-time acknowledgment is the consent envelope (§4); after it, all invokers are equivalent. The acknowledgment itself is recorded only by a UI act — `routines_save` and `routines_enable` have no parameter that can supply it; `enable` on an unacknowledged automatic routine returns the validation error.
- **Journals close the loop:** `routines_journal_get` gives agents the same evidence a human exports — "why did last night's run fail?" is answerable from the record.

## 14. Definition format

One JSON schema; the export format is the storage format. Individual files under `routines/` in the config directory — git-diffable, portable between machines, agent-writable.

```json
{
  "routine": "morning-ics-cycle",
  "schema_version": 1,
  "transmit_mode": "automatic",
  "transmit_ack": { "by": "<callsign>", "at": "<timestamp>" },
  "on_interrupted": "stay",
  "inputs": [],
  "triggers": [
    { "type": "schedule", "every": "30m", "align": "hour",
      "window": "06:00-22:00", "if_missed": "skip" }
  ],
  "tracks": [
    { "name": "connect-cycle", "steps": [
      { "id": "s1", "action": "radio.connect",
        "params": { "stations": "@station-set:or-gateways",
                    "bands": ["40m", "80m"], "listen_before_tx_s": 5 },
        "timeout_s": 300, "on_radio_busy": "wait" },
      { "id": "s2", "control": "branch", "on": "s1.connected",
        "then": ["s3"], "else": ["s4"] }
    ] }
  ]
}
```

Load-bearing conventions: **`@`-references** name every external entity (what reference validation resolves); **`stepId.output`** paths are the variable system the type-checker walks; `schema_version` gates evolution. Default failure semantics: a step without a failure branch fails the run (verbatim cause, journaled); a failure branch makes failure a handled path.

## 15. Testing

- **Engine core** (state machine, write-ahead journal, timeouts) and **scheduler** (alignment, windows, missed-fire policies) test against a mock clock — deterministic, no radio, no real waiting.
- **Arbiter:** scripted contention scenarios (run-vs-run queueing, human preemption, take-the-radio pausing) as unit tests.
- **Validator:** table-driven fixture corpus — the Laserfiche failure taxonomy becomes the test suite; every rot class is a fixture expecting a specific error code, plus call-graph-closure and fleet-check cases.
- **Actions** sit behind the trait the dry-run mocks implement: dry-run and the test harness are one mechanism — the path users trust for dry-run is the path CI exercises.
- **Frontend:** canvas model (graph edits ↔ JSON), run monitor rendering from journal fixtures; app-level contract tests with production providers per project testing policy.
- **On-air validation is operator-only**, per standing policy (RADIO-1): CI proves the engine; a licensed operator proves the Connect attempt. The ARDOP loopback rig covers the middle ground locally.

## 16. Decomposition note

This spec is one design; implementation decomposes along module boundaries already drawn here (engine + scheduler / arbiter / validator / journal + export / canvas + dashboard UI / dockable surfaces / MCP tools / action catalog). All pieces are in scope and get built (ADR 0022 — no agent-initiated deferrals); the implementation plan (writing-plans) sequences them.
