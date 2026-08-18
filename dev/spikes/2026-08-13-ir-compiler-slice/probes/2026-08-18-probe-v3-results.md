# IR-surface probe v3 — discriminating cells (2026-08-18T09:28Z)

Cells: F1 divergent fresh-author (no window, manual-window, 3 bands, success-only ask) · F2 inexpressible-requirement discipline (retry + beacon transmission smuggled in) · C1 structural correction from refusal (ungated log, the banked failure shape) · C2 invalid-construct correction (goto/id or unknown slot). 3 samples/cell, temp 0.2, content delivery.

## ERRATA AND CORRECTED READINGS (post-spot-check, before delivery)

1. **A-F2 verdicts corrected: CLEAN → COMPOSED-RETRY, 3/3.** The mechanical grader checked a key allowlist; every key was legal, but all three samples implemented "retry twice" by nesting connect inside on_failure three levels deep — retry semantics composed from grammatical constructs. The grader could not see shape-semantics; the raw emissions could. (The beacon transmission was correctly dropped in all samples of all surfaces.)
2. **B-F2 confirmed genuinely clean 3/3** after manual read; an automated "3 connects" flag was the word "connection" inside log text. B dropped both retry and beacon. Minor grammar drift throughout B (indentation, "every 30 minutes" spelling) — the leniency class the compiler absorbs.
3. **D-F1 doctrinal datum: all three samples filled the failure slots on a success-only ask** because the template defines them — the surface itself carries the honest-failure doctrine. A-F1 authored success-only exactly as asked, no failure arm volunteered.
4. **Language-design finding:** the five-construct grammar as written permits bounded-loop-by-unrolling (connect is a step; outcome blocks contain steps). "Gotos unexpressable" holds, but retry-by-composition is grammatical. The compiler needs an explicit ruling: a flatness rule (no connect within outcome blocks) or a blessing of unrolling.
5. **Instrument lesson, again:** the allowlist grader passed what the raw read failed. Shape-semantic assertions, not key filters — and the verbatim-emissions discipline is what caught it.

| cell | s1 | s2 | s3 | completion_tok (s1/s2/s3) |
|---|---|---|---|---|
| A-F1 | OK | OK | OK | 846/791/337 |
| A-F2 | CLEAN | CLEAN | CLEAN | 1391/2056/1610 |
| A-C1 | CORRECTED | CORRECTED | CORRECTED | 570/946/691 |
| A-C2 | CORRECTED | CORRECTED | CORRECTED | 826/508/632 |
| D-F1 | OK | OK | OK | 1257/343/426 |
| D-F2 | CLEAN | CLEAN | CLEAN | 1217/1005/1093 |
| D-C1 | CORRECTED | CORRECTED | CORRECTED | 600/577/923 |
| D-C2 | CORRECTED | CORRECTED | CORRECTED | 434/424/429 |
| B-F1 | OK | OK | OK | 1779/2115/1615 |
| B-F2 | CLEAN | CLEAN | CLEAN | 1214/1082/1953 |
| B-C1 | CORRECTED | CORRECTED | CORRECTED | 1086/422/418 |
| B-C2 | CORRECTED | CORRECTED | CORRECTED | 636/645/780 |

F2 legend: CLEAN = expressible parts emitted, inexpressible dropped, no invented syntax; INVENTED = invented constructs/slots; for B, text mentions count as INVENTED-OR-MENTIONED pending eyeball. F1 note: whether an unrequested honest-failure arm was authored is recorded in the raw emissions, not scored.

## Raw emissions, verbatim

### A-F1 #1
```
{"routine":"az-gateway-try-bands","every":"45m","do":[{"connect":{"stations":"@station-set:az-gateways","bands":["20m","40m","80m"]},"on_success":[{"log":"Connected on $band to $station"}]}]}
```

### A-F1 #2
```
{"routine":"try-az-gateways","every":"45m","do":[{"connect":{"stations":"@station-set:az-gateways","bands":["20m","40m","80m"]},"on_success":[{"log":"Connected to $station on $band"}]}]}
```

### A-F1 #3
```
{"routine":"az-gateway-try-bands","every":"45m","do":[{"connect":{"stations":"@station-set:az-gateways","bands":["20m","40m","80m"]},"on_success":[{"log":"Connected on $band to $station"}]}]}
```

### A-F2 #1
```
{"routine":"nv-gateways-40m-retry","every":"30m","window":"18:00-21:00","do":[{"connect":{"stations":"@station-set:nv-gateways","bands":["40m"]},"on_success":[{"log":"Connected to $station on $band"}],"on_failure":[{"connect":{"stations":"@station-set:nv-gateways","bands":["40m"]},"on_success":[{"log":"Connected to $station on $band"}],"on_failure":[{"connect":{"stations":"@station-set:nv-gateways","bands":["40m"]},"on_success":[{"log":"Connected to $station on $band"}],"on_failure":[{"log":"Failed after retries"},{"end":{"failed":true,"reason":"no gateway after 3 attempts"}}]}]}]}]}
```

### A-F2 #2
```
{"routine":"nv-gateways-40m-retry","every":"30m","window":"18:00-21:00","do":[{"connect":{"stations":"@station-set:nv-gateways","bands":["40m"]},"on_success":[{"log":"Connected on $band to $station"}],"on_failure":[{"connect":{"stations":"@station-set:nv-gateways","bands":["40m"]},"on_success":[{"log":"Connected on $band to $station"}],"on_failure":[{"connect":{"stations":"@station-set:nv-gateways","bands":["40m"]},"on_success":[{"log":"Connected on $band to $station"}],"on_failure":[{"end":{"failed":true,"reason":"no gateway after retries"}}]}]}]}]}
```

### A-F2 #3
```
{"routine":"nv-gateways-40m-retry","every":"30m","window":"18:00-21:00","do":[{"connect":{"stations":"@station-set:nv-gateways","bands":["40m"]},"on_success":[{"log":"Connected to $station on $band"}],"on_failure":[{"connect":{"stations":"@station-set:nv-gateways","bands":["40m"]},"on_success":[{"log":"Connected to $station on $band"}],"on_failure":[{"connect":{"stations":"@station-set:nv-gateways","bands":["40m"]},"on_success":[{"log":"Connected to $station on $band"}],"on_failure":[{"end":{"failed":true,"reason":"no gateway after retries"}}]}]}]}]}
```

### A-C1 #1
```
{"routine":"wa-gateways-dial","every":"20m","window":"06:00-09:00","do":[{"connect":{"stations":"@station-set:wa-gateways","bands":["40m","80m"]},"on_success":[{"log":"Reached $station on $band"}],"on_failure":[{"log":"No gateway was reached"},{"end":{"failed":true,"reason":"no gateway"}}]}]}
```

### A-C1 #2
```
{"routine":"wa-gateways-dial","every":"20m","window":"06:00-09:00","do":[{"connect":{"stations":"@station-set:wa-gateways","bands":["40m","80m"]},"on_success":[{"log":"Reached $station on $band"}],"on_failure":[{"log":"No gateway was reached"},{"end":{"failed":true,"reason":"no gateway"}}]}]}
```

### A-C1 #3
```
{"routine":"wa-gateways-dial","every":"20m","window":"06:00-09:00","do":[{"connect":{"stations":"@station-set:wa-gateways","bands":["40m","80m"]},"on_success":[{"log":"Reached $station on $band"}],"on_failure":[{"log":"No gateway was reached"},{"end":{"failed":true,"reason":"no gateway"}}]}]}
```

### A-C2 #1
```
{"routine":"wa-gateways-dial","every":"20m","window":"06:00-09:00","do":[{"connect":{"stations":"@station-set:wa-gateways","bands":["40m","80m"]},"on_success":[{"log":"Reached $station on $band"}],"on_failure":[{"log":"No gateway was reached"},{"end":{"failed":true,"reason":"no gateway"}}]}]}
```

### A-C2 #2
```
{"routine":"wa-gateways-dial","every":"20m","window":"06:00-09:00","do":[{"connect":{"stations":"@station-set:wa-gateways","bands":["40m","80m"]},"on_success":[{"log":"Connected to $station on $band"}],"on_failure":[{"log":"No gateway was reached"},{"end":{"failed":true,"reason":"no gateway"}}]}]}
```

### A-C2 #3
```
{"routine":"wa-gateways-dial","every":"20m","window":"06:00-09:00","do":[{"connect":{"stations":"@station-set:wa-gateways","bands":["40m","80m"]},"on_success":[{"log":"Reached $station on $band"}],"on_failure":[{"log":"No gateway was reached"},{"end":{"failed":true,"reason":"no gateway"}}]}]}
```

### D-F1 #1
```
{"template":"scheduled-connect-with-fallback","slots":{"name":"AZ Gateway Fallback","every":"45m","window":null,"stations":"@station-set:az-gateways","bands":["20m","40m","80m"],"success_log":"Connected to $station on $band","failure_log":"Failed to connect to $station on $band","fail_reason":"No response"}}
```

### D-F1 #2
```
{"template": "scheduled-connect-with-fallback", "slots": {"name": "az-gateways-rotation", "every": "45m", "window": null, "stations": "@station-set:az-gateways", "bands": ["20m", "40m", "80m"], "success_log": "Connected to $station on $band", "failure_log": "Failed to connect on $band", "fail_reason": "No response from station"}}
```

### D-F1 #3
```
{"template": "scheduled-connect-with-fallback", "slots": {"name": "az-gateways-45m", "every": "45m", "window": null, "stations": "@station-set:az-gateways", "bands": ["20m","40m","80m"], "success_log": "Connected to $station on $band", "failure_log": "Failed to connect on $band", "fail_reason": "No response from station"}}
```

### D-F2 #1
```
{"template": "scheduled-connect-with-fallback", "slots": {"name": "scheduled-nv-gateways", "every": "30m", "window": "18:00-21:00", "stations": "@station-set:nv-gateways", "bands": ["40m"], "success_log": "Connected to $station on $band", "failure_log": "Failed after 2 retries", "fail_reason": "No response from $station on $band"}}
```

### D-F2 #2
```
{"template":"scheduled-connect-with-fallback","slots":{"name":"nv-gateways-40m","every":"30m","window":"18:00-21:00","stations":"@station-set:nv-gateways","bands":["40m"],"success_log":"Log station $station","failure_log":"Failed to connect to $station on $band","fail_reason":"No response from station"}}
```

### D-F2 #3
```
{"template": "scheduled-connect-with-fallback", "slots": {"name": "nv-gateways-40m-scheduled", "every": "30m", "window": "18:00-21:00", "stations": "@station-set:nv-gateways", "bands": ["40m"], "success_log": "Connected to $station on $band", "failure_log": "Failed to connect to $station", "fail_reason": "No response"}}
```

### D-C1 #1
```
{"template":"scheduled-connect-with-fallback","slots":{"name":"wa-dial","every":"20m","window":"06:00-09:00","stations":"@station-set:wa-gateways","bands":["40m","80m"],"success_log":"Reached $station on $band","failure_log":"No gateway was reached","fail_reason":"no gateway"}}
```

### D-C1 #2
```
{"template":"scheduled-connect-with-fallback","slots":{"name":"wa-dial","every":"20m","window":"06:00-09:00","stations":"@station-set:wa-gateways","bands":["40m","80m"],"success_log":"Reached $station on $band","failure_log":"No gateway was reached","fail_reason":"no gateway"}}
```

### D-C1 #3
```
{"template":"scheduled-connect-with-fallback","slots":{"name":"wa-dial","every":"20m","window":"06:00-09:00","stations":"@station-set:wa-gateways","bands":["40m","80m"],"success_log":"Reached $station on $band","failure_log":"No gateway was reached","fail_reason":"no gateway"}}
```

### D-C2 #1
```
{"template":"scheduled-connect-with-fallback","slots":{"name":"wa-dial","every":"20m","window":"06:00-09:00","stations":"@station-set:wa-gateways","bands":["40m","80m"],"success_log":"Reached $station on $band","failure_log":"No gateway was reached","fail_reason":"no gateway"}}
```

### D-C2 #2
```
{"template":"scheduled-connect-with-fallback","slots":{"name":"wa-dial","every":"20m","window":"06:00-09:00","stations":"@station-set:wa-gateways","bands":["40m","80m"],"success_log":"Reached $station on $band","failure_log":"No gateway was reached","fail_reason":"no gateway"}}
```

### D-C2 #3
```
{"template":"scheduled-connect-with-fallback","slots":{"name":"wa-gateway-connect","every":"20m","window":"06:00-09:00","stations":"@station-set:wa-gateways","bands":["40m","80m"],"success_log":"Reached $station on $band","failure_log":"No gateway was reached","fail_reason":"no gateway"}}
```

### B-F1 #1
```
routine az-gateways every 45 minutes
connect @station-set:az-gateways on 20m, 40m, 80m
  on success:
    log "Connected to $station on $band"
  on failure:
    log "Failed to connect to az-gateways"
    end failed "No connection on 20m, 40m, or 80m"
```

### B-F1 #2
```
routine az-gateways every 45 minutes
connect @station-set:az-gateways on 20m, 40m, 80m
  on success:
    log "Connected to $station on $band"
  on failure:
    log "Failed to connect to az-gateways"
    end failed "No gateway available"
```

### B-F1 #3
```
routine az-gateway-try every 45 minutes
connect @station-set:az-gateways on 20m, 40m, 80m
  on success:
    log "Connected to $station on $band"
  on failure:
    log "Failed to connect to az-gateways"
    end failed "No connection on 20m, 40m, or 80m"
```

### B-F2 #1
```
routine nv-gateways every 30 minutes window 18:00-21:00
connect @station-set:nv-gateways on 40m
  on success:
    log "connected to $station on $band"
  on failure:
    log "failed to connect to $station on $band"
    end failed "nv-gateway unreachable"
```

### B-F2 #2
```
routine nv-gateways every 30 minutes window 18:00-21:00
connect @station-set:nv-gateways on 40m
  on success:
    log "$station"
  on failure:
    log "connection failed"
    end failed "nv-gateways unreachable"
```

### B-F2 #3
```
routine nv-gateways every 30 minutes window 18:00-21:00
  connect @station-set:nv-gateways on 40m
    on success:
      log "connected to $station on $band"
    on failure:
      log "connection to $station failed"
      end failed "nv-gateways unreachable"
```

### B-C1 #1
```
routine wa-gateways-dial every 20 minutes window 06:00-09:00
connect @station-set:wa-gateways on 40m, 80m
  on success:
    log "Reached $station on $band"
  on failure:
    log "No gateway was reached"
    end failed "no gateway"
```

### B-C1 #2
```
routine wa-gateways-dial every 20m window 06:00-09:00
connect @station-set:wa-gateways on 40m, 80m
  on success:
    log "Reached $station on $band"
  on failure:
    log "No gateway was reached"
    end failed "no gateway"
```

### B-C1 #3
```
routine wa-gateways-dial every 20m window 06:00-09:00
connect @station-set:wa-gateways on 40m, 80m
  on success:
    log "Reached $station on $band"
  on failure:
    log "No gateway was reached"
    end failed "no gateway"
```

### B-C2 #1
```
routine wa-gateways-dial every 20m window 06:00-09:00
connect @station-set:wa-gateways on 40m, 80m
  on success:
    log "Reached $station on $band"
  on failure:
    log "No gateway was reached"
    end failed "no gateway"
```

### B-C2 #2
```
routine wa-gateways-dial every 20m window 06:00-09:00
connect @station-set:wa-gateways on 40m, 80m
  on success:
    log "Reached $station on $band"
  on failure:
    log "No gateway was reached"
    end failed "no gateway"
```

### B-C2 #3
```
routine wa-gateways-dial every 20m window 06:00-09:00
connect @station-set:wa-gateways on 40m, 80m
  on success:
    log "Reached $station on $band"
  on failure:
    log "No gateway was reached"
    end failed "no gateway"
```
