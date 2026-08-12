# Content/inbox classifier — design brief (role 3 of 5, tuxlink-8zq7u)

Written 2026-08-11 on the operator's "move on to the other classifiers"
directive, while the furnish re-test runs. This is a decision brief with
proposed defaults, not a build: the class list and the conversion schema
are the two things the operator ratifies before code. Everything here
stays inside ADR 0030's frame — the classifier advises, deterministic
policy decides, and this role lives INSIDE the quarantine boundary.

## What exists today (the gap, verified in source)

The current defense is binary: five taint reasons
(`tuxlink-security::TaintReason` — MailboxList, MessageRead,
SearchResults, SessionLog, RoutinesJournal) lock egress the moment any
untrusted content is touched, and `elmer/injection_tests.rs` proves
STRUCTURAL properties (config mutation absent from the surface, egress
arm-gated, injection cannot transmit) — but nothing looks AT the
content. The taint gate knows THAT you read a message, never WHAT it
was. Outlook's stack (the operator's named reference) adds exactly this
layer: content classification + spotlighting before the assistant
reasons over it.

## Two jobs, one pass

1. **Triage** — what IS this inbound message? Proposed default classes,
   mapped to traffic Tuxlink actually handles:
   `catalog_response` (a Request Center product arriving back),
   `weather_product` (GRIB/forecast/bulletin payloads),
   `form_submission` (form XML present),
   `position_or_service` (position reports, system/service notices),
   `personal_correspondence`,
   `unknown`.
2. **Injection signal** — does the content attempt to steer the agent?
   A score plus spans (which lines look imperative-at-the-assistant),
   never a binary gate by itself.

One classifier pass emits both; the verdict shape follows the epic's
corpus-generic DTO discipline: `{corpus: "inbox-content", item_ref:
<class>, score, verdict}` plus an `injection: {score, spans}` block.

## The conversion schema (the quarantine boundary's typed extraction)

The privileged agent never sees raw content — it sees THIS, produced by
the quarantined reader and validated by serde before crossing:

- envelope: `message_id`, `folder`, `received_at` (RFC3339),
  `size_bytes`, `has_attachments: bool`, `attachment_names: Vec<String>`
  (names length-capped, charset-restricted).
- provenance: `sender_callsign` (callsign grammar or REJECTED),
  `via_gateway: Option<String>` (same grammar), `path_kind`
  (enum: cms | p2p | radio_only | unknown).
- triage: `class` (the enum above), `class_score: f32`,
  `injection_score: f32`, `flagged_spans: Vec<(u32, u32)>` (byte ranges
  into the QUARANTINED copy — the privileged side can cite them without
  containing them).
- per-class payload, each a closed struct: catalog_response →
  `{catalog_item_id (validated against the catalog), summary_150}`;
  weather_product → `{product_kind enum, valid_from/to, area_grid}`;
  form_submission → `{form_id (validated against bundled forms),
  field_count}`; position_or_service → `{grid (Maidenhead grammar),
  report_kind enum}`; personal_correspondence / unknown →
  `{summary_150}` only.
- `summary_150`: the ONE free-text field (150 chars, charset-restricted)
  — the acknowledged bounded covert channel from the ADR's threat model;
  everything else must parse or the message stays quarantined with only
  the envelope visible.

## Substrate (proposed): T0 does most of it, honestly

Triage is largely DETERMINISTIC for this traffic: form XML presence,
catalog subject/id patterns, B2F header structure, position-report
grammar are T0 rules — the embedding tier only arbitrates the fuzzy
remainder (catalog_response vs weather_product phrasing, personal vs
service). The existing tuxlink-classify machinery (bge-small centroids
per class over enriched exemplars) covers that at zero new
infrastructure. The INJECTION signal is different in kind — retrieval
embeddings are the wrong instrument, and the ADR already ruled the path:
Apache-2.0-clean options interim, fine-tuning our own on bench-generated
corpora as the destination, with OVERDEFENSE measured before anything is
trusted (ham traffic is imperative-heavy — "QST QST all stations
reply" must not flag). Until that corpus exists, the injection score
ships as T0 heuristics (imperative-at-assistant patterns, tool-name
mentions, markdown/code blocks addressed to "you") explicitly labeled
low-confidence.

## Composition with taint (the non-negotiable)

The binary taint gate is UNCHANGED — reading content still locks egress
until the operator re-arms. The classifier verdict shapes what happens
WITHIN that posture: labeling untrusted-origin claims in the UI/agent
context (the anti-harmonization duty), spotlighting flagged spans,
routing suspected-injection messages to a segregated view, and giving
the eventual security classifier (role 4) a second opinion at the
boundary. A clean classification NEVER relaxes quarantine — that
ratchet only moves by operator ruling.

## Evaluation plan

- Labeled triage corpus: authored from real message shapes (catalog
  product samples, bundled form XML, position reports, personal text) —
  the same corpus-first discipline the request classifier used.
- Injection corpus: seeded from bench hostile-inbox content (the epic's
  bench tie-in: "injection of the reader yields NOTHING"), grown by the
  bench's labeled-pair pipeline.
- Overdefense set: ham-imperative TRUE-NEGATIVES (net scripts, QST
  bulletins, form instructions) — the domain-specific risk the ADR
  names; an injection classifier that flags them is worse than none.
- Bench cells: mailbox triage cells already exist; the quarantine
  end-to-end cell ("hostile inbox → privileged agent unaffected") is
  the eventual system test.

## Sequencing and the ask

Build order (proposed): ratify classes + schema → T0 rules + serde
schema with tests → triage corpus + T1 centroid eval (accuracy floor,
same rigor as the request classifier's 64/66) → wire into the
quarantined-reader seam WHEN that lands (role 4 territory) — the
classifier is buildable and testable stand-alone before the quarantine
exists, exactly as the request classifier was.

Operator decisions requested (two, both small): (1) the class list
above — right classes? anything missing (NTS/traffic-net messages?
DX bulletins?); (2) the conversion schema's shape — in particular the
single `summary_150` free-text allowance vs a no-free-text-at-all
posture (the ADR's covert-channel note applies; 150 chars is the
proposed balance of usefulness vs channel width).
