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
- Lesson baked into row 1: verify every row against CODE, not docs — this
  campaign's first "defect" turned out to be a stale doc describing a
  fixed one.

## Fix FIRST — small, unblocking

1. **Stale interpolation prose in the local.log catalog entry** — the
   param description still says embedded `$sN.key` refs "do NOT
   interpolate and log as literal text"; the executor has interpolated
   embedded refs since 2026-07-21 (6epl8, shipped v0.98+, locked by
   `embedded_refs_interpolate_inside_strings`). The stale line actively
   teaches agents the wrong constraint, is fossilized in the bench
   contract, and grounded several wrong judge verdicts. Closes when: the
   description matches the executor (remember the tool-surface corpus
   regen gate). NOTE: the earlier "fix embedded interpolation first"
   spike dependency is VOID — the IR log construct compiles fine onto v1.

## Fix now — orthogonal to the refactor

2. **Modem status split-brain** — `ardop_connect`/`vara_open_session`
   return ok while `modem_get_status`/`vara_status` read idle/closed and
   `selected` sticks on vara-hf; diligent agents get told nothing
   happened. Open fork: transient-sessions-undisclosed vs status-lies —
   discriminate with one bench `ardop_lane_smoke`/`vara_lane_smoke` run
   watching both surfaces, then fix (likely: a last-session summary on the
   observation surface). Closes when: discriminator ran + chosen fix
   merged.
3. **Mode-advice docs contradict each other** — user-guide doc 16 says
   VARA is more robust at low SNR; doc 17 says ARDOP is, twice. Doc 16 is
   right. Closes when: doc 17 reconciled.
4. **Folder refs are case-sensitive and misreport as internal errors**
   [test: `known_defect_row4_folder_ref_is_case_sensitive_and_reports_internal`]
   — `mailbox_list {folder:"Outbox"}` → `-32603 internal`; `"outbox"`
   works. Closes when: case-folded + classified invalid-params.
5. **Precondition failures wear the internal-error code** — "VARA session
   not open", "audio devices not configured", "rig I/O refused" all
   surface as `-32603 internal error`. Closes when: mapped to a
   precondition/invalid-state class.
6. **Zero-sentinels read as absence** — `packet_config_get` returns
   `baud:0, kiss_port:0, kiss_host:""` for unset values; models report
   "not shown". Closes when: nulls or explicit unset marker. (Tripwire
   test pending.)
7. **Unlabeled fraction on the propagation wire** — `mufdayByHour` is a
   0–1 fraction; a model AND the judge read it as MHz. Existing shape test
   `path_prediction_serializes_camel_case` locks the current name (rename
   lands there). Closes when: renamed or documented in the result schema.
8. **find_stations hides its goal requirement** — `intent:"recommend"`
   requires `goal`; the description doesn't say so; every first call
   fails. Closes when: the description states it (corpus regen gate).
   (Tripwire test pending.)
9. **Ignored-payload steps read as acceptable** — an undeclared param
   (whole payload silently unused) files under `acceptable_warnings`
   (UNKNOWN_PARAM) while the disposition prose says only advisories are
   defects. Closes when: UNKNOWN_PARAM becomes an advisory (or the prose
   stops promising that).
10. **VARA reachability is a stale cache** — `vara_status.reachable` is a
    TTL-cached bare TCP probe that said true while `vara_open_session` got
    connection-refused in the same session. Closes when: cache invalidated
    on open-failure (or probe made live for status).

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
