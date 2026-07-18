// M2a mandatory work item (2): non-native tool-syntax detector/retry.
//
// The F1 sibling seam (report.md): a model emits XML-style pseudo-tool-calls
// as plain TEXT (e.g. <tool_call>...</tool_call>, <function=bash>...), the
// harness executes nothing, and the session dies on a silent empty final
// message. This extension detects that pattern on any finalized assistant
// message that contains NO native tool call, and injects a corrective user
// follow-up so the model retries through the native interface.
//
// Retry budget: 3 corrections per session, then it stays silent (prevents
// an infinite correction loop with a model that cannot recover).
const PSEUDO_TOOL_PATTERNS = [
  /<tool_call>/i,
  /<\/tool_call>/i,
  /<function_call>/i,
  /<function\s*=\s*["']?\w+/i,
  /<invoke\s+name\s*=/i,
  /<tools?>[\s\S]*<\/tools?>/i,
  /^```(?:xml|json)?\s*\{\s*"name"\s*:\s*"(?:bash|read|edit|write)"/im,
];

export default function (pi) {
  let corrections = 0;

  pi.on("message_end", async (event) => {
    const msg = event.message;
    if (!msg || msg.role !== "assistant") return;
    const content = Array.isArray(msg.content) ? msg.content : [];
    const hasNativeToolCall = content.some((c) => c.type === "toolCall");
    if (hasNativeToolCall) return;
    const text = content
      .filter((c) => c.type === "text")
      .map((c) => c.text || "")
      .join("\n");
    if (!text) return;
    if (!PSEUDO_TOOL_PATTERNS.some((re) => re.test(text))) return;
    if (corrections >= 3) return;
    corrections += 1;
    await pi.sendUserMessage(
      "Your last message wrote a tool call as plain text — nothing was " +
        "executed. Invoke tools ONLY through the native tool-calling " +
        "interface (no XML or JSON blocks in your message text). Retry now.",
      { deliverAs: "followUp" },
    );
  });
}
