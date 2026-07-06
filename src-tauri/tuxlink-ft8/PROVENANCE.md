# tuxlink-ft8 — constants & tables provenance ledger

**Clean-room rule.** Every magic constant, table, or algorithm in this crate cites
an **allowed** source below. Allowed sources:

- QEX 2020 "The FT4 and FT8 Communication Protocols" (Franke/Somerville/Taylor)
- WB2FKO "Synchronization in FT8"
- `ft8_lib` (kgoba) — **MIT**
- `RustFT8` (jl1nie) — **MIT**

**Forbidden.** `wsjtr` / WSJT-X are **GPL** and are used ONLY as a pre-built binary
test oracle (feed a WAV, read the stdout decode list). Never:

- read `wsjtr`/WSJT-X source,
- run `strings` / `nm` / `objdump` / a decompiler on the oracle binary,
- copy WSJT-X's `generator.dat` / `parity.dat` (the LDPC matrix comes from MIT
  `ft8_lib`, or is regenerated from the spec).

A CI grep-guard fails the build if any crate file references `wsjtr` internals or
the GPL `.dat` matrix files.

## Ledger

| Constant / table | Value / status | Allowed source |
|---|---|---|
| Costas array | `3,1,4,0,6,5,2` | QEX 2020 §4; `ft8_lib` `constants.c` (MIT) |
| Frame geometry | 79 = {7,29,7,29,7} | QEX 2020 §4 |
| Info symbols | 58 (× 3 bits = 174) | QEX 2020 §4 |
| Payload / codeword / msg+CRC | 77 / 174 / 91 | QEX 2020 §2–3 |
| Tone spacing / symbol time | 6.25 Hz / 0.160 s | QEX 2020 §4, Table 4 |
| CRC-14 polynomial | TBD (T0.3) — `0x6757` per QEX §3, `0x2757` low-14 in `ft8_lib` `crc.c`; pin by KAT | QEX 2020 §3 / `ft8_lib` (MIT) |
| Gray map (symbol↔bits) | TBD (T0.4) | QEX 2020 Table 3 |
| LDPC(174,91) parity matrix + generator | TBD (T0.5) — from MIT `ft8_lib`, **NOT** WSJT-X | `ft8_lib` (MIT) |
| Soft-demapper LLR scale | TBD (T1.1) | `ft8_lib` decode (MIT) |
| Callsign hash (10/12/22-bit) | TBD (T0.2) | `ft8_lib` `pack.c` (MIT) |

Update this table in the same commit that introduces each constant.
