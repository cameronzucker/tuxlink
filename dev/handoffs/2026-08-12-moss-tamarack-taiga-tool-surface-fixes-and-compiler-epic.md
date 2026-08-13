# Handoff — moss-tamarack-taiga, 2026-08-12 (tool-surface fixes shipped; compiler epic scoped)

COMPACTION ANCHOR. Read this before acting; do not re-derive it.

## 1. What LANDED (merged to main)

**PR #1342 merged as `edc41c43`**, 16 commits. All 14 CI checks green.

Everything in it came from reading all 2274 tool calls in the v26 bench run
rather than from taste. Attribution work is `tuxlink-0rc3h`.

- **`find_stations`** (41 rejections): one rejection now returns the whole
  contract instead of one missing field per attempt. NOTE the obvious diagnosis
  was WRONG: our schema is a correct `oneOf` and reaches the model verbatim
  (`provider.rs:1573`). A root-level `oneOf` is simply satisfied unreliably.
- **`predict_path`** (23, every predict_path failure in the run): accepts the
  MHz spelling. Ranges cannot overlap so it is normalization, not guessing.
  `tuxlink-9n4cr` had already seen 15 of these and responded by adding a HINT to
  the error text; that produced 23 more. Prose is not a fix.
- **Errors naming a button an agent cannot press** (~15): now name the tool.
  `vara_open_session` existed the whole time.
- **Mailbox folders** (20): errors name the valid set, point at
  `user_folders_list`.
- **Band-named routines**: `20m-net` was illegal because names had to start with
  a letter. Traversal guard unaffected and still tested.
- **`routines_save`** (25): the envelope now travels IN the rejection. Found a
  better bug doing it — `is_path_anchored` treated any `"top level:"` message as
  already-localised and SUPPRESSED the help, so the two most common failures were
  silently denied assistance that already existed.
- **Security, from the verification round** (`tuxlink-krl6n`): `Conversion` can no
  longer be forged from outside the crate (`non_exhaustive`, proven by a
  `compile_fail` doctest that I CONTROLLED by removing the attribute and
  confirming it then fails); the message handle is keyed with `RandomState` so it
  cannot be steered.
- **Per-datum input provenance** (`tuxlink-security/src/provenance.rs`), wired to
  `mailbox_move` and `attachment_save`. `attachment_save`'s `dest` is now optional
  and derived-when-omitted, which makes it bounded.
- **Classifier hosting substrate** (`tuxlink-classify/src/hosting.rs`).

## 2. What is IN FLIGHT

**Branch `bd-tuxlink-3ddk2/draft-reference`**, pushed, one commit `61b386c3`:
`DraftLibrary::get(slot_id)`. 14 tests green. No PR yet. Useful regardless of
what follows.

## 3. The measurement debt — THE HONEST NEXT THING

Every fix above is unit-tested against the exact arguments that failed. The
END-TO-END claim is NOT measured. The number to beat: **123 of 405 units (30.4%)
hit at least one rejected tool call**. Re-running needs the bench pin moved to a
build containing these fixes, and the bench repo is a SIBLING that needs its own
session root.

**PROVENANCE HAZARD in the bench repo** (recorded, not fixed, needs a bench
session): `bench-battery/Cargo.toml` pins rev `cff02cbe` while `Cargo.lock`
resolves `4f967fa5`. The built binary used the lock (verified: it contains
`repair_truncated_object`), but a fresh clone or `cargo update` would silently
build PRE-truncation-fix code.

Also outstanding: the fixture differential probe. FIVE seams verified identical
to production by revision equality (tool surface, argument validation, error
rendering, agent loop, and the client-vs-simulated-far-end protocol stack) —
recorded on `tuxlink-10iw0`. STILL UNVERIFIED: the bench's deliberate far-end
modifications, the SIMULATED OPERATOR CHANNEL (highest value — every
`needs_operator` outcome originates there), seeded state, and the judge.

## 4. Two epics scoped this session

**`tuxlink-s3h20` — Elmer Compiler (P1).** Operator-originated concept. Model
emits a compact IR; a deterministic compiler expands it from templates we own;
the existing validator reads the emitted bytes. TWO SERVERS: the Rust runtime MCP
(unchanged, holds compile/validate/authority) plus a NEW TypeScript INFORMATION
MCP that is read-only, holds no state, performs no action, has no authority, and
publishes the grammar as TypeScript DECLARATIONS. Direct connection, direct
consult. The Code Mode insight is NOT "let the model write code" — the model
never writes code — it is "present the capability surface the way a developer
reads an SDK." A code-execution variant was considered and REJECTED (invents a
sandbox problem that need not exist). DELIBERATELY BLOCKED on `tuxlink-10iw0`
because the intake half has no valid numbers.

**`tuxlink-3ddk2` — `@draft:` reference (P2).** Routines cannot reference a
FILLED form, so "file a filled ICS-213 check-in every morning" is not
expressible. `FormDraftLibrary` already stores filled values; the gap is a
reference kind, not a capability.

- ADDRESSING SETTLED: `slot_id` is a UUID v4 PRIMARY KEY, globally unique, so
  `@draft:<slot_id>` is FLAT. (My filed claim that a composite `form_id`+`slot_id`
  was needed was WRONG — that came from the resolver's note about non-unique
  LABELS.) Labels are refused as addresses.
- PIN vs LIVE DECIDED = **LIVE**, adversarial round on `gpt-5.5`. Decided on an
  invariant I had missed: routines ALREADY resolve `@` refs into a run-start
  snapshot stored in the journal, and `@preset`/`@station-set`/`@identity` are
  all live. PIN turns a reference into a fork.
- **BLOCKER FOUND, VERIFIED**: `local.rs:381` builds `OutboundMessage` with
  `attachments: Vec::new()` hardcoded. So `@draft:` alone renders values into
  subject/body with NO FORM XML — the exact silent interop degradation the issue
  exists to prevent. This feature needs a FORM-AWARE COMPOSE PATH or it ships the
  bug it was meant to fix.
- **OPEN, OPERATOR**: draft slots are deliberately PARTIAL (`CheckInForm.tsx:98`
  excludes datetime, GPS-derived fields, comments, msgsender). Does runtime fill
  the volatile fields, are they required elsewhere, or does validation fail?

## 5. Open operator-facing items

- **MSRV** (`tuxlink-qt7zi`): 1.75 is FALSE, not merely untested. Measured ladder:
  max declared across 568 crates is 1.89; 1.89 FAILS to compile (`libsqlite3-sys`
  needs `cfg_select`, bisected to 1.95); `cargo check` PASSES at 1.95; stable is
  1.97.1. So the honest window is ~2 minor versions. Also: an MSRV CI job must NOT
  run `clippy -D warnings` (older clippy flags what newer relaxed). No manifest
  touched.
- **Classifier hosting** (`tuxlink-13ofm`): weights never ship in the installer
  (operator ruling). Acquire at first-run setup while at home; anything acquired
  later must be SELF-HOSTABLE with no dependency on an outside party; skipping is
  first class. Substrate built and tested.
- **Dependency vulns** (`tuxlink-izcq0`): "P1, not today."

## 6. Cautions earned today

- **Verify the premise before accepting a constraint.** MSRV 1.75, candle's MSRV,
  R2's toolchain, and my own composite-address claim were all false.
- **A test that cannot fail is decoration.** The `compile_fail` doctest only
  counts because removing the attribute made it fail. Three separate guarantees
  today rested on tests that could not have caught their own failure.
- **Prose is never the fix.** The kHz hint produced 23 more failures.
- **Check the outcome, not the exit code.** The first MSRV consult "succeeded"
  with exit 0 while my own invented 1500s timeout had killed it mid-answer.
- **One cargo build per target dir.** Two concurrent runs deadlocked on the lock
  for 68 minutes looking alive.
- R2 CANNOT complete the full test suite: four `winlink_backend` packet tests hang
  forever on a localhost KISS socket. Not in any of my diffs; CI runs them fine.
  Use `-- --skip native_read_state_tests` there.
