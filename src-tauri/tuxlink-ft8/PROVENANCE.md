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
| Callsign hash (10/12/22-bit) | multiplier `47055833459` (`0xAF5A2E6F3`); n12=n22>>10, n10=n22>>12 | `ft8_lib` `message.c` `save_callsign` (MIT); QEX Table 2 `h22/h12/h10` |
| Payload byte length | 10 bytes (77 bits, top 3 unused) | `ft8_lib` `message.h` `FTX_PAYLOAD_LENGTH_BYTES` (MIT) |
| Special-token limits | `MAX22=4194304`, `NTOKENS=2063592`, `MAXGRID4=32400` | `ft8_lib` `message.c` (MIT) |
| Char tables | FULL(42) `" 0-9A-Z+-./?"`, ALNUM_SPACE_SLASH(38), ALNUM_SPACE(37), LETTERS_SPACE(27), ALNUM(36), NUMERIC(10) | `ft8_lib` `text.h` table comments (MIT); QEX Table 2 |
| Basecall mixed-radix | `37·36·10·27·27·27` | `ft8_lib` `message.c` `pack_basecall` (MIT); QEX Table 2 `c28` |
| Special c28 tokens | `DE=0, QRZ=1, CQ=2` | `ft8_lib` `message.c` `pack28`/`unpack28` (MIT) |
| Grid/report sentinels | grid=g15; blank=`MAXGRID4+1`; RRR/RR73/73=`+2/+3/+4`; report=`MAXGRID4+35+dd` | `ft8_lib` `message.c` `packgrid`/`unpackgrid` (MIT); QEX Table 2 `g15/R1/r2` |
| Free-text / telemetry pack | base-42 over 13 chars (f71) / 71-bit hex (t71), left-shift-by-1 into 10-byte payload | `ft8_lib` `message.c` `ftx_message_encode_free`/`_telemetry` (MIT); QEX Table 1 rows `0.0`/`0.5` |
| Std message bit layout | `c28 r1 c28 r1 R1 g15`, `i3` at bits 74..76, `n3` at 71..73 | `ft8_lib` `message.c` `ftx_message_encode_std`/`ftx_message_get_i3`/`_get_n3` (MIT); QEX Table 1 |

**Deferred to T0.2-follow-up** (marked `TODO(T0.2-follow-up)` in `message.rs`,
not half-implemented): EU VHF (`i3=2`), RTTY RU (`i3=3`), full nonstandard-call
type-4 packing (`pack58`/`unpack58`), DXpedition (`n3=1`), Field Day (`n3=3/4`),
`CQ nnn`/`CQ a[bcd]` modifiers, and the `3DA0`/`3X` prefix work-arounds.

Update this table in the same commit that introduces each constant.
