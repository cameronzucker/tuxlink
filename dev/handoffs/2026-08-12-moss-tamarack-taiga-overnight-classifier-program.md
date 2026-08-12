# Overnight anchor — moss-tamarack-taiga, 2026-08-12 (classifier program)

COMPACTION ANCHOR. Operator is asleep. Standing directive, verbatim intent:
**build toward the fully built classifier program — the several different
classifiers doing different things per ADR 0030 — so Elmer is both SAFER and
MORE CAPABLE. If there is dead time, work on Elmer Advanced mode.**

Read this section first, then act. Do not re-derive what is below.

## 1. What is TRUE now (verified today, not inherited)

- **A serving bug was silently truncating every streamed tool call's
  arguments** — the closing `"}}` is one token, emitted in the same decode
  step as the stop sentinel, and vLLM's stop handling swallowed it. The
  client saw unparseable JSON and nulled the arguments. **The model was
  emitting perfect arguments the whole time.** Fixed client-side in
  `tuxlink-agent-frontend/src/provider.rs` (`parse_streamed_arguments`),
  merged as #1340 → `4f967fa5`. Production Elmer streams by default, so this
  was a real product bug, not only a bench artifact.
- **Confirmed end-to-end**, not asserted: `TR2-GRID-THEN-PATH` went 3/3
  fatal → 3/3 complete, arguments intact, and the transcript shows the model
  reading a real tool error (MHz vs kHz) and retrying correctly. That
  recovery loop is what the truncation was destroying.
- **ALL PRIOR BENCH NUMBERS ARE VOID** (operator ruling; `FINDINGS-THREEWAY.md`
  carries a retraction header). Do not cite the 86.4/87.3/87.7, the 42.9%
  elmer recovery, or any three-way table. The fixture diverges from
  production in ways found only reactively — see `FLOOR-AUTOPSY.md` and bd
  `tuxlink-10iw0` for the fixture-validity program that must pass before any
  future bench run counts as data.
- **The content/inbox classifier foundation SHIPPED** (#1341 → `565e84fc`):
  `src-tauri/tuxlink-classify/src/inbox.rs` — the typed conversion schema a
  privileged agent reads INSTEAD of raw mail, plus T0 triage and a
  low-confidence injection signal. Two review rounds found real defects; all
  fixed with regression tests (23/23, clippy clean). The big one: derived
  `Deserialize` let anyone rebuild a "validated" value from arbitrary JSON at
  the serialized boundary — boundary types are SERIALIZE-ONLY now.
- **`tuxlink-classify` is linkable by the app again**: candle is used by
  exactly one module and is now optional behind a default-on `t1-candle`
  feature; CI checks the `--no-default-features` path every run.

## 2. The autonomous queue, in order

1. **Codex MSRV consult (BLOCKED ON CAPACITY, do not shortcut).** The
   operator asked for consultation "until you substantially agree on a
   solution which won't break the project." Brief:
   `scratchpad/codex-msrv-1.txt`. Retry loop running (`gpt-5.6-sol` /
   `gpt-5.5` with backoff). **NEVER substitute a cheaper tier** — Luna was
   explicitly rejected ("like consulting Sonnet"). Do NOT touch the 14
   manifests until there is a written agreed plan. bd `tuxlink-qt7zi`.
   Context that dissolved the scary part: no machine is pinned to 1.75 —
   R2's apt cargo just won on PATH, now fixed (R2 reports 1.97.1 on a bare
   `cargo --version`), CI uses stable, and the ECT low-floor target is
   GLIBC-constrained, not Rust-constrained.
2. **Codex verification round on the six security fixes** in `inbox.rs`
   (same capacity constraint, same tier rule). The fixes are merged but the
   FIXES themselves have not been adversarially re-reviewed.
3. **v26 re-baseline → the first honest floor number.** Running now
   (`~/bench-overnight/inkling-v26-baseline`, 405 units, direct serving
   endpoint, one coherent binary vintage). When runner + judge drain:
   analyze per tier, write findings, and state plainly what Inkling's real
   floor is. This is the number that answers "Elmer as Inkling fails too
   often." Expect the fixture caveats in `FLOOR-AUTOPSY.md` to still apply —
   say so rather than overclaiming.
4. **Role 4 — security/injection classifier + quarantined reader**
   (bd `tuxlink-vcjo2`). This is the highest-value next build: it is what
   makes the shipped conversion schema actually REACHABLE, and it is the
   "safer" half of the directive. The blunt taint gate stays; the classifier
   shapes behavior within it (labeling untrusted-origin claims, spotlighting
   flagged spans, segregating suspected injection). A clean classification
   NEVER relaxes quarantine.
5. **Role 5 — capability-grant adjudicator** (bd `tuxlink-ct6zu`). This is
   the gate that makes **Elmer Advanced** (shell access) safe, so it is the
   natural bridge to the operator's dead-time suggestion: scoped grants,
   scored capability asks, deterministic policy deciding.
6. **Role 2 — trend classifier** (bd `tuxlink-1zn1e`) — the "more capable"
   role, least safety-critical, good dead-time work.
7. **Elmer Advanced mode** — design + build behind role 5's adjudicator.

## 3. Hard-won cautions from today (all of these cost real time)

- **Verify the premise before accepting a constraint.** Three "facts" turned
  out false in one session: MSRV 1.75 was a scaffold artifact nothing tests;
  candle declares no `rust-version` at all; R2 was never pinned. When
  something blocks the work, check whether the block is real.
- **Fix conditions, don't document them.** Writing "known blocker" into a
  doc comment instead of fixing a contained problem is the inert-slice
  pattern and the operator will (rightly) call it out.
- **No jargon on autonomous decisions.** If the operator cannot evaluate the
  reasoning, the decision is unreviewable. Say what it means in plain words.
- **Make guarantees TESTED, not asserted.** Every claim that failed today
  was one nobody tested. New invariants get a CI step or a unit test.
- **Uncapped collections are the recurring defect class.** Three instances
  in one module (attachment list, span vector, and the analyzer's own
  pairing). Cap AND disclose — the repo's never-silent-truncation idiom.
- One git op per call; standalone `cd` (the race hook judges payload cwd).
- `pgrep` bracket trick in watch loops; probe the endpoint a profile
  actually binds; split ledger 404s by path before reading them as outage.

## 4. Running processes at anchor time

- **R2:** `bench_runner` on the v26 baseline (~289/405 at write time);
  `bench-dashboard` systemd unit pinned to `--only inkling-v26-baseline`
  (`http://r2-poe:8899`). R2's `~/.bashrc` now prepends rustup to PATH
  (backup `~/.bashrc.bak-*`).
- **Pi:** `contract_judge` watching the v26 store (~262 judged); a
  breaker/completion watcher (kills the runner after 3 consecutive serving
  health failures); the Codex MSRV retry loop.
- **Sparks:** Inkling TP2 serving, healthy, reached directly at
  `https://inference.twin-bramble.ts.net/v1/chat/completions`.

## 4b. SESSION CONTINUATION — what happened after the anchor was written

Appended, not rewritten: everything above remains the record as of `e1054b80`.

**Operator intervened twice mid-session** (he was still up):

1. *"Start thinking about how we're going to actually host all these
   classifiers too. How does that work in setup? We don't have any plan for
   that right now."* — correct, there was none. ADR 0030 settles WHERE
   inference runs and says nothing about where the WEIGHTS come from.
2. I routed that to `office-hours`; he stopped it: *"No office-hours it's
   nearly 1 AM. If it needs that we'll put a pin in and get to it in the
   morning once the supporting elements are built and tested by you. Then we
   can decide on the human-centric UX bits which need me."* Saved as memory
   `feedback_build_substrate_first_ux_with_operator`. **The rule: build and
   test the substrate autonomously, defer the human-centric half to a session
   he is present for.**

**Built + tested (commits `77d387a0`, `4df66de5`, `cc23ee1f`):**
`src-tauri/tuxlink-classify/src/hosting.rs` — the model resolution layer.
`CandleBert::load` took a directory and trusted it; nothing decided WHICH
directory, so a missing file surfaced as an opaque candle io error. Now:
ordered search path (env override → XDG → `/usr/share/tuxlink/models`),
completeness + optional `manifest.json` byte-length verification, explicit
Incomplete/Absent reporting that names files and locations, shadowed-root
disclosure, capped-and-disclosed root list, and model-id validation before any
path is built (`../../etc` escaped the root — latent, fixed).

Deliberately: pure `std`, no HTTP client, so "we never silently download
weights" is provable by dependency absence; and NOT behind `t1-candle`, so a
`--no-default-features` build can still report that T1 is unavailable.

Verified on R2 (rustc 1.97.1), both feature configs: 49 tests pass, clippy
`-D warnings` clean on each. Plus an `#[ignore]`d end-to-end test against REAL
bge-small weights proving the locator's output actually loads in candle and
embeds to 384-dim unit vectors with sane semantics — the unit tests use
two-byte fake files and only prove the locator agrees with itself.

**NOT wired into the app, on purpose.** Linking it is one whole feature WITH
the setup surface (ADR 0022, no inert half), and the setup surface is the
pinned operator decision. Do not link it before that call.

**Measured numbers for the morning decision** (bd `tuxlink-13ofm`):
- bge-small-en-v1.5 required payload ≈ **134 MB** (`model.safetensors`
  133,466,304 B + `tokenizer.json` 711,396 B + `config.json` 743 B).
- MiniLM-L6 alternate ≈ 90 MB; gte-small 65 MB and e5-small 129 MB were both
  rejected by the T1 spike on rejection-gap grounds, so they are not options.
- Existing bundled-asset precedent: `resources/basemap/world-z0-6.pmtiles` =
  44,615,273 B, shipped via the `tauri.conf.json` resources glob;
  `resources/` totals 55 MB today. Bundling bge-small takes that to ~189 MB,
  and the ECT low-floor `.deb` inherits it.

**Correction landed:** the claim that the crate sat outside the app because
"candle's MSRV exceeds the app's 1.75" is FALSE and is now corrected in both
the crate manifest and `inbox.rs`. candle 0.9 declares no `rust-version`.

## 5. Open operator-facing items (do not decide unilaterally)

- **MSRV** (`tuxlink-qt7zi`) — policy; needs the Codex agreement first.
- **Dependency vulnerabilities** (`tuxlink-izcq0`) — operator said "P1, not
  today". Dependabot has already opened the two runtime-facing fixes
  (#1313 dompurify, #1312 mermaid); fast merge whenever he wants them.
- **Taint gate scope for local-only writes** — resolved in principle: the
  answer is the classifier program, not loosening the gate.
- **Classifier model hosting + setup surface** (`tuxlink-13ofm`, NEW) — the
  substrate is built and tested; the three decisions are his: (1) bundle
  weights in the `.deb` vs first-run download vs operator-supplied path,
  (2) what the setup surface says about classifiers, including when weights
  are absent, (3) whether size-only integrity suffices or a digest is wanted
  — note a `.deb`-bundled model is already covered by dpkg integrity while a
  sideloaded one is not, so this follows from (1).

**A design note for whoever picks up role 4.** Addendum 2's stack is
`quarantined reader → typed schema-validated extraction → PER-DATUM taint
provenance → deterministic consent gate → scoped grants`. The schema (step 2)
shipped. Step 3 is the piece that answers the operator's complaint that the
taint gate refuses correct model behaviour: `tuxlink-security`'s `EgressGuard`
holds ONE session-global sticky taint flag, so any read locks everything.
Per-datum provenance means actions gate on the taint of their INPUTS and
retain full capability on untainted parameters — addendum 2 is explicit that
this *refines, not replaces*, the mailbox-read-locks-send doctrine. It is
deterministic and needs no model, so it is not blocked on the hosting
decision. It IS a change to a security boundary and wants an adversarial round
plus operator awareness before it lands; do not do it unreviewed.
