# Handoff — gully-marsh-maple — RF observability harness built; Pi↔radio Bluetooth Page-Timeout blocker

> **Date:** 2026-05-23 · **Agent:** gully-marsh-maple · **Machine:** pandora (Pi 5)
> **Operator is relocating to LA and will redeploy to continue.** This handoff is self-contained
> (tools embedded) so it survives a redeploy to a different machine. Gitignored artifacts
> (`dev/scratch/`, `dev/adversarial/`) are local to pandora — their essence is reproduced below.

---

## 0. TL;DR / session arc

The session goal was: finish the PR #125 consolidation (two gates), then build an **RF observability
harness** (RTL-SDR) so the agent can self-assess RF protocols, then **fix AX.25 1200-baud packet RX**
by comparing against prior art. What actually happened:

1. ✅ **RF observability harness BUILT + validated** on real off-air signals (RTL-SDR → decode). Big win.
2. ✅ **AX.25 prior-art investigation done** (Dire Wolf / wl2k-go+Pat / benlink+HTCommander). Findings in §4.
3. 🔴 **ACTIVE BLOCKER:** the Pi can no longer open a **Bluetooth connection to the UV-Pro radio** —
   every connect fails with **HCI Page Timeout (0x04) / `EHOSTDOWN`**. This blocks the in-app packet
   path *and* the RX-only transport test. Fully diagnosed (see §2); fix not yet applied. **Resume here.**
4. ⏸️ **PR #125 consolidation gates UNTOUCHED this session** (the BT fight ate the time). Still open. §5.

**The single most important thing for next session: §2 (the Bluetooth Page-Timeout) is the gate to
all the on-Pi radio work. Start there.**

---

## 1. ⚠️ READ FIRST — corrections the operator made this session (don't repeat my mistakes)

The operator is the **station licensee and the authority on the RF/hardware**; their hands-on tests
overruled my analysis repeatedly. Internalize these:

- **"AI output on amateur-radio specifics is structurally unreliable; operator pushback = ground truth."**
  I twice asserted things the operator disproved with hardware tests. Defer to them on RF/BT hardware.
- **The UV-Pro DOES speak raw KISS over Bluetooth SPP.** I floated a "needs GAIA/Benshi framing +
  REGISTER_NOTIFICATION" theory (from benlink/HTCommander). **RETRACTED** — operator's hard evidence:
  Windows **Winlink Express** and Android **WoAD** both use it as a plain KISS BT device with nothing
  but pair+connect. (benlink/HTCommander use the radio's *other*, richer GAIA protocol *by choice*;
  raw KISS is also available and is what tuxlink/Winlink/WoAD use.) **Do not rewrite the transport to GAIA.**
- **The SDR "hears nothing on 2m" was a PHYSICAL ground-plane/capacitance issue**, not antenna/gain/
  sample-rate/filter. Operator diagnosed it on a reference Windows host (SDR Console) — holding the
  dongle body + bending the USB lead vertical fixed it; then a better antenna. After that, **strong
  APRS reception on 144.390 confirmed.** (My software theories — gain/AGC/desense/filter — were wrong.)
- **The Bluetooth failure is a Pi-side issue, NOT the radio.** Operator was emphatic. (And §2's btmon
  capture confirms it's a Pi-side ACL paging problem.)

---

## 2. 🔴 ACTIVE BLOCKER — Pi↔UV-Pro Bluetooth: HCI **Page Timeout (0x04)**

**Radio:** BTECH UV-PRO, MAC **`38:D2:00:01:55:5C`**, paired/bonded/trusted. KISS TNC mode is on the
radio's Menu › General Settings › KISS TNC. **Pi BT controller:** onboard BCM4345C0, hci0, addr
`88:A2:9E:83:A8:A6`.

### Symptom
Every method of opening a connection to the radio fails:
| Method | Result |
|---|---|
| `l2ping <mac>` | `Can't connect: Host is down` |
| raw `AF_BLUETOOTH`/`BTPROTO_RFCOMM` socket `connect()` (tuxlink's actual method) | `OSError errno 112 EHOSTDOWN` |
| `bluetoothctl connect <mac>` | fails; journal: `src/profile.c:record_cb() Unable to get Hands-Free unit SDP record: Host is down` |
| desktop GUI "Connect" | `Connection failed - no usable services on this device` |
| `sdptool records <mac>` | returns nothing |

### THE decisive finding (HCI ground truth via `btmon`)
```
HCI Event: Connect Complete (0x03)
    Status: Page Timeout (0x04)     <-- maps to EHOSTDOWN
    Link type: ACL (0x01)
    Address: 38:D2:00:01:55:5C
```
**The Pi pages the radio, waits ~5 s, the radio never answers.** This is at the **ACL paging layer** —
*below* SPP/RFCOMM/SDP/bond entirely. So:
- The GUI **"no usable services" is EXPECTED for a bare-SPP TNC** (BlueZ has no auto-connect handler
  for plain Serial Port; Winlink Express/WoAD/tuxlink all use a *raw RFCOMM socket*, not the OS Connect).
  **It is not the bug** — it's a red herring at the profile layer.
- The real fault is the ACL page. SPP/RFCOMM/bond are irrelevant until the link forms.

### Key clue — the pair-OK / connect-fail split
**Re-pairing the radio TODAY SUCCEEDED** (pairing rides an ACL → the radio *did* answer a page then),
yet every *connect* page times out. So the radio is page-scannable at pair-time but the Pi's connect-page
goes unanswered → points at **page parameters / timing on the Pi side** (clock-offset / page-scan-
repetition-mode the kernel uses for the connect vs. what pairing used). Windows/Android connect fine,
so it's specific to this Pi's BlueZ/kernel state.

### Ruled out
- **WiFi/BT coexistence** (BCM43455 combo chip): `iw dev wlan0 link` = **Not connected** (Pi is on
  Ethernet) → no 2.4 GHz contention. Refuted.
- **Stale/LE-only bond:** bond has a valid **BR/EDR `[LinkKey]`** (no LE keys). Address correct. Firmware clean.
- **Controller mode:** `ControllerMode` default dual, BR/EDR enabled, `UP RUNNING PSCAN`.

### Already tried — NONE fixed it
`systemctl restart bluetooth` · `hciconfig hci0 reset` · `rfkill block/unblock bluetooth` · full
`hci_uart_bcm` unbind+rebind (firmware reloaded clean, BCM4345C0 build 0382, addr stayed correct) ·
**Codex #1 BR/EDR-only cycle (`btmgmt … le off`)** — l2ping still Page-Timeouts (but see btmgmt gotcha).

### ⚠️ `btmgmt` GOTCHA
`btmgmt` **hangs at its interactive prompt** when run non-interactively (esp. `btmgmt info`). Always
wrap: `sudo timeout 6 btmgmt <cmd> </dev/null`. Because of this I could **not confirm `le off` actually
latched** — re-verify before trusting Codex #1 as refuted.

### Cross-provider consult: Codex (gpt-5.5)
Full transcript: **`dev/adversarial/2026-05-23-bt-rfcomm-unreachable-codex.md`** (gitignored, on pandora).
Codex's ranked hypotheses + exact commands. Its decisive contribution was the **btmon classify** above.
Ranking: #1 ACL page failure (confirmed by btmon); #2 stale BlueZ device/cache; #3 coexistence (refuted);
#4 HFP noise; #5 RFCOMM kernel; #6 page-scan/role.

### ▶️ RESUME PLAN (next session, in order)
1. **Re-confirm the layer is still Page Timeout** (in case a reboot already changed it):
   ```bash
   MAC=38:D2:00:01:55:5C
   sudo timeout 12 btmon -w /tmp/uvpro.snoop >/dev/null 2>&1 & BG=$!
   sleep 1; sudo l2ping -c 2 -t 5 "$MAC"; sleep 2; sudo kill "$BG" 2>/dev/null
   sudo btmon -r /tmp/uvpro.snoop | egrep -i -A5 'Connect Complete|Status:|Page Timeout'
   ```
2. **Codex #2 — clear the BlueZ device + cache dirs and re-pair** (refresh page params; reversible/backed-up).
   Operator must put the radio in pairing mode for the re-pair:
   ```bash
   ADAPTER=$(cat /sys/class/bluetooth/hci0/address); DEV=${MAC//:/_}
   sudo cp -a "/var/lib/bluetooth/$ADAPTER/$DEV" "/root/bt-backup-$DEV-$(date +%s)" 2>/dev/null || true
   bluetoothctl remove "$MAC"; sudo systemctl stop bluetooth
   sudo rm -rf "/var/lib/bluetooth/$ADAPTER/$DEV" "/var/lib/bluetooth/$ADAPTER/cache/$DEV"
   sudo systemctl start bluetooth
   # operator: radio into pairing mode, then:  bluetoothctl --timeout 60 pair $MAC; bluetoothctl trust $MAC
   sudo l2ping -c 3 -t 5 "$MAC"     # verify the page completes (no more Page Timeout)
   ```
3. **Fresh inquiry before connect** (refresh PSRM + clock-offset the kernel uses to page):
   `sudo timeout 12 bluetoothctl --timeout 10 scan on` then immediately `sudo l2ping -c 3 -t 5 "$MAC"`.
4. **REBOOT** — highest-probability clean restore (operator connected fine on *this same boot* yesterday;
   all config files are default). Operator's call (kills VNC/gqrx/session). After reboot, re-run step 1.
5. Once `l2ping` succeeds → run the RX-only transport test (§4) and proceed to the AX.25 work.

**bd issue filed:** see §6 (Pi-side BT Page-Timeout blocker).

---

## 3. ✅ RF OBSERVABILITY HARNESS — built + validated (the win)

Purpose: an **independent off-air receiver** so the agent can see what is actually transmitted (the
RF-layer equivalent of `btmon` for Bluetooth). RX-only — never transmits, so no RADIO-1 gate.

### Hardware / setup on pandora (reproduce on redeploy)
- **RTL-SDR:** Realtek RTL2838 + R820T tuner, currently on a **USB-3 port** (Bus 003).
- **apt-installed this session:** `direwolf` (1.7), `multimon-ng` (1.3.1), `gqrx-sdr` (2.17.6).
  (`rtl-sdr` tools `rtl_test/rtl_fm/rtl_sdr/rtl_power/rtl_tcp` were already present.)
- **DVB blacklist:** `/etc/modprobe.d/blacklist-rtl-sdr.conf` blacklists `dvb_usb_rtl28xxu` etc. (takes
  effect on reboot; librtlsdr auto-detaches at runtime regardless).
- **gqrx config:** `~/.config/gqrx/default.conf` → `device=rtl=0`, `frequency=144390000`,
  **`sample_rate=2400000`**.
  **🔑 CRITICAL: use 2.4 Msps, NOT 1.0 Msps.** 1.0 Msps makes the R820T spam `[R82XX] PLL not locked!`
  → no/garbage RX. 2.4 Msps locks clean (3 startup retunes only).
- **The ground-plane fix matters** (see §1) — without good grounding/antenna the SDR is deaf on 2m.

### Frequencies
- **145.710 MHz** = the area's main AX.25 packet gateway (point the harness here for on-air capture).
- **144.390 MHz** = US APRS — *busy* diagnostic channel. **>60 s of silence here ⇒ the SDR RX is broken.**
  (This is the operator's "is it working at all" check. It works now: strong signals confirmed.)

### Decode chain (all validated: synthetic `gen_packets`→`atest`, FM-broadcast positive control, AND real APRS)
- **gqrx** — visual waterfall (operator verification). Launch:
  `WAYLAND_DISPLAY=wayland-0 DISPLAY=:0 XDG_RUNTIME_DIR=/run/user/1000 gqrx` (appears on the labwc/wayvnc desktop).
- **multimon-ng** — live decode: `rtl_fm -f F -M fm -s 22050 -g G - | multimon-ng -a AFSK1200 -t raw -`
- **atest** (Dire Wolf's engine, best demod) — offline on a recorded WAV: `atest -B 1200 capture.wav`
- ⚠️ `direwolf` *live* needs an audio output device (errors `524` headless) — use `atest` on recordings instead.

### Tool 1 — `dev/scratch/rf-monitor.sh` (RX-only SDR capture + dual decode)
Usage: `./rf-monitor.sh <freq_MHz> [gain_dB] [seconds]` (e.g. `./rf-monitor.sh 145.710 49 120`).
Records a WAV + live-multimon + offline-atest into `dev/scratch/rf-captures/<ts>_<freq>/`. Core pipeline:
```bash
timeout "$SECS" rtl_fm -f "${FREQ_MHZ}M" -M fm -s 22050 -g "$GAIN" - 2>rtl_fm.err \
  | tee >(multimon-ng -a AFSK1200 -t raw - >live.log 2>&1) \
  | sox -t raw -r 22050 -e signed -b 16 -c 1 - capture.wav
atest -B 1200 capture.wav        # high-sensitivity offline pass
```

### Tool 2 — `dev/scratch/rfcomm-rx-dump.py` (RX-only BT raw-socket sniffer = tuxlink's exact method)
RX-only; opens the **same `AF_BLUETOOTH`/`BTPROTO_RFCOMM` socket tuxlink's `RfcommSocket` uses**,
resolves the SPP channel via `sdptool`, dumps RX bytes, tags `KISS (C0…)` vs `GAIA (FF 01…)`.
Usage: `python3 rfcomm-rx-dump.py 38:D2:00:01:55:5C`. **Blocked today by the §2 Page-Timeout.** Once
BT is fixed, this is the decisive transport test: tune the radio to busy APRS (144.390) + KISS mode,
run this, watch gqrx in parallel as ground truth — KISS frames arriving = our socket RX works.

### Synergy for Gate 2 / on-air AX.25
The harness lets the agent **independently decode what the radio transmits**. During an operator on-air
dial, point `rf-monitor.sh 145.710` at the air: it captures our SABM (verify it's well-formed AX.25)
and any gateway reply — turning "TX works but RX broken" from a guess into evidence.

---

## 4. AX.25 prior-art investigation — findings (the original goal)

Three parallel agents read prior art vs our stack (`src-tauri/src/winlink/ax25/` on `main`/the oxi
worktree; **NOT** on stale `task-amd-main-ui`). Clones on pandora: `dev/scratch/ax25-prior-art/{direwolf,
wl2k-go,pat,benlink,HTCommander,kiss-tnc-test,bt-ht-n76}`.

**Our code (read this session, all in the oxi worktree):** `frame.rs` (AX.25 codec), `kiss.rs` (KISS),
`datalink.rs` (connected-mode state machine + `Ax25Stream`), `link.rs` (byte transports), `rfcomm.rs`
(raw BT socket). The KISS deframer + frame codec are **spec-correct**; `recv_frame` reads→decodes→
filters by `dest==mycall` correctly; `Ok(0)`→EOF→ConnectionAborted; WouldBlock/TimedOut→Ok(None).

### Actionable conclusions
- **`tuxlink-b0i` is REAL (P2, exists):** `Path::encode` ([frame.rs:223-226]) hardcodes dest C-bit=1 /
  src C-bit=0 = **command** for *every* frame. AX.25 v2.2: command = dest1/src0, **response = dest0/src1**.
  Our **SABM is correctly a command** (Dire Wolf agrees), but our **UA/DM/RR-as-response/acks are
  mislabeled commands** → a strict gateway logs a protocol error and can drop connected-mode data.
  **TX-side defect; does NOT corrupt our RX** (decode discards C-bits). Fix: thread cmd/res through
  `Path::encode`/`Address::encode` like Dire Wolf's `set_addrs` (`ax25_pad2.c:734-743`).
- **Our SSID-exact UA match is CORRECT — do NOT loosen it.** Dire Wolf (`ax25_link.c:774-820`) matches
  SSID-exact too. (`datalink.rs:112-114,168-169`.)
- **No KISS "wake"/init command exists** for standard TNCs (Dire Wolf as host sends nothing). So the
  "RX broken" is *not* a missing init.
- **The UV-Pro speaks raw KISS over SPP** (operator ground truth — §1). GAIA theory **retracted**.
- **Strategic (wl2k-go/Pat):** the mature Linux clients **delegate connected-mode AX.25 to the KERNEL
  stack** (`kissattach` + `AF_AX25`), not a hand-rolled KISS state machine. tuxlink hand-rolls
  (`datalink.rs`) — i.e. the thing under suspicion is what the reference deliberately avoids. *Possible
  longer-term pivot:* feed the UV-Pro RFCOMM-socket bytes into the kernel AX.25 stack (via a socket↔PTY
  shim, since `kissattach /dev/rfcommN` kills the UV-Pro's SPP per `rfcomm.rs` history). Not today's fix.

### `tuxlink-4ef` ("RX broken") — re-anchored partition plan
Raw KISS BT works (operator). So 4ef is in our socket path/config OR a test artifact — **not** a need-GAIA.
To partition (once §2 BT is fixed):
1. **Transport test (RX-only, no TX):** `rfcomm-rx-dump.py` on the radio tuned to busy APRS + KISS mode.
   KISS frames arrive in sync with gqrx bursts ⇒ socket RX works ⇒ 4ef is higher up (connect loop /
   addressing / or KISS-mode-off at test time). No frames ⇒ transport/mode issue.
2. **On-air test (operator, RADIO-1):** one bounded+abortable dial via the app with `TUXLINK_RFCOMM_TRACE=1`
   on 145.710, while `rf-monitor.sh 145.710` captures the air independently. Trace RX bytes `FF 01…`=GAIA /
   `C0…`=KISS / nothing; RTL-SDR shows whether our SABM is well-formed + whether the gateway replies.

---

## 5. PR #125 consolidation — STILL OPEN, gates untouched this session

PR **#125** (`bd-tuxlink-oxi/consolidate` → `main`) merges AX.25/Bluetooth + session-selector + the
ka7/p5u/efo/2y4 fixes + the 4ef RFCOMM *trace* (instrumentation, not a fix). **All unit gates green.**
**Both merge gates remain PENDING (operator-interactive):**
- **Gate 1 — browser re-smoke** (restart-free cms-z + Plaintext dial proves the ka7 fix). From
  `worktrees/bd-tuxlink-oxi-consolidate`: `pnpm tauri dev`, select Winlink-CMS → cms-z + Plaintext →
  connect → confirm it dials cms-z **plaintext** (not prod-TLS) with no app restart. *(This Pi has no
  Wayland input-injection tool, so the click-through is operator-driven; the agent can launch + grim-screenshot.)*
- **Gate 2 — packet on-air** (`tuxlink-4ef`, RADIO-1, operator-only) — now also the 4ef diagnostic per §4.
- After both: merge #125, close #123/#124, close `tuxlink-oxi`, dispose 3 worktrees (`3pb`, `uhc`, `oxi-consolidate`).
- Prod-TLS CMS stays blocked on `tuxlink-9h8` (register the SID). Refinement: `tuxlink-0ja` (abort-write TOCTOU).
- Full prior context: `dev/handoffs/2026-05-22-marsh-hemlock-lichen-consolidation-config-radio1.md`
  (committed on the oxi branch) + `2026-05-22-redwood-cypress-spruce-selector-smoke-consolidation.md`.

---

## 6. bd issues

- **NEW (filed this session):** Pi-side Bluetooth ACL Page-Timeout blocker — see §2. (ID in the commit / `bd ready`.)
- **Existing, relevant:** `tuxlink-4ef` (RX broken — partition plan §4), `tuxlink-b0i` (C/R bits — confirmed real),
  `tuxlink-uhc` (double-key/params), `tuxlink-sox` (transport persistence), `tuxlink-0ja` (abort TOCTOU),
  `tuxlink-9h8` (register SID), `tuxlink-oxi` (consolidation, in_progress until #125 merges).

---

## 7. Machine / repo state

- **Branch:** `task-amd-main-ui` (main checkout). **NOTE:** this branch is stale (419 behind `main`,
  no AX.25 engine). All AX.25 code lives on `main` / the oxi worktree. This handoff is committed here
  for continuity; the real code work happens on `main`-descended branches.
- **Working tree:** untracked `dev/scratch/` (harness tools + prior-art clones + rf-captures),
  `dev/adversarial/` (Codex output), plus pre-existing untracked items (assets/, docs/design/, etc.).
  `.beads/issues.jsonl` is MM — **do not commit it** (bd owns it via Dolt; `bd dolt push` instead).
- **Worktrees:** the consolidation + per-task worktrees remain (`worktrees/bd-tuxlink-*`). The 3 to
  dispose post-#125-merge: `3pb`, `uhc`, `oxi-consolidate` (ADR 0009 ritual).
- **No tracked src/ changed this session** (harness + diagnostics only). No quality gates needed.
- **System changes:** apt installs (direwolf/multimon-ng/gqrx-sdr), DVB blacklist file, gqrx config.
  Bluetooth controller was reset/rebound multiple times (no persistent config change left behind;
  `input.conf`'s duplicated `Disable=Headset` predates this session and is a no-op).

---

## 8. Next step is §2. Everything else waits on the Bluetooth link.
