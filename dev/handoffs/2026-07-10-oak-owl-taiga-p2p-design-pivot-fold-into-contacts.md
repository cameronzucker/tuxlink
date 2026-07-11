# Handoff — 2026-07-10 — `oak-owl-taiga` — P2P build 26/29 done, then a design pivot at the Task-25 mock gate

Picks up from `tanager-sequoia-opossum`'s 2026-07-10 handoff (plan complete, 3
rounds reviewed). This session executed the plan through Task 27 via
subagent-driven-development, hit the Task 25 design-mock gate, and the operator's
review of that mock produced a **design pivot** that reshapes the remaining work.
Read the pivot section — it overrides parts of the committed plan.

## Execution state (what is built, reviewed, pushed)

- **Branch:** `bd-tuxlink-c39af/vara-p2p-session`, worktree
  `worktrees/bd-tuxlink-c39af-vara-p2p-session/`, clean and up to date with
  origin. Head `053a1480`.
- **PR:** #1069 (draft) — opened early so CI compiles each task's Rust (the Pi
  cannot finish a cold cargo build).
- **26 of 29 tasks complete, each two-stage reviewed (spec + quality) with fix
  loops.** Phases 0–4 (all backend: VARA protocol completeness, the peers roster
  store, the 8-site record matrix, trust boundary + agent surface) and Phase 5
  frontend through Tasks 22 (types/hook), 23 (finder peer rows), 23a (the
  load-bearing connect seam), 24 (map peer circles), 26 (capability activation +
  cross-store consistency), 27 (bench runbook doc).
- **The load-bearing result:** Task 23a wired the frontend→backend Connect seam
  the three plan-review rounds flagged as the highest-risk gap. Every protocol
  (VARA/ARDOP/packet/telnet) now traces click → `connectFor({sessionType:'p2p'})`
  → the backend command with `intent=p2p` and the channel's via/path, no CMS
  fallthrough. Independently re-traced in review.
- **CI:** green throughout on both arches. Five CI failures were fixed as they
  surfaced (three clippy/exhaustiveness ripples the local Pi cannot catch:
  `manual_contains`, `Debug`-derive, `EgressDenied` non-exhaustive match; two
  test issues: a packet dial-fail timeout from the hard-coded 25 s
  `Ax25Params::connect_timeout`, and an AppShell assertion that needed `intent`).
  One **unrelated flake** (`tuxlink-jt9::discover::override_wins_and_version_comes_from_sibling`,
  amd64-only, probes the runner filesystem for a wsjtx/jt9 binary) failed once and
  was re-run; the branch never touches `tuxlink-jt9`. Confirm the run on
  `053a1480` is green.
- **SDD ledger (gitignored):**
  `worktrees/bd-tuxlink-c39af-vara-p2p-session/.superpowers/sdd/progress.md` — per-task
  commit ranges, review verdicts, and every logged Minor/follow-up. This is the
  recovery map; trust it and `git log` over reconstruction.

## THE DESIGN PIVOT (this overrides the committed plan for Tasks 19/20/25/26)

The plan's Task 25 was a "P2P Peers roster editor" in Settings. Its Step 0 was a
mandatory high-fidelity mock with operator sign-off before code. The mock
(`dev/scratch/p2p-peers-settings-mock.html`, published at
`https://claude.ai/code/artifact/1fcd01a4-dd9a-4e2f-a021-967264592685`) was
**thoroughly rejected.** The gate did its job: the operator grounded the design
against on-air reality and Winlink Express, and the following decisions came out.
These are **operator decisions**, recorded verbatim in intent:

1. **Fold peer management into Contacts. Do not ship a separate "P2P Peers"
   surface, and do not put it in Settings.** A peer is a contact — a station you
   communicate with. The separate `peers.json` store and its management screen
   were engineering parallelism (mirroring `contacts/store.rs`), not a user need.
   The user model is "the stations I talk to," and Contacts already is that.
   Peer data (heard on VARA HF at 2300, last seen 2h ago, dialable) is
   **reachability attached to a contact**, not a roster you manage. Hearing a
   station can create or enrich a contact. Adding a peer to dial is adding a
   contact. The **only** genuinely-missing user need is "type a callsign you
   have NOT heard and dial it," and that belongs next to where you dial, not in
   Settings.
2. **Remove the "operator verified" promotion ceremony and the "unverified
   claimed identity" badge.** It is theater. Callsign identity is not verifiable
   (anyone can transmit any callsign), clicking "verify" changes no Tuxlink-side
   state, and "operator verified" means nothing stronger in emcomm than "a human
   clicked a box." Its only real job was gating the agent telnet dial (see #3),
   which is being removed, so the ceremony dies with it.
3. **Remove the agent telnet P2P dial entirely.** Not for consent reasons — the
   **armed send gate (EgressGuard) is the consent mechanism and it works; agent
   transmit under an armed gate is RADIO-1 working as designed, used everywhere.**
   The reason is destination-trust: a telnet endpoint is a host:port that could be
   attacker-controlled if auto-observed, and an armed agent connecting to it still
   leaks to that destination (arming authorizes the action, not the destination).
   That destination-trust problem needed a DNS-rebinding denylist +
   operator-provenance resolution stacked on top of the armed gate, and it is not
   worth carrying for a mode almost nobody uses. **Just do not let agents dial
   telnet P2P.** This also removes the reason to expose a peer's telnet host:port
   to the agent at all (tighten Task 19's curated read).
4. **Keep the radio (VARA/ARDOP) agent P2P dial as-is.** Radio has no destination
   to distrust — no host, just a callsign on a frequency — so the armed send gate
   alone is the complete, correct control, identical to every other agent egress
   the project already uses. The controller's earlier instinct to strip this was
   wrong and the operator corrected it.
5. **The operator dialing telnet P2P himself is unchanged.** Only the *agent's*
   ability to dial telnet is removed, not the operator's manual dial.

Why this is legitimate against a 3-rounds-reviewed plan: the Task 25 mock gate
exists precisely to let the operator reshape the surface at build time. This is
that reshaping. The backend machinery already built is correct as machinery; the
pivot is about which of it is surfaced to the user and which agent capability is
carried.

## What the pivot changes, task by task

- **Task 25 (settings roster editor): DO NOT BUILD IT.** Redesign as "peer
  reachability folded into Contacts." This needs a fresh design pass with the
  operator (he prefers a conversational design discussion over a heavy skill
  process — he interrupted the brainstorming skill this session and asked for
  plain explanation). The first design question is scoping: does `peers.json`
  stay as an underlying reachability store that the Contacts UI reads and enriches
  (UI-level fold, least rework), or do the two stores actually merge? Lean:
  UI-level fold — keep the peers store as the auto-tracked reachability record,
  surface it through Contacts, no separate managed screen. Confirm with operator.
- **Task 20 (agent telnet dial): REVERT / REMOVE.** Landed, reviewed, CI-green —
  now to be removed. Delete the `telnet_p2p_connect` MCP tool + `EgressPort`
  method + all 3 mock impls + the elmer arg-helper ripple + the `minimal_args`
  trip-wires; the IP denylist (`ip_is_denied`, `vet_candidates`,
  `connect_and_exchange_to_addrs`, `resolve_agent_dialable_endpoint`) and the
  `EgressDenied` variant and its `ui_commands` match arm; the agent-path
  `ObservationGuard` for telnet. **Keep** the operator-facing telnet P2P dial
  (`connect_and_exchange` / `exchange_over_stream` in `winlink/telnet_p2p.rs`,
  `ui_commands::telnet_p2p_connect`). Flip `agent_telnet_dial` capability false /
  remove the bit. Use `git revert`/named-file edits (destructive-git ban; the
  hook denies reset/force). The commits to consult:
  `ff5d0848` (Task 20) + `c7b3a77a` (its CGNAT/doc-fix).
- **Task 19 (find_peers curated read): TIGHTEN.** Stop revealing telnet endpoint
  host:port to the agent (it can no longer dial telnet, so it has no use for the
  address). The operator-provenance-gated endpoint reveal in `curate_peer` should
  drop the host entirely. Keep the arm-gated curated peer read otherwise.
- **Task 21 (VARA agent egress): KEEP.** No change. Commit `93726ff7`.
- **Task 26 (capabilities): ADJUST.** `finder_peers` + `map_peers` are flipped
  true and correct. `agent_telnet_dial` must go false / be removed with Task 20's
  removal. `settings_editor` — its meaning changes with the Contacts fold; it was
  going to gate the (now-cancelled) settings roster editor. Reconcile the
  capability set with the redesigned surface.
- **Task 28 (wire-walk hard gate): STILL REQUIRED, retrace after redesign.** The
  operator's two flows are recorded verbatim in the plan's "Definition of done"
  section (Flow 1 inbound listen → tac map, Flow 2 outbound dial to heard/manual
  peer, plus the skill-mandated Flow 0 fresh-install setup). Trace them against
  the *redesigned* Contacts surface, not the cancelled settings editor. The
  "manually define a peer to dial" hop must land somewhere reachable.

## Surfaces that already serve the flows (do not rebuild)

- Find a Station finder: peer rows + Peer type filter + Connect (Tasks 23/23a),
  `finder_peers`-gated.
- Tac map + APRS map: peer circles, per-map tier scheme (Task 24),
  `map_peers`-gated.
- The connect seam: every protocol dials p2p correctly (Task 23a).
- The auto-tracked roster underneath (peers.json, the 8 record sites, the
  recorder, the limiter) — keep as the reachability substrate; it is what makes
  heard stations show up ready to click.

## Logged non-blocking follow-ups (from task reviews, for a bd sweep)

- Finder runs VOACAP over stations not peers, so a pure-P2P peer not in the
  station catalog renders untiered-dashed on the finder even with a known grid
  (Task 24 review) — spec §5 arguably wants per-peer prediction; larger change.
- `contacts:changed` listener reconciles peers on *every* contact mutation, not
  just delete (Task 26 review) — idempotent and cheap, tighten only if it storms.
- Non-allowlist telnet inbound rejects (IPv6/BadCallsign/EOF) are invisible to the
  spoofing-loop quarantine counter (Task 16 review) — spec-author follow-up.
- The bench runbook assumes the "G90 self-decode rig" is transmit-capable; the
  operator must confirm it is not receive-only before running Steps 3–4 (now
  flagged in the doc's pre-flight, Task 27).

## Branch / worktree state

- Clean, synced with origin, head `053a1480`.
- Untracked/gitignored: `.superpowers/sdd/*` (SDD briefs, reports, ledger, review
  diffs), `dev/scratch/p2p-peers-settings-mock.html` (the rejected mock). Local
  dev scratch, intentionally not pushed.
- bd `tuxlink-c39af` remains `in_progress`. Do not close it — the feature is not
  shipped; the Contacts redesign, the Task 20 removal, and the wire-walk gate
  remain.

## Next-session order

1. Read the plan's "Review-fold binding amendments" and "Definition of done"
   sections, and THIS handoff's pivot section (it overrides Tasks 19/20/25/26).
2. Design pass with the operator on the Contacts fold (conversational, not a
   heavy skill unless he asks). Settle the store-merge-vs-UI-fold question first.
3. Execute the pivot rework: remove Task 20 (agent telnet dial + its machinery),
   tighten Task 19 (no telnet host to the agent), adjust Task 26 capabilities,
   build the Contacts-folded surface (replaces Task 25), keep Task 21.
4. Task 28 wire-walk against the redesigned surface, then final whole-branch
   review, then mark PR #1069 ready.

Agent: oak-owl-taiga
