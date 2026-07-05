# Local LLMs on low-power edge hardware — measured realities

> **Status:** Empirical findings from real testing on one reference host
> (2026-07-01), for the Elmer local-model path (Ollama). This territory is
> largely undocumented, so the numbers below are indicative rather than
> exhaustive: a single hardware sample, specific model builds, and one Ollama
> version. They establish the shape of the problem, not a universal benchmark.
> Companion analysis of the local-vs-cloud tradeoff lives in
> [`dev/research/2026-06-27-local-edge-ai-vs-cloud-agent-equivalence.md`](../../dev/research/2026-06-27-local-edge-ai-vs-cloud-agent-equivalence.md).

## Why this document exists

Elmer can run against a local model served by Ollama instead of a cloud provider,
for privacy and offline operation. Whether that is *useful* on a given piece of
hardware is a separate question from whether it *runs*. Low-power edge hardware
(fanless mini-PCs, single-board computers, 10–20 W SoCs) is the class an operator
is most likely to have on hand and the class where the answer is least obvious.
The findings here come from driving real models through real prompts on real
hardware, including several results that contradict first-instinct assumptions.

## Reference host

| Component | Detail |
|---|---|
| SoC | Intel Core i3-N305 (Alder Lake-N, 8 Gracemont E-cores, ~15 W, AVX2, no AVX-512) |
| RAM | ~46 GiB usable DDR5 (single dual-rank module) |
| iGPU | Intel UHD Graphics (ADL-N), Xe-LP, 32 EUs, shares system RAM |
| Serving | Ollama 0.30.11 with `OLLAMA_FLASH_ATTENTION=1`, `OLLAMA_KV_CACHE_TYPE=q8_0` |
| Power | USB-C PD → barrel, 15 V (board nominal 12 V) |

An i3-N305 with ~46 GiB is a generous member of the edge class. Smaller boards
(8–16 GiB, weaker iGPU) will fare worse; the constraints below tighten, they do
not relax.

## Memory: what fits

A ~46 GiB host runs **one** ~30B-parameter model at 8-bit quantization at a time.
A 30B q8 model occupies roughly 34 GiB resident at a 32k context window
(weights plus KV cache plus the inference graph). Two such models do not
co-reside. Switching models therefore requires unloading the previous one;
Elmer requests `keep_alive: 0` on the native path so a model switch frees the
prior model immediately.

Context length is cheaper than it appears for some architectures. Mamba-hybrid
models (for example the `nemotron_h_moe` family) carry a fixed, small state on
most of their layers rather than a per-token KV cache, so a large context adds
far less memory than a flat "every layer is attention" estimate predicts. Do not
size these models with the flat formula; measure the resident footprint after a
real load.

A hard memory ceiling on the serving process (a systemd `MemoryMax` cgroup limit)
turns an over-large load into a recoverable process kill instead of a host crash.
Keep such a limit set **above** the intended working set, not below it: a limit
tuned for a smaller model will kill a larger one on load and present as
instability.

## Power: transient current, not average budget

The most disruptive failure observed was violent and deterministic: certain
models, at certain quantizations, crashed the host instantly with **nothing in
the system logs** — no out-of-memory record, no kernel panic, no thermal event.
A logless, instantaneous reset is the signature of lost power, not a software
fault. Software crashes leave a trace; this did not.

The cause was the power supply, not the model. Under heavy inference the SoC
draws sharp current transients (measured spikes into the 65 W range on a host
whose average draw is far lower). A USB-C PD supply with marginal transient
response trips its own over-current protection on such a spike and cuts power for
an instant, which the SoC experiences as the rail vanishing mid-instruction.
Different quantizations of the same model produce different current waveforms, so
one quant crashes reliably while another does not — the tell that the compute is
driving the hardware past a limit, not that the code has a bug.

Replacing the supply with one that has real transient headroom (a 140 W-class
AVS/EPR unit) eliminated the crashes at the **same** voltage. The lesson: for
sustained inference, size the supply for peak transient response, not for average
watts, and prefer a stiff fixed-voltage source over a marginal one.

## The prefill wall

The dominant cost on edge hardware is **prompt prefill** — the one-time pass over
the entire input before the first output token. Generation speed matters, but for
an agentic assistant with a large tool surface the prefill dominates, because the
model re-reads a large prompt every turn.

Elmer's prompt is tool-rich by design: a compact system prompt plus the JSON
schemas for its full tool surface. Measured at runtime (Ollama's
`task.n_tokens`), a single-turn tool call runs on the order of 5,000 tokens, and
a representative tool-heavy turn lands near 8,000; a multi-turn agentic session
climbs past 20,000 as tool results accumulate in context. The tool schemas are
the overwhelming majority of the fixed cost. That surface is a deliberate product
choice, not a defect to trim. It does, however, set the prefill bill, which grows
with the conversation rather than sitting at a flat per-turn figure.

Prefill throughput measured on the reference host (tokens/second; higher is
faster), with the first-token latency estimated for a representative ~8k turn:

| Model | Quant | Prefill (CPU) | Prefill (iGPU) | Est. ~8k prefill (iGPU) |
|---|---|---:|---:|---:|
| qwen3:30b-a3b | q8 | *did not load* | 19.4 | ~7 min |
| nemotron-3-nano:30b | q8 | *did not load* | 26.0 | ~5 min |
| nemotron-3-nano:30b | q4 | 18.3 | 45.7 | ~3 min |
| gpt-oss:20b | — | 13.5 | 50.6 | ~2.5 min |
| qwen2.5:14b | q4 | 3.7 | 50.5 | ~2.5 min |

On CPU alone, an 8k prefill runs from roughly 7 minutes (fastest loading model)
to over half an hour (slowest), before a single token of output. That is not
interactive. Note that the two 30B MoE rows (qwen3-a3b, nemotron-a3b) report
iGPU figures from runs that *happened* to complete: those models crash
intermittently on the iGPU (see "MoE models crash on the Vulkan iGPU path"
below), so their iGPU numbers are indicative of a path that is not currently
reliable.

## iGPU offload

Enabling the integrated GPU is the single most effective lever, and on this class
of hardware it requires no additional software. Recent Ollama ships a Vulkan
backend and detects the Intel iGPU automatically; it declines to use integrated
GPUs unless told to:

```
Environment="OLLAMA_IGPU_ENABLE=1"
```

Set on the Ollama service and restart. No oneAPI, IPEX-LLM, or Vulkan SDK install
is needed if the backend is already present (`OLLAMA_VULKAN:true` in the server
config log, with the iGPU listed as a Vulkan device).

Two results, both confirmed against the model-load logs (offloaded-layer counts
and buffer placement, not inferred):

1. **The iGPU accelerates prefill by 2.5× to 13×** (see the table). Prefill is
   compute-bound — a large batched matrix multiply — so the iGPU's parallel
   execution units outrun a handful of CPU cores. Because the iGPU shares the same
   DDR5, there is no memory-bandwidth penalty for the offload.

2. **The iGPU is neutral to slightly slower at generation.** Token generation is
   memory-bandwidth-bound (one token at a time, streaming weights from the same
   RAM), so the GPU's compute advantage does not apply and its overhead can make
   it marginally slower than CPU. On the reference host, generation ran a few
   tokens/second either way, with CPU ahead on some models.

Since the edge bottleneck is prefill, the net effect strongly favors enabling the
iGPU. A secondary benefit: the 30B q8 models that were killed on a CPU-only load
under the memory-limit cgroup loaded successfully on the iGPU, because the GPU's
allocations are accounted differently than CPU process memory. On this host the
iGPU is what lets the higher-quality models run at all.

One operational cost accompanies the offload, and it is significant. On the
version tested, the iGPU compute path **leaks GPU memory at the driver level**.
When a model's inference process exits, its i915 GEM buffer objects are not
reclaimed: they persist as non-reclaimable shared memory (`Shmem` in
`/proc/meminfo`, the `shared` column in `free`) as orphan objects with **no owning
process** — the inference subprocess is gone, no client holds them, yet the pages
remain. They accumulate across runs, reducing `MemAvailable` below what the next
model needs, so a subsequent load fails even though nothing is resident and the
bulk of `cached` is ordinary reclaimable page cache.

None of the gentle remedies reclaim it: an Ollama service restart does not (the
leak is below the process, in the driver); the i915 GEM shrinker
(`/sys/kernel/debug/dri/N/i915_gem_drop_caches`) retires the object handles but
frees no pages, because the orphan objects are marked in-use with no client to
release them. A GPU reset or reloading the i915 module would reclaim it but takes
the display session down. In practice the only reliable clear on a desktop system
is a **reboot**.

The signal to watch is `Shmem` climbing with nothing in `/api/ps`. Budget for a
reboot when swapping models on the iGPU path, and prefer a **headless**
configuration: running compute on the same iGPU that drives a live desktop
compositor and remote-desktop server multiplies the GPU clients and the leak
surface. This cost is the counterweight to the prefill speedup — real, and to be
planned around, not a reason to forgo the iGPU.

## MoE models crash on the Vulkan iGPU path

This is the sharpest constraint found, and it cuts against the models that are
otherwise the best fit for edge hardware. Mixture-of-Experts (MoE) models —
those that activate only a few billion of their total parameters per token — are
exactly what makes a 30B model viable on a 15 W chip. On the reference host, every
MoE model tested **aborts on the Vulkan iGPU backend** with:

```
GGML_ASSERT(id >= 0 && id < n_expert) failed
```

The process core-dumps; the request returns an HTTP 500. It reproduced across two
different MoE architectures (a Mamba-hybrid MoE and a standard-attention MoE),
which is the tell that it is a **backend bug, not a model bug** — `n_expert` only
exists in MoE models, so a dense model cannot trip this assert and none did. The
crash is intermittent because expert routing is input-dependent: one prompt
completes, the next aborts mid-generation, often right as the model goes to emit a
tool call.

This is a known upstream defect
([ggml-org/llama.cpp#18786](https://github.com/ggml-org/llama.cpp/issues/18786)),
a regression the reporter also observed only on Vulkan with MoE models while dense
models ran fine. One instance was fixed upstream in January 2026
([PR #18945](https://github.com/ggml-org/llama.cpp/pull/18945)) — but the Ollama
build tested here (0.30.11, released June 2026, five months after that fix) still
aborts, and the current release (0.31.1) advertises no Vulkan MoE fix. So an
Ollama upgrade is not a confirmed remedy; either its vendored GGML fork does not
carry the fix, or this is a distinct live variant of a cluster of Vulkan-on-Intel
MoE issues.

The consequence is severe: the fast path (iGPU) and the capable-at-low-memory
architecture (MoE) do not currently work together in this stack. The paths that
*do* work:

- **Dense models on the iGPU** — no experts, no assert. Fast prefill, crash-free.
  The trade is capability: a dense 14B is less able than a 30B-A3B MoE, and a
  dense 32B is heavy.
- **MoE models on CPU** (`num_gpu: 0`) — slow, but stable; the CPU backend does
  not hit the bug.
- **Vanilla upstream llama.cpp** rather than Ollama — it carries the #18945 fix,
  and `--n-cpu-moe` can keep expert matrices on the CPU while attention runs on
  the GPU, sidestepping the crashing kernel.

Until the backend is fixed, treat MoE-on-iGPU as unavailable and plan around dense
models or the CPU fallback.

## Quantization is a speed dial, not just a quality dial

Higher quantization improves output quality at a direct speed cost. An 8-bit
model prefills roughly half as fast as the 4-bit build of the same model
(nemotron-3-nano: 45.7 t/s at q4 versus 26.0 t/s at q8 on the iGPU), because
generation and prefill on this class of hardware are gated by how many bytes must
move per operation. There is a genuine three-way tension between quality, speed,
and memory; no single setting wins all three.

## Practical configuration

- **Supply:** a stiff supply with transient headroom (well above the SoC's peak
  transient, not its average), or a fixed-voltage bench/laptop-style brick, in
  preference to a marginal USB-C PD adapter.
- **iGPU:** enable it (`OLLAMA_IGPU_ENABLE=1`). It is the largest available speed
  lever on the prefill bottleneck and costs nothing to try.
- **Memory limit:** set the serving cgroup limit above the working set, keep swap
  disabled, and keep flash attention on.
- **Context:** size `num_ctx` to the real workload; the in-app estimate is
  conservative and reads pessimistically for Mamba-hybrid models, so trust a
  measured load over the formula for those.
- **Model choice:** on the reference host the 30B q8 models are the quality
  target but remain the slowest to prefill; smaller or lower-quant models trade
  quality for a materially faster first token.

## Where local fits

The honest conclusion from the measurements: on low-power edge hardware, a local
model behind a large tool surface is a **non-interactive tier**. On the iGPU, a
single-turn tool call against a 30B q8 model takes several minutes of prefill
before the first token, and a multi-turn agentic session lengthens as context
grows. And the 30B models worth running are MoE, which currently crash on the
iGPU (above), so the practical working combinations are narrower still: dense
models on the iGPU (fast, less capable) or MoE on the CPU (capable, slow). Either
way the latency suits deferred work — a task set running while the operator is
away, an offline query answered in its own time — where privacy or the absence of
a network is worth the wait.

Interactive, conversational use of an agentic assistant remains the cloud path.
Local edge hardware earns its place for offline and privacy-sensitive work that
tolerates latency, not as a drop-in replacement for a responsive cloud model.

## Method and caveats

Throughput figures come from Ollama's own timing fields (`prompt_eval_count` and
`prompt_eval_duration` for prefill; `eval_count` and `eval_duration` for
generation) over a fixed multi-thousand-token prompt, with each model loaded cold.
CPU-only figures were produced by forcing `num_gpu: 0` on the request while the
iGPU was otherwise enabled, and were confirmed against the load logs (CPU buffer,
zero offloaded layers). GPU figures were likewise confirmed against the logs
(offloaded-layer counts and Vulkan buffer placement) after an earlier run was
mislabeled — a reminder to verify device placement from the logs rather than
assume it.

These results reflect one host, specific model builds, and one Ollama version.
Treat them as the shape of the problem for the edge class, not as fixed constants.
A structured, repeatable model-quality-and-speed evaluation (distinct from the
throughput measurements here, which do not assess output quality or tool-use
correctness) is tracked separately as future work.
