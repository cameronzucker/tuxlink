# Handoff — 2026-07-09 — `jay-marsh-yew` — WLE differential test staged on R2; CMS channel data cracks the 33-dial silence (SSID + bandwidth defects)

## Headline

**The zero-answer mystery decomposes into concrete, per-dial explanations from the CMS
gateway API — no exotic RF fault required.** Cross-referencing the campaign log against
`api.winlink.org/gateway/status.json`: two of the dialed targets don't exist as
addressed (gateways are `KB2PCN-5` / `NS7K-10`; we dialed the base callsigns — VARA
ignores ConReqs not matching its MYCALL), KD6OAT's entire HF lineup including the dialed
14112.5 is **VARA 500-only** (a 2300 ConReq is undecodable there), and three more
targets (K7DAV 14107, WB0ECX 14102, KD0SFY 14105.5) are **VARA 2750** channels where
2300→2750 downward negotiation is unverified. Separately: **WLE is installed and
launching under WINE on R2** (`~/.wine-wle`), ready for the operator's differential
dial of KD7ZDO. PR #1061 (passband −1 + PTT tail-hold) is **merged**.

## Operator's syntax question — answered definitively

Tuxlink's VARA command stream is **syntactically correct** per the official *VARA
Protocol Native TNC Commands* doc (EA5HVK, 2022-02-13; fetched from the n8jja/Pat-Vara
repo, copy in scratchpad). We send `MYCALL <call>`, optional `BW2300`,
`CONNECT <mycall> <target>`, CR-terminated — all spec-exact. Every command we omit is a
spec default that is *correct for Winlink gateway dialing*: `WINLINK SESSION` (default;
`P2P SESSION` is the deviation — it only changes retry cadence 4.0s→4.6s),
`COMPRESSION TEXT` (default, "Recommended for Winlink"), `LISTEN OFF` (default).
The syntax axis is CLOSED. Two latent nits filed: `tuxlink-c39af` (Compression enum
carries invalid `BINARY`/`AUTO` tokens; valid set is OFF/TEXT/FILES; also consider
explicit `WINLINK SESSION` + owning `RETRIES` via cmd socket) — no send site reaches
them today.

## The per-dial post-mortem (from gateway/status.json, queried 2026-07-09 ~17:10 MST)

| Dialed | Freq (center) | Reality per API | Verdict |
|---|---|---|---|
| KB2PCN 14108.0 | KB2PCN**-5**, VARA 2750, 00-23 | **Wrong callsign** — unanswerable |
| NS7K 14104.9 | NS7K**-10**, VARA **2300**, 00-23 | **Wrong callsign** — otherwise a perfect target |
| KD6OAT 14112.5 | VARA **500**, 00-23 (all 4 HF ch are 500) | **BW mismatch** — 2300 ConReq undecodable |
| KD0SFY 14105.5 | VARA 2750 (their 500 ch is 14094.5) | 2750-vs-2300, negotiation unverified |
| K7DAV 14107.0 | VARA 2750, hrs 12-05 (in window) | 2750-vs-2300, negotiation unverified |
| WB0ECX 14102.0 | VARA 2750, 00-23 | 2750-vs-2300, negotiation unverified |
| KD7ZDO 14108.2 | VARA **2300**, 00-23, status current | Clean target — the differential dial |

Frequency-convention note: API `Frequency` is the **center**; dial = center − 1500 Hz
USB (matches WLE `VaraSession.cs` math). The campaign's "M" variant was the correct
convention all along.

**API details** (now in `tuxlink-hmoz8` notes): POST
`https://api.winlink.org/gateway/status.json`, form params `Mode=anyall`,
`ServiceCodes=PUBLIC`, `key=1880278F11684B358F36845615BD039A` (Pat's public WDT key).
Response: per-gateway `Callsign` (SSID-bearing!), lat/lon, per-channel
`OperatingHours` ("00-23" / "12-05" UTC), `SupportedModes` + `Mode`
(50=VARA 2300, 53=VARA 500, 54=VARA 2750), center `Frequency`. 1996 gateways, ~1.5 MB.
Corroboration: pat had the same defect (never commanding BW500 for 500-Hz stations) —
see pat-users "VARA & VARA FM beta" thread.

## Issues filed / updated this session

- `tuxlink-gbb05` **P1** [fable] — dial path loses gateway SSID (KB2PCN-5→KB2PCN,
  NS7K-10→NS7K). Determine where the SSID dies (listing ingest vs catalog vs dial).
- `tuxlink-c39af` P2 [fable] — Compression enum invalid tokens + WINLINK SESSION /
  RETRIES parity notes.
- `tuxlink-hmoz8` P1 — updated with live API confirmation; the ingest feature remains
  the fix (hours + bandwidth + SSID'd callsign through catalog→finder→dial; auto-match
  `BW<N>` per channel).
- PR **#1061 MERGED** (merge commit `392dfd84`): `86055ba1` passband −1, `acbdcf01`
  250 ms PTT tail-hold. Branch `bd-tuxlink-yrrjq/vara-ptt-keying` is merged-dead
  (ADR 0017). R2's diagnostic build already contains both fixes.

## WLE on R2 — staged, operator finishes via VNC (display :1)

- Prefix `~/.wine-wle` (win32, wine 9.0, dotnet48 + corefonts via winetricks; system
  wine, NOT box64 — R2 is x86_64). Installer needed `DISPLAY` set +
  `/VERYSILENT /SUPPRESSMSGBOXES`; plain `/SILENT` over ssh exits 1 silently.
- Install dir `~/.wine-wle/drive_c/RMS Express/`. **WLE launches and renders** (license
  dialog verified on :1 via xwd screenshot — xwd on R2, convert/ffmpeg on the Pi;
  no imagemagick/xdotool on R2).
- COM mappings in the WLE prefix: **COM1 → /dev/ttyUSB0 (FT-710 Enhanced CAT, 38400)**,
  COM2 → /dev/ttyUSB2 (Digirig CP2102N / G90). Ports were free when checked.
- VARA1 untouched in `~/.wine-vara` (port 8300, `Retries=30`); its TCP indicator shows
  no host attached — Tuxlink session closed, send authority disarmed. WLE↔VARA is plain
  TCP so the prefix split is invisible to it. VARA titlebar reads **N7CPZ-1** — verify
  VARA.ini callsign vs what the host sends as MYCALL/CONNECT source (Tuxlink sends
  `N7CPZ`) before reading too much into any WLE result.

### Operator runbook (VNC → R2 :1)

1. WLE license dialog: agree + Begin Using Program. First-run: callsign `N7CPZ`,
   grid `DM33WP`, Winlink account password.
2. Open a **Vara HF Winlink** session → Settings → Vara TNC Setup: address `127.0.0.1`,
   ports 8300/8301, **UNCHECK "Automatically launch Vara TNC"** (Vara.exe lives in the
   other prefix; autolaunch would error).
3. Radio Setup: Yaesu FT-710, **COM1**, 38400, PTT via CAT. Radio is already
   DATA-U + `DATA MOD SOURCE=REAR`.
4. Dial **KD7ZDO**, center 14108.2 (WLE shows dial 14106.7). WLE sends BW2300 itself.
   Clear-channel check first; ≥5 min between dials (110 °F antenna discipline).
5. Watch VARA1's waterfall/PTT + FT-710 power. Retries cycle ~4 s.
   - WLE connects → Tuxlink is the remaining variable on 2300 paths (then run the
     corrected-target Tuxlink dials below and diff).
   - WLE also fails → environment/propagation/registration axis, Tuxlink exonerated.

### Corrected-target Tuxlink dials (after/alongside WLE test)

- `NS7K-10` (not NS7K) — center 14104.9 → dial 14103.4, BW2300. The best "first blood"
  candidate: 2300 channel, 00-23, current status.
- 500 Hz axis (`config_set_vara bandwidth_hz=500`, UI dropdown exists in Modem panel):
  `N0DAJ` 7108.0 center → 7106.5 dial (94 km, 40m NVIS, 500, 00-23);
  `KM7N` 14100.1 → 14098.6 (192 km); redial `KD6OAT` 14112.5 → 14111.0 now at BW500.
- 2750-negotiation probe (optional): `KB2PCN-5` 14108.0 → 14106.5 at BW2300; an answer
  proves downward negotiation, silence + a later BW-matched success disproves it.

## R2 machine state deltas (vs kestrel-butte-granite handoff)

- NEW: `~/.wine-wle` prefix (WLE + dotnet48), `~/bin/winetricks`, `/tmp/wle-*.log`,
  `/tmp/gateway_status.json` (full API snapshot), WLE process may be left at the
  license dialog on :1.
- Unchanged: diagnostic Tuxlink build + Vite :1420, VARA1/VARA2, wireplumber pin,
  audio (DRA sink 0.70), G90 dummy-load rig. Nothing killed, nothing reconfigured.
- Pi scratchpad holds `gateway_status.json`, the VARA TNC commands PDF, and the
  Pat-Vara clone (session-scoped; API snapshot is reproducible via the curl above).

## Worktrees / branches

- This handoff: `worktrees/jay-marsh-yew-handoff` on
  `agent-jay-marsh-yew/wle-differential-handoff` (off `392dfd84` = merged #1061),
  merged to main immediately (no parking).
- `worktrees/tuxlink-yrrjq` — branch merged-dead; worktree pending disposal ritual
  (ADR 0009) next session. Pending from before: `worktrees/tuxlink-graylinefix`,
  `worktrees/verify-087`.
- Main checkout untouched on `bd-tuxlink-ant8s/ardop-connect-fixes` (operator state).

Agent: jay-marsh-yew
