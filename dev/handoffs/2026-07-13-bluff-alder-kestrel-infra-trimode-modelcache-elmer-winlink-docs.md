# Handoff — 2026-07-13 — `bluff-alder-kestrel` — infra + two open threads (TrueNAS model-cache decision, Elmer WLE/Pat docs kickoff)

Long multi-topic session. Two things are **open and need the next session**: a
pending TrueNAS decision, and the not-yet-started Elmer Winlink-docs work
(`tuxlink-aib3n`). Everything else below is done/context.

## Shipped this session (on main)

- **P2P contacts-pivot** — merged (PR #1069) and released **v0.88.0**. Peers folded
  into Contacts; credential-exfil cluster found by Codex + closed. Done.
- **Contact-click bug** — PR **#1075 merged**. On an empty store the Contacts list
  is mailbox *suggestion* rows whose body had no click handler (only "Save"), so
  "clicking a contact did nothing." Fixed (suggestion rows open a detail) +
  removed the silent `.catch(() => {})` in `useContacts`. Regression test added.
- **RMS Trimode under WINE on R2** — installed in `~/.wine-trimode` (cloned from the
  proven `~/.wine-wle` for .NET), verified it launches headless. Blog-ready doc
  committed to the **wine-vara-setup** sibling repo: `docs/trimode-rms-under-wine.md`
  (commit `805f3e9`, pushed). Operator still owes the GUI config (Registration +
  Winlink password, CMS host = cms-z, channel call N7CPZ-1, VARA/radio) and the
  on-air run. Closes the software half of `tuxlink-dzp9n`.
- **R2 screen-lock disabled** — gsettings `lock-enabled=false`, `idle-activation=false`,
  `idle-delay=0` (persisted). No more VNC lock screen.

## OPEN #1 — TrueNAS model-cache offload for the DGX Spark (DECISION PENDING)

**Goal:** the Spark (`gx10-65aa`; tailnet `inference` / `100.92.82.52`) has a 916G
NVMe **73% full** (562G HF model cache). Stand up a TrueNAS cold store
(`truenas.mohaverad.io` / `192.168.20.114`, SCALE **25.04**, AD-joined MOHAVERAD)
over **10GbE**, with warm/cool rotation scripts (warm cold→hot ~65s/70GB).

**Wall we hit — and it's the load-bearing decision:**
- TrueNAS is an **AD member server**; the Linux **kernel cifs client cannot auth a
  local Samba user** against it (verified: userspace smbclient works with
  `-W truenas`, kernel `mount.cifs` fails every domain/sec/username permutation).
- So the account must be a **domain account**. Operator created AD `spark-svc`
  (winbind resolves it; kernel cifs domain-auth would work). **BUT** its primary
  group is `MOHAVERAD\domain users`, which has **FULL_CONTROL on the critical
  `SSD-SMB-Share`** → the account is over-privileged, and storing its creds on the
  Spark (encrypted or not) = a full-control key to a critical share. Operator
  (correctly) rejected that.
- **Proposed fix, awaiting operator yes/no:** create an **isolated dataset**
  `SAS SSDs/ai-model-cache` (or similar), ACL granting **only `spark-svc` Modify,
  NO `domain users`**, its own SMB share. Then the credential's blast radius = just
  re-downloadable public model weights. Storage then: **systemd-creds** encrypted
  (Spark has systemd 255 but **no usable TPM** → host-key sealed; decrypt to tmpfs
  at mount) — or Kerberos/keytab if zero-at-rest is wanted. Operator also flagged
  the pre-existing `domain users → FULL_CONTROL` on the critical share as too broad
  (their call, separate).
- **Current on-disk state (clean):** local TrueNAS `spark-svc` was created then
  **deleted** (only AD domain `spark-svc` remains). Spark `/etc/cifs/truenas.cred`
  **removed** — no plaintext creds anywhere. `/mnt/ai-cold` empty, unmounted.
- **CLEANUP OWED:** a **revocable root SSH key to TrueNAS is still authorized**
  (root's authorized_keys, comment `tmp-truenas-svc-setup-20260712-revoke-after-use`;
  private key on the Pi at `~/.ssh/id_ed25519_truenas_svc_setup`; TrueNAS SSH still
  enabled). Revoke when the cache work is done (or keep it to finish the work).
- Also live: **Tailscale Serve** on the Spark proxies `https://inference.twin-bramble.ts.net/`
  → vLLM `127.0.0.1:8000`. vLLM OpenAI base URL = `https://inference.twin-bramble.ts.net/v1`,
  model `qwen3-coder-next` (changes per `vllm serve`), no API key (use `EMPTY`
  placeholder). `IP:8000` will NOT work (loopback bind). Operator reported a build
  bug wiping this endpoint field — not yet investigated.

## OPEN #2 — Elmer agent docs for Winlink Express + Pat (`tuxlink-aib3n`, NOT STARTED)

Filed this session, **P2, open**. Context ran out before starting.

**Why:** in an emcomm incident an operator may be helping someone on a *different*
client (WLE or Pat); a Tuxlink operator running Elmer should get accurate answers
about any common Winlink client, not just Tuxlink. Motivating question (verbatim,
KJ4UYO): **"What is the syntax for Pat Winlink in EmComm Tools in ax.25 to connect
via a digipeater?"**

**First step next session (before drafting):** map Elmer's **knowledge / MCP tier**
— how docs are stored, indexed, and retrieved — so new docs plug in correctly.
Start from bd `tuxlink-cvx84.5-mcp-knowledge` and the knowledge MCP tool in the
Rust backend (`src-tauri/src/mcp_ports.rs` / `tuxlink-mcp-core`). Then draft
`pat-winlink` + `winlink-express` agent docs (Pat CLI/interactive connect incl.
`ax25:///DIGI1,DIGI2/TARGET` digipeater form, transports, ETC wrapping; WLE session
types + packet path entry), wire retrieval, add an eval set (KJ4UYO Q = #1).
**Accuracy bar: RF/connect syntax must be correct** — verify Pat's ax.25 digipeater
string against Pat's own docs (`pat`/`pat connect --help` if installed; Hamexandria
DB at `~/Code/library-of-hamexandria`, `uv run ham-search`) before shipping.

## Model context (for the cache work)
Spark HF cache holds a 120B-class bake-off: `nvidia/Nemotron-3-Super-120B-A12B`
(NemotronH = Mamba+MoE), `Qwen3.5-122B-A10B` (NVFP4+GGUF), `Qwen3-Coder-Next-FP8`,
`gpt-oss-120b`, `Mistral-Small-4-119B`. R2 has `nemotron-3-nano` + qwen3/gemma nano
via ollama. GLM-4.5-Air (106B-A12B) recommended as the next bake-off add (fits the
Spark; GLM-4.6 full too big; 5.x is API-only). DDR4 is EOL/expensive — R740 RAM plan
is "runs on the 256GB you own (16×16GB DDR4-2133)"; balanced 6-channel wants 12 or 24
DIMMs (384GB via 8 more 16GB sticks), not 16.

Agent: bluff-alder-kestrel
