# IR-surface probe v2 — detailed results (2026-08-18T08:55Z)

Endpoint: https://inference.twin-bramble.ts.net/v1/chat/completions · model thinkingmachines/Inkling-Small-NVFP4 · temp 0.2 · max_tokens 4096 · 3 samples/wing

| sample | finish | prompt_tok | completion_tok | reasoning_len | content_len | verdict |
|---|---|---|---|---|---|---|
| A#1 | stop | 325 | 696 | 2282 | 293 | PARSE+NEST-OK |
| A#2 | stop | 325 | 673 | 2284 | 293 | PARSE+NEST-OK |
| A#3 | stop | 325 | 826 | 2804 | 289 | PARSE+NEST-OK |
| D#1 | stop | 213 | 481 | 1549 | 289 | PARSE-OK |
| D#2 | stop | 213 | 453 | 1409 | 291 | PARSE-OK |
| D#3 | stop | 213 | 505 | 1608 | 289 | PARSE-OK |
| B#1 | stop | 185 | 812 | 2711 | 217 | grammar-lines=5/5 |
| B#2 | stop | 185 | 1236 | 4410 | 227 | grammar-lines=5/5 |
| B#3 | stop | 185 | 1131 | 4032 | 227 | grammar-lines=5/5 |
| AT#1 | stop | 397 | 289 | 689 | 0 | ARGS-OK |
| AT#2 | stop | 397 | 296 | 705 | 0 | ARGS-OK |
| AT#3 | stop | 397 | 302 | 743 | 0 | ARGS-OK |
| DT#1 | stop | 281 | 291 | 751 | 0 | ARGS-OK |
| DT#2 | stop | 281 | 234 | 542 | 0 | ARGS-OK |
| DT#3 | stop | 281 | 271 | 711 | 0 | ARGS-OK |

## Raw emissions (content or tool args, verbatim)

### A#1
```
{"routine":"wa-gateways-dial","every":"20m","window":"06:00-09:00","do":[{"connect":{"stations":"@station-set:wa-gateways","bands":["40m","80m"]},"on_success":[{"log":"Reached $station on $band"}],"on_failure":[{"log":"No gateway was reached"},{"end":{"failed":true,"reason":"no gateway"}}]}]}
```

### A#2
```
{"routine":"wa-gateways-dial","every":"20m","window":"06:00-09:00","do":[{"connect":{"stations":"@station-set:wa-gateways","bands":["40m","80m"]},"on_success":[{"log":"Reached $station on $band"}],"on_failure":[{"log":"No gateway was reached"},{"end":{"failed":true,"reason":"no gateway"}}]}]}
```

### A#3
```
{"routine":"wa-gateways-dial","every":"20m","window":"06:00-09:00","do":[{"connect":{"stations":"@station-set:wa-gateways","bands":["40m","80m"]},"on_success":[{"log":"Reached $station on $band"}],"on_failure":[{"log":"No gateway reached"},{"end":{"failed":true,"reason":"no gateway"}}]}]}
```

### D#1
```
{"template":"scheduled-connect-with-fallback","slots":{"name":"wa-gateway-routine","every":"20m","window":"06:00-09:00","stations":"@station-set:wa-gateways","bands":["40m","80m"],"success_log":"Reached $station on $band","failure_log":"No gateway was reached","fail_reason":"no gateway"}}
```

### D#2
```
{"template":"scheduled-connect-with-fallback","slots":{"name":"wa-gateway-scheduled","every":"20m","window":"06:00-09:00","stations":"@station-set:wa-gateways","bands":["40m","80m"],"success_log":"Reached $station on $band","failure_log":"No gateway was reached","fail_reason":"no gateway"}}
```

### D#3
```
{"template":"scheduled-connect-with-fallback","slots":{"name":"wa-gateway-connect","every":"20m","window":"06:00-09:00","stations":"@station-set:wa-gateways","bands":["40m","80m"],"success_log":"Reached $station on $band","failure_log":"No gateway was reached","fail_reason":"no gateway"}}
```

### B#1
```
routine wa-gateway every 20m window 06:00-09:00
connect @station-set:wa-gateways on 40m, 80m
  on success:
    log "Reached $station on $band"
  on failure:
    log "No gateway was reached"
    end failed "no gateway"
```

### B#2
```
routine gateway-check every 20 minutes window 06:00-09:00
connect @station-set:wa-gateways on 40m, 80m
  on success:
    log "Reached $station on $band"
  on failure:
    log "No gateway was reached"
    end failed "no gateway"
```

### B#3
```
routine gateway-check every 20 minutes window 06:00-09:00
connect @station-set:wa-gateways on 40m, 80m
  on success:
    log "Reached $station on $band"
  on failure:
    log "No gateway was reached"
    end failed "no gateway"
```

### AT#1
```
{"ir":{"routine":"wa-gateway-connect","every":"20m","window":"06:00-09:00","do":[{"connect":{"stations":"@station-set:wa-gateways","bands":["40m","80m"]},"on_success":[{"log":"Connected on $band to $station"}],"on_failure":[{"log":"No gateway was reached"},{"end":{"failed":true,"reason":"no gateway"}}]}]}}
```

### AT#2
```
{"ir":{"routine":"wa-gateways-dial","every":"20m","window":"06:00-09:00","do":[{"connect":{"stations":"@station-set:wa-gateways","bands":["40m","80m"]},"on_success":[{"log":"Connected on $band to $station"}],"on_failure":[{"log":"No gateway was reached"},{"end":{"failed":true,"reason":"no gateway"}}]}]}}
```

### AT#3
```
{"ir":{"routine":"wa-gateway-check","every":"20m","window":"06:00-09:00","do":[{"connect":{"stations":"@station-set:wa-gateways","bands":["40m","80m"]},"on_success":[{"log":"Connected on $band to $station"}],"on_failure":[{"log":"No gateway was reached"},{"end":{"failed":true,"reason":"no gateway"}}]}]}}
```

### DT#1
```
{"slots":{"bands":["40m","80m"],"every":"20m","fail_reason":"no gateway","failure_log":"No gateway was reached","name":"wa-gateways-scheduled","stations":"@station-set:wa-gateways","success_log":"Reached $station on $band","window":"06:00-09:00"},"template":"scheduled-connect-with-fallback"}
```

### DT#2
```
{"slots":{"bands":["40m","80m"],"every":"20m","fail_reason":"no gateway","failure_log":"No gateway was reached","name":"WA Gateway Scheduled Connect","stations":"@station-set:wa-gateways","success_log":"Reached $station on $band","window":"06:00-09:00"},"template":"scheduled-connect-with-fallback"}
```

### DT#3
```
{"slots":{"bands":["40m","80m"],"every":"20m","fail_reason":"no gateway","failure_log":"No gateway was reached","name":"WA Gateway Scheduled Connect","stations":"@station-set:wa-gateways","success_log":"Reached $station on $band","window":"06:00-09:00"},"template":"scheduled-connect-with-fallback"}
```
