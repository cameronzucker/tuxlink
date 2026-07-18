// M2a post-hoc probe #2 (Responses route): Pi provider extension registering
// the E122 OpenRouter model over `api: "openai-responses"` instead of the
// builtin catalog's `openai-completions`. Every other field mirrors the
// builtin `openrouter` entry for qwen/qwen3.5-122b-a10b (pi-ai 0.80.10
// models.generated.js) so the wire route is the ONLY changed variable.
// Loaded per-run via `pi -e pi-openrouter-responses.js`; nothing global.
export default function (pi) {
  pi.registerProvider("openrouter-responses", {
    name: "OpenRouter (Responses API route)",
    baseUrl: "https://openrouter.ai/api/v1",
    apiKey: "$OPENROUTER_API_KEY",
    api: "openai-responses",
    models: [
      {
        id: "qwen/qwen3.5-122b-a10b",
        name: "Qwen3.5 122B A10B (OpenRouter, Responses route)",
        reasoning: true,
        compat: { supportsDeveloperRole: false },
        input: ["text"],
        cost: { input: 0.26, output: 2.08, cacheRead: 0, cacheWrite: 0 },
        contextWindow: 262144,
        maxTokens: 65536,
      },
    ],
  });
}
