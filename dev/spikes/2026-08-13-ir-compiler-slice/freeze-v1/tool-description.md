# Frozen tool description — `routine_template_compile` (v1)

Everything between the BEGIN/END markers below is the MCP tool
`description` field, byte-for-byte. It is the candidate's ENTIRE
instruction artifact: no sheet, no system-prompt additions, no other
teaching surface exists in the evaluation (design §1, Registry discovery).
Used unchanged in evaluation and, if the premise survives, carried toward
shipping.

<!-- BEGIN TOOL DESCRIPTION v1 -->
Create a routine by filling in a template. You pick one template id and
supply its slots; the compiler builds, validates, and (only if you pass
save:true) saves the routine. You never write steps, ids, or structure —
the compiler owns all of that.

Compile vs save: with save omitted or false, the result is a compiled and
validated DRAFT — nothing is stored and nothing will run. Pass save:true
to also save it. Saving never enables or runs the routine.

Templates (these three ids are the complete set):

1. "scheduled-connect-with-fallback" — on a schedule, attempt a Winlink
   connect across a saved station set, walking stations and bands in
   order; log the outcome either way; a fully failed pass ends the run
   failed. Slots:
   - name: routine name, lowercase kebab-case (e.g. "wa-gateway-check")
   - every: how often, like "30m", "2h", "45s" (or null: manual runs only)
   - window: local-time window "HH:MM-HH:MM" (or null: no window). A
     window requires a schedule (every must not be null).
   - stations: the saved station set to try, by its exact id
   - bands: ordered list of bands to try per station, like ["40m", "80m"].
     Must be non-empty with no repeats. Order is meaning: it is the
     fallback order.
   - success_log: text logged after a successful connect. Exactly two
     placeholders exist: $station and $band (the ones that connected).
   - failure_log: text logged when every attempt failed. $station/$band
     are NOT available here (nothing connected).
   - fail_reason: short reason recorded on the failed end.

2. "scheduled-aprs-broadcast" — on a schedule, transmit one APRS broadcast
   of fixed text. No connection is made. Slots:
   - name: routine name, lowercase kebab-case
   - every: how often, like "1h" (required — this template is a schedule)
   - window: local-time window "HH:MM-HH:MM" (or null)
   - message: the exact text to broadcast (no placeholders; sent as-is)

3. "scheduled-log-entry" — on a schedule, write one local log line. No
   radio involved. Slots:
   - name: routine name, lowercase kebab-case
   - every: how often, like "1h" (required)
   - note: the exact text to log (no placeholders; logged as-is)

Value forms: durations are a number plus s/m/h ("90s", "15m", "2h");
plain English like "15 minutes" or "2 hours" is understood. Bands are the
standard labels 160m 80m 60m 40m 30m 20m 17m 15m 12m 10m; "40 meters" is
understood. Windows are 24-hour "HH:MM-HH:MM"; an overnight window like
"22:00-06:00" is valid.

Worked examples:

Call: {"template": "scheduled-connect-with-fallback", "slots": {"name":
"wa-gateway-check", "every": "2h", "window": "08:00-18:00", "stations":
"wa-gateways", "bands": ["40m", "80m"], "success_log": "Connected to
$station on $band", "failure_log": "All WA gateways unreachable this
cycle", "fail_reason": "no gateway reachable"}}
Result: lowering ok, not saved, validation valid — a compiled draft plus
a readback of what it will do.

Call: {"template": "scheduled-aprs-broadcast", "slots": {"name":
"noon-status", "every": "1h", "window": "11:00-13:00", "message": "K7ABC
portable, monitoring 146.52"}}
Result: lowering ok, not saved, validation valid.

Call: {"template": "scheduled-log-entry", "slots": {"name":
"hourly-heartbeat", "every": "1 hour", "note": "station heartbeat"},
"save": true}
Result: lowering ok, SAVED at a revision, validation valid ("1 hour" was
normalized to "1h").

A refusal, worked: {"template": "scheduled-connect-with-fallback",
"slots": {..., "every": "15 minutes then retry", ...}} returns lowering
failed with a finding naming the slot: code SLOT_NOT_A_DURATION, slot
"every", value "15 minutes then retry", rule "every takes a single
duration like '15m' or '2h'; extra words are not part of a duration",
remedy "call routine_template_compile again with slots.every set to just
the interval, e.g. \"15m\"". Nothing was created. Fix exactly the named
slot and call again once.

If the user asks for behavior no slot expresses (retries, extra steps,
different actions), do NOT put it in any slot text and do NOT approximate
it with another tool: compile what the slots can express and tell the
user plainly what was left out.
<!-- END TOOL DESCRIPTION v1 -->

## Notes for the reviewer (not part of the description)

- The final paragraph is the only behavioral instruction in the artifact.
  It is tool-surface text, not system-prompt doctrine (design §1: no new
  system-prompt additions), and it states the N2-correct behavior
  (omit + tell the user) plus the no-native-tool rule positively.
- The worked refusal deliberately shows the full finding shape so the
  model has SEEN one before its first real refusal (examples-outrank-prose,
  lnctz).
- The third worked example doubles as the save:true demonstration and the
  normalization demonstration.
- Token footprint of this description is recorded per run (design §4 CTRL
  launch assertions: catalog hashes + token counts).
