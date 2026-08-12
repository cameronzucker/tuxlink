# Spark serving runbook — control plane, recipes, restore

The two DGX Sparks (`gx10-65aa` = 10.55.0.1 head, 10.55.0.2 worker, QSFP
RoCE fabric) serve models for Elmer and for experiments. **All serving
lifecycle goes through the control plane.** Raw `docker`
run/stop/exec against these hosts is banned and hook-enforced
(`.claude/hooks/block-spark-oob-serving.sh`); the 2026-08-11 incident that
motivated the hook: an out-of-band `docker stop` deleted the live Inkling
container (it ran with auto-remove), and the ad-hoc restore attempt
reproduced the pinned Triton failure class the recipes exist to avoid.

## The control plane (spark-dashboard)

- **Topology: one instance PER NODE, loosely linked.** Each Spark runs
  its own copy of `~/serving/spark-dashboard/app.py` (FastAPI; systemd
  unit `spark-dashboard.service`, though instances are sometimes run
  out-of-unit via nohup — check `ss` for uvicorn on 8090, not just
  `systemctl is-active`). The HEAD instance (the node with
  `cluster.json`) is the cluster orchestrator: its `/api/cluster/*`
  endpoints drive the worker over ssh. The worker's instance manages
  solo serving on that node only and answers "not the cluster head" to
  cluster calls. There is NO config sync between them — the two
  `app.py` copies were observed divergent (2026-08-11); edits must be
  applied per node, and take effect on that node's next restart.
- **UI:** https://inference.twin-bramble.ts.net:8443 (tailnet-only;
  tailscale-serve fronts local port 8090).
- **Profiles:** `~/serving/spark-dashboard/profiles.json` — named serving
  configs (as of 2026-08-11: `cn`, `q122`, `ns120`, `ns120nt`,
  `gptoss120`, `mistral119`, `laguna`, and cluster profiles `q122-tp2`,
  `inkling-tp2`, `q235-tp2`). `cluster.json` marks the head.
- **API** (POST unless noted):
  - `/api/switch/{name}` — switch solo (single-node) serving to a profile
  - `/api/cluster/switch/{name}` — switch two-node (TP2) serving
  - `/api/cluster/stop` — stop both ranks cleanly
  - `/api/status`, `/api/stats` (GET) — what's serving, wall power, etc.
  - `/api/fetch` — model download management; `/api/history` (GET)

**Restore Inkling** (the default assistant serving) — RESOLVED
2026-08-11 (operator directive "all other work stops until API-only
serving is confirmed reliable"):

```
curl -X POST https://inference.twin-bramble.ts.net:8443/api/cluster/switch/inkling-tp2
```

then poll `/api/status` until `serving.id` =
`thinkingmachines/Inkling-Small-NVFP4` (several minutes; 159 GiB
checkpoint). This works now because the switch DELEGATES: profiles
carrying a `recipe` field (inkling-tp2 does) get stop + `docker rm -f`
on both nodes (corpse-proof) and then `run-recipe.sh <recipe>` launched
detached — the ONE launcher that applies the recipe's mods and pins.
The dashboard's own docker/ray composition remains only for mod-free
cluster profiles (q122-tp2, q235-tp2). History: the pre-fix API path
skipped mods and died at startup (`KeyError: invalid tool call parser:
inkling` — the parser is registered by
`mods/inkling-fix-streaming-tool-calls`), which is why serving broke
every time it was touched; two launchers each held half the truth.

**Proven 2026-08-11 (4/4 API-only legs, each ending in a live
completion):** Inkling cold recycle → cross-model to q122-tp2 →
cross-model back to Inkling → idempotent Inkling→Inkling repeat.
Caveat observed en route: the LEGACY cluster composition binds vLLM to
the QSFP IP, so q122-tp2 is NOT reachable via the 443 front (reach it
at `http://10.55.0.1:8000` from the head) — a legacy-path property,
unchanged; Inkling's recipe binds 0.0.0.0 and the 443 front serves it.

Implementation lives in `~/serving/spark-dashboard/app.py` (`RECIPE
DELEGATION` block in `_do_cluster_switch`) + `profiles.json`
(inkling-tp2: `recipe`, corrected `served_name`/`container`); timestamped
`.bak-*` files sit beside both. Cluster containers launch with
`--log-driver=journald` (added to `launch-cluster.sh` 2026-08-11): a
dead container's traceback survives `--rm` —
`journalctl CONTAINER_NAME=vllm_node --since <t>` on the node.

## Recipes (the pins live here)

`~/spark-vllm-docker/recipes/*.yaml` on the head node encode every
hard-won serving pin per model (image pins, mods, memory/context
settings). Read the maintenance warning at the top of
`inkling-small-nvfp4.yaml` before "simplifying" anything — each pin
eliminated a reproduced GB10 failure (tracked via bd `tuxlink-fa6x4` and
the bench re-check issue). The launch tooling
(`launch-cluster.sh`, `run-recipe.sh`) is what the control plane drives;
operators may run it directly, agents use the API.

## Rules of engagement for agents

1. Serving lifecycle = control-plane API only. No out-of-band containers
   (classifier-kickoff standing rule), no dev-script launches, no
   `docker exec` into serving containers.
2. Read-only diagnosis is always fine: `docker ps`/`logs`/`inspect`,
   `curl` to endpoints, `ss`, reading recipes and logs.
3. If serving is broken and the API can't fix it, STOP and surface to the
   operator — do not improvise a restore.
4. Test/panel models are serving lifecycle too: get them a profile (or
   operator go-ahead for a recipe) rather than ad-hoc containers.

This runbook is linked from the dashboard footer as the operator escape
hatch — if an agent needs manual direction, point it here.

Session provenance: written by moss-tamarack-taiga after the 2026-08-11
incident; the dashboard/API surface was read from `app.py` and
`profiles.json` on the head node that night. Update alongside dashboard
changes.
