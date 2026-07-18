// M2a probe #3 — reasoning-reviver extension (mandatory-work-item follow-up).
//
// Diagnosis (2026-07-18 bisect, see addendum-responses-probe2.md): the
// Qwen3.5 chat template only opens a fresh <think> block for an assistant
// turn that directly follows a USER message. In an agentic loop, every
// continuation follows a function_call_output, so the model never re-enters
// thinking — measured 44 reasoning tokens on turn 0 and ~0 on every
// subsequent turn, while the same requests with a trailing user message
// produce 250-450 reasoning tokens (bisect Y2/Y3, reproduced 2/2).
//
// Fix: on each LLM call whose context ends with a tool result, append a
// minimal, neutral user turn ("Continue.") to the OUTGOING REQUEST ONLY.
// Pi's transformContext is per-call and transient — the session history
// stays clean. The nudge text is deliberately content-free so the treatment
// is the template trigger, not prompt-level steering.
export default function (pi) {
  pi.on("context", (event) => {
    const msgs = event.messages;
    if (!msgs || msgs.length === 0) return;
    if (msgs[msgs.length - 1].role !== "toolResult") return;
    return {
      messages: [
        ...msgs,
        {
          role: "user",
          content: [{ type: "text", text: "Continue." }],
          timestamp: Date.now(),
        },
      ],
    };
  });
}
