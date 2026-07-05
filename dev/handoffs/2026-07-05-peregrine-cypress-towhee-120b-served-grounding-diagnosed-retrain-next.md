# Handoff — elmer-120b SERVED end-to-end (re-fuser proven); grounding root-caused; retrain is next

**Agent:** peregrine-cypress-towhee · **Date:** 2026-07-05
**Branches:** PR #1007 MERGED to main (eec3e716); serving/re-fuser code on `bd-tuxlink-6zkb6/discriminating-eval` (this worktree, unmerged)
**Next session = the 120b RETRAIN on an un-memorizable environment.** Read the "NEXT" section first.

## Headline
The serving wall is DOWN. The per-expert-LoRA'd gpt-oss-120b now serves end-to-end
(re-fuser → fused → GGUF → llama-server → tailnet, ~214 tok/s at Q4/32k). The
remaining problem is **model quality = grounding**: the model hallucinates
(grid→place, the station directory, Tuxlink's own internals) because the training
environment is *memorizable* and lacks tools for whole question classes. That is a
retrain problem, not a serving one. Diagnosis + fixes are filed as P0/P1 issues.

## What SHIPPED (PR #1007, merged to main)
Four Elmer fixes, all Codex-reviewed, CI-green (verify + build-linux both arches):
- **Custom-endpoint 404 (4th recurrence, tuxlink-1hv4j CLOSED).** Root cause: the
  OpenAI-compat transport POSTs the stored endpoint verbatim while the model-list
  path *derives* `/v1/models`; a base URL (`…/v1`) 404'd on chat. The recurrence
  cause was an **instrumentation gap** — the error omitted the request URL. Fix:
  `redacted_url()` in every transport error + `normalize_chat_endpoint()` at store
  time (base URL → `<origin>/v1/chat/completions`).
- **Invisible "new conversation" button.** Header buttons referenced undefined
  theme tokens (`--text-2`/`--text-muted`/`--surface-hover`) → invalid → inherit.
  Ground-truthed via the WebKitGTK render harness (`dev/render-harness/elmer-buttons-repro.html`).
- **Context-window control not applied to the OpenAI path (tuxlink-evucv CLOSED).**
  `num_ctx` was Ollama-only → long tool-loops overflowed → HTTP 400. Fix: client-side
  unit-based transcript trim (`trim_messages_to_budget`, never splits a tool block,
  never empties).
- Codex adrev P1/P2 folded in (reqwest URL cred-leak → `without_url()`; trailing-slash
  routes; tool-schema token estimate).

## The SERVING win (tuxlink-pt2xo — PROVEN, code on this branch, unmerged)
Toolkit in `dev/elmer-distill/`: `run_gate.py`+`src/elmer_distill/key_gate.py`
(mechanical layout gate), `src/elmer_distill/refuse.py` + `run_refuse_live.py`
(one-pass live dequant+fuse), `run_merge.py`, `refuse_oracle.py`,
`docs/serving-refuser-runbook.md`. Proven on the pod:
- A1 (unsloth `save_pretrained_merged`) HOLLOWS (#3701) — gate caught it instantly.
- `run_refuse_live.py` = the path that works: `merge_and_unload` bakes deltas, dequant
  each Linear4bit → bf16, stack per-expert → fused. Real 120b confirmed the transpose
  derivation (gate_up (5760,2880) → slice (2880,5760); square down_proj). **Preserve
  bare params — gpt-oss `self_attn.sinks` are not `.weight`** (llama.cpp refused to load
  without them; fixed via a `named_parameters()` sweep).
- key-gate PASS → `convert_hf_to_gguf --outtype q8_0`/`bf16` → `llama-quantize Q4_K_M` →
  `llama-server -ngl 999 -c 32768 -fa on -ctk q8_0 -ctv q8_0`. Content-oracle: served ≈
  as-trained (semantic; auto char-ratio under-reports because of the harmony CoT channel).

## NEXT SESSION — the retrain (start HERE)
The model calls `position_status` but skips `find_stations` and recalls the directory;
it invents grid→place; it confabulates Tuxlink internals (e.g. "passwords in
`~/.config/tuxlink.cfg`" — WRONG, Tuxlink uses the OS keyring). **All one disease:
ungrounded confabulation from a memorizable environment + missing tools + no coverage.**
Do these in order:

1. **tuxlink-74at8 (P0) — FIRST. Randomize the `find_stations` directory per scenario.**
   `dev/elmer-distill/src/elmer_distill/simulator.py:19-29` is a FIXED, small, real-callsign
   gateway table served identically every scenario → memorizable → gold-gen distills the
   table, not the tool-call. Synthesize it per scenario (synthetic callsigns — the pattern
   already exists for recipients at `scenariogen.py:50`; randomized-but-VALID Maidenhead
   grids with distances COMPUTED from grid geometry so `distance_band` stays consistent).
   Then recall is impossible, gold yields only grounded trajectories, and the frozen gate
   (which already hard-requires `required_tools` + grounds predicates against real records)
   becomes genuinely discriminating. **Until this lands, everything downstream distills shortcuts.**
2. **tuxlink-0mudm (P0)** — docs/help retrieval tool over `docs/user-guide/` + a "refuse
   when ungrounded" reflex (system prompt + TRAINED), so product/how-it-works questions
   are answered from docs, not invented. Add a product/help scenario family to gold-gen.
3. **tuxlink-atnsu (P1)** — `resolve_grid` + enrich tool outputs with `{lat,lon,place_label}`
   (`position/maidenhead.rs:grid_to_lat_lon` exists; needs a small offline gazetteer for names).
   **tuxlink-e7z7d (P1)** — `find_stations` distance + `predict_path` args.
4. **Regenerate gold on the un-memorizable env → retrain the 120b (tuxlink-48nyh).** NEEDS the pod.

Principle to carry: *any observation the model can memorize WILL be memorized — enumerate
every operator question class, give each a grounding tool + un-memorizable env + trained
call/refuse behavior; any missed class is a confident-hallucination surface.*

## State / cleanup
- **Pod `216.243.220.242:13443`** (RTX PRO 6000, BILLING): serving Q4/32k. **TEAR-DOWN
  recommended** — grounding is a retrain problem, nothing more to learn from the current
  model. Adapter is safe locally at `/home/administrator/elmer-artifacts/adapter-120b-2026-07-04/`;
  the Q4 GGUF + `merged-fused` are reproducible via the runbook (~50 min). Tailnet endpoint
  `https://elmer-120b-pod.twin-bramble.ts.net/v1` (tailscale userspace + `tailscale serve`;
  dies with the pod).
- **Worktrees:** `bd-tuxlink-1hv4j` (PR merged → branch DEAD, dispose per ADR 0009);
  `bd-tuxlink-6zkb6` (ALIVE — the elmer-distill training branch, where the re-fuser + gold-gen live).
- **Gotchas learned:** use `git -C <abs-worktree>` to dodge the main-checkout-race hook when a
  second session is active; pod `/workspace` is quota-limited (~66 GB, use local `/`);
  Q4_K_M of 120b is 88 GB (fits 96 GB with `-fa` + q8 KV at 32k); `llama-quantize` can't
  requantize from q8 (need bf16 intermediate); CI apt cache went stale on the 2026-07-05
  runner image (fixed on main by tuxlink-84vzn: explicit GTK/glib dev pkgs + v3 salt).
