// Mistral round (operator-directed): Pi provider extension for the Spark
// vLLM endpoint serving mistral-small-4-119b (NVFP4, mistral-format load,
// --tool-call-parser mistral — profile mistral119, first served 2026-07-18.
// Serving required VLLM_MLA_DISABLE=1 (the GB10 nightly's only MLA backend,
// TRITON_MLA, crashes on this model's latent-attention dims) which
// materializes full KV and caps context at 32768 on this host.
// Chat-completions route for comparability with the other
// Spark cells. reasoning:false — served without a reasoning parser; if the
// model emits [THINK] text inline it lands in content and is measured from
// the transcript, matching how the other Spark arms were treated.
export default function (pi) {
  pi.registerProvider("spark-mistral", {
    name: "Spark vLLM Mistral (gx10-65aa)",
    baseUrl: "https://inference.twin-bramble.ts.net/v1",
    apiKey: "$SPARK_API_KEY",
    api: "openai-completions",
    models: [
      {
        id: "mistral-small-4-119b",
        name: "Mistral Small 4 119B NVFP4 (Spark)",
        reasoning: false,
        input: ["text"],
        cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
        // Serving max is 32768 (MLA disabled). Pi CANNOT auto-compact
        // mid-run in -p mode (compaction is checked on agent_end only), so
        // no registered margin can save a long session — register the true
        // ceiling for maximum headroom. Small fixed output cap keeps Pi's
        // per-turn clamp (window - estimated input) from collapsing to ~1
        // token near the ceiling (the "Let"/"Now" one-token final-message
        // deaths). Pi's estimate also undercounts vs tekken by ~2%.
        contextWindow: 32768,
        maxTokens: 4096,  // raised for a2: trimmer holds input ~24k, so 4k output fits; 2048 truncated a1 final analysis
      },
    ],
  });
}
