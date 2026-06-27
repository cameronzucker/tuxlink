# Running a Tuxlink assistant on local hardware

The Tuxlink MCP server lets an AI assistant operate the station: read status,
look up gateways, predict paths, and stage messages, with transmission and
config writes gated behind operator authorization. That assistant can run in the
cloud or on **local hardware the operator owns**. Local hosting matters for
offline and contingency deployments — a field kit on a Starlink-Mini link, an
EOC with no reliable internet, or an operator who prefers their data never leave
the bench.

This reference answers three questions: what local hardware is needed, which
models fit, and how a local assistant compares to a cloud one.

## The short answer

A single GB10-class mini-PC — **NVIDIA RTX Spark or DGX Spark, 128 GB unified
memory** — running **`gpt-oss-20b`** hosts a capable Tuxlink assistant over the
MCP for short, well-scoped tasks: diagnosing a connection, finding a gateway,
predicting a band, composing a message. The MCP authorization model behaves
identically whether the assistant is local or cloud. A local assistant trades
away **depth of expert judgment and reliability on long multi-step chains** — not
safety.

## Hardware envelopes

| Class | Memory | Interactive agentic model | Assistant capability |
|---|---|---|---|
| RTX 50-series laptop | ≤24 GB GDDR7 | Qwen3-Coder-Flash-30B (4-bit, ~18 GB) | Below the comfortable tier; light assist only |
| **Single RTX / DGX Spark** | **128 GB unified** | **`gpt-oss-20b` (MXFP4 MoE)** | **Capable, interactive — the recommended local target** |
| Dual Spark (ConnectX-linked) | 256 GB pooled | Qwen3-Coder-480B (1–2 bit) | Higher reasoning depth, but non-interactive decode speed |
| Cloud (frontier) | — | Sonnet 4.5+ / Opus tier | Deepest domain reasoning, best long-horizon reliability |

**RTX Spark and DGX Spark are the same GB10 Grace Blackwell silicon in two
packages** — RTX Spark is the Windows consumer machine; DGX Spark is the
Linux developer workstation. Either fits the same models. The binding constraint
is **memory bandwidth (~273–300 GB/s), not capacity**: a 128 GB box *holds* a
200B model under heavy quantization, but dense decode at that size is too slow to
feel interactive.

## Which model, and why MoE

A Mixture-of-Experts model is the right choice on a bandwidth-limited box. A
dense 70B reads every weight on every token (single-stream decode ≈ 3 tokens/s on
a Spark — unusable for a tool loop), whereas an MoE reads only its active experts
per token. `gpt-oss-20b` (≈3.6B active) decodes at roughly 50–65 tokens/s and is
purpose-built for function-calling, which is what a Tuxlink assistant does. The
larger `gpt-oss-120b` fits a single Spark but its single-stream speed is not
reliably characterized; treat it as a candidate to validate, not a default.
`Qwen3-Coder-Flash-30B` is the practical fit for a 24 GB laptop.

## What a local assistant does well, and where it falls short

- **Safety and authorization comprehension: equal to cloud.** A local-capable
  model internalizes the arm/taint model from the agent guide as completely as a
  frontier model. It checks whether send-authority is armed, avoids tainting the
  session when a diagnosis does not require untrusted content, and relays
  not-authorized denials rather than forcing past them.
- **Short, scoped tool calls: strong.** Reading status, ranking gateways,
  predicting paths, and staging a draft are well within reach.
- **Deep domain reasoning: shallower.** A frontier model distinguishes NVIS from
  skip-distance propagation when choosing a gateway, or cross-references modem
  configuration to spot a connect blocker. A local model reaches a serviceable
  answer with less of that expert judgment.
- **Long-horizon autonomous chains: unreliable — for every model.** No model,
  cloud or local, reliably completes long multi-step tool sequences; local models
  trail the frontier by a wide margin. Scope tasks short and keep the operator in
  the loop.
- **Fast-moving external facts: correctable.** A local model carries a smaller,
  staler knowledge base than a cloud one. Expect to correct it on recent
  hardware, callsigns, and network specifics. The operator's ground truth
  governs.

## Safety is independent of where the assistant runs

The arm/taint authorization model is enforced by the **MCP server in this
application**, not by the model. Transmission and config writes require the
operator to arm send-authority in the GUI; reading untrusted message content
taints the session and re-locks egress. A local assistant has no more authority
than a cloud one: it cannot key the transmitter or change configuration on its
own. The human-in-the-loop arm gate is also a capability fit — it keeps the
assistant in the short, confirmed regime where a local model performs best.

## Deployment guidance

- Target a **single RTX/DGX Spark with `gpt-oss-20b`** for a field assistant.
- **Expose resource reads** (`tuxlink://` URIs) to the local MCP client, not only
  tools. The curated knowledge — band plans, device guides, playbooks, this
  reference — is load-bearing for assistant quality; without it a local model
  falls back to generic amateur-radio guesses on station-specific questions.
- Keep tasks short and operator-confirmed; do not delegate long autonomous tool
  chains.
- Plan for correction: surface the assistant's reasoning so the operator can
  catch a stale or overconfident claim.

---

Provenance: distilled from the local-edge-AI vs cloud-agent equivalence study
(`dev/research/2026-06-27-local-edge-ai-vs-cloud-agent-equivalence.md`), which
combines a hardware/benchmark survey with a live-MCP experiment across matched
capability tiers. Hardware figures reflect the GB10 generation; verify current
specifications and model rankings before a purchase or deployment decision.
