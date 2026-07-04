# DGX Spark (GB10) fine-tuning readiness

Verified research pass, 2026-07-04 (deep-research, 21 primary/forum sources, 25 claims
adversarially verified — 23 confirmed, 2 refuted). Purpose: de-risk porting the Elmer QLoRA
stack from the x86 RTX PRO 6000 Blackwell to a DGX Spark (GB10 Grace-Blackwell, aarch64,
128 GB unified memory, sm_121) **before** the hardware arrives.

**Bottom line: the aarch64 port is well-supported and largely turnkey — NOT wheel-hell.** NVIDIA
ships first-party fine-tuning playbooks; bitsandbytes has prebuilt aarch64 sm_121 wheels (no source
build). The one genuinely-unproven axis is *our exact case*: gpt-oss MXFP4 **expert-LoRA training**
on GB10 (see "Unproven" below). Re-verify pins at provisioning time — this is a fast-moving target.

## The supported path (use this, don't hand-roll)

NVIDIA ships first-party playbooks — treat these as the source of truth, not the generic NGC docs:
- **`build.nvidia.com/spark/pytorch-fine-tune`** and **`build.nvidia.com/spark/unsloth`**
- backed by **`github.com/NVIDIA/dgx-spark-playbooks`**

They run inside the NGC container **`nvcr.io/nvidia/pytorch:25.11-py3`** (CUDA 13.0), which supplies
a Blackwell/aarch64 PyTorch+CUDA base — so **no manual torch/CUDA wheel selection**. Only the
higher-level libs are pip-installed on top:

```bash
docker pull nvcr.io/nvidia/pytorch:25.11-py3          # tag 25.11 = Nov 2025 (25.09 had GPU-detect bugs)
# PyTorch playbook:
pip install transformers peft datasets trl bitsandbytes
# Unsloth playbook (--no-deps preserves the container's tuned torch/CUDA):
pip install transformers peft hf_transfer "datasets==4.3.0" "trl==0.26.1"
pip install --no-deps unsloth unsloth_zoo bitsandbytes
```

On DGX Spark, aarch64 CUDA wheels live at the **cu130** index (`--index-url .../whl/cu130`); CUDA 12
libraries do not exist on the platform (libcudart.so.12 import fails). This differs from our x86 pod
(cu128) — the container handles it.

## Component status (verified)

| Component | Status on GB10 aarch64 |
|---|---|
| **PyTorch + CUDA** | Turnkey via the NGC container (CUDA 13.0). No manual wheel choice. |
| **bitsandbytes** | **No source build.** Prebuilt aarch64 (sbsa) wheels since 0.46.0; the CUDA 12.8–13.0 wheels' arch row is `sm75 sm80 sm90 sm100 sm110 sm120 sm121` — **sm_121 = GB10 is covered**. 0.49.0 added an explicit "DGX Spark cuda121" wheel. (Only Jetson/L4T needs a source build; Spark is standard sbsa, not Jetson.) |
| **unsloth** | **Officially supported.** NVIDIA lists DGX Spark alongside RTX PRO 6000 Blackwell as a supported Unsloth platform + ships the `nvidia/unsloth` playbook. |
| **triton / xformers** | Unsloth's recipe **builds both from source** for Blackwell: triton pinned to commit `c5d671f91d90f40900027382f98b17a3e04045f6`; xformers with `TORCH_CUDA_ARCH_LIST="12.1"` (sm_121, **not** 12.0 — that claim was refuted). A config-only triton alternative exists (set `TRITON_PTXAS_PATH` to CUDA 13.0 ptxas + `TORCH_CUDA_ARCH_LIST="12.1+PTX"`, triton issue #10331). |
| **4-bit QLoRA training** | **Validated by NVIDIA** — first-party `Llama3_70B_qLoRA_finetuning.py` uses real NF4 4-bit QLoRA; NVIDIA benchmarks ~5,079 tok/s on Llama 3.3 70B. (Caveat: vendor number; no independent end-to-end training log found.) |
| **Unified memory** | Simplifies sizing (one 128 GB pool, no static VRAM ceiling) **but** causes apparent-OOM-within-capacity: the kernel page cache retains memory after processes exit. Workaround: `sudo sh -c "sync; echo 3 > /proc/sys/vm/drop_caches"` before large runs, after stopping containers, and between runs (palliative). ~5 GiB reserved floor; `cudaMemGetInfo` under-reports. |

## gpt-oss on Spark — extra friction (documented, solvable)

gpt-oss-20b (both bnb-4bit and MXFP4) is in Unsloth's supported set, but on real GB10 aarch64 it
needs **three transformers patches** (`hub_kernels.py` + two edits to
`modeling_flash_attention_utils.py` ~lines 187, 248) because no aarch64 build variant of
`kernels-community/vllm-flash-attn3` exists — without them, loading fails with
`FileNotFoundError: Kernel ... does not have build variant: torch210-cxx11-cu130-aarch64-linux`.
Post-patch, gpt-oss-20b (bnb-4bit) loads for **inference** at ~12.5 GB of 121 GB, FA2=True,
Xformers=None. Source: unsloth issue #4867 (real GB10, ~2026-04).

## Unproven for OUR exact case — treat as plausible-but-verify

- **gpt-oss MXFP4 *expert-LoRA training* on GB10 aarch64 is NOT empirically confirmed** in any source.
  #4867 is **inference-only**; NVIDIA's MXFP4/NVFP4 gpt-oss benchmark ran on an **x86 RTX 5090**, not
  a Spark. A **fourth** patch (unsloth studio `trainer.py`) is referenced for training but no
  end-to-end MXFP4 MoE expert-LoRA *training-completion* log on physical GB10 exists yet.
- No independent third party has posted a full 4-bit QLoRA training-completion trace on physical
  Spark via NVIDIA's exact recipe (only NVIDIA's own tok/s number).

**Implication:** the go/no-go on Spark is the same as it was here — run `micro_lora_smoke.py` first.
If it reaches `TRAINING PATH PASS` (4-bit load + attn/expert LoRA + backprop), we're in business;
that is the one thing the research can't pre-confirm for our MoE-expert-LoRA case.

## Our stack vs the Spark recipe

We run (x86 pod, works): `torch 2.10.0+cu128, bitsandbytes 0.49.2, unsloth 2026.6.9, peft, trl`.
Open question for provisioning: whether that exact pin set installs on the NGC 25.11 aarch64
container, or whether we adopt NVIDIA's unpinned `--no-deps` recipe. bnb **0.49.0** is the version
documented to carry the sm_121 Spark wheel; confirm 0.49.2 also does (it is later, so almost
certainly yes, but verify).

## Sources (primary)
- bitsandbytes: issue #1930, releases, `docs/source/installation.mdx`, PR #1829
- NVIDIA: `build.nvidia.com/spark/{pytorch-fine-tune,unsloth}`, `github.com/NVIDIA/dgx-spark-playbooks`,
  dev blog "Train an LLM on an NVIDIA Blackwell Desktop with Unsloth" (2025-10-23), DeepWiki 8.3
- unsloth: `unsloth.ai/docs/blog/fine-tuning-llms-with-nvidia-dgx-spark-and-unsloth`, issue #4867, #3733

_Time-sensitivity: container tags, bnb versions, the triton pin, and the transformers-patch
requirement are all churning (2025→2026). Re-verify at provisioning time; the go/no-go smoke is
authoritative regardless._
