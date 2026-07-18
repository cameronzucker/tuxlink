// M2a spike: Pi provider extension for the Spark vLLM endpoint.
// Loaded per-run via `pi -e pi-spark.js`; nothing global is mutated.
// Both Spark-served models are registered; --model selects per cell.
// reasoning:false — CN is a non-thinking coder model; Q122 is served with
// the patched chat template that forces enable_thinking=false (ladder
// ledger 2026-07-16). Costs zero: local inference.
export default function (pi) {
  pi.registerProvider("spark", {
    name: "Spark vLLM (gx10-65aa)",
    baseUrl: "https://inference.twin-bramble.ts.net/v1",
    apiKey: "$SPARK_API_KEY",
    api: "openai-completions",
    models: [
      {
        id: "qwen3-coder-next",
        name: "Qwen3 Coder Next FP8 (Spark)",
        reasoning: false,
        input: ["text"],
        cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
        contextWindow: 262144,
        maxTokens: 32768,
      },
      {
        id: "qwen35-122b-nvfp4",
        name: "Qwen3.5 122B A10B NVFP4 (Spark, no-think template)",
        reasoning: false,
        input: ["text"],
        cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
        contextWindow: 131072,
        maxTokens: 32768,
      },
    ],
  });
}
