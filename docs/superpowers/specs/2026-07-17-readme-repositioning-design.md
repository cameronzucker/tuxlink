# README repositioning pass + Elmer deep doc: design

- **bd:** tuxlink-d8f3l
- **Date:** 2026-07-17
- **Brainstormed with:** the operator (poplar-mink-chasm session)
- **Status:** approved in dialogue; this document is the written record

## 1. Purpose and audience

The README's audience has widened. Alongside the amateur-radio userbase, professional engineers at the operator's employer and at other companies are now evaluating Tuxlink as evidence of working agentic software. The repository must present the full current capability set accurately for both audiences without distorting the README's ham-first identity.

Two constraints are binding and come from standing operator direction:

1. **Honest maturity.** Alpha means "looking for testers," never defensive self-assertion. Proven capabilities are stated as plain fact. Unproven ones are named as unproven with precise qualifiers. Fabricated precision is worse than absence.
2. **Voice.** All prose follows the operator's imported writing-voice profile (private copy at `~/.claude/cameron-writing-voice-profile.md`; not committed to this repository). Universal rules apply everywhere: no em-dashes in any context, calibrated hedging only where genuine uncertainty exists, misconceptions named and corrected explicitly, authoritative citations for general claims. The register for both deliverable documents is the profile's Context 1 (teaching guide: complete sentences, bold definitional lead-ins, exact values and paths).

## 2. Deliverables

1. **README.md full rewrite** from the origin/main base. The uncommitted ~420-line README draft in the main checkout is superseded and ignored (its base predates the AGPL relicense and the current maturity matrix). Anything valuable in it is re-derived, not copied.
2. **docs/ELMER.md**, a new standalone deep doc for the agentic capability set, linkable as a single URL.
3. **A refreshed screenshot set** under `docs/readme/images/`, captured from the real running application.
4. Voice-profile relocation and memory update (already done during brainstorm; recorded here for completeness).

## 3. README structure (rewrite, order preserved)

The current section order is kept. Every section is rewritten in the profile voice. Content changes by section:

- **Title/tagline/opener.** The "No Windows. No browser tab to maintain." identity stays. The opening paragraph states what the software does today: a native Rust Winlink client that drives real radios over HF, VHF packet, APRS, and the internet, runs a live HF Winlink session and VHF APRS tactical chat simultaneously, and is operable by AI agents under operator-controlled consent.
- **Badges.** Unchanged, including the AGPL v3 badge. Review explicitly guards against regressing it to MIT (the superseded draft's badge was stale).
- **Comparison table.** Gains rows: automation routines; in-app AI assistant with MCP control; multi-window operation; FT8 band monitoring; off-air space weather. Each row's Winlink Express / Pat cells state their actual capability honestly (verified against the agent-only knowledge corpus in `docs/knowledge/`, which exists precisely to prevent confabulation about other clients).
- **Alpha note.** The testers-wanted posture and the release-tag caveat stay.
- **Features.** Rewritten groups plus new sections:
  - **Routines** (new): the flowchart designer, schedule triggers, the run journal and fleet dashboard, and consent gating presented as Part 97 modeled in the product (attended routines pause for per-transmit confirmation; automatic routines require an explicit one-time operator acknowledgment at design time). Screenshot: the designer canvas; the dashboard if a second shot earns its space.
  - **Elmer** (new, teaser): what the in-app assistant is and one or two concrete things it does (drive a multi-station connect loop under an arm window; answer questions from the built-in docs corpus), closing with a link to `docs/ELMER.md`. Screenshot: the Elmer pane mid-conversation with a visible tool call.
  - **Multi-window operation** (new): pop out Routines, the Tac Map, or APRS Chat into separate OS windows; layouts persist across restarts; closing a popped window never disturbs the mailbox. Screenshot: the multi-window workspace (also the hero candidate, below).
  - **FT8 listener** (new): receive-only by design, waterfall, band strip, CAT band-sweep; positioned as a propagation/band-openness instrument, not a message transport. Screenshot: the waterfall.
  - **Off-air space weather** (new): SWPC solar indices decoded from WWV/WWVH voice broadcasts with no internet path, including the one-time STT model fetch. Named plainly as a capability Winlink Express does not have.
  - **VARA and Wine** (new): resolves the "No Windows" tension explicitly. No Windows OS is required; VARA is a Windows program that Tuxlink's guided installer runs under Wine on x86-64, using the vendored [wine-vara-setup](https://github.com/cameronzucker/wine-vara-setup) project (named in the README for the first time). ARM users run ARDOP natively instead. The known first-run caveat is stated in one honest line: the current release still requires a handful of manual audio/CAT provisioning steps outside the product; automating them is tracked work.
- **Install.** Verified against current release artifacts; prose voice pass only unless facts drifted.
- **Interface.** Wizard and Request Center shots re-verified against the current UI; the onboarding tour/spotlight gets a sentence (it exists and the README currently describes only the wizard).
- **Maturity: what is and is not proven.** Same four-bucket structure with bold definitional lead-ins. Updates: dockable surfaces and Routines enter as built and CI-tested with field validation pending at the next release cut; APRS beacon transmit remains operator-pending; VARA P2P remains pending; FT8 is receive-only by design; telnet/CMS remains validated-internet. The production-CMS-registration gating statement stays.
- **Architecture.** Gains the in-process MCP invoker sentence (Elmer and external agents share one tool router) and links `docs/ELMER.md`.
- **Part 97.** Gains the Routines consent-model sentence; the existing consent policy language stays.
- **Documentation.** Unchanged in substance.

## 4. docs/ELMER.md outline

Product capabilities only. No development-process content. Engineer audience. Every architectural claim cites the in-repo spec, ADR, or source path it derives from. Outline:

1. **What Elmer is.** One paragraph: the in-app assistant that operates the station through the same tool surface external agents get. Not a chat overlay; an operator of the product under the product's own safety model.
2. **Architecture.** The in-process MCP invoker (`InProcessMcpInvoker` over an in-memory duplex) versus the external Unix-socket path; one shared 79-tool router; a mermaid diagram of both paths converging on the router, EgressGuard, and transports. No network port is ever opened.
3. **Security model** (the centerpiece). Operator-armed egress with bounded windows, live countdown, and disarm. Taint containment: reading any untrusted content (message bodies, search results, logs) locks send authority until application restart, and taint survives arming. Typed-callsign validation against modem-command injection. Real per-transport aborts, not advisory flags. Denied calls return plain-language reasons.
4. **What the agent can do.** The agent-send loop (path prediction, per-station dial with agent-chosen target/frequency/QSY candidates, compose, transmit, all inside one arm window). Full VARA lifecycle including guided Wine installation. Authoring, validating, and running Routines through the 10-tool routines family. Documentation retrieval (BM25 search plus full-document read over the user guide and the competitor-client knowledge corpus). The `point_at` UI spotlight.
5. **Models.** Local Ollama and cloud providers as equal peers over OpenAI-compatible chat completions; keys in the OS keyring only; live model switching; the tile-based onboarding flow.
6. **Honest limits.** Agent-initiated transmission inherits each transport's validation state (stated per transport). What Elmer refuses and why. Known rough edges.
7. **Pointers.** User guide page 35, the `tuxlink://agents/guide` resource, the connect flow for external agents.

## 5. Screenshots

**Capture source (operator-corrected):** a full build from THIS pass's worktree, launched against the operator's real application environment (existing config, fetched STT model, map tiles). The converge-build script is explicitly NOT used: it omits real feature dependencies. Exact launch mechanics (dev build vs packaged artifact, display/session handling, capture via grim) are resolved at plan time; capture happens on this Pi's real WebKitGTK/labwc stack.

**New captures:** multi-window workspace (hero: main window plus popped Tac Map and APRS Chat), Elmer conversation with visible tool call, Routines designer (dashboard optional), FT8 waterfall, VARA setup wizard (the wizard surface only; VARA itself cannot run on this ARM machine; a live-VARA shot from R2 is optional and operator-supplied if wanted).

**Kept-if-current:** color-scheme pair, first-run wizard, Request Center, radio panel, mailbox hero. Each is diffed against the live UI, not assumed.

**Rules:** privacy check every capture before commit (message content, station positions; the operator's callsign is public and fine). PNGs optimized before commit; repository weight is watched.

## 6. Out of scope

- The development-methodology story (CLAUDE.md and dev/ already carry it; the operator tells the velocity story himself).
- Restructuring the user guide.
- Any change to LICENSE or badges beyond guarding the existing AGPL v3 state.
- Release cutting or promotion (operator-only).

## 7. Process

- Branch `bd-tuxlink-d8f3l/readme-elmer-pass` in worktree `worktrees/bd-tuxlink-d8f3l-readme-elmer-pass`; the main checkout stays untouched.
- Docs-plus-images change surface; `pnpm lint:docs` gates links; no Rust or TS surface is expected to change.
- One cross-provider adversarial review round via Codex on GPT-5.5 (ADR 0023) over the finished README and ELMER.md, focused on claim accuracy, tone against the voice profile, audience fit, and the AGPL badge guard. Standard review loop before PR.
- The superseded README draft in the main checkout is left alone; the session handoff notes it can be checked out away by whoever owns that tree.
