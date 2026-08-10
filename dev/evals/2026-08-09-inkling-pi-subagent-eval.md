# Eval: Inkling as a real code-work subagent (pi harness)

Operator ask (2026-08-09, tuxlink-ja6ix): evaluate Inkling — the local
`inkling-small-nvfp4` served by vLLM on the Spark at 256K context — as a
genuine coding subagent, driven by the previously-tested **pi** harness
(the pi coding-agent CLI, not this Raspberry Pi). Three graduated real
tasks, independent verification on every one, session JSONLs retained in
`dev/scratch/inkling-subagent-eval/` (gitignored) for forensics.

## Verdict

**Viable.** Clean sweep: 3/3 tasks passed every independent check, including
hidden known-answer tests the model never saw, a first-compile write-only
Rust library, and a 5/5 comprehension score against a pre-authored answer
key. The binding constraint is serving pace (3–24 min per task), not
capability. Recommended role: a parallel offload lane for bounded,
spec-complete tasks with verification gates — not unsupervised repo access
(see convention-absorption note).

## Harness wiring (operationally load-bearing)

- The July-tested harness migrated scopes: it is now
  **`@earendil-works/pi-coding-agent`** (0.84.1). Requires node ≥22.19 —
  userland install at `~/.local/node22` (system node is 20).
- Custom provider goes in `~/.pi/agent/models.json` under a **top-level
  `"providers"` key** (mirroring the models-STORE schema instead yields
  `auth check` → `invalid_state`, and an invalid provider makes `-p` mode
  **hang silently forever** — no error, no output). Keyless vLLM needs a
  dummy `apiKey`. Working entry: `api: openai-completions`, `reasoning:
  true` (vLLM streams thinking as `delta.reasoning`; `content` is null
  until reasoning ends), `compat: { supportsDeveloperRole: false,
  supportsReasoningEffort: false }`.
- `@file` attachments must be their own argv token: `-p @TASK.md "msg"`,
  never `-p "@TASK.md msg"` (the whole string is taken as a filename).

## Tasks and results

| Task | Shape | Wall | Own tests | Hidden/independent checks | Verdict |
|---|---|---|---|---|---|
| T1 maidenhead grid utils | TS + vitest, iterate-until-green | 23.5 min | 25/25 | **11/11 hidden known-answer** (1e-6 centers, corner clamps, roundtrip) | PASS |
| T2 catalog-format parser | Rust, **write-only** (no cargo on the authoring box) | 12.9 min | 12/12 | **7/7 hidden** on R2 incl. full 1,477-item real catalog (1477 items/0 warnings); clippy clean; compiled first try | PASS |
| T3 real-module comprehension | Read production `parser.rs`, 4 trace questions + author a test for an uncovered edge | 3.4 min | — | **5/5 vs pre-authored answer key**; chose the key's top edge (whitespace-only line kills startup parse); authored test compiles + passes (11/11) | PASS |

T1 profile: 27 tool calls (15 bash / 7 edit / 4 write / 1 read), 5 errored
results of which **one** was a genuine malformed tool call (edit missing
`path` — the family-quirk class; self-recovered next turn) and the rest
healthy iteration (it discovered vitest's include pattern excluded `test/`
and authored a `vitest.config.ts` unprompted). Code quality: correct math
with epsilon-guarded FP floors; visible iteration residue (one dead helper,
a vestigial pre-fix block, narration comments) — reviewer nits, not defects.

T2 standout: unable to execute Rust, it **simulated its parser's semantics
in python against the raw fixture bytes** (xxd inspection, BOM probing,
locating the deliberate truncation seam in the fixture) and derived its
44-item/2-warning fixture assertions empirically — all exactly right on
first compile. One honesty flag: its summary said "verified (12/12 pass)",
which reads like a test run; the verification was real but by simulation —
wording overclaims the evidence class.

## Cross-cutting observations

- **Convention absorption is strong — and double-edged.** pi walks parent
  directories for context files; from inside `dev/scratch/` the model
  absorbed the repo's AGENTS.md: it wrote handoff docs in house style,
  cited the banned-commands list, and invented a commit-trailer moniker
  ("oak-fern-heron") for commits it never made — the exact known
  subagent-moniker behavior. Great instruction uptake; it also means a
  "sandboxed" workspace inside the repo tree is not context-isolated, and
  an Inkling subagent behaves like a session agent, not a tool.
- **Tool-call reliability:** ~1 malformed call in ~40+ (≈2.5%),
  self-recovered. **Zero** bench-8b7-class streaming drops (pi's parser
  handled every final tool-argument JSON; that bug remains specific to the
  tuxlink agent-frontend parser path).
- **Latency:** the Spark serves this model deliberately; per-task wall time
  3.4–23.5 min. Fine for a background lane, wrong for interactive pairing.
- **Honesty:** no fabricated values, runs, or results observed anywhere in
  the three sessions; the one flag is the T2 wording above.

## Recommended operating envelope (for the operator's disposition)

1. Dispatch shape: one bounded task per session, spec-complete, with
   explicit "cannot run X here" statements when true — the write-only Rust
   flow matched this repo's actual Pi-edit/R2-compile loop and performed
   flawlessly.
2. Always pair with an independent verification gate (hidden tests /
   compile elsewhere) — not because it failed one, but because the T2
   wording shows verification claims need class-checking.
3. Keep workspaces outside the repo tree (or accept AGENTS.md absorption
   as a feature and give it real conventions deliberately).

Cross-links: tuxlink-ja6ix (this eval), tuxlink-efk3k (T2/T3 doubled as
catalog-domain evidence), bench-8b7 (not reproduced under pi's parser).
Raw sessions + workspaces: `dev/scratch/inkling-subagent-eval/` (local).
