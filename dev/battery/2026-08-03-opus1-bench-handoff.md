
## Addendum (operator finding, 2026-08-03): the v1 harness was implicitly tuned to Qwen 122B

The dev-loop model's idioms became the harness's cost-free path; every other
family paid taxes. Quantified: the ae1pt idiom adjustment moves control2
(Qwen) by only +2.2 strict points but DSV4 by +17.8 and gptoss by +12.4 —
the fingerprint of a validator green-path that matches the dev-loop model's
habits. The absorption layer shows the same history: Qwen's stringified args
got lenient deserializers and documented tolerance; Mistral's strict-encoder
400s, Laguna's stringify, and DSV4's arg quirks became terminal
provider_error/invalid_action outcomes instead. Bounds: Inkling still tops
the raw board and the graveyard cells punish Qwen equally, so the bias
handicaps rather than disqualifies — v1 strict scores conflate capability
with Qwen-compatibility by roughly 10-18 points for non-Qwen families.

**Appliance requirements:** (1) develop all absorption/compat behavior
against a MULTI-MODEL panel from day one — never a single dev-loop model;
(2) when a model-family quirk surfaces, the default disposition is absorb
(lenient parse + recoverable error back to the model), not terminal outcome,
unless the quirk is itself the thing being measured; (3) report
idiom-adjusted scores alongside strict until per-family compat parity is
demonstrated; (4) stage-gate fix cycles attribute failures per-family, not
against the house model.
