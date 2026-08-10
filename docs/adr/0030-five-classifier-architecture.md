# 30. Five-classifier security and triage architecture

Date: 2026-08-10
Status: Proposed (drafted by agent session on explicit operator direction; awaiting operator ratification — all eight decision points were operator-ruled 2026-08-10 and are recorded verbatim-intent in tuxlink-efk3k addendum 6)
Deciders: cameronzucker (N7CPZ), moss-tamarack-taiga (authoring session)

## Context

Post-DefCon operator direction (2026-08-09): Tuxlink needs classifiers — not
one but a set, modeled on the problem shape Microsoft's mail stack solved
(layered classification plus sandboxed untrusted-content processing). The
design record is **tuxlink-efk3k addenda 1–6** (quarantine sketch; five roles
+ doctrine; mechanics; research-currency sweep; Phase-1 spike evidence;
operator rulings) — that issue is the canonical evidence trail and this ADR
transcribes it without re-derivation. Supporting artifacts on main: the
kickoff plan (`dev/plans/2026-08-09-elmer-classifier-architecture-kickoff.md`),
the two-host T1 spike (`dev/spikes/2026-08-09-t1-catalog-embedding/FINDINGS.md`),
and the CPU-viability evals (`dev/evals/2026-08-10-cpu-only-elmer-viability.md`),
whose field correction showed the full tool surface prefilling ~15k tokens —
making classifier-driven narrowing a practicality unlock for small backends,
not only a safety architecture.

External grounding (efk3k addendum 4): a June-2026 adaptive evaluation of
out-of-band defenses empirically endorses this design class — deterministic
out-of-band enforcement holds under adaptive attack while in-band model-based
detection collapses — and names the residual weaknesses this ADR's threat
model carries forward.

## Decision

**Roles.** Five advisory classifiers: request (human ask → catalog/tool
surface), trend (patterns over time), content (message/attachment triage,
placed inside the quarantine boundary), security/injection (quarantine
boundary), capability-grant (authority boundary; the symmetric twin of
request — it scores agent asks for capabilities as request scores human asks
against the catalog).

**Doctrine.** Every classifier ADVISES; deterministic policy DECIDES. A
fooled classifier cannot mint authority alone. Binding, non-overrulable
determinism exists only at the security and capability-grant boundaries.

**Tiers.** One verdict schema over three backends, degrading T2→T1→T0:

- **T0 — deterministic rules** (always, every deployment): schema validation,
  taint tracking, allowlists, bounds, thresholds, index-time structured
  parsing (geo/coordinates/numerics).
- **T1 — CPU encoder floor** (mandatory; offline-first): embedding retrieval
  and small fine-tuned encoders. Model: **bge-small-en-v1.5** primary (97.2%
  top-1 / 100% section zero-shot on the real 1,477-item catalog; the only
  candidate with a zero-overlap no-match reject gap; MIT), MiniLM-L6
  alternate. Runtime: **native (candle first, ort fallback)** — the
  60–300MB budget is unreachable through Python/torch (~1.1GB measured) and
  true single-query compute is ~14ms on x86; a small confirming candle spike
  precedes quoting its numbers as fact.
- **T2 — guard LM ceiling**: 1–8B-class models served ONLY via a
  control-plane profile, pinned off live bench-serving lanes.

**Deployment matrix** (operator-ruled): classifier inference co-locates with
Elmer's LAN inference backend when one exists; otherwise T0+T1 run where the
app runs. The T1 host is **choosable via a backend seam** (in-process candle
is the mandatory offline floor; any OpenAI-compat embeddings endpoint is a
configurable alternative) — on-device is the floor, not a cage. When Elmer
is not configured, Tuxlink spawns **nothing** — no unrequested inference
processes — and first-party docs carry measured performance/resource
expectations (tuxlink-nsnre).

**Request-classifier semantics** (operator-ruled after direct challenge):
verdicts are **advisory context to the model, never a gate on it** — a
capable backend may overrule candidates, browse the raw catalog, or resolve
an "ambiguous" verdict from context; thresholds fully drive behavior only in
flows with no capable model (Routines, degraded tiers). The verdict schema
is **corpus-generic** (`corpus`, `item_ref`, `score`, `verdict`) — the
Winlink catalog is instance #1, not the schema's shape.

**Thresholds.** Two plain numbers in T0 config per corpus/model/template: a
reject floor on top-1 similarity (no-match) and an ask trigger on top1−top2
margin (genuinely-close → ask one clarifying question). The ML supplies
scores, never verdicts. Calibration is a config regeneration when a corpus
or model changes — never retraining.

**Index enrichment** (conditional ruling): embed `SECTION ID: description`
(+8–17 top-1 points over description-only); T0 parses geo at index time;
station-locality context is appended to queries. Conditional on an
Inkling-parseability spike (does the real backend consume the enriched
candidate surface correctly) as ch3e9 prototype step 1; the zqox2
operator-vs-station location semantics ride along.

**Security-role models** (operator-ruled): **Apache-2.0-clean only**. Llama
Prompt Guard 2 is rejected on license (Llama-4 Community terms) regardless
of benchmark appeal — "no forbidden fruit." Fine-tuning our own on
bench-generated corpora is the destination; overdefense on ham-imperative
traffic ("QST QST, all stations reply") must be measured on the bench before
any injection classifier is trusted. Placement rules from the design record
hold: the security classifier is never Elmer's backend family;
same-model single-token judgments are acceptable for request/grant fit only;
all vendored code AGPLv3-compatible (weights are data).

**Sequencing** (operator-ruled): r5jsj tool-surface survey → ch3e9
request→catalog prototype → security boundary last, after the
quarantined-reader design lands.

## Consequences

- Offline-first survives with honest numbers behind it, and the same
  narrowing that serves safety makes small/CPU backends practical
  (tuxlink-8dkcy compounds this: shorter static prefixes warm faster).
- Schema/policy maintenance is a real ongoing cost (named in the literature
  as this defense class's burden), and a measured utility tax is expected —
  the bench measures it rather than assuming zero.
- Per-corpus threshold calibration becomes an operational step on catalog or
  model changes.
- The threat model carries the sweep's residual weaknesses explicitly:
  bounded 1-of-N covert channels through typed extraction, operator
  paste-through taint laundering, approval fatigue, and text-to-content
  harms that architecture alone cannot remove.

## Watched failure modes

- **Advisory→binding creep**: any change making request-classifier verdicts
  binding on a capable model must amend this ADR, not slip in as a patch.
- **Threshold rot**: catalog updates without recalibration silently degrade
  reject/ask behavior — recalibration hooks into the index build.
- **Run-config vs code misattribution**: the tuxlink-nyyr2 lesson (a stale
  `ELMER_MAX_TOKENS=3000` masqueraded as a provider defect for 176/180
  bundles) — before attributing classifier failures to models or code, diff
  the run configuration of the failing context against the working one.

## Alternatives considered

- **One do-everything classifier**: rejected — shared blind spots across
  roles, no degradation story, and the security boundary needs
  family-separation from Elmer's backend.
- **LLM-judgment everywhere including the security boundary**: rejected on
  the adaptive-attack evidence — in-band model detection collapses under
  adaptive pressure; deterministic out-of-band enforcement holds.
- **Prompt Guard 2 despite its license**: rejected by operator ruling.
- **Fine-tune-first**: rejected for sequencing — zero-shot already clears
  the bar for the request role and the bench's labeled-pair pipeline is not
  yet built.

## Pointers

tuxlink-efk3k (design record; addenda 1–6) · kickoff plan · T1 spike
FINDINGS · CPU-viability evals + field correction · ADR 0027 (tool-count
budget interplay) · ADR 0022 (features built whole — applies to the
prototype phases) · CLAUDE.md carries only a routing pointer per the
propagation contract.
