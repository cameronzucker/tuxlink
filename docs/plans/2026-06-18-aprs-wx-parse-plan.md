# Plan: APRS weather (WX) parse + engine emit — tuxlink-wu2x

RX-only backend slice. Mirrors PR #792 (telemetry emit). Source-reactive panel is the
separate tuxlink-2phz follow-on; this slice = pure parser + engine seam + DTO contract.
Design doc: ~/.gstack/projects/cameronzucker-tuxlink/administrator-bd-tuxlink-xygm-design-20260617-102445.md

## Licensing note (important)
Implement from the **documented APRS WX format** (APRS101 §12) — the field letters, units,
and conversions are protocol FACTS. Write idiomatic Rust from scratch. Do NOT copy
aprslib's Python code structure. (aprslib was used only to confirm the facts below.)

## Field facts (the format)
Wind prefix: a report's WX data begins `ddd/sss` = wind direction (deg) `/` sustained speed (mph).
Then field-letter groups (each letter + fixed-width digits):
| letter | field | width | unit / conversion |
|---|---|---|---|
| `g` | wind gust | 3 | mph |
| `t` | temperature | 3, OR `t-NN` (2, negative) | °F |
| `r` | rain last hour | 3 | 1/100 in → in (÷100) |
| `p` | rain last 24h | 3 | 1/100 in |
| `P` | rain since midnight | 3 | 1/100 in |
| `h` | humidity | 2 | %, `h00` = 100% |
| `b` | barometric pressure | 5 | 1/10 hPa → hPa (÷10) |
| `l`/`L` | luminosity | 3 | W/m²; `l` value +1000 |
| `s` | snowfall last 24h | 3 | inches |
| `#` | raw rain counter | 3 | count |

### Adversarial edge-cases (MUST test)
1. **`s` is overloaded** — `s` after the wind prefix means wind SPEED (the `/sss` form), but a
   later `s` means SNOW. Handle the wind prefix FIRST (consume `ddd/sss` as dir+speed), so a
   subsequent `sNNN` is unambiguously snow. (This is the trap aprslib hard-codes around.)
2. **Negative temp** `t-05` (2 digits after the minus).
3. **`h00` = 100%**, not 0.
4. **Partial field sets** — only some letters present. RF-honesty: absent fields are `None`,
   never 0.
5. **Two carriers**: (a) positionless weather report — DTI `_` then `MDHM`(8-char timestamp)
   then the WX data; (b) a position report whose symbol is the weather `_` — the WX data is in
   the position COMMENT (after lat/lon/symbol).
6. Wind direction may be `...`/blank (unknown) → `None`.
7. Trailing text after the parsable WX run is a comment, not a field.

## Files (mirror #792)
- NEW `src-tauri/src/winlink/aprs/weather.rs`:
  - `WeatherReport` struct, serde `rename_all="camelCase"`, all fields `Option` (+ `station`,
    `comment`): `wind_direction_deg:Option<u16>`, `wind_speed_mph`, `wind_gust_mph`,
    `temperature_f`, `humidity_pct`, `pressure_hpa`, `rain_1h_in`, `rain_24h_in`,
    `rain_since_midnight_in`, `luminosity_wm2`, `snow_in` (f64 where fractional).
    (Ham-conventional units on the wire; a metric toggle is deferred to the panel.)
  - `parse_weather_data(body:&str) -> Option<(WeatherReport-fields, comment)>` — the core
    field parser; returns None if no WX fields present.
  - `parse_positionless_weather(info:&[u8]) -> Option<WeatherReport>` — info starts `_`, strip
    DTI + 8-char timestamp, then `parse_weather_data`.
  - `is_weather_symbol(table:char, code:char) -> bool` — code == '_' (weather).
  - Pure + fully unit-tested.
- `mod.rs`: `pub mod weather;`
- `engine.rs`:
  - `EventSink`: add `fn emit_weather(&self, ev: WeatherReport);` — impl in `TauriEventSink`
    (`self.app.emit("aprs-weather:new", &ev)`) + BOTH test `RecSink`s (engine.rs + native_driver.rs:
    add a `weather` Vec field in the engine RecSink, no-op or record in native_driver RecSink).
  - `ingest_ax25` None arm: if `info` starts with `_`, `parse_positionless_weather` → `emit_weather`
    (alongside the existing raw-feed row; mirror the T# telemetry seam).
  - `try_emit_position`: when the decoded position's `symbol_code == '_'`, run `parse_weather_data`
    on the position COMMENT; if Some, `emit_weather` (station = sender). READ try_emit_position to
    wire this without disturbing the existing position emit.
  - No engine state field needed (weather is per-report; history is frontend).
- `src/aprs/aprsTypes.ts`: `WeatherReportDto` interface mirroring the serde shape (all fields
  optional `number|null`, plus `station`, `comment`). Doc-comment the channel kinds the
  source-reactive panel will derive (wind_dir/wind_speed/wind_gust/temperature/humidity/pressure/
  rain/luminosity/snow).

## Tests
- weather.rs unit tests: full Davis-style body (`220/004g005t068r000p000P000h53b10138`),
  negative temp (`t-05`), `h00`→100, snow-after-wind disambiguation, partial fields, positionless
  (`_10090556c220s004g005t068...`), position-embedded (symbol `_`, WX in comment), unknown wind.
- engine.rs integration test via the existing `inbound_with(src, info)` helper: feed a positionless
  WX frame + a `_`-symbol position frame, assert the recorded `weather` Vec has the parsed fields.

## Gate / process
- Rust is NOT compilable on this Pi (cold cargo) → write Rust + tests, let CI compile/run.
  `pnpm -C <worktree> typecheck` validates the TS DTO locally.
- Implementer subagent: ABSOLUTE paths only (cwd resets); code + run typecheck + STOP UNCOMMITTED;
  the parent commits.
- Then: parent commit → Codex cross-provider adrev on the diff (WX wire-format correctness + seam)
  → draft PR → CI green (fix-forward) → ready + no-ff merge + worktree disposal (ADR-0009).
- Moniker: kite-harrier-shoal.
