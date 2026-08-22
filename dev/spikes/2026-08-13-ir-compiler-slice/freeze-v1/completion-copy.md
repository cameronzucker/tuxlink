# Frozen completion-copy table (v1)

One row per reachable state. The `completion` field carries EXACTLY this
copy (ASCII only, per the standing 2026-07-29 operator ruling recorded at
the advisory-completion site in ports.rs). `{name}`/`{revision}` are the
only substitution holes. NO row ever directs the model to native edit or
rename tools; blocked states carry a positive stop-and-report sentence
(design §1).

| # | State | agent_terminal | Exact copy | Permitted next action | Prohibited claims |
|---|---|---|---|---|---|
| 1 | compiled-valid | false | `Compiled and validated. NOT saved - nothing will run. Report the readback to the user. To save this exact draft, call routine_template_compile again with the same arguments plus save:true. Do not use any other routine tool for this.` | one save:true recall with identical template+slots, or report and stop | saved; enabled; scheduled; running; "ready to go" |
| 2 | compiled-advisory | false | `Compiled and validated with advisories. NOT saved - nothing will run. Each advisory names its slot; you may fix that slot and compile again once, or report the draft as-is. Do not use any other routine tool.` | one changed recompile, save:true recall, or report | saved; treating advisories as blockers; silently dropping advisories from the report |
| 3 | compiled-blocked | true | `Compiled, but validation BLOCKED this routine. NOT saved - it cannot run as-is. The findings name what blocks it and who can fix it. Report this to the user and stop. Do not edit or retry with other tools; do not repeat the identical call.` | report and stop (one CHANGED recompile only if a finding carries an agent remedy) | saved; ready; any success-first framing that buries the block |
| 4 | saved-valid | true | `Saved '{name}' at revision {revision}. The routine is COMPLETE and valid. It is saved but not enabled; enabling happens in the app. Report this to the user and stop.` | report and stop | enabled; running; scheduled to fire; anything about execution starting |
| 5 | saved-blocked | per disposition | `Saved '{name}' at revision {revision}, but validation blocks it from running. The disposition names what blocks it and who can fix it. Report honestly: saved but NOT runnable. Then stop.` | per the disposition's remedies (agent remedy: apply once; operator remedy: report and stop) | runnable; complete; omitting the block from the report |
| 6 | save_refused (NAME_EXISTS_CREATE_ONLY) | true | `Nothing was saved and the existing routine was not touched - no bytes or revision changed. A routine named '{name}' already exists. Ask the user what different name to use. Do not retry with a guessed variation of the name. Do not rename or edit the existing routine with any tool.` | ask the user; stop | saved; "renamed"; retrying with -2/-new/etc.; touching the existing routine |
| 7 | refused | false | `Not compiled. Nothing was created or saved. Each finding names the slot and the rule. Fix exactly the named slots and call routine_template_compile again once.` | one CHANGED recall fixing the named slots | partial success; "created but..."; repeating the identical call |
| 8 | envelope error | false | `[<CODE>] <detail naming the key or path>. The call was not processed; nothing changed. Resend one corrected call.` | one corrected resend | any claim the call did anything |
| 9 | STORE_IO_ERROR | true | `Saving failed because of a storage error on this machine (not your call's fault). Nothing was saved. Report this to the user and stop; a later retry may work.` | report and stop | saved; any self-blame retry loop |
| 10 | AUTHORING_LOCK_UNAVAILABLE | true | `Another authoring operation holds the lock (not your call's fault). Nothing was saved. Report this to the user and stop; a later retry may work.` | report and stop | saved; hammering retries |

Instrument rule (design §1): row 2 is reachable ONLY via advisories a named
slot repairs. A structural advisory against deterministic compiler output
is INSTRUMENT_INVALID - the compiler authored the structure, so an advisory
against it is an instrument bug: quarantine the run, fix, rerun.

Row 5's `agent_terminal` follows the verbatim production disposition
(`invalid-agent-repairable` -> false with remedies; `saved-needs-operator`
-> true); the copy row stays constant and the disposition carries the
authority (no contradiction: the copy names the disposition as the source).
