# UV-Pro Benshi protocol — golden byte vectors (codec test fixtures)

Generated from **benlink's authoritative pure encoder** (`Message(...).to_bytes()`)
offline — no radio required (the bitfield codec is pure). Source:
github.com/khusmann/benlink @ cloned HEAD 2026-06-12. These are the oracles the
Rust codec is TDD'd against; each is a verified `encode`/`decode` round-trip in
benlink. Regenerate with `dev/scratch/benshi-re/` clone + a venv with
`pydantic bleak typing_extensions` (see GROUNDING-FINDINGS.md).

Header recap: `command_group:u16 + is_reply:1bit + command:15bit + body`, big-endian.
`is_reply` is the **MSB of byte 2**. GAIA (RFCOMM) wrap: `ff 01 <flags> <n=len-4> <data> [csum?]`.

## Encode direction (host → radio)

| Message | Bytes (hex) |
|---|---|
| `GET_HT_STATUS` request | `00 02 00 14` |
| `READ_RF_CH(0)` request | `00 02 00 0d 00` |
| `READ_STATUS(battery%)` request | `00 02 00 05 00 04` |
| `REGISTER_NOTIFICATION(HT_STATUS_CHANGED)` | `00 02 00 06 01` |
| `GET_DEV_INFO` request | `00 02 00 04 03` |
| `WRITE_RF_CH` ch0 146.520 MHz simplex FM WIDE, no tone, name "CALL" | `00 02 00 0e 00 08 bb b7 c0 08 bb b7 c0 00 00 00 00 50 00 43 41 4c 4c 00 00 00 00 00 00` |
| ↳ same, GAIA-wrapped (RFCOMM) | `ff 01 00 19 00 02 00 0e 00 08 bb b7 c0 08 bb b7 c0 00 00 00 00 50 00 43 41 4c 4c 00 00 00 00 00 00` |

Notes:
- `RfCh` is **200 bits = 25 bytes**. Freq is `tx_mod:2 + tx_freq:u30` packed into 4
  bytes: `08 bb b7 c0` → top 2 bits = mod (00=FM), low 30 bits = 146 520 000
  (= round(146.520 × 1e6)). rx is the same 4 bytes. Then sub-audio `00 00 00 00`
  (none), flag byte `50` + `00`, then `name_str[10]` = "CALL" + zero pad.
- GAIA `n` = `0x19` = 25 = (29-byte message − 4 header bytes).

## Decode direction (radio → host)

| Message | Bytes (hex) | Key decoded fields |
|---|---|---|
| `GET_HT_STATUS` reply, StatusExt | `00 02 80 14 00 b4 3c c0 00` | reply, SUCCESS, tx=F rx=T sq=T ch_lower=3 rssi=80 |
| `EVENT_NOTIFICATION HT_CH_CHANGED` ch5 446.000 FM NARROW "UHF" | `00 02 00 09 05 05 1a 95 6b 80 1a 95 6b 80 00 00 00 00 40 00 55 48 46 00 00 00 00 00 00 00` | event_type=5, RfCh ch5 |
| `READ_STATUS` reply, battery 73% | `00 02 80 05 00 00 04 49` | reply, SUCCESS, type=4, value=0x49=73 |
| `WRITE_RF_CH` reply, ok | `00 02 80 0e 00 00` | reply, SUCCESS, channel_id=0 |
| GAIA two frames back-to-back | `ff 01 00 05 00 02 80 14 00 b4 3c c0 00` + `ff 01 00 00 00 02 00 14` | deframer multi-frame test; 2nd frame `n=0` |

## Settings (for `uvpro_set_channel` via WRITE_SETTINGS)

- `Settings` is **176 bits = 22 bytes**, identity round-trip verified.
- The active channel is `Settings.channel_a` (VFO A) / `channel_b` (VFO B), each a
  nibble-split `*_lower` (4b) + `*_upper` (4b) → 0..255.
- Byte 0 = `(channel_a_lower << 4) | channel_b_lower`. Example with
  channel_a_lower=1, channel_b_lower=2 → byte 0 = `0x12`.
- `set_channel` keeps the raw 22 bytes from `READ_SETTINGS` and patches ONLY the
  `channel_a` (or `channel_b`) nibbles at their pinned bit offsets, then
  `WRITE_SETTINGS` — preserving every other field. (Offsets to be pinned by a
  diff-of-two-encodings golden vector at implementation time.)
- Sample (channel_a=0/1): `12 13 94 0a 51 60 04 02 28 00 00 00 04 00 00 00 00 00 00 00 00 00`

## Status sizes
- `Status` = 16 bits (2 bytes); `StatusExt` = 32 bits (4 bytes; adds rssi/region/
  channel-upper). Discriminate by reply body length.
- `ReplyStatus`: SUCCESS=0, NOT_SUPPORTED=1, NOT_AUTHENTICATED=2,
  INSUFFICIENT_RESOURCES=3, AUTHENTICATING=4, INVALID_PARAMETER=5,
  INCORRECT_STATE=6, IN_PROGRESS=7.
