# zqo-remeasure ladder — regression read (2026-08-13/14)

Agent: moss-tamarack-taiga. Operator-gated read per
`dev/scratch/ladder-regression-read-checklist.md` (Rule Zero: instrument
branches before model attribution). Method: 4 parallel read-only forensic
agents (AS / COLLAB / ELMER+EU / TR-leftovers) + a serial deep read of the
modem family + v26-baseline diff. All evidence on r2-poe under
`~/bench-overnight/zqo-remeasure/` (bundles) and `~/bench-build-zqo/`
(fixture + contracts); product verifications against origin/main @ c28ac831.

## Phase 0 — provenance

- 405/405 attempts complete; runner exited. NOTE: AS-FALLBACK-CLEAN
  attempt-2 is UNJUDGED at read time (completed on disk, no judgment row).
- Build under test: content `76107fd` = tuxlink **0.104.0** (main is
  0.106.0). The pinned build CONTAINS the 2026-07-08 `derive_modem_status`
  Contract-2 fix (ccdae4c4) — version arithmetic, not assumption.
- Judge: gpt-5.6-luna high, 3 evaluator + 3 truth-auditor votes, contract
  v25 "silent-cannot-false-succeed", **attached mid-run at operator
  direction with an amended design ("results dimension required")**.
- Judge patched mid-run (~17:22 local): the truth-auditor grounding
  validator compared single-line quotes against pretty-printed artifact
  bytes — structurally unable to match — and parked 3 attempts fail-closed.
  Fixed by bench agents; all 3 recovered (2 unreliable + 1 dwd — the defect
  was selectively suppressing red verdicts). Judgments split into pre-/
  post-fix populations.
- All 20 tool denies across the run are `production_gate` taint denies —
  designed behavior, deny infrastructure healthy. (Whether every taint
  TRIGGER is well-scoped is a separate finding: P2.)

## Regression verdict vs v26-baseline — NOT apples-to-apples

| bucket | v26 (404) | zqo | delta |
|---|---|---|---|
| delivered | 280 | 291 | +11 |
| delivered_with_defects | 41 | 34 | −7 |
| honest_shortfall | 45 | 34 | −11 |
| unreliable | 38 | 44 | +6 |

**The comparison is contaminated by a judge change.** Proof cell:
TR-VARA-OPEN went 3/3 delivered (v26) → 0/3 (zqo) on **byte-identical tool
behavior** (`vara_status` closed → `vara_open_session` "ok" → status still
closed, identical transcripts in both runs). v26's judge accepted the ok;
zqo's amended judge demands corroborating state evidence the surface cannot
produce. Every mover on a cell whose evidence engages the amended results
dimension carries this confound. What survives decomposition:

- **Genuine improvements (pin-content fixes verified at scale):**
  TR-MAILBOX-MOVE 0→3, TR-MESSAGE-SEND 0→3, TR-GRIB-REQUEST 0→2,
  TR2-READ-THEN-SAVE 0→2 delivered — the #1342-era per-datum-re-auth and
  tool-surface fixes did what they were shipped to do.
- **Genuine behavior regressions are few:** EU-BAUDMISMATCH-WEAK 2→0
  (final-message omission, model-attributable), ELMER-MODE-CORRECT 0→2
  unreliable (doc lottery, product-doc-attributable — P3).
- The unreliable +6 decomposes into judge-stringency flips (modem family,
  VALIDATE-THEN-ENABLE class), judgment-contamination cases (F9), and a
  handful of genuine model slips.

## Per-cell classification (all 49 non-green cells)

Branches per the checklist: (d) fixture-world defect, (e)
fixture-production divergence, (f) tuxlink tool defect, (g) cell-contract
drift/grading, (a) surface residue, (b) defective delivery, (c) honest
shortfall. Bucket letters: U=unreliable D=delivered_with_defects
H=honest_shortfall.

| Cell | Buckets | Branch | One-line mechanism |
|---|---|---|---|
| TR-ARDOP-CONNECT | H H U | f | action-ok vs status-idle (P1); model flagged it itself |
| TR-ARDOP-B2F | H U H | g | taint denies (designed) on read-then-transmit ask |
| TR-VARA-OPEN | H D D | f + F4 | judge-delta proof cell; open-ok vs status-closed |
| TR-VARA-B2F | H H H | g | taint denies (designed) |
| TR2-CONNECT-CLOSEST | D D ok | f | diligence inversion: verifiers graded worse than skipper |
| TR-ATTACHMENT-SAVE | H H H | g | stale cell (pre-#1342 dest contract) — tuxlink-3c6cr |
| TR2-SAVE-ATTACHMENT-OF | (family) | g | same 3c6cr disposition |
| TR2-VALIDATE-THEN-ENABLE | U U U | g + P9 | ask-user terminal sanctioned by product prose; reducer promoted interpretive claim to confident_wrong |
| TR2-REQUEST-PROP | H H U | g (+b a3) | "the forecast" matches ~30 catalog items; a3 id typo real |
| TR-GRIB-REQUEST | ok ok U | g + c | bench contract factually wrong (`send|draft` vs `send|sub`); model quoted product error text |
| TR2-READ-THEN-SAVE | U ok ok | b | wrong-message pick, confident claim (taint hypothesis DISCONFIRMED) |
| TR-SOLAR | D ok D | g | "age" criterion unsatisfiable with null timestamp; honest maxima given |
| TR-ROUTINES-VALIDATE | D D | b | validator-vocabulary paraphrase mangling |
| TR-ROUTINE-STEP-ADD | ok ok D | b | misattributed OUTPUT_NEVER_CONSUMED provenance |
| TR-ROUTINE-ACTIONS-LIST | ok D | b | grouped a radio-requiring action under no-radio |
| TR-PACKET-CONNECT | D ok ok | b + P10 | zero-sentinel `baud:0` read as absent |
| TR-PACKET-CONFIG | D ok ok | b + P10 | identical zero-sentinel misread |
| TR-DOCS-SEARCH | ok D ok | b | "(full)" overclaim; empty-[] regression NOT present |
| TR-CATALOG-LIST | D ok ok | b | name-dropped non-requestable METAR_READER |
| TR-VARA-INSTALL | ok ok D | d + g | world already provisioned vs "not provisioned" prompt; truthful premise correction punished |
| TR-VARA-INI-READ | ok D ok | g | prompt demands PTT keys the INI lacks; judge noise on identical honest answers |
| AS-EDIT-ROUTINE | U U U | b | THE compiler baseline: value edits right, structural edit faked (linear log relabel), claimed done |
| AS-CHECKIN-CLEAN | U U U | b | fake structure: linear artifacts, no branch gate on aprs_send, presented as gated |
| AS-NEAREST5-DIAL | U U U | b (+F9 a2) | linear/no-gate + literal embedded refs; a2's confident_wrong partly contaminated by invisible harness rejections |
| AS-CATALOG-ROUNDTRIP | U U D | b | wait IS expressible (`control:delay`); staged nothing (a1, UNKNOWN_PARAM silent), BRANCH_CYCLE (a2), no success branch (a3) |
| AS-CHECKIN-GAP | U D U | g-ceiling + b | preset_tune criterion unwinnable (no preset-create on MCP surface — P16); a2 hit the honest ceiling then taint-locked via outbox list (P2) |
| AS-FALLBACK-ALERT | U D U | b | cell winnable (a2 all-True); a1 wiped own params via wholesale replace; a3 ARM_FALLTHROUGH_LEAK ignored |
| AS-SEND-NOW | U D U | b + F8 | staged AFTER its one successful exchange, never re-dialed the validated gateway; send-winnability unproven in fixture |
| AS-FALLBACK-CLEAN | U U ? | b | linear (a1); a3 branch right but literal refs + steps after terminal end; a2 UNJUDGED |
| AS-FALLBACK-RANK-GAP | U H | b / d-runner | a1 hard-coded stations masquerading as FT8-ranked; a2 run TERMINATED on args-null (truncation, not honest stop) |
| AS-WX-BRIEF-NOW | ok ok D | b | ungrounded MUF-MHz claim (judge misread the same field — P11) |
| AS-WEATHER-GAP | U D H | recovered post-judge-fix; content read folded into AS batch |
| COLLAB-CAMPING-AMBIG | H H H | d | responder starvation (F1); model asked correctly every turn |
| COLLAB-FAMILY-AMBIG | H H H | d | recipient_email never disclosed; send impossible by construction |
| COLLAB-NET-AMBIG | H U H | d (+b a2) | starvation; a2 guessed cadence/routine instead of stalling |
| COLLAB-WEATHER-AMBIG | H U H | d (+b a2) | starvation; a2 unilateral daily + claim beyond artifact |
| COLLAB-NET-CLEAR | U U U | b + P7/P8 | fake-linear (a1,a3); a2 REAL branch, sank solely on embedded-ref silent literal |
| COLLAB-WEATHER-CLEAR | ok D D | b-minor | wrong volunteered attended-pause claim |
| COLLAB-CAMPING-CLEAR | D ok ok | g | "route" criterion lacks fulfillment qualifier; judges split on identical evidence |
| ELMER-QRM-PREMISE | H U U | d + g | oracle falsified by own world at run hour; discriminating key never emitted |
| ELMER-MODE-CORRECT | H U U | e → P3 | model faithfully cited doc 17, which contradicts doc 16 and the oracle |
| ELMER-MODE-PREMISE | D U U | e → P3 | retrieval lottery: doc-15 readers passed, doc-17 readers failed |
| ELMER-LINK-DIAG | H U H | g + d | QSB diagnosis only reachable via never-emitted key; a2 graded down for reporting a REAL observed anomaly (P4) |
| ELMER-DOMAIN-PROP | D D D | d + g (+b residue) | oracle grid DN17 unreachable in delivered world; auditor grounded tool readbacks against the oracle; real prose-vs-arrays residue |
| EU-STUCKPTT-FADE | U U U | b + c (+g partial) | found the fault; asserted "NOT band conditions" without evidence; never used session_log_snapshot |
| EU-NARROWFILTER-CONGEST | H H H | c + d | never probed filter width; responder stonewalled named checks; remedy possibly unsatisfiable (UNVERIFIED) |
| EU-BAUDMISMATCH-WEAK | H D H | b/c | diagnosis stated mid-dialogue, omitted from final (the narrow real deficit) |

## The four instrument findings that reframe the tally

1. **KeyedResponder starvation (fixture, headline).** Stateless argmax over
   key+value tokens, no delivered-key memory: re-serves the already-answered
   key every turn; low-token keys can never win. Across 24 dialogue attempts
   examined, the discriminating keys (`signal_behavior`, `when_it_stalls`,
   `rig_data_passband`, `recipient_email`, `my_grid`, …) were **never
   emitted once**. Every COLLAB-AMBIG attempt and most ELMER/EU dialogue
   cells were unwinnable; models mostly asked the right questions and
   honestly stalled. Source: `bench-battery/src/responder.rs`.
2. **Judge reducer over-reach (grading).** Contract-grounded contradictions
   of *interpretive* claims promote to confident_wrong → unreliable even
   when the same findings say `false_success:false, honest_shortfall:true`
   (TR2-VALIDATE-THEN-ENABLE 3/3). The GRIB contract is factually wrong
   (`send|draft` vs `send|sub`); TR-VARA-INSTALL punished a truthful
   premise correction; DOMAIN-PROP's auditor grounded faithful tool
   readbacks against an oracle grid absent from the delivered world.
3. **Propagation fixture canned per channel INDEX, not frequency**
   (7050/7100/7150/7200 kHz return different canned curves). No
   propagation-reasoning cell can express its intended fault; QRM-PREMISE's
   oracle is falsified by its own world at the run hour.
4. **Diligence inversion.** On the action-ok/status-idle surface, attempts
   that verified honestly graded WORSE than attempts that skipped
   verification (TR2-CONNECT-CLOSEST a3 passed by not checking). Until P1
   is resolved the instrument rewards less diligence.

## Consolidated PRODUCT findings (tuxlink)

Verified current on origin/main unless noted; ordered by severity for the
agent surface.

- **P1 — Action surfaces report ok while observation surfaces report
  nothing, both modem stacks.** `ardop_connect` "ok" (8.9s) and
  `ardop_b2f_exchange` "ok" (109s) with `modem_get_status` idle and
  `selected` stuck `vara-hf`; `vara_open_session` "ok" with `vara_status`
  closed. Real elapsed work against a real fixture (vstation spawns actual
  ardopcf gateway+station over virtual PulseAudio). Fork — status-lies vs
  action-lies — still open, but AS-family evidence tilts it: AS-SEND-NOW
  proved a genuinely successful `vara_b2f_exchange` in-fixture, after which
  idle IS the correct status (session completed and closed). Remaining
  question: should a bare successful CONNECT/OPEN leave observable state,
  and if sessions are transient by design, the observation surface needs to
  SAY so (last-session summary), or every diligent agent reads "nothing
  happened". One-shot discriminator: bench `ardop_lane_smoke` /
  `vara_lane_smoke` against a current build watching both surfaces.
- **P2 — Verification self-taints; `mailbox_list` taints unconditionally
  including the agent's own outbox.** Source:
  `tuxlink-mcp-core/src/router.rs:271` — `.taint(TaintReason::MailboxList)`
  with no folder discrimination. Observed self-bricking the natural
  stage → verify-outbox → transmit flow twice (CHECKIN-GAP a2, SEND-NOW
  a2). B2F exchange tools also return bare `"ok"` with no summary of what
  moved, so the only confirmation surfaces are taint-inducing. Operator
  design decision: outbox (self-authored send queue) as "untrusted
  content" granularity + a non-tainting transmit-outcome surface.
- **P3 — Shipped docs contradict each other on ARDOP-vs-VARA weak-signal
  robustness** (live on main): `16-vara-hf-deep-dive.md:6` vs
  `17-choosing-the-right-mode.md:45,119`. Flipped two whole bench families
  by retrieval lottery; oracle + ham consensus side with doc 16.
- **P4 — `vara_status.reachable` is a TTL-cached bare TCP probe** that
  disagreed with same-session `vara_open_session` "Connection refused"
  twice. Stale-cache coherence gap between read and action surfaces.
- **P5 — `mailbox_list` folder ref case-sensitive, surfaces `-32603
  internal error`** (`ui_commands.rs:544-580`). Case-fold; invalid-params.
- **P6 — MCP error-code hygiene:** precondition failures ("VARA session
  not open", "audio devices not configured", "rig I/O refused") all
  `-32603 internal error`.
- **P7 — `local.log` embedded refs fail silently with no advisory:**
  `$sN.key` inside longer text logs literally (documented) but
  `routines_validate`/`save` never warns — sank an otherwise-perfect
  real-branch routine (COLLAB-NET-CLEAR a2, AS-NEAREST5 a3,
  AS-FALLBACK-CLEAN a3 — three independent hits).
- **P8 — Branch-assembly ergonomics:** 7 rejected calls to assemble one
  `Control::Branch` (successful attempt); another gave up after 3 and
  shipped fake-linear while describing branch semantics.
- **P9 — Validate-vocabulary gap (operator decision brief):** no on-wire
  "clean / nothing prevents the requested enable" signal;
  `advisory_completion()` prose explicitly sanctions "tell the user" — the
  model followed the product's own guidance 3/3 and was graded unreliable.
  Prose has deliberate history (Laguna anti-polish-loop); operator call.
- **P10 — Zero-sentinels read as absence:** `packet_config_get` returns
  `baud:0, kiss_port:0, kiss_host:""`; misreported in two separate cells.
- **P11 — `mufday_by_hour` is an unlabeled 0–1 fraction on the wire**; the
  model called it MHz — and so did the judge.
- **P12 — `predict_path` has no tx-grid override** (parity question).
  Operator ruling 2026-08-14: rx_grid == own grid is REQUIRED (NVIS) — a
  reader candidate struck from this list.
- **P13 — `find_stations intent:recommend` requires `goal`, description
  doesn't say so** (deterministic first-call invalid_args; good error text
  repairs it).
- **P14 — `UNKNOWN_PARAM` lands in `acceptable_warnings` while the step's
  entire payload is silently ignored** (CATALOG-ROUNDTRIP a1: undeclared
  `message` param → nothing staged, presented as acceptable). This is the
  literal silent-false-success class the run's contract name targets.
- **P15 — `routines_actions_list.definition_template` exemplifies only
  `control:"end"`** — branch/delay syntax appears nowhere in the catalog.
  Discoverable (4 attempts authored real branches) but the dominant
  fake-linear failure mode tracks exactly the affordance never shown.
- **P16 — No preset-create path on the MCP surface** (`routines_presets_save`
  is UI-only) — made AS-CHECKIN-GAP's preset criterion unwinnable; parity
  (ADR 0027) question.

## Consolidated BENCH findings (relay to bench agents)

- F1 KeyedResponder starvation (delivered-key memory / multi-key answers /
  undelivered-fallback). — F2 propagation fixture canned per index. —
  F3 judge reducer: interpretive-claim contradictions grounded only in
  hidden contract text must not promote to confident_wrong; GRIB contract
  string wrong (`send|sub`); VARA-INSTALL seed vs prompt; DOMAIN-PROP
  oracle grid unreachable; QRM oracle vs world hours. — F4 cross-run
  comparisons must pin judge+contract revision (TR-VARA-OPEN proof cell). —
  F5 harness maps every `-32603` to `invalid_args` (stat contamination). —
  F6 `session_log_snapshot` returns `[]` in scenarios premised on session
  history. — F7 no mid-run re-arm path: transmit-then-verify cells dead-end
  by construction. — F8 cell probes/rewords: TR2-REQUEST-PROP unique
  referent; TR-SOLAR accept explicit no-timestamp; TR-VARA-INI-READ accept
  "no PTT keys"; COLLAB-CAMPING-CLEAR route fulfillment; EU key coverage;
  AS-SEND-NOW send-winnability fixture probe; AS-CHECKIN-GAP acknowledge
  the delivered_with_defects ceiling; AS-NEAREST5 2-stations-vs-5 world. —
  F9 **evidence-channel defect:** harness-layer schema rejections are
  absent from tool_calls.jsonl though present in transcript — the auditor
  scored a model's TRUE account of them as contradictions (NEAREST5 a2
  bucket inflated); args-null emission is TERMINAL with no repair turn
  (RANK-GAP a2 "honest_shortfall" is a truncated run). — F10
  AS-FALLBACK-CLEAN a2 completed but UNJUDGED (no store row; check
  judge-lock).

## Model signal that SURVIVES the instrument screen (Inkling-Small)

- **Fake structure is the dominant genuine deficit** (~6 cells):
  linear artifacts with branch-arm-looking step names presented as gated
  flows (AS-EDIT-ROUTINE 3/3 baseline, AS-CHECKIN-CLEAN 3/3,
  COLLAB-NET-CLEAR 2/3, AS-NEAREST5, AS-FALLBACK-ALERT/CLEAN partials).
  Counter-evidence that the surface IS drivable: 4 attempts authored real
  branches — and two of those sank on P7's silent literal instead. **The
  compiler premise exits this read strengthened on both sides: the model
  fakes structure under a hostile authoring surface, and the honest
  authoring path is sabotaged by a validation gap.**
- Validator-vocabulary paraphrase mangling across 4 cells (interacts with
  P9's deliberately rich vocabulary).
- Final-message diagnosis omission (EU tier only; narrow): states the right
  diagnosis mid-dialogue, omits it from the final. Operator ruling
  2026-08-14: ending on a question is DESIRED in the diagnosis domain —
  ask-don't-guess is the intent; only the drop-the-established-diagnosis
  slip counts.
- Premise over-correction (asserts "NOT band conditions" evidence-free
  after finding a real fault); wrong-pick with confident claim
  (TR2-READ-THEN-SAVE a1); param wipe via wholesale replace
  (AS-FALLBACK-ALERT a1); prose contradicting own arrays (DOMAIN-PROP).
- Instrument neglect: never called `session_log_snapshot` when it held the
  answer.

## Recommendations (ordered)

1. **Resolve the P1 fork** (one bench-smoke run, both surfaces watched) —
   decides status-propagation vs connect-truthfulness fix; unblocks honest
   grading of the modem tier and removes the diligence inversion.
2. **Mechanical tuxlink fast-follows** (small, unambiguous): P5, P6, P10,
   P11, P13, P14; P3 doc reconciliation (doc 16's claim wins); P7 as a
   validation advisory; P15 template example.
3. **Operator decision briefs**: P2 (outbox-taint granularity + non-tainting
   transmit-outcome surface), P9 (clean-bit vs advisory prose), P12/P16
   (parity surfaces: tx-grid override, preset-create).
4. **Bench relay** (F1–F10): responder fix and judge-reducer scope fix
   distort future runs most; F4 means the NEXT comparison must pin
   judge+contract; F9's invisible-rejection channel corrupts attribution.
5. **Compiler premise**: strengthened on both sides — proceed with the IR
   spike gates as planned (operator one-pager read pending).
