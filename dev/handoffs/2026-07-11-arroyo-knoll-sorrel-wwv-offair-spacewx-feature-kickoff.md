# Handoff — NEW FEATURE to build: off-air WWV/WWVH space-weather decode (WLE gap)

- **Agent:** arroyo-knoll-sorrel
- **Date:** 2026-07-11
- **Session arc:** refreshed the elmer-distill README (PR #1073, merged), then a research thread on ionosondes/propagation surfaced a genuinely valuable new feature the operator wants built next. **This handoff exists to kick that feature off.** bd: **tuxlink-xscum** (has the full locked design — read it first).

## THE FEATURE — bd tuxlink-xscum (feature, P2)

**Decode the NOAA SWPC space-weather bulletin off-air from WWV/WWVH, using the operator's primary radio.** Winlink Express has no off-air space weather; this is a real EmComm capability gap and the operator called it "excellent … huge value added over WLE."

**Why it works (verified 2026-07-11, lightweight web check):**
- WWV voice-broadcasts the SWPC geophysical alert at **:18** past each hour; **WWVH at :45**. <45 s, refreshed every 3 h. Sourced from NOAA but carried **over HF** → receivable with no internet.
- It's a **rigid machine-generated template** (`services.swpc.noaa.gov/text/wwv.txt`): "Solar flux 107 and estimated planetary A-index 12" / "The estimated planetary K-index at 1200 UTC on 11 July was 1.33" / "No space weather storms were observed…" / "…next 24 hours is predicted to be minor" / "Geomagnetic storms reaching the G1 level are likely." Closed vocabulary = numbers + NOAA G/S/R scale phrases.
- No digital space-weather subcarrier (the 100 Hz BCD subcarrier is TIME only) → decode is **voice STT**, but bounded by the fixed grammar.

**Design LOCKED with the operator:**
- **Primary transceiver is first-class.** Reuse what Tuxlink already has: CAT (`rig_tune` VFO+mode) + the RX audio path VARA/ARDOP already use. **No new hardware.** CAT-tune to a WWV frequency in **SSB** (SSB copy of WWV voice is fine — no AM mode needed), capture ~60 s across :18, STT+parse, tune back.
- **SDR is a supported *optional* method, never required** — it dovetails with the separately-planned agent-usable SDR self-diagnostic intelligence.
- Capture **occasional / on-demand / pre-flight; NEVER mid-session**; missing a 3 h cycle is fine. Frequency by time-of-day (10 MHz all-rounder; 5/2.5 night; 15/20 day); fall back to WWVH :45 or next cycle on no-copy. WWV's own time code self-syncs the capture window.
- **Constrained STT** (Whisper-class, offline) + regex/template parse → `{solar_flux, a_index, k_index, storms_observed, forecast_24h, source, utc}`. First-run internet ONLY to seed/validate the parser format; **runtime is fully off-air.** Feed into `solar_conditions` / the propagation view, stamped "off-air WWV HH:18 UTC."

**HARD PART (don't hand-wave it):** STT on noisy HF voice. Mitigated by the tiny fixed grammar (recognize numbers + ~20 fixed phrases, not open speech); retry next cycle or show the clip to confirm on low SNR.

**EXPLICITLY OUT OF SCOPE — do NOT conflate:** the continuous **wideband-SDR multi-band beacon/MUF sensing** (WWV/CHU/broadcasters as always-on band-open beacons; highest-audible-freq ≈ MUF) is a *separate, heavier, optional* feature. tuxlink-xscum is ONLY the space-weather decode. Keeping them separate is what let the primary-radio path drop the SDR requirement.

## How this feature was reached (context for the design choices)

The operator's ruthless EmComm filter did the pruning, and it should stay applied while building:
- **No internet at runtime.** This killed the obvious "just pull GIRO/SWPC over HTTP" idea — worthless when HF matters (internet is down). Off-air only. WWV works because the data rides HF.
- **Must produce operator-usable decision data**, not science telemetry. This is why WWV space-weather (actionable: SFI/K/storm state) is in and Grape-style WWV **Doppler** (ionospheric motion/TIDs — a science derivative the operator can't act on) and **Chirpsounder2** ionograms (real, but on the sounder's path, hardware-heavy) were both cut.

## Next session — START HERE

1. **Read bd tuxlink-xscum** (full design) + this handoff.
2. **Brainstorm/design first** (`superpowers:brainstorming`) — this is a new feature; do NOT jump to code. Decide: STT engine + offline model, capture/scheduler design, rig-state save/restore, SSB vs AM, UI surface, how it feeds `solar_conditions`, and the optional-SDR seam.
3. Build primary-radio path first; SDR as the optional second method.
4. **Wire-walk at done-time** (hard gate) — the operator supplies the flows; trace them to code before any "shipped."

## State

- **PR #1073** (elmer-distill README → Qwen 235B/397B→Coder Next + Phase A executed): **MERGED.**
- **bd tuxlink-xscum:** open, the feature to build.
- **Worktrees:** `worktrees/handoff-wwv` (this handoff — dispose after push, ADR 0009). No other agent worktrees from this session.
- **Process note (in memory `feedback_reserve_heavy_workflows_for_coding`):** the `deep-research` workflow fanned to 105 agents / ~4.6M tokens for a research question — too costly. Use a few `WebSearch` calls for research; reserve heavy multi-agent orchestration for coding.
