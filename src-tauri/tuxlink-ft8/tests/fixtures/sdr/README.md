# SDR FT-8 capture fixtures — M3/M4 exit-gate oracle

Real off-air 20m + 40m FT-8 captures with **ground-truth decode lists**, for the
Station Intelligence decoder's exit gate (bd `tuxlink-b026z.1`, plan milestones
M3/M4): our clean-room decoder must recover **≥85% of these reference messages**
(callsign-pair + payload match), **zero false decodes**, on each capture.

## The reference is AP-DISABLED (this matters)

The `.jt9-ap-off.txt` files are WSJT-X's decoder (`jt9`) run on the WAV with **no
`--my-call` / DX context**, so **a-priori (AP) decoding is inert**. This is
deliberate: AP decodes are messages WSJT-X only recovers by *assuming* known bits
mid-QSO, which a passive unaided decoder structurally cannot reproduce. Diffing
against an AP-*enabled* reference would rig the ≥85% gate. Regenerate a reference
with exactly:

```
jt9 -8 <capture.wav>        # FT-8 mode, no my-call => AP disabled
```

Match rule (per the plan): multiset on normalized message identity; standard msgs
by callsign-pair+report+grid; free-text/telemetry by exact payload; hashed `<...>`
as their own class.

## Fixtures (captured 2026-07-06, RTL-SDR V3 on a Delta Loop)

| File | Band | Dial | UTC slot | Ref decodes |
|---|---|---|---|---|
| `ft8-40m-crowded-20260706T121300Z.wav`  | 40m | 7.074 MHz  | 12:13:00Z | 10 |
| `ft8-40m-ordinary-20260706T121215Z.wav` | 40m | 7.074 MHz  | 12:12:15Z | 5  |
| `ft8-20m-busier-20260706T121415Z.wav`   | 20m | 14.074 MHz | 12:14:15Z | 4  |
| `ft8-20m-quiet-20260706T121400Z.wav`    | 20m | 14.074 MHz | 12:14:00Z | 2  |

WAVs are the WSJT-X standard: **12000 Hz, mono, 16-bit signed, exactly 15 s**
(one FT-8 T/R slot, boundary-aligned).

> **Note on "crowded":** 20m was quiet at capture time (≤4 decodes). The plan's
> crowded-band arm (which stresses multi-pass subtraction, ~20–40 overlapping
> signals) is only lightly exercised here. When 20m/40m are busy, re-capture a
> denser slot with the recipe below and add it as `*-crowded-*`.

## Capture recipe (RTL-SDR Blog V3, direct sampling)

The V3 receives HF via **direct sampling on the Q-branch (input 2)** — `rtl_fm -E
**direct2**` (NOT `-E direct`, which is the empty I-branch). PPM 0, AGC. `rtl_fm`
cosmetically logs "Tuned to 7330000" in direct-sampling mode; ignore it, the band
is correct. Receive-only — no transmit, no RADIO-1 concern.

```bash
# one slot-aligned 15 s FT-8 WAV on <dial> (7074000 = 40m, 14074000 = 20m)
now=$(date -u +%s); sleep $(( 15 - now % 15 ))            # align to :00/:15/:30/:45
timeout 16 rtl_fm -M usb -E direct2 -f <dial> -s 1024000 -r 12000 cap.s16
sox -t raw -r 12000 -e signed -b 16 -c 1 cap.s16 cap_raw.wav
sox cap_raw.wav slot.wav trim 0 15                        # exactly 15 s for jt9
jt9 -8 slot.wav                                           # decode (AP disabled)
```

Tools on this Pi: `rtl_fm`, `sox`, `jt9` (all preinstalled). NTP must be synced
(it is) for slot alignment.
