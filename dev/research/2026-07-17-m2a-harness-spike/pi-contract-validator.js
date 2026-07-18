// M2 extension: final-message contract validator (definitive-report.md
// build-list item 3, prioritized by M5 — it would have intercepted all four
// OpenRouter-Mistral failures and the Spark truncated finals).
//
// A finalized assistant message with NO tool call is about to end the agent
// run in -p mode. If it does not satisfy the job contract — a Status line
// (DONE | DONE_WITH_CONCERNS | BLOCKED) — or is suspiciously short (the
// truncated-final / bare "Task completed." shapes), inject ONE corrective
// follow-up restating the contract. Budget 2 per session to prevent loops.
const STATUS_RE = /Status\s*:?\s*(?:\(?\s*)?(DONE|DONE_WITH_CONCERNS|BLOCKED)/i;
const MIN_FINAL_CHARS = 120;

export default function (pi) {
  let corrections = 0;

  pi.on("message_end", async (event) => {
    const msg = event.message;
    if (!msg || msg.role !== "assistant") return;
    const content = Array.isArray(msg.content) ? msg.content : [];
    if (content.some((c) => c.type === "toolCall")) return; // run continues
    const text = content
      .filter((c) => c.type === "text")
      .map((c) => c.text || "")
      .join("\n")
      .trim();
    if (STATUS_RE.test(text) && text.length >= MIN_FINAL_CHARS) return;
    if (corrections >= 2) return;
    corrections += 1;
    await pi.sendUserMessage(
      "Your last message would END this session, but it does not satisfy " +
        "the completion contract. Before finishing you MUST: (1) write your " +
        "full report to the report file path given in your instructions, " +
        "and (2) end your final message with the Status line " +
        "(DONE | DONE_WITH_CONCERNS | BLOCKED), a one-line test summary, " +
        "concerns if any, and the report file path. If the work is not " +
        "actually complete, continue working instead of declaring done.",
      { deliverAs: "followUp" },
    );
  });
}
