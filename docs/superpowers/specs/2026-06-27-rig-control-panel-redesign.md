# Rig-control panel redesign — design spec

**bd:** tuxlink-31c63 · **Session:** marsh-fjord-condor · 2026-06-27
**Depends on:** tuxlink-qevsf (QSY Part-97 mitigation, #935) landing first.

## Problem

The shipped rig control (tuxlink-8fkkk) is not usable as shipped. Seven operator critiques:

1. CAT serial **port is hand-typed** — nobody can fill that in.
2. **QSY-on-fail** is a random checkbox buried in rig settings — out of context. *(Also a Part 97 violation — handled separately, see Scope.)*
3. Three tiny checkboxes vertically stacked → dead space beside them.
4. Only the **FT-710** is supported — useless for the userbase, violates the alpha ethos.
5. A **"CAT backend" field** is shown but not editable — purposeless.
6. Two **adjacent dropdowns** (PTT-method + rig-model) do overlapping work — no contextual sense.
7. The **Tune button** floats with negative space, contextually random (no other physical-radio controls near it).

## Design overview — radio-centric, one surface

Collapse today's two overlapping surfaces (the "Radio" expander with audio + a PTT-method dropdown, and the "Rig control" expander with a rig-model dropdown + CAT fields + checkboxes) into **one collapsible "Radio & audio" group**. All fields are **equal-weight and visible** (operator decision: there is no "advanced" tier — every setting must be set). The group is collapsible (one expander), so it reclaims space once configured.

## The "Radio & audio" group (ARDOP)

One collapsible group, rows in order:

- **Radio** — model picker (see below).
- **CAT port** — detected-serial-ports dropdown + ↻ refresh + manual fallback (reuses the existing serial enumeration that the AX.25/PTT pickers already use). Fixes (1).
- **PTT method** — VOX / Serial RTS / CAT command.
- **Mode** — data mode (e.g. PKTUSB), the token rigctld sets.
- **CAT baud**.
- **Audio in** / **Audio out** — the existing ardopcf capture/playback pickers.
- **Close serial during audio** — single labeled control (not a bare stacked checkbox).

Fixes (3) (no stacked-checkbox dead space — fields are full-width rows), (5) (the inert "CAT backend: Managed rigctld" label is deleted — it conveyed nothing editable), (6) (the rig-model dropdown and the PTT-method dropdown no longer sit as two competing selectors; the radio picker is the single anchor, PTT method is one field under it).

## Radio model picker (fixes 4)

- **Source:** the installed hamlib's actual supported models, queried at runtime via `rigctl -l` (new backend command `rig_list_models() -> Vec<RigModel { id: u32, manufacturer: String, model: String }>`). Parsed + grouped by manufacturer, **searchable**. No list for us to maintain; always accurate to the installed hamlib.
- **No curated/"popular" pins** — full A–Z-by-manufacturer list + search only. (Product-neutral; no operator-preference baked in.)
- **Fallback:** if `rigctl -l` is unavailable/empty, the picker degrades to a manual hamlib-model-number entry + a clear note. Selecting nothing = no rig control (back-compat).

## Per-radio pre-fill (override-respecting)

- A **bundled, documented** profile table maps `hamlib_model_id → { ptt_method, data_mode, baud, close_serial }` for radios with **documented known-good** values. It is objective/product data — NOT any operator's personal tuning. It starts small (radios with documented ARDOP/VARA settings) and grows; a model NOT in the table simply gets no pre-fill (fields keep their current values).
- **On radio selection:** for each profile-managed field (PTT method, Mode, baud, close-serial), apply the new radio's profile value **only if the operator has not overridden that field**. Fields the operator has edited are left untouched.
- **Override tracking:** `Config.rig` gains a persisted set of overridden field keys (e.g. `rig_field_overrides: Vec<String>`). Editing a profile-managed field adds its key; a later radio change skips any key in the set. Survives restarts. (No churn to the existing rig fields; this is additive.)

## Connect section

- **To** · **Frequency (MHz)** with the **Tune button inline** to the right of the frequency input (fixes 7 — it's now adjacent to the value it acts on, not floating). Tune calls the existing mode-agnostic `ardop_tune_rig`.
- **No QSY checkbox** (removed — fixes 2 + the Part 97 issue).

## What is removed

The separate "Rig control" expander; the duplicate rig-model dropdown; the "CAT backend" non-editable label; the three stacked checkboxes as a cluster; the floating Tune; the QSY-on-fail checkbox.

## Scope boundaries (explicit)

- **VARA — needs an operator call (flagged for spec review).** Two field classes are involved: the **rig-config rows** (Radio · CAT port · Mode · baud · close-serial) are the *shared* rig control (rendered in both panels today); the **audio in/out + PTT method rows** are *ARDOP-only* (ardopcf concerns) and are NOT part of VARA. The ARDOP "Radio & audio" group merges both classes; VARA only ever shows the rig-config class. The open question: when the shared rig control is redesigned (model picker, CAT-port picker, pre-fill), VARA's rig-config rows inherit those improvements automatically. **Is that OK (VARA gets the better pickers, same fields, no behavior change), or do you want VARA's rig UI left exactly as-is for now (ARDOP-only redesign)?** I did NOT invent any VARA-specific change (no dropping PTT/audio — that was my earlier overreach, struck); this is purely "does the shared fix land in VARA too." Default if unspecified: yes, VARA inherits the shared pickers (it has the same hand-typed-port / FT-710-only problems), with zero VARA-specific or behavioral changes.
- **Compliant multi-station calling is a SEPARATE spec** (the full resolution of tuxlink-qevsf): make **Find a Station the operator-driven Channel Selection** — ranked channels shown WITH frequencies, operator selects each (WLE parity, Part-97-clean). This redesign only *removes* the non-compliant auto-QSY surface; it does not build the replacement.
- **Product, not personalized.** No operator-preference radios/settings shipped as pins or defaults. The radio list, grouping, and any profile are objective product data.
- Depends on the #935 QSY mitigation landing (so main is compliant in the interim).

## Data model / backend

- New `rig_list_models()` Tauri command (parse `rigctl -l`).
- `Config.rig.rig_field_overrides: Vec<String>` (additive; `#[serde(default)]`; migration-free).
- Existing rig fields (`rig_hamlib_model`, `cat_serial_path`, `cat_baud`, `rigctld_*`, `close_serial_sequencing`, `qsy_on_fail`, `live_vfo_poll`) retained; `qsy_on_fail` stays in the DTO (control removed) per the #935 mitigation.

## Error handling

- `rigctl -l` missing/fails → empty model list + manual model-# entry + note (no crash).
- No serial ports detected → manual CAT-port entry (the existing fallback pattern).
- Radio not in the profile table → no pre-fill (fields unchanged).

## Testing

- Frontend (vitest + tsc): the consolidated group renders the field set; radio selection pre-fills only non-overridden fields; editing a field marks it overridden + survives a later radio change; CAT-port picker lists detected ports + manual fallback; Tune is inline + disabled when freq empty; the QSY checkbox is absent.
- Backend (CI): `rig_list_models` parses `rigctl -l` output (unit test against a captured sample); the override-set round-trips in `Config.rig`.
- On-air validation is operator-only (RADIO-1), on the operator's G90+Digirig+VARA path — that's *validation*, not a product binding.

## Non-goals

Compliant multi-station calling / Find-a-Station Channel Selection (separate). VARA-specific layout changes. Any curated/personalized radio pins. Live VFO readout changes.
