# Running VARA HF on a Raspberry Pi 5 (ARM64) under box64 emulation

**Status: WORKING.** VARA HF v4.9.0 (a 32-bit Windows app) runs on a Raspberry Pi 5 with no
x86 hardware, via `box64` + `wine`, stable, with its TCP control/data ports (8300/8301) live
and answering commands. First confirmed 2026-06-24 (N7CPZ / `pandora`).

This is, as far as we can find, undocumented in the amateur-radio space — the common assumption
is "VARA needs Windows/x86." It does not strictly need x86 *hardware*; it needs x86 *emulation*
done correctly. This guide is the full, reproducible recipe, including every wall hit and why.

> **Honest scope (read first):** this proves VARA **launches, runs stably, and serves its TCP
> interface** under emulation. It does **NOT** yet prove the real-time **DSP keeps up under load**
> (decoding/encoding an actual signal in real time) — that is the separate, harder question and is
> tested elsewhere. Idle-runs ≠ ARQ-works. Treat "it runs" and "it's usable on-air" as two claims.

## Why anyone would want this
A single ~8 W Raspberry Pi can then run **the radio interface, VARA, and a Winlink client all in
one box** (client → `localhost:8300` → VARA → radio), with no second x86 machine and no split of
mail/state across two hosts ("data island"). That is the motivating payoff — *if* the DSP holds.

---

## Environment this was proven on
| | |
|---|---|
| Board | Raspberry Pi 5 Model B Rev 1.1 (16 GB) |
| OS | Debian GNU/Linux 13 (trixie), aarch64 |
| Kernel | `6.18.34+rpt-rpi-v8`, **4 KB page size** (see step 1 — this is critical) |
| box64 | **v0.4.3** (commit `75c0edc`), built from source — apt's 0.3.4 does **not** work |
| wine | **Kron4ek `wine-11.11-amd64-wow64`** (x86_64 wow64 portable build) |
| winetricks | 20250102 |
| VARA | HF v4.9.0 (32-bit x86, Visual Basic 6 — `file VARA.exe` → `PE32 … Intel i386`) |

Why VARA is 32-bit and Windows-locked: it's a **Visual Basic 6** program (`MSVBVM60`,
`MSSTDFMT.DLL`). VB6 only ever produced 32-bit x86. That dictates the whole emulation stack below.

---

## The recipe

### 1. Boot a 4 KB-page kernel (mandatory)
The Pi 5 default kernel (`…-2712`) uses **16 KB pages**; wine's `wineserver` aborts on it
(`wineserver: page size is 16k but Wine requires 4k pages`). Switch to the 4 KB kernel
(`kernel8.img`, already on the system):
```bash
echo "kernel=kernel8.img" | sudo tee -a /boot/firmware/config.txt   # under [all]
sudo reboot
# verify after reboot:
getconf PAGESIZE        # must print 4096
```

### 2. box64 **v0.4.x from source** (apt 0.3.4 is too old — this was the hardest wall)
With apt's box64 0.3.4, the x86 wow64 wine builds a prefix but then **cannot load `kernel32.dll`**
(`status c0000135`) — box64 0.3.4 predates mature new-wow64 support. Build current box64:
```bash
sudo apt-get install -y git cmake make gcc
git clone --depth 1 https://github.com/ptitSeb/box64 ~/box64-src
cd ~/box64-src && mkdir build && cd build
cmake .. -DRPI5ARM64=1 -DCMAKE_BUILD_TYPE=RelWithDebInfo
make -j3            # ~25-40 min on a Pi 5 (multi-pass dynarec)
sudo make install   # installs to /usr/local/bin/box64
# point binfmt (used for wine's child processes) at the new box64:
sudo ln -sf /usr/local/bin/box64 /usr/bin/box64   # reversible: apt reinstall box64
box64 --version     # expect v0.4.x
```

### 3. An x86_64 **wow64** wine, run under box64
The apt `wine` on arm64 is **ARM-native** (`/usr/lib/wine/wine64` → `ELF … ARM aarch64`) and
fails with `wineserver doesn't support the 01c4 architecture`. You need a real **x86_64** wine,
which box64 emulates. Use a portable **wow64** build (runs 32-bit apps without 32-bit libs):
```bash
mkdir -p ~/vara-box64-work && cd ~/vara-box64-work
curl -L -o wine.tar.xz \
  https://github.com/Kron4ek/Wine-Builds/releases/download/11.11/wine-11.11-amd64-wow64.tar.xz
mkdir -p wine-x86 && tar -xf wine.tar.xz -C wine-x86 --strip-components=1
file wine-x86/bin/wine    # ELF 64-bit … x86-64  (box64 will run this)
```
Set up the env (used for every step below):
```bash
export WINEPREFIX="$HOME/vara-box64-work/vara.wine"
export WINELOADER="$HOME/vara-box64-work/wine-x86/bin/wine"
export PATH="$HOME/vara-box64-work/wine-x86/bin:$PATH"
export WINEDEBUG=-all
export BOX64_LD_LIBRARY_PATH="$HOME/vara-box64-work/wine-x86/lib/wine/x86_64-unix:$HOME/vara-box64-work/wine-x86/lib"
```

### 4. Initialize the prefix (decline .NET/Gecko)
```bash
box64 "$WINELOADER" wineboot --init
```
A dialog offers to install **wine-mono (.NET)** and possibly **wine-gecko**. **Decline both** —
VARA is VB6, not .NET; they're a needless ~100 MB download under emulation. wineboot blocks on
the dialog, so dismiss it for init to finish (prefix `system32`/`syswow64` populate, ~minutes).

### 5. Install VARA
Get the VARA HF installer (`VARA setup (Run as Administrator).exe`) from
`https://rosmodem.wordpress.com/` (EA5HVK). Then:
```bash
box64 "$WINELOADER" "VARA setup (Run as Administrator).exe" /VERYSILENT /DIR='C:\VARA HF'
ls "$WINEPREFIX/drive_c/VARA HF/"     # VARA.exe, OCX/, VARAHF{500,2300,2750}.dat …
```

### 6. Install the **VB6 runtime** (`MSVBVM60`)
Without it: `import_dll MSVBVM60.DLL not found, c0000135` and VARA exits immediately.
```bash
sudo apt-get install -y winetricks
winetricks -q vb6run          # MSVBVM60 + oleaut32/olepro32/comcat/stdole2
```

### 7. **Register VARA's bundled OCX controls** (the silent install does NOT)
Without these: VARA runs ~18 s then dies with
`com_get_class_object {248dd896-…} not registered` (that GUID is `MSWINSCK.OCX`, the control
VARA uses for its 8300/8301 TCP ports; `MSCHRT20.OCX` is its spectrum display).
```bash
OCX="$WINEPREFIX/drive_c/VARA HF/OCX"
cp "$OCX"/*.OCX "$OCX"/*.DLL "$WINEPREFIX/drive_c/windows/syswow64/"
for c in MSCOMCTL.OCX COMDLG32.OCX MSCOMM32.OCX MSWINSCK.OCX MSCHRT20.OCX MSSTDFMT.DLL; do
  box64 "$WINELOADER" regsvr32 /s "C:\\windows\\syswow64\\$c"
done
```

### 8. Run it
```bash
box64 "$WINELOADER" "$WINEPREFIX/drive_c/VARA HF/VARA.exe" &
# verify:
ss -ltn | grep -E ':830[01]'                 # both LISTEN
printf 'VERSION\r' | nc -w2 127.0.0.1 8300    # → "VERSION VARA HF v4.9.0"
```
VARA's window appears; the command port answers. Idle CPU ~6–8% (one core, box64 dynarec).

---

## Failure → cause quick map (so others don't re-debug)
| Symptom | Cause / fix |
|---|---|
| `wineserver: page size is 16k … requires 4k pages` | 16 KB kernel → step 1 (`kernel8.img` + reboot) |
| `wineserver doesn't support the 01c4 architecture` | using the **ARM** apt wine → step 3 (x86_64 wow64 wine) |
| prefix builds but `could not load kernel32.dll` (c0000135) | **box64 0.3.4 too old** → step 2 (build v0.4.x) |
| `import_dll MSVBVM60.DLL not found` | missing VB6 runtime → step 6 (`vb6run`) |
| runs ~18 s then `com_get_class_object {248dd896-…} not registered` | OCXs unregistered → step 7 (`regsvr32`) |
| stray VS Code `rg` pegging CPU | keep the wine/box64 trees **out of the editor workspace** (huge file counts + symlink follow) |

## Driving it: PTT via Pat/Hamlib (NOT VARA's wine serial)
VARA's own PTT does not work reliably through wine's serial emulation. The working pattern (confirmed by
the dl1gkk Pi-5 guide and pat-users) is to **let the host client (Pat) key the radio via Hamlib/rigctld**
— VARA never touches the serial line:
```bash
rigctld -m <hamlib_model> -r /dev/ttyUSBx -s <baud> --set-conf=dtr_state=OFF,rts_state=OFF &
```
```jsonc
// ~/.config/pat/config.json — Pat watches VARA's PTT request and keys the rig itself
"varahf": { "addr": "localhost:8300", "bandwidth": 500, "rig": "<rig>", "ptt_ctrl": true }
```
Then `pat connect "varahf:///<gateway>"`. Pat holds the VARA link (VARA's TCP indicator goes **green** —
red just means *no host attached*, not a fault) and drives the connect. Use a **clear channel / dummy
load**: VARA's busy detector (no GUI toggle) won't key an occupied frequency. If VARA crashes on launch
on ARM, disable the pdh.dll sensor: `wine reg add "HKCU\Software\Wine\DllOverrides" /v pdh /t REG_SZ /d "" /f`.

> **Single-cable radios (e.g. FT-710):** rigctld holds the CAT serial open, which on a radio whose CAT +
> USB-audio share one internal hub resets the codec mid-stream. Use a **separate-interface rig (e.g. a
> DigiRig: distinct USB audio + CAT)**, or route PTT through a *close-serial* shim that opens the port
> only momentarily to key. A radio with independent audio and CAT paths "just works."

## What's proven vs not
- **Proven:** install + launch + stable run + functional TCP interface + **license**, on emulation, no
  x86 HW. **Pat → VARA host link works** (green, accepts dial commands).
- **Proven (DSP receive path):** with a sound card configured, VARA's real-time receive DSP / busy
  detector demodulates the audio stream **at ~25–34% of one core** (≈8% of a 4-core Pi 5) — the
  deadline-critical path keeps up with ~3× headroom. (Measured via an `snd-aloop` loopback.)
- **Open:** a full bidirectional ARQ throughput run, gated only on getting a clean PTT path keying the
  radio (Pat/Hamlib, above) into a clear channel. Not an emulation question — an integration one.

## Reproducibility notes
- The build of box64 is the load-bearing, non-obvious step; pin a known-good commit if archiving.
- wine version matters: a too-new or too-old wine vs the box64 version can break wow64 DLL loading.
  This combo (box64 v0.4.3 + wine 11.11 wow64) is confirmed; others may work but were not tested.
- Everything (wine, prefix, installer, logs) was staged in `~/vara-box64-work/`, deliberately
  **outside** any editor workspace to avoid a file-watcher scanning the thousands of prefix files.
