# Session state, chasm-wren-crag (2026-07-27) — MID-SESSION capture before context compaction

Session continues after this doc; written as compaction insurance, not session end.

## Shipped and merged tonight (all on main)

- **PR #1260** — Ladder-2 battery infrastructure (41-commit kz4rg branch) + 5 harness-defect fixes.
- **PR #1261** — battery teaching fixes: DENY_TEACHING goal-mapping rewrite, empty-inventory critique-laundering guard, judge harness-context, 14 read-only grounding tools into ALLOWED_TOOLS. (tuxlink-hwo1b closed)
- **PR #1262** — authoring-surface fixes: anchoring traps (template modes/trigger, system-prompt recurrence rule, outbox-flush prose, spacewx indices keys, branch polarity), five validator advisories (OUTPUT_NEVER_CONSUMED gated on is_pure_read, COMPOSE_AFTER_CONNECT via outbox roles, BRANCH_OP_VALUE_PAIR, BRANCH_BOTH_ARMS_EMPTY, TX_ONLY_ON_FAILURE_ARM producer-checked), radio.connect walk no-frequency soft-fail, routines_actions_list action/section narrowing, docs page refresh. Codex adrev: 1 P1 + 6 P2 all dispositioned with regression tests. (tuxlink-6i8jz, tuxlink-rrk51 closed)
- **PR #1263** — rev_on arms RETIRED durably (drivers default REVCONDS="off", dashboard columns dropped, runbook + judge rubric updated). Evidence: rev_off 39% vs rev_on 28%. (tuxlink-jaer0 closed)
- **PR #1264** — MERGED: every rung 3x unconditionally in both canonical drivers + runbook. (tuxlink-x43aa closed)

## The lnctz root-cause study (what drove everything)

Four parallel transcript readers over 108 judged bundles. Consolidated: the tool surface taught the failures (template anchoring, "ONLY when operator asked" trigger rule, DENY_TEACHING give-up loop, denied grounding tools vs station-fitted rubric, opaque indices, positional then/else examples). Model-side residue for fine-tuning: fetch=check conflation, then/else polarity collapse under editing pressure, interactive connect-then-send prior, plan-tail attrition, ask-permission deferral. Full detail in the four reader reports (session transcript) + PR bodies.

## IN FLIGHT: surface1 battery run on R2

- Tree: `r2-poe:~/6i8jz-run` (clone @ b8923868, content-identical to merged main for the measured surface). Results: `~/6i8jz-run/battery-results/surface1/`.
- Driver relaunched 05:00Z with **3x-unconditional patch applied to the run copy** (PID 268777 at capture). 216-bundle target. Catalog.json regenerated from the live surface (modes-free template + polarity text verified in what models receive).
- Config (lnctz-comparable): conc=8, turn 1800s, run 7200s, temp 0.2, cap 40, 18 cells x base/skill x build+rev_off, Nemotron/Nebius fp4 reasoning OFF reviewer, Qwen-on-Spark QEP https://inference.twin-bramble.ts.net/v1/chat/completions.
- **Five truncated bundles archived to `surface1/truncated-archive/` and slots CLEARED** — they re-run. Workers may have already passed those cells this launch, so: **on RUN COMPLETE, fire one sweep relaunch (same nohup command, idempotent) before declaring done; verify 216 scored bundles.** Launch recipe (key never on disk): runbook §Launch, secret-tool service elmer-openrouter account teacher piped into ssh env; LADDER2_CONC=8 LADDER2_TURN_TIMEOUT_SECS=1800 TUXLINK_MAX_RUN_SECS=7200 LADDER2_REVCONDS_SKILL=off.
- Monitoring: ladder dashboard r2-poe:8899 (surface1, rev_on columns removed live); Spark dashboard :8443 upgraded (7 thermal zones + GPU SM/mem clocks + CPU cluster clocks + sparklines; committed 372cf5a in ~/serving/spark-dashboard repo on the Spark). Judge daemon on Pi PID 2436797, workdir `dev/scratch/surface1-judge/` (R2DIR patched to surface1). Truncation+liveness Monitor armed in-session.

## Truncation root cause (CLOSED — do not re-litigate)

Silent-stall class is PRE-EXISTING: lnctz had 3 wall-clock stalls (base/E1 rev_off x2, base/S3) labeled needs_operator/cancelled = 5.6% of base arm; surface1's 5 (all base arm) is statistically indistinguishable. Tonight's "regression" was the new loud `truncated` label + live dashboard making an always-present, analysis-censored phenomenon visible. Retracted along the way: thermal cause, double-vLLM theory (the "host" vllm was the container's own process via host PID namespace), operator-started-it misattribution. All recorded on **tuxlink-3cal1** (first-token ~120s + idle-stream ~300s retryable timeouts in tuxlink-agent-frontend provider) — **land 3cal1 BETWEEN runs, never mid-run**.

## Spark (ASUS GX10) state

- Firmware now EC 0x02000007 / UEFI 0x03000008 (lvfs-testing; ahead of the forum-thread fix pair). Under sustained conc-8 122B load: zones plateau 88-94C, GPU 83-87C, **SM clock rock-stable 2314-2411 MHz, no sag = no throttle**. Thermals similar to pre-fix in degrees but without the throttle symptom; ground truth needs prochot.
- **spark_hwmon DKMS blocked on Secure Boot MOK enrollment (physical console required).** State: dkms package installed, spbm/0.3.0 `added`, source at ~/spark_hwmon. Console-day recipe: `sudo dkms build spbm/0.3.0 && sudo dkms install spbm/0.3.0` in a real TTY (answer the MOK password prompt) → reboot → blue MOK-manager → Enroll MOK → password → then `sudo modprobe spbm` / check `sensors` for cpu_p_clu*, gpu, soc temps + prochot + pl_level. Then wire prochot + cluster temps into the dashboard and re-tune alerts to clock-sag/prochot rather than temperature.
- Serving via spark-dashboard profiles (q122 = ladder-proven). Add to the switch FSM: guard against a pre-existing host-level vllm before docker start (race noted 2026-07-27). vLLM stack note: with unified memory, 115/121 GB used is NORMAL preallocation.

## Tuesday (second Spark) prep

- Dual-endpoint driver patch: per-worker QEP round-robin + `box` field in manifest/latency rows; conc=16. ~2x wall clock (216 bundles in ~4-5h), chain-granularity tail rounds it slightly off.
- Provision Spark-2 to the pinned recipe (same NVFP4 quant, template, vLLM args, --max-num-seqs 8), tailscale serve, dashboard, MOK enrollment while physically present.
- Strategic: serve-on-one/train-on-other per the fine-tuning plan (delivered in-session; battery = per-LoRA-cycle regression gate).

## Post-run analysis plan (when surface1 completes)

1. Sweep relaunch → verify 216 scored → judge daemon drains → judgments.jsonl.
2. Per-rung stability rates (the 3x point) — flag flaky greens; A1 base has NEVER judge-passed while det-green across 2 runs (F-over-sg pattern) — targeted read of its rubric failures; candidates for the fine-tuning target list vs harness list.
3. Before/after vs lnctz on the attempt-1 subset. Watch: transmit_mode automatic-rate (smoke P2 chose automatic under new recurrence rule → AUTO_TX_UNACKED honest terminal; distribution shift matters), warning-induced churn from the 5 new advisories (anti-ping-pong), section-narrowing adoption (smoke model used it unprompted), honest-stop rate on C1.
4. Truncation accounting: archive dir separate from model samples.

## Housekeeping debt

- Local worktrees: kz4rg, hwo1b, 6i8jz, jaer0 = merged-dead (ADR 0009 disposal ritual); x43aa = active until #1264 merges. R2: ~/kz4rg-build, ~/lnctz-test, ~/lnctz-bins (jay-heron-clover debt) + ~/hwo1b-check (scratch mirror) need disposal; ~/6i8jz-run ACTIVE; ~/lnctz-retest KEEP (data).
- Codex adrev raw transcript: gitignored dev/adversarial/2026-07-27-authoring-surface-codex.md (local-only by policy).
- bd open from tonight: tuxlink-3cal1 (P1, pre-next-run), tuxlink-tii83 (P2 gateable health outputs), tuxlink-2grt7 (P2 NUT shutdown surface).
- Memories written: project_battery_methodology_settled (rev_on retired, 3x unconditional, stall base rate, reviewer pin).

Agent: chasm-wren-crag
