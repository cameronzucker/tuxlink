# Surface-repair campaign ledger (opened 2026-08-14)

**This is the single canonical state of the agent-surface repair campaign**
that came out of the zqo ladder regression read
(`dev/bug-hunts/2026-08-13-zqo-ladder-regression-read.md` carries the full
evidence; this page carries the STATE). Rules:

- Rows only ever CLOSE (with the closing PR number). The file shrinks
  toward empty; that is the campaign finishing.
- Any PR touching the MCP/routines tool surface cites the row it advances
  in its body. "No row" is a valid citation; silence is not.
- Rows marked **[test]** have a tripwire test asserting current behavior
  (grep `known_defect` / the named test). If your change turns it red, you
  are standing on a ledger row: fix properly, then in the same PR flip the
  assertions, RENAME the test out of the `known_defect_` namespace (it
  becomes ordinary regression coverage), and close the row — `grep
  known_defect` must always list exactly the OPEN tripwires.
- The session-start briefing prints this file's path and open-row count.
- Founding lesson (see the first Closed entry): verify every row against
  CODE, not docs — this campaign's first "defect" turned out to be a stale
  doc describing a fixed one.

## Fix now — orthogonal to the refactor

*(empty — every fix-now row is closed; see Closed below. The campaign's
mechanical phase is COMPLETE as of 2026-08-16/17.)*

## Written off — absorbed by the compiler (DO NOT fix; do not re-file)

11. **Branch/goto wiring hazards** (inverted-arm landmine, fall-through,
    hand-wired ends) — the IR compiler emits the wiring mechanically.
12. **Step-assembly ergonomics** (6–12 rejected calls per branch) —
    agents emit IR, not step_add sequences.
13. **Catalog never exemplifies branch/delay syntax** — moot on the agent
    path once agents author IR.
14. **Model paraphrase of validate-disposition vocabulary** — the
    compiler driver interprets dispositions in code.
15. **Ungated-transmit default, agent-authored path** — IR failure blocks
    are explicit; the readback echo exposes ungated sends. (The engine
    default itself is row 19.)

Write-offs are contingent on the spike reaching GO; if it NO-GOes, these
rows reopen. **Spike note:** the pre-registered baseline (AS-EDIT-ROUTINE
3/3) is retired — that cell is exonerated (correct artifact judged against
a stale doc). Re-baseline the A/B on the real-gating class
(AS-CHECKIN-CLEAN / COLLAB-NET-CLEAR).

## Operator queue (decisions, not code)

16. **Outbox taint scope** — listing your own send queue counts as
    untrusted content and locks transmit; and no non-tainting way exists
    to verify a transmit outcome. Decide granularity + whether transmit
    tools return an outcome summary.
17. **The "clean" bit** — no on-wire signal for "nothing prevents the
    requested enable"; `advisory_completion()` prose sanctions ask-first.
    Decide: add the bit, or bless ask-first as correct.
18. **Parity additions** — `predict_path` tx-grid override; preset-create
    on the agent surface (ADR 0027 lane).
19. **Engine default: continue past a failed connect** — hand-authored
    linear routines transmit into the void after a failed connect. Decide:
    keep + a linear-flow lint (analog of ARM_END_INVERTED), or change the
    default.
20. **IR one-pager taste** — the spike's execution gate
    (`dev/spikes/2026-08-13-ir-compiler-slice/IR-ONEPAGER.md`), with the
    baseline re-selection above.

## Bench-side (relayed; theirs, not ours)

21. **Bench relay batch** — one consolidated relay: responder answer-key
    starvation; judge-reducer
    scope on interpretive claims; propagation fixture per-index curves;
    judge+contract pinning for cross-run comparisons;
    `-32603`→invalid_args stat mapping; invisible harness-layer
    rejections; empty session-log-history worlds; the cell-reword batch;
    one unjudged attempt; **and the stale-knowledge fossil class: the
    bench contract encodes product docs as ground truth (the interpolation
    rule, the GRIB mode list) — contracts must be grounded against product
    CODE/behavior at the pin, and the log-criterion verdicts graded off
    the stale rule (AS-EDIT-ROUTINE all, COLLAB-NET-CLEAR a2 success-log,
    AS-NEAREST5-DIAL a3 log_station, AS-FALLBACK-CLEAN a3) need
    re-judgment.** Full text: read doc §"Consolidated BENCH findings" +
    its 2026-08-14 correction. Closes when: relayed and acknowledged.

## Closed

- 1 — stale interpolation prose in the local.log catalog entry — closed
  2026-08-14, row-1 PR under umbrella tuxlink-4280b. (The spike's
  "fix interpolation first" dependency was void; note preserved in the
  write-off section's spike note. A 2026-07-24 battery disagreement file
  had already flagged this exact stale line — fossils leave paper trails.)
- 4 — folder refs case-sensitive + misreported as internal — closed
  2026-08-16, error-hygiene batch PR #1354 (umbrella tuxlink-4280b): system
  names case-fold, user slugs validate the original spelling, list/read
  boundary classifies parse failures as InvalidInput (matching the move
  path); tripwire flipped and renamed to
  folder_ref_case_folds_system_names_and_stays_strict_on_slugs.
- 9 — ignored-payload steps read as acceptable — closed 2026-08-16, same
  PR (#1354): UNKNOWN_PARAM moved into ADVISORY_CODES (repairable, named in
  advisories, completion sentence stops calling it unrepairable); new
  classify test pins the membership.
- 6 — zero-sentinels read as absence — closed 2026-08-16, wire-shape batch
  PR #1357 (umbrella tuxlink-4280b): mcp-core `PacketConfigDto` host/port/baud
  are `Option`, unset passes through as explicit null from the monolith DTO
  (the sentinels were manufactured at the mapping); keys stay present so
  the config-read field inventory is stable. New regression test
  `packet_config_unset_fields_serialize_as_null_not_sentinels` pins the
  shape; the routines `data.read packet_config` byte-identical test now
  carries a null `baud` through the whole path.
- 3 — mode-advice docs contradict each other — closed 2026-08-16, row-3 PR
  #1358 (umbrella tuxlink-4280b): docs 17 AND 14 reconciled to docs 15/16
  (VARA generally more robust at low SNR; ham consensus + the bench oracle
  agree). Five spots carried the explicit claim, not two: doc 17's
  conditions-table row and quick-lookup row (the P3 pair), its
  decision-tree "more forgiving fallback" label and plain-English summary,
  and doc 14's backpack row (Codex round). Two siblings also fixed: doc
  14's implicit "spotty → ARDOP" row, and doc 17's UNSOURCED
  ARDOP-more-forgiving-on-rapid-QSB comparative, softened to
  non-comparative (neither direction is sourced in docs 15/16). ARDOP's
  remaining recommendation grounds are the true ones: not installed, no
  x86-64/Wine, untested VARA tier on the target RMS, open-source
  requirement.
- 7 — unlabeled fraction on the propagation wire — closed 2026-08-16, same
  PR (#1357): `mufday_by_hour`/`mufdayByHour` renamed to
  `mufday_fraction_by_hour`/`mufdayFractionByHour` on BOTH wires (mcp-core
  DTO and the camelCase UI shape) so the name carries the unit; doc
  comments state "fraction of days below the predicted MUF, not a
  frequency". Rename landed in `path_prediction_serializes_camel_case` as
  the row predicted. Tool descriptions untouched — the corpus regen gate
  stays with row 8.
- 8 — find_stations hides its goal requirement — closed 2026-08-16, row-8
  PR #1361 (umbrella tuxlink-4280b): the description now states up front that
  `intent:"recommend"` REQUIRES `goal`, with the same concrete example
  `intent_help` teaches on failure. New regression test
  `find_stations_description_discloses_goal_requirement_for_recommend`
  pins the disclosure (the row's pending tripwire lands as this ordinary
  coverage). Corpus regenerated on R2 per the gate — one row changed.
  ADR 0030 note: the tuxlink-tools threshold entry was calibrated against
  the prior corpus content; recalibration is recorded on tuxlink-4280b as
  owed at classifier wiring time (the classifier is not live yet).
- 5 — precondition failures wear the internal-error code — closed
  2026-08-16, row-5 PR #1359 (umbrella tuxlink-4280b): new
  `EgressPortError::Precondition` ("precondition not met (your call was
  fine; fix the named state, then retry): <seam detail>") with a
  marker classifier at the egress seams; UiError-typed seams classify
  NotConfigured/Unavailable directly while Transport marker-gates (a
  blanket promotion would mislabel real network faults — Codex round).
  Carrier deliberately stays internal_error per the write-tier
  Unavailable anti-guess-loop rationale; the class lives in the message
  the agent reads. Classifier inventory + router wire shape pinned by
  tests against the real seam strings.
- 10 — VARA reachability is a stale cache — closed 2026-08-16, row-10 PR
  #1360 (umbrella tuxlink-4280b): both `vara_open_session` connect
  outcomes overwrite the reachability TTL cache — a real socket result
  outranks any bare probe. Codex-round hardening: only a CMD-stage
  connect failure caches false (new `connect_staged` tags the stage; a
  data-port lag on a WINE restart no longer misreports the cmd port), and
  a probe generation guard keeps a slow probe from overwriting a newer
  open outcome. Success half proven through the REAL open seam (the
  readiness gate fails open, so bare listeners drive a genuine Ok).
- 2 — modem status is amnesiac — closed 2026-08-17, row-2 PR #1362
  (bd tuxlink-wovan): `modem_get_status.last_session` remembers the most
  recent OUTBOUND dial outcome (ARDOP + VARA + packet completion
  chokepoints, panicked-task path included; inbound listener exchanges
  deliberately excluded — they surface via contact observations, and
  recording them would let an inbound call overwrite the memory of the
  dial the agent just ran). `selected.note` carries the
  persisted-target-not-live caveat ON the wire (with_edit_protocol
  precedent; corpus-neutral, no description change). VARA open/status
  coherence test pins open-ok ⇒ status Open ⇒ stop ⇒ Closed and records
  the settled open ≠ dial semantics — the app-layer answer to the row's
  carried caveat. The Elmer battery harness manages the new store (the
  state()-before-manage() panic class its comments already warn about).
  THE LAST FIX-NOW ROW: the campaign's mechanical phase is complete.
