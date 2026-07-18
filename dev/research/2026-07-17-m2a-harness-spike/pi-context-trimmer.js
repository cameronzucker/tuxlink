// M2 extension: v1 mid-run context trimmer (definitive-report.md build-list
// item 2, motivated by M4 — Pi never auto-compacts inside a -p run, which is
// fatal at small serving windows like the Spark Mistral profile's 32k).
//
// On each LLM call (Pi's `context` event; transient per-request transform,
// session history untouched): estimate total context size by characters; if
// over budget, elide the OLDEST tool results down to a stub, preserving the
// last KEEP_RECENT tool results and all non-toolResult messages in full.
// Char-based estimation is deliberately conservative (~3.2 chars/token) so
// tokenizer divergence (M3) stays inside the margin.
// v2 (a1 lesson): elide by SIZE, not age — in the failing run the oldest
// tool results were tiny bash outputs while the newest were the giant file
// reads that actually blew the window; recency-based elision protected
// exactly the wrong messages. Spare only the most recent 2 tool results.
// v3 calibration: Pi's context event EXCLUDES the system prompt and tool
// schemas (~8-10k tokens invisible to extensions). Measured at death:
// 70.6k visible chars = 28.8k total tekken tokens. Budget must therefore
// bind on VISIBLE chars well below the ceiling: 55k visible ≈ 24k total.
const CHAR_BUDGET = 55000;
const KEEP_RECENT = 2;       // most recent tool results kept verbatim
const STUB_CHARS = 400;

function sizeOf(msg) {
  try {
    return JSON.stringify(msg.content ?? "").length;
  } catch {
    return 0;
  }
}

export default function (pi) {
  pi.on("context", (event) => {
    const msgs = event.messages;
    if (!msgs || msgs.length === 0) return;
    let total = msgs.reduce((n, m) => n + sizeOf(m), 0);
    if (total <= CHAR_BUDGET) return;

    const toolResultIdx = [];
    for (let i = 0; i < msgs.length; i++) {
      if (msgs[i].role === "toolResult") toolResultIdx.push(i);
    }
    // Largest-first among all but the most recent KEEP_RECENT tool results.
    const elidable = toolResultIdx
      .slice(0, Math.max(0, toolResultIdx.length - KEEP_RECENT))
      .sort((a, b) => sizeOf(msgs[b]) - sizeOf(msgs[a]));
    if (elidable.length === 0) return;

    const out = msgs.slice();
    for (const i of elidable) {
      if (total <= CHAR_BUDGET) break;
      const m = out[i];
      const before = sizeOf(m);
      if (before <= STUB_CHARS + 120) continue;
      const text = (Array.isArray(m.content) ? m.content : [])
        .filter((c) => c.type === "text")
        .map((c) => c.text || "")
        .join("\n");
      out[i] = {
        ...m,
        content: [
          {
            type: "text",
            text:
              text.slice(0, STUB_CHARS) +
              `\n…[elided by harness: tool result truncated from ${text.length} chars to fit the context window — re-run the tool if you need the full output]`,
          },
        ],
      };
      total -= before - sizeOf(out[i]);
    }
    return { messages: out };
  });
}
