# Handoff — 2026-07-11 — `bluff-alder-kestrel` — P2P contacts-pivot executed end-to-end, wire-walk + cross-provider adrev gates run, credential-exfil cluster closed

Picks up from `oak-owl-taiga`'s 2026-07-10 pivot handoff. That session built P2P
26/29 and hit a design pivot at the Task-25 mock gate. This session settled the
Contacts design conversationally with the operator, then executed the whole
remaining pivot via subagent-driven-development, ran the wire-walk and final
review gates, and closed a credential-exfil cluster the cross-provider
adversarial round surfaced.

## Branch / PR state

- **Branch:** `bd-tuxlink-c39af/vara-p2p-session`, worktree
  `worktrees/bd-tuxlink-c39af-vara-p2p-session/`. Head `efe0f61f`.
- **PR #1069** (draft). Merged `origin/main` in at `4fe4dcfc` (was CONFLICTING
  after main landed ft8 station-intel + elmer README; now MERGEABLE).
- **bd `tuxlink-c39af`** remains `in_progress` until the PR merges.
- Working tree clean except gitignored SDD scratch (`.superpowers/sdd/*`) and the
  gitignored `dev/adversarial/*-codex.md` transcripts.

## The design conversation (operator, this session)

The operator resolved the store-merge-vs-UI-fold question decisively: **Contacts
is the superset of added + observed stations.** A tier field (`confirmed` |
`unconfirmed`) keeps auto-created heard stations out of the curated list. "Verified"
means curation ("the operator confirmed this entry into the address book"), never
identity authentication. UI: a **"Recent"** section (unconfirmed contacts); within
it, rows with a completed session carry a **"Heard"** distinction, attempt-only
rows read "dialed · not reached yet" — the row, not the section, makes the RF claim.
Recorded verbatim in the spec §AMENDMENT (`docs/superpowers/specs/2026-07-10-p2p-peer-model-design.md`)
and the 2026-06-07 design's A.3 amendment.

## What shipped (pivot tasks T-A..T-G, each task-reviewed)

- **T-A** `d0710924` — removed the agent telnet dial (MCP tool, denylist,
  `EgressDenied`, 3 mock impls, elmer ripple). Operator dial + radio agent dial kept.
- **T-B+T-C** `a81e907c` (6 commits) — Contact grows tier + reachability
  (channels/endpoints/grid); `contacts.json` v1→v2 migration; peers store deleted;
  observation recorder moved to contacts with exact-callsign attach; keyring
  re-keyed `p2p-endpoint:<contact_id>:<endpoint_id>`; agent curated read rewritten
  (telnet host:port never crosses the agent surface). CI GREEN.
- **T-D** `52464296` — capability bits reconciled (dropped `agent_telnet_dial` +
  `settings_editor`, renamed `favorites_peer_link`→`favorites_contact_link`).
- **T-E** `812981c3` — frontend re-sourced from `contacts_read`/`contacts:changed`;
  `usePeers` is now a thin projection of `useContacts`.
- **T-F** `2d3588a2..88eeb0fe` — ContactsPanel Recent section + contact-detail
  reachability with Connect; review caught a label-truth bug (a failed dial could
  overwrite the heard/reached verb) → fixed with `last_ok`/`last_ok_direction`
  (success-only recency, captured atomically).
- **T-G** `37d3df10` — finder "Dial a station" manual-dial affordance (Flow 2b),
  pure frontend (the observation sites persist the unconfirmed contact for free).

## Gates run this session

- **Wire-walk (T-H):** traced the operator's recorded greenfield flows (Flow 0/1/2)
  against the redesigned surface. Found **one ❌**: P2P **ARDOP inbound was
  unreachable** — the `p2p` session-type protocol list omitted ARDOP HF, so no
  selectable sidebar row, though the whole ARDOP P2P listener/answer backend was
  built and P2P-aware. Fixed `b8d8f976` (one catalog entry + regression test;
  verified the entire downstream chain — computePanelMode, panel mount,
  `ardop_listen`, `ardop_answer_observation_sink` — already handled p2p). All flows
  now ✅ (on-air RF is ⚠️ operator-only, RADIO-1).
- **Final review (T-I) — whole-branch (Claude):** READY. Trust boundary (agent DTO),
  contacts migration, keyring migration, observation seam, label-truth all coherent;
  deleted machinery genuinely gone.
- **Final review (T-I) — Codex cross-provider adrev:** found a **P1 credential-exfil
  vector the Claude review missed** (it checked the agent boundary; Codex checked the
  operator dial). See below.

## Security cluster — found by Codex, closed this session

- **P1 (credential exfil) — CLOSED.** The operator telnet dial looked up a stored
  password *by callsign* and sent it to a request-supplied host; the UI rendered a
  Connect button on attacker-creatable `ObservedIncoming` endpoints. An inbound peer
  claiming a known callsign could get the operator's stored password sent to their
  host. Fix `981db616`: `p2p_dial_password_decision` gates on
  `is_password_eligible_operator_endpoint` (Operator provenance + host/port match),
  reads only the id-keyed secret; the by-callsign lookup is now reachable only from a
  read-only status probe. **Codex re-check found a residual legacy-only two-step**
  (an outbound dial auto-stamps its host as an Operator endpoint, so a
  socially-engineered manual dial to an attacker host under a legacy-secret callsign
  could leak the legacy password on a second dial) → fix `efe0f61f`: legacy-secret
  migration removed from the dial path entirely (id-keyed secrets only; legacy
  secrets never auto-sent — fail-safe orphaned; a legacy user re-sets via the
  explicit endpoint-password affordance). Both directions pinned by tests.
- **P2a (bloat) — CLOSED** `981db616`: per-contact observed-endpoint cap (32),
  never evicts Operator endpoints, cascades evicted secrets.
- **P3 (AX.25) — CLOSED** `be07f29d`: `validate_ax25_hop` on packet target + via
  before the wire and in the `find_peers` curation.
- **P2b residual — FILED** (`tuxlink-*`, P2): inbound observations attach to any tier
  including Confirmed, so spoofed reachability can appear under `tier:confirmed` in
  the agent DTO. The exfil impact is closed by P1; the residual is data-integrity
  hardening (distinguish observed vs operator reachability per-row).

## Filed follow-ups (bd)

- `tuxlink-3jrr9` (P2) — fold Favorites (stations.json) into Contacts as an elevated
  category (the operator's stated target model; deliberately out of this PR's scope).
- P2b residual (P2) — observed-vs-operator reachability provenance in the DTO/UI.
- M1 (P3) — the favorites→contact bridge is inert (`contact_id` never populated; the
  delete-cascade can't fire). Cosmetic (stale Recent rows survive contact deletion).
- M2 (P4) — stale doc-comments after the pivot (cancelled Task-25 refs, deleted
  `peers/recorder.rs` ref, a schema_version "1" comment where code sets 2).
- Also standing: the `tuxlink-jt9::discover` amd64/arm64 flake (filesystem version
  probe; the branch never touches jt9) and a pre-existing packet-answer
  double-record flake (filed this session).

## CI

CI green on `5399dd12` (T-B/T-F backend Rust's first real run — a `last_ok` test had
an off-by-one on the accumulated fail count, fixed there). The SEC Rust
(contacts/p2p/ax25/winlink_backend) passed on both arches on `981db616`; its only red
was the `tuxlink-jt9::discover` flake (arm64-only, unrelated). Confirm the run on
`efe0f61f` (the head) is green — re-run the jt9 job if it recurs; it is not a P2P
regression.

## SDD ledger

`.superpowers/sdd/progress.md` (gitignored) has the full per-task commit ranges,
review verdicts, both Codex rounds' dispositions, and every logged Minor. Trust it +
`git log` over reconstruction.

## What remains

1. Confirm `efe0f61f` CI green (re-run the jt9 flake job if needed).
2. Mark PR #1069 ready.
3. Operator: the two-rig on-air bench (spec §8 / `docs/design/2026-07-10-p2p-bench-runbook.md`)
   is the only remaining validation — RADIO-1 operator-only; no agent runs it.

Agent: bluff-alder-kestrel
