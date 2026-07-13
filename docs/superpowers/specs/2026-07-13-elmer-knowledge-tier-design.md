# Elmer knowledge tier: grounded retrieval + general Winlink-client coverage

- **Date:** 2026-07-13
- **Agent:** sumac-magnolia-fen
- **Issues:** `tuxlink-aib3n` (Winlink Express + Pat agent docs), `tuxlink-0mudm` (P0 — docs/help retrieval tool + refuse-when-ungrounded)
- **Status:** approved (design), pending implementation plan

## Problem

Elmer cannot read documentation. It can only search it.

`docs_search` exists and works: it runs BM25 over an FTS5 index and returns up to 30
hits shaped `{slug, title, snippet}`. The snippet is `snippet(docs_fts, 2, …, 12)` —
a twelve-token window around the match ([`docs_index.rs:78`](../../../src-tauri/src/search/docs_index.rs)).
No tool converts a slug into the document body; `docs_search` is the only `docs_*`
tool registered in [`router.rs`](../../../src-tauri/tuxlink-mcp-core/src/router.rs).
The search tool is therefore a locator with no destination.

A second corpus, `docs/mcp-knowledge/`, is exposed over the MCP **resource** tier as
`tuxlink://` URIs. In-app Elmer never reads resources: its runner's only schema source
is `list_tools_as_specs()` ([`executor.rs:114`](../../../src-tauri/src/elmer/executor.rs)),
and `read_resource` / `list_resources` appear nowhere in the app or in either agent
crate. That corpus is readable by external MCP clients and invisible to Elmer.

The consequence is the failure recorded in `tuxlink-0mudm`: the model answers questions
about Tuxlink's own internals from nothing, and fabricates. Tuxlink is absent from
pretraining, so every ungrounded claim it makes about the product is invented.

This blocks `tuxlink-aib3n`. Adding Pat and Winlink Express documentation to a tier
Elmer cannot read reproduces the same confabulation with more source material behind it.

## Goals

1. Elmer retrieves full document text at answer time.
2. Elmer answers accurately about **other** Winlink clients — Pat and Winlink Express —
   without conflating them with Tuxlink.
3. A document added to the corpus is provably retrievable, enforced by test.
4. Connection and RF syntax is correct. Operators key these strings on the air.

## Non-goals

- Changing the MCP resource tier. External clients keep working unchanged.
- Chunking, embeddings, or vector search. BM25 over whole documents is sufficient.
- Training a refusal reflex. `tuxlink-0mudm`'s own notes supersede that clause:
  grounded honesty is delivered by tool plus system prompt, not by fine-tuning.
- Rebuilding `dev/elmer-distill/`. It judges tool-use trajectories and continues to.

## Architecture

One index, three sources, two tools.

| Source dir | Indexed (Elmer searches) | Help sidebar | Purpose |
|---|---|---|---|
| `docs/user-guide/` | yes | yes | Tuxlink's operator manual |
| `docs/knowledge/` (new) | yes | no | Agent-only reference: Pat, Winlink Express |
| `docs/mcp-knowledge/` | yes (new) | no | Playbooks and specs |

Indexing `docs/mcp-knowledge/` costs one registry entry per file and gives Elmer the
existing troubleshooting playbooks (`playbook-ardop-wont-connect`,
`playbook-cms-z-password-lag`) that it currently cannot see.

The tool surface gains its missing half:

- **`docs_search(query)`** — unchanged contract. BM25 → `{slug, title, snippet}`. The locator.
- **`docs_read(slug)`** — new. Returns the full markdown body. The destination.

`docs_read` serves the `body` column already stored in `docs_fts`. The full text is
present in the database today and has simply never been exposed, so no new storage,
no second corpus, and no drift between what is searchable and what is readable.

Both tools are read-only, app-owned, and non-tainting, matching `docs_search`'s
existing classification.

### Data flow

```
"Pat ax.25 connect via digipeater?"
  → docs_search("pat ax25 digipeater")  → {slug: "pat-winlink", snippet: "…via LA1B digi…"}
  → docs_read("pat-winlink")            → full document, grammar and examples included
  → grounded answer: ax25:///DIGI/TARGET
```

### Design constraint: the consuming model is small and tool-reliant

The target deployment is Qwen3-Coder-Next served from the Spark. It is capable but
carries no domain knowledge of Tuxlink or of Winlink internals, so the tools are its
sole source of information in this domain. Two consequences bind the implementation:

1. **The two-step protocol is encoded in the tool descriptions, not only in the system
   prompt.** Tool schemas are the one context the runner always presents.
   `docs_read`'s description states that `docs_search` supplies the slug;
   `docs_search`'s description states that snippets are fragments and that `docs_read`
   returns the full text.
2. **Documents are short, dense, and syntax-forward.** `docs_read` returns a whole
   document into a bounded context. Grammar and examples lead; prose is minimal.

## Components

### Backend

1. **`src-tauri/src/search/docs_bundle.rs`** — `BUNDLED_TOPICS` gains a `source` field
   (`UserGuide` | `Knowledge` | `McpKnowledge`) and entries for the new sources.
   Remains `include_str!` at compile time. The existing runtime slug-drift
   reconciliation in [`search/mod.rs:73`](../../../src-tauri/src/search/mod.rs)
   repopulates the index on app restart, so installed clients self-heal with no
   manual reindex step.
2. **`src-tauri/src/search/docs_index.rs`** — add `read_doc(slug) -> Option<DocBody>`,
   a single lookup by slug against the existing `body` column. No schema change.
3. **`src-tauri/src/mcp_ports.rs`** — `SearchPort` and `MonolithSearchPort` gain the
   corresponding `read` method beside `docs`.
4. **`src-tauri/tuxlink-mcp-core/src/router.rs`** — register `#[tool] docs_read`.

### System prompt

`ELMER_SYSTEM_PROMPT`
([`provider.rs:829`](../../../src-tauri/tuxlink-agent-frontend/src/provider.rs))
contains no mention of documentation today. It gains the clause `tuxlink-0mudm`
specifies: answer product, configuration, and how-it-works questions only from the docs
tools; search and then read before answering; when the documentation does not cover the
question, say so rather than guess.

### Content

Two agent-only documents in `docs/knowledge/`:

**`pat-winlink.md`** — the connect-URL grammar, transports, and the ax.25 digipeater
path. Ground truth is `pat v1.0.0`'s own `connect --help`, not recollection:

```
transport://[host][/digi]/targetcall[?params...]
```

`host` addresses the local TNC or modem. The path is the RF route; its last element is
the target callsign and any preceding elements are digipeater hops. Verbatim from the
binary:

```
connect ax25:///LA1B-10              Connect to the RMS Gateway LA1B-10 using AX.25 engine as per configuration.
connect ax25+linux://tmd710/LA1B-10  Connect to LA1B-10 using Linux kernel's AX.25 stack on axport 'tmd710'.
connect ax25:///LA1B/LA5NTA          Peer-to-peer connection with LA5NTA via LA1B digipeater.
```

The document covers: the empty-host triple slash; the axport-versus-path distinction;
engines (`ax25`, `ax25+linux`, `ax25+agwpe`, `ax25+serial-tnc`); multi-hop paths;
`?freq=` (accepted on `ax25` and `ardop` only); CLI and `interactive` equivalence;
config location; and the EmComm Tools packaging note.

**Accuracy correction recorded here deliberately:** `tuxlink-aib3n`'s description gives
the digipeater form as `ax25:///DIGI1,DIGI2/TARGET`. That is wrong — hops are separated
by `/`, not by commas. The issue text is not a source; the binary is.

**`winlink-express.md`** — session types (Telnet, Packet, VARA, ARDOP, Pactor), opening
a session, channel selection, digipeater path entry in a packet session, forms, and
account and password basics.

**Relationship to the existing `32-from-express-or-pat.md`.** That user-guide topic
already documents the Winlink client landscape and how each client differs from Tuxlink,
including a per-client comparison table. The new documents do **not** restate it. Their
purpose is orthogonal: `32-from-express-or-pat` answers *"I am moving to Tuxlink from
Pat — what changes?"*, whereas `docs/knowledge/pat-winlink` answers *"how do I operate
Pat?"* for an operator Elmer is helping at another station. Each new document
cross-references `32-from-express-or-pat` for the comparison rather than duplicating it.
Two documents making the same comparison in different words is precisely how a corpus
starts contradicting itself, and the retrieval tier surfaces whichever one BM25 happens
to rank first.

Conflation remains the specific harm `tuxlink-aib3n` exists to prevent, so each document
states its subject client in the title and first line, and the system-prompt clause
directs Elmer to name the client it is answering about.

Two claims remain unverified and are resolved during implementation, not asserted:
multi-hop paths beyond one digipeater follow from the stated grammar but are not
exemplified in Pat's help; and EmComm Tools is believed to ship Pat stock, which is
confirmed before the document ships because the motivating question was asked in ETC terms.

## Testing

### Registry drift guard

A test enumerates `.md` files across the three source directories and asserts each is
present in `BUNDLED_TOPICS`.

The existing `list_resources_returns_full_catalog` asserts `len == CATALOG.len()`, which
is self-referential and cannot detect an unregistered file. This is how
`docs/user-guide/36-off-air-space-weather.md` came to exist on disk, be absent from
`BUNDLED_TOPICS`, and therefore be absent from the FTS index and unfindable by
`docs_search` — while still rendering in the sidebar, because
[`topics.ts:129`](../../../src/help/topics.ts) discovers files by `import.meta.glob`
while the Rust registry is hand-maintained. The frontend self-heals; the backend rots.
Registering that file is part of this work.

The reverse direction needs no test: a registered slug with no file fails to compile,
because `include_str!` resolves at build time.

### Retrieval evals

No test measures document retrieval today. A golden set asserts question → expected
slug, and for syntax questions an expected substring of the document body:

| # | Question | Expected slug | Body must contain |
|---|---|---|---|
| 1 | "Pat Winlink in EmComm Tools, ax.25, connect via a digipeater — syntax?" (KJ4UYO) | `pat-winlink` | `ax25:///` |
| 2 | "Where does Tuxlink store my Winlink password?" | `27-settings` or `02-first-launch-wizard` | `keyring` |
| 3 | "ARDOP won't connect" | `playbook-ardop-wont-connect` | — |
| 4 | "How do I enter a digipeater path in Winlink Express?" | `winlink-express` | — |
| 5 | "I'm switching to Tuxlink from Pat — what changes?" | `32-from-express-or-pat` | — |

Assertions are "expected slug appears in the returned hits", not "is rank 1", except
where noted. BM25 rank ordering is not a stable contract and tests that pin it are
brittle.

These run as Rust tests against the real index: fast, deterministic, no model in the
loop. They assert retrievability, which is what is broken. Answer quality with a model
in the loop remains the domain of `dev/elmer-distill/`.

Question 2 is `tuxlink-0mudm`'s original symptom — the model inventing
`~/.config/tuxlink/tuxlink.cfg base64/mode 600` when the truth is the OS keyring.
`keyring` is documented today in `27-settings.md` and `02-first-launch-wizard.md`, so
this eval asserts the existing corpus is reachable rather than requiring new content.

Questions 1/4 and 5 together are the conflation guard: the operational documents and the
migration topic must be separately retrievable, and a question about *operating Pat* must
not land only on the *migrating from Pat* topic.

## Error handling

`docs_read` on an unknown slug returns a structured not-found result listing valid
slugs rather than an error. A wrong guess steers the model instead of derailing the
turn. `docs_search` on an empty query returns `[]`, unchanged.

## Rollout

`docs_read` is additive. Existing `docs_search` callers, the MCP resource tier, and
external clients are unaffected. Installed clients repopulate the index on next app
start via existing slug-drift reconciliation.

## Open questions

None blocking. The two accuracy items noted under **Content** (multi-hop exemplification,
ETC packaging) are verification tasks inside implementation, not design decisions.
