# Handoff — 2026-07-13 — `sumac-magnolia-fen` — Elmer knowledge tier + FT-8/WWV MCP surface

Long session, four PRs. **Two merged, two pending CI** (both already verified green on
R2 — the CI runner queue is just backed up).

## The through-line

The session started as "write two docs for Elmer" (`tuxlink-aib3n`) and turned into
**"Elmer had no working knowledge tier at all"** — then into "and it is blind to two
whole subsystems." Each fix exposed the next.

## Shipped (merged to main)

### PR #1091 — Elmer knowledge tier (`tuxlink-aib3n` closed)

**Elmer could search documentation but not read it.** `docs_search` returned a 12-token
`snippet()` window and no tool could turn its slug into the document — a locator with no
destination. Separately `docs/mcp-knowledge/` was served only over the MCP *resource*
tier, which in-app Elmer never lists or reads (its runner's sole schema source is
`list_tools_as_specs`), so the ARDOP/CMS-Z playbooks were readable by Claude Desktop and
invisible to Elmer.

**And `docs_search` hard-errored on any natural-language question.** FTS5's `MATCH`
argument is a query language: `How do I connect?` → syntax error near `?`; the motivating
question → syntax error near `.` (from "ax.25"); a bare slug → error (`-` is NOT). Bare
terms are implicit-AND, so one absent word returned silent zeroes. The in-app Help search
box hit the same bug.

Shipped: `docs_read(slug)`; an FTS5 tokenize-and-OR fallback; one index over three source
dirs (user-guide + new agent-only `docs/knowledge/` + mcp-knowledge); the
`tuxlink-0mudm` system-prompt grounding clause; `DocsHitDto.path` → `slug`; a
registry-drift test (it caught a real orphan, `36-off-air-space-weather.md`); six
retrieval evals using the operator's *actual words*.

**Accuracy note that matters:** `tuxlink-aib3n`'s own description gave Pat's digipeater
form as `ax25:///DIGI1,DIGI2/TARGET`. **Comma-separated is wrong.** A web search then
invented an `ax25:///TARGET via DIGI` form that does not exist. Ground truth is `pat
v1.0.0`'s `connect --help` and its parser (`la5nta/wl2k-go`, `transport/url.go`): hops are
**slash**-separated, multi-hop is real, and Pat *rejects* digis on ardop/telnet.

### PR #1092 — FT-8 MCP surface (`tuxlink-dof5j` closed)

FT-8 was fully built at the Tauri layer (12 commands) and bridged to MCP **nowhere**, and
undocumented anywhere in the corpus — blind in both tiers. Six tools now:
`ft8_heard_stations` (deduped: call, grid, best SNR, times heard), `ft8_status`,
`ft8_start_listening`, `ft8_stop_listening`, `ft8_set_band`, `ft8_list_audio_devices`.
Plus `docs/user-guide/37-ft8.md`.

**Operator-corrected decision — FT-8 decodes do NOT taint.** I proposed tainting because
`DecodeDto.message` is free text off the air. Wrong: FT-8's payload is 77 *bits* over a
fixed type set, and FreeText is hard-capped — our own decoder rejects >13 chars or an
out-of-alphabet character. A prompt injection does not fit. Tainting would block transmit
after listening, breaking the whole FT-8 loop, to defend a threat the channel cannot
carry. **Calibrate the threat model to the channel's capacity, not the field's type.**

## Pending CI (verified green on R2, merge when CI clears)

### PR #1093 — off-air WWV (`tuxlink-l44dm`)

**I corrected my own filing.** I claimed `solar_conditions` was "the ONLINE feed" that
"cannot work grid-down." Wrong — it reads a *cached snapshot from disk* and works offline.

The real defect: its description said *"Report **current** space-weather indices… Public
data."* and never mentioned the `source`/`updated_at_ms` fields it returns. `source` can be
`"bundled"` — shipped with the app, never updated — and a model reports those as **today's**
conditions. An operator picks a band on that. And `wwv_offair_refresh` existed at the Tauri
layer, bridged nowhere, so Elmer could not offer to fix it.

Shipped: rewritten `solar_conditions` description (cached, names the fields, explains
`bundled`, points at the no-internet refresh); `wwv_capture_offair`; `wwv_offair_available`;
same lie corrected in the compiled-in `agents-guide.md`; a lint catching descriptions that
name camelCase fields the snake_case payload lacks (I shipped that bug in my own spec).

### PR #1104 — docs index reconciles on CONTENT (`tuxlink-cr0wz`)

Startup compared bundled **slug sets**, so a **body** edit to an existing page never
reached an existing install. Survivable with a 12-token snippet; **not** survivable now
that `docs_read` hands whole documents to the model as ground truth — a corrected connect
string would never reach a single existing user. Now fingerprints actual content. The
subtle half is idempotence: `populate_docs` stores `extract_markdown(...)`, so the
bundle-side fingerprint must extract too, or the app repopulates on every launch forever.

## Open

- **`tuxlink-0mudm` (P0)** — stays open **deliberately**. Its tool half and prompt half
  shipped in #1091; its clause (4) **gold-gen coverage gap** in `dev/elmer-distill/` did
  not (no PRODUCT/HELP question family). Closing it would misreport that.
- **`tuxlink-lc4k6` (P3)** — `docs_read` has no router-level test; `MockSearch::doc`
  always returns `Some`, so the unknown-slug steering branch is untestable at the tool
  boundary. Not a correctness defect.

## Machine state — READ THIS, it changes the loop

**`r2-poe` compiles this workspace over SSH in ~2 minutes.** The operator surfaced this
mid-session after I had burned several 15-minute CI round-trips on trivial compile errors.

Two traps:
1. `/usr/bin/cargo` on R2 is the **distro 1.75** and cannot parse modern dependency
   manifests (`idna_adapter` declares `edition = "2024"`); it fails with a misleading
   *"failed to download"*. The **rustup stable toolchain (1.96) is installed but not on
   `$PATH`** — use `~/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin`.
2. **CI lints with `--workspace`.** Without it, member crates' **test targets are not
   linted** — a run without `--workspace` is a false green. That is exactly where the
   `E0422` in #1093 lived.

Recipe is in memory (`project_r2_rust_compile_box.md`). **Compile on R2 before pushing.**

## Worktrees in flight

| Worktree | Branch | State |
|---|---|---|
| `bd-tuxlink-aib3n-elmer-winlink-docs` | merged (#1091) | disposable |
| `bd-tuxlink-dof5j-ft8-mcp-surface` | merged (#1092) | disposable |
| `bd-tuxlink-l44dm-wwv-mcp-surface` | PR #1093 open | keep until merged |
| `bd-tuxlink-cr0wz-docs-content-drift` | PR #1104 open | keep until merged |

Untracked in each: `.superpowers/sdd/` (briefs, reports, review diffs) and `node_modules/`.
Nothing else at risk. R2 has copies under `~/build/tuxlink-{wwv,ft8,cr0wz}` — pure build
scratch, safe to delete.

## What I would do next

1. Merge #1093 and #1104 once CI clears (both verified on R2).
2. **Run Elmer against Qwen3-Coder-Next** — the thing no test can prove is whether a small
   model actually *chains* `docs_search` → `docs_read` rather than answering from the
   snippet. The tool descriptions carry that protocol deliberately; it is unverified
   against a live model.
3. `tuxlink-0mudm`'s gold-gen family in `elmer-distill`.

Agent: sumac-magnolia-fen
