# Handoff — 2026-07-19 (isthmus-sage-owl): Overwatch designed + ready to build; agent-native ADR crystallized

Long session (started as VARA.ini continuation, became a deep Overwatch design + a
foundational architecture principle). Two shippable outcomes and one build-ready design.

## ⭐ Continue here: BUILD the Overwatch routine (epic `tuxlink-nsfo8`)

**The design is APPROVED and every piece is confirmed feasible against `origin/main`.
No fundamental unknowns remain.** The next session's job is to build it. Read these FIRST,
in order:

1. **Design doc (APPROVED, canonical):**
   `~/.gstack/projects/cameronzucker-tuxlink/administrator-bd-tuxlink-ant8s-ardop-connect-fixes-design-20260718-223436-overnight-watch.md`
   (rewritten several times to the corrected shape; read the WHOLE thing, especially
   Recommended Approach and Dependencies).
2. **ADR 0025** (this commit, `docs/adr/0025-agent-native-full-functionality-parity.md`,
   Status: Proposed) — the binding principle the build must honor: every feature's
   full functionality must be reachable by the AGENT, not just the human UI.
3. This handoff.

### The corrected Overwatch shape (do NOT revert to earlier framings)
- **Elmer AUTHORS a routine** (bounded, once). **The routine runs COLD overnight, no LLM
  in the loop:** scheduler → per-window lease/tune/capture/transcribe → transcript into the
  step OUTPUT (durable in the run journal). **Elmer DISTILLS the journal outputs on demand
  at wake-up** (`routines_journal_get` → `export_report` markdown). The LLM touches it twice:
  build-time and report-time. It never babysits capture (power/efficiency; Elmer is
  single-threaded/interactive).
- **Two concurrent tracks in one `Overwatch` routine:** `long-haul-am` (HF rig, arecord)
  + `local-fm` (UV-Pro, RX audio). Multi-track is a shipped primitive; per-rig leases are
  independent (arbiter keys per rig-id), so the two radios never contend and preemption is
  per-radio.
- **Brief = markdown** via `export_report` → `~/Documents/Tuxlink/reports/`. NOT the outbox
  (outbox is the send queue; a brief is awareness output). Winlink dissemination is a
  separate explicit gated act.
- KNX-overnight being mundane is EXPECTED. The value is the local+long-haul SYNTHESIS mapped
  to the tac map. A "quiet night" brief is a correct output.

### Build pieces (ALL confirmed feasible; build on R2, which cold-builds the monolith)
1. **AM capture-transcribe routine action** — generalize the SHIPPED+RUNNING
   `data.spacewx_wwv` (`src-tauri/src/routines/actions/data.rs`; the real impl is
   `MonolithDataService::wwv_capture` at data.rs:647, AppHandle-coupled). Generalize to:
   arbitrary freq + mode (incl. AM) + chunked duration, output = raw transcript. This is the
   foundation both tracks feed. START HERE. NOTE: `MonolithDataService::wwv_capture` is
   coupled to the Tauri AppHandle (config/arbiter/resources), so running it needs app
   context — scope how it tunes/sets-mode/captures/transcribes by reading data.rs:647 first.
2. **UV-Pro continuous-voice RX consumer** — over the SHIPPED `pump_rx`
   (`src-tauri/src/winlink/ax25/uvpro/audio/transport.rs`). Confirmed: RX_AUDIO IS the
   receive-monitoring path (the app hears the radio through it; SSTV just STFT-decodes that
   same stream). Net-new = loop `pump_rx` for CONTINUOUS voice (do NOT wait for SSTV's finite
   `AudioEnd`), feed PCM to the same STT path. Small.
3. **Agent-shaped map-marker tool** — the tac map ALREADY draws arbitrary named points via
   APRS OBJECT (`;`) / ITEM (`)`) support (`src-tauri/src/winlink/aprs/engine.rs`
   `parse_object_or_item` + `AprsObject`, rendered as named pins at lat/lon). Net-new = an
   agent tool `mark_observation(lat, lon, label)` over that render path (LOCAL, non-transmitted
   object). Verify a local-injection seam exists or add one. This is finding #7 on `to358`
   and a direct ADR 0025 case (capability exists, not agent-discoverable).
4. **City-level gazetteer** — resolve spoken place names to ~city-level lat/lon. Do NOT ship
   Valhalla (routing engine, GB + service; wrong tool). Use a small bundled cities gazetteer
   (GeoNames `cities15000`-class, ~1-3 MB indexed SQLite, offline, string match). Negligible
   next to the 44 MB world basemap already shipped. Granularity = city/neighborhood, NOT
   street. Unresolved names stay in the text brief (no pin; no fabricated coords). Do NOT let
   the small model emit coords (ADR 0025 — unreliable geo recall).
5. **The two-track `Overwatch` routine template** + Elmer distillation → `export_report`.

### ⚠️ Build guardrails
- **COLLISION SURFACE:** adding a routine action touches `src-tauri/src/routines/actions/`,
  which the iizmk Routines session was ACTIVELY editing tonight (it authored ADR 0024).
  Check iizmk's state and coordinate BEFORE landing action-registry changes; branch from a
  clean `origin/main` and expect merge-time reconciliation. Do not blindly merge.
- **R2 is the build box** (`ssh r2-poe`; `export PATH=$HOME/.cargo/bin:$PATH` for rustup TC,
  not distro 1.75). The Pi can NOT cold-build. "Needs CI" is NOT a real blocker — R2 gives a
  full red-green loop. R2 build worktree from earlier: `r2-poe:~/tuxlink-iww9r-build`.
- Receive-only feature; RADIO-1 fine (no TX). On-air validation is operator-only.
- This is a multi-piece build; it likely does NOT finish in one overnight session. Sensible
  first target: the AM capture-transcribe action + the single-track AM routine end-to-end,
  THEN the FM/UV-Pro track, THEN map+gazetteer. Use `build-robust-features` / `writing-plans`;
  adversarial (Codex) round before any PR.

## Second outcome: ADR 0025 (agent-native) — the session's biggest finding
Repeatedly tonight a real capability existed but was NOT agent-reachable/discoverable (CAT
mode + S-meter; printing via `print_document`/`export_report`; map-draw via APRS objects). A
frontier model needed source access to find each; the shipped small model (Qwen 3.5 122b-class)
cannot. **The MCP tool surface is the agent's entire reality.** ADR 0025 makes it a
definition-of-done invariant: a feature isn't shipped until the agent reaches its COMPLETE
functionality; "agent can't do X" = same severity as "human can't do X"; test reachability
with the SHIPPED model. **Status: Proposed** — awaiting operator review (0024 is also Proposed;
operator may want to review the two together — 0025 generalizes 0024, which becomes its
parity-of-existence instance). Consequences to propagate on Accept: a wire-walk AGENT lane,
features-shipped includes the agent path, feature design asks the agent-surface question at
conception. `tuxlink-to358` is the remediation audit for the legacy 82-tool surface.

- Portable (employer-facing) version of the thesis, no Tuxlink specifics, no em-dashes (per
  the operator writing-voice rule): `~/agent-native-vs-sediment.md`.

## First outcome (DONE this session): VARA.ini agent-config shipped
PR #1156 MERGED (`5f0bda1e`). `tuxlink-iww9r` CLOSED. The agent can now read/apply VARA's
`VARA.ini` via MCP (`vara_ini_read` / `vara_ini_apply`, gated). Live-validated on R2. Worktree
disposed (transcript archived to `.claude/worktree-archives/`); `git worktree prune` +
`git branch -d bd-tuxlink-iww9r/vara-ini-config` were deferred (hook + live sessions) — safe
to run from a session that owns the main checkout.

## Artifacts & state
- **bd:** `tuxlink-nsfo8` (Overwatch epic, P1, full corrected shape in notes) ·
  `tuxlink-sgqjk` (AM first-slice child, carries the live-validation results + fixture path) ·
  `tuxlink-to358` (agent-surface audit, P1, 7 findings incl. the map-draw one) ·
  `tuxlink-ffs5i` (Routine Template system idea, P3 — Overwatch ships via Import + routine
  `inputs` today, don't block on it).
- **KNX fixture (real, off the production rig path):**
  `r2-poe:~/tuxlink-fixtures/knx1070-am-skywave-*.wav` (30 min, 16 kHz mono). Transcripts were
  at `r2-poe:/tmp/knx-transcripts.txt` (ephemeral; reproducible from the fixture via the
  scratch example at `r2-poe:~/tuxlink-iww9r-build/src-tauri/tuxlink-stt/examples/transcribe_wav.rs`,
  uncommitted — promote it when building).
- **Demo brief** (what Overwatch produces, distilled from the real KNX fixture):
  `~/overwatch-brief-demo-knx.md`. Proved: real situational items (West Hollywood water main
  break w/ closure, Highland fatal shooting, Garden Grove hazmat) extracted from ~80% ads; the
  STT confidence gate correctly caught Whisper hallucination loops. Ads are the real adversary,
  not transcription quality.
- **Memories written:** `outbox-is-send-queue-not-awareness-store`,
  `mcp-surface-is-the-agent-ceiling`, `tuxlink-is-agent-native-full-parity`,
  `shared-rig-no-touching-without-arbitration`.
- **Hardware:** FT-710 restored to 10.136 MHz PKTUSB (as found). UV-Pro disconnected after
  probing (paired/bonded, `38:D2:00:01:55:5C`). FT-710 MW/AM receive + `tuxlink-stt`
  end-to-end were live-validated tonight.

## Pending operator decisions (surface early next session)
1. **ADR 0025 review** (Proposed) + whether to reconcile with 0024. Then `to358` reclassify
   from enhancement to incomplete-feature remediation (operator said "probably yes").
2. **Map-annotation feature ID:** operator recalled filing one; I could not find it in bd or
   `src/map/`. We now know APRS objects are the draw primitive, so it may be moot — confirm.

## Moniker
isthmus-sage-owl.
