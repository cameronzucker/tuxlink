# ch3e9 tools-corpus threshold recalibration — post-row-8 corpus

The recalibration the surface-repair campaign's row 8 left owed (ledger
row-8 Closed entry; ADR 0030's threshold-rot watched failure mode): the
`tuxlink-tools` entry in `resources/catalog/classify-thresholds.json` was
measured against the 2026-08-10 corpus, and the corpus has since changed —
the row-8 `find_stations` description rewrite plus corpus growth from 92
to **95 tools** (the classify-weights hosting tools and the off-air WWV
tools landed in between). This run re-measures both numbers against the
corpus as shipped on current main, before any classifier wiring can mint
verdicts from them.

## Provenance

- Host: R2 (i3-N305), the acceptance platform; `RAYON_NUM_THREADS=4`
- Code: detached worktree `~/tuxlink-recal-wt` @ `b342bb98` (current
  main, the compaction-anchor merge), reusing the
  `~/tuxlink-ch3e9-build` cargo target as `CARGO_TARGET_DIR`
- Command: `cargo run --release --example eval_tools -p tuxlink-classify`
  with `TUXLINK_BGE_DIR=~/classify-models/bge-small-en-v1.5` (the same
  bge-small-en-v1.5 snapshot as every prior run)
- Queries: the calibration set, 55 labeled asks at
  `dev/spikes/2026-08-10-tool-surface-embedding/queries.jsonl` (default
  path, no `TUXLINK_QUERIES` override)
- Raw log: `~/tuxlink-recal-eval.log` on R2; per-query table reproduced
  below

## Results

**Shortlist chart (47 tool-labeled queries): identical to the 2026-08-10
curated run** — hit@5 85.1%, hit@8 89.4%, **hit@12 93.6%**, hit@16 95.7%.
Selection accuracy is corpus-growth-stable. The two @16 misses are the
same two known vocabulary-gap residuals (`routine-build` never finds
`routines_save`; `docs-lookup` never finds `docs_search`) — the class the
operator ruled the recovery paths carry, not synonym curation.

**The shipped reject floor was stale, and the recalibration caught a real
flip.** `none-license` ("what are the requirements for a general class
license" — correctly a no-tool knowledge ask) now top-1s the NEW
`classify_weights_status` tool at **0.5860**, above the old floor of
0.582: under the stale calibration this none-class ask would have minted
a Match-side verdict. The token overlap doing it is almost self-referential
— "general **class** license" against "**classif**ier … integrity tier …
release-pinned" — the classifier's own status tool is the absorber.

**Reject gap: SEPARATED, but compressed 0.028 → 0.0013.** none-class max
0.5860 (`none-license`) vs true-class min 0.5873 (`read-msg`). Both new
numbers moved toward each other: the new hosting tool raised the
none-class ceiling, and `read-msg`'s top-1 score sits lower against the
grown corpus. The measured midpoint ships as the new floor.

**Margins: unchanged.** ask max 0.0133 (`ambig-stop`) vs answer min
0.0025 (`read-msg`); midpoint 0.008, same as shipped.

## Shipped thresholds (this run's measured midpoints)

```json
{"tuxlink-tools/bge-small-en-v1.5/enriched-v1":{"reject_floor":0.587,"ask_margin":0.008}}
```

`winlink-catalog` untouched (its corpus did not change; its calibration
stands per `dev/evals/2026-08-10-ch3e9-t1-floor-calibration.md`).

## Fragility note (carry into wiring, do not chase now)

A 0.0013 reject gap means ANY future corpus edit can flip it to OVERLAP —
one more tool whose description brushes against a no-tool phrasing, and
the floor stops separating. Two implications for the wiring layer:

1. The recalibration-tooth test (`thresholds.rs`) enforces entry
   *presence* per (corpus, model, template); it cannot enforce entry
   *freshness* against corpus content. The regen gate that already forces
   `TUXLINK_REGEN_TOOL_SURFACE=1` on description edits is the moment to
   re-run this eval — treat "corpus regenerated" and "thresholds
   re-measured" as one obligation, which is exactly how ADR 0030 framed
   threshold rot.
2. This is corpus-shape pressure, not threshold-tuning pressure. If the
   gap collapses at a future regen, the answer is looking at which entry
   absorbed a none-class ask (here: `classify_weights_status`), not
   inventing a floor between overlapping classes. The NoMatch verdict is
   advisory either way — the model can always reach the full toolset by
   name (the operator's reachability requirement), so a collapsed gap
   degrades to noise, never to a wall.

## Per-query table (id / rank / top1 / score / margin / top-5)

```text
armed-check	1	server_info	0.6824	0.0331	server_info,config_set_privacy,packet_config_set,vara_status,rig_status
backend-up	1	backend_status	0.7600	0.0795	backend_status,cms_connect,vara_engine_available,find_stations,catalog_list
radio-freq	10	ft8_heard_stations	0.6917	0.0120	ft8_heard_stations,rig_tune,ft8_status,ardop_connect,vara_b2f_exchange
inbox-list	1	mailbox_list	0.6353	0.0296	mailbox_list,user_folders_list,catalog_send_inquiry,send_form,server_info
read-msg	5	catalog_send_inquiry	0.5873	0.0025	catalog_send_inquiry,message_send,cms_connect,tauri_search_run,message_read
save-attachment	1	message_attachment_save	0.7620	0.1456	message_attachment_save,message_read,message_send,mailbox_move,export_report
move-to-folder	6	mailbox_list	0.6669	0.0039	mailbox_list,user_folders_list,message_attachment_save,tauri_search_run,export_report
list-folders	2	mailbox_list	0.7842	0.0357	mailbox_list,user_folders_list,mailbox_move,catalog_send_inquiry,tauri_search_run
compose-send	15	cms_connect	0.6704	0.0418	cms_connect,catalog_send_inquiry,backend_status,ardop_connect,find_stations
send-ics213	1	send_form	0.6390	0.0345	send_form,catalog_send_inquiry,config_set_grid,ardop_b2f_exchange,catalog_list
catalog-browse	1	catalog_list	0.8114	0.0962	catalog_list,catalog_send_inquiry,grib_send_request,backend_status,wwv_offair_available
catalog-request	3	grib_send_request	0.7063	0.0512	grib_send_request,catalog_list,catalog_send_inquiry,solar_conditions,wwv_capture_offair
grib-request	1	grib_send_request	0.6764	0.0615	grib_send_request,catalog_send_inquiry,catalog_list,ardop_connect,position_status
cms-dial	1	cms_connect	0.7301	0.1083	cms_connect,catalog_send_inquiry,ardop_connect,backend_status,find_stations
cms-stop	1	cms_abort	0.8334	0.1461	cms_abort,modem_ardop_disconnect,vara_stop_session,verify_cms_connection,cms_connect
gateway-hunt	1	find_stations	0.7405	0.0607	find_stations,backend_status,cms_connect,catalog_list,find_peers
peer-roster	1	find_peers	0.7059	0.0664	find_peers,find_stations,p2p_peer_password_status,catalog_send_inquiry,vara_status
prop-check	9	wwv_capture_offair	0.5977	0.0160	wwv_capture_offair,grib_send_request,position_status,vara_status,solar_conditions
solar-now	2	ft8_status	0.6324	0.0037	ft8_status,solar_conditions,vara_status,ft8_set_band,ft8_heard_stations
ardop-call	2	modem_ardop_disconnect	0.7222	0.0170	modem_ardop_disconnect,ardop_connect,ardop_b2f_exchange,config_get_ardop,config_set_ardop
ardop-mail	1	ardop_b2f_exchange	0.7135	0.0677	ardop_b2f_exchange,cms_connect,ardop_connect,config_set_ardop,modem_ardop_disconnect
ardop-hangup	1	modem_ardop_disconnect	0.7404	0.0978	modem_ardop_disconnect,config_set_ardop,ardop_connect,ardop_b2f_exchange,ardop_list_audio_devices
vara-mail	1	vara_b2f_exchange	0.7194	0.0446	vara_b2f_exchange,vara_open_session,vara_install_start,vara_ini_apply,ardop_b2f_exchange
vara-installed	3	vara_install_start	0.7199	0.0059	vara_install_start,vara_install_status,vara_engine_available,vara_ini_read,config_set_vara
vara-install	1	vara_install_start	0.7395	0.0424	vara_install_start,vara_ini_read,vara_install_status,vara_engine_available,vara_open_session
vara-stop	1	vara_stop_session	0.7671	0.0765	vara_stop_session,vara_ini_apply,vara_open_session,config_set_vara,vara_ini_read
vara-drive	1	config_set_vara	0.7655	0.0942	config_set_vara,vara_ini_read,vara_ini_apply,config_get_vara,vara_probe
packet-bt	1	packet_list_bluetooth_devices	0.7153	0.0267	packet_list_bluetooth_devices,list_audio_devices,packet_list_serial_devices,ft8_list_audio_devices,ardop_list_audio_devices
packet-dial	1	packet_connect	0.7113	0.0603	packet_connect,packet_config_get,cms_connect,packet_config_set,ardop_connect
packet-listen	2	ft8_start_listening	0.7012	0.0362	ft8_start_listening,packet_set_listen,ft8_status,packet_config_get,backend_status
ft8-monitor	1	ft8_start_listening	0.7752	0.0034	ft8_start_listening,ft8_status,ft8_heard_stations,ft8_set_band,ft8_stop_listening
ft8-heard	1	ft8_heard_stations	0.7236	0.0736	ft8_heard_stations,ft8_status,ft8_start_listening,ft8_stop_listening,ft8_list_audio_devices
ft8-band	1	ft8_set_band	0.7282	0.0611	ft8_set_band,ft8_start_listening,ft8_stop_listening,config_set_vara,rig_tune
grid-set	1	config_set_grid	0.6666	0.0724	config_set_grid,grib_send_request,config_set_vara,routines_meta_set,position_set_source
gps-source	2	position_status	0.7206	0.0183	position_status,position_set_source,config_set_grid,config_set_privacy,grib_send_request
where-am-i	1	position_status	0.7076	0.0886	position_status,point_at,server_info,position_set_source,routines_step_move
routine-list	6	routines_actions_list	0.7324	0.0132	routines_actions_list,routines_trigger_set,routines_enable,routines_dry_run,routines_step_move
routine-run	3	routines_dry_run	0.6163	0.0174	routines_dry_run,routines_run_status,routines_run,routines_actions_list,routines_step_add
routine-history	1	routines_journal_get	0.6738	0.0372	routines_journal_get,routines_dry_run,routines_run,routines_run_status,routines_trigger_set
routine-build	-	solar_conditions	0.6116	0.0207	solar_conditions,catalog_list,routines_actions_list,routines_dry_run,routines_trigger_set
print-msg	1	print_document	0.6413	0.0151	print_document,message_read,printer_list,message_send,server_info
export-log	1	export_report	0.7401	0.1065	export_report,print_document,routines_journal_get,session_log_snapshot,vara_ini_apply
show-ui	1	point_at	0.6684	0.0328	point_at,send_form,message_send,routines_step_add,catalog_send_inquiry
session-log	1	session_log_snapshot	0.7573	0.0989	session_log_snapshot,routines_journal_get,vara_status,backend_status,modem_get_status
docs-lookup	-	find_stations	0.6133	0.0548	find_stations,vara_install_start,classify_weights_download,config_set_grid,vara_open_session
tune-radio	1	rig_tune	0.6813	0.0319	rig_tune,ft8_heard_stations,ft8_status,ft8_start_listening,ft8_set_band
wwv-check	1	wwv_capture_offair	0.8279	0.1207	wwv_capture_offair,wwv_offair_available,solar_conditions,catalog_list,grib_send_request
ambig-status	-	ft8_status	0.6885	0.0085	ft8_status,backend_status,vara_status,position_status,modem_get_status
ambig-connect	-	packet_connect	0.6224	0.0060	packet_connect,ardop_connect,cms_connect,vara_open_session,find_stations
ambig-config	-	routines_meta_set	0.6041	0.0029	routines_meta_set,config_set_privacy,config_set_vara,config_set_grid,routines_trigger_set
ambig-stop	-	cms_abort	0.6562	0.0133	cms_abort,modem_ardop_disconnect,vara_stop_session,ft8_stop_listening,routines_track_remove
none-joke	-	config_get_rig	0.5530	0.0229	config_get_rig,routines_journal_get,routines_dry_run,packet_list_bluetooth_devices,catalog_send_inquiry
none-license	-	classify_weights_status	0.5860	0.0394	classify_weights_status,routines_run,routines_meta_set,config_set_ardop,grib_send_request
none-antenna	-	grib_send_request	0.5697	0.0107	grib_send_request,wwv_capture_offair,predict_path,ft8_set_band,config_set_vara
none-dinner	-	printer_list	0.5141	0.0150	printer_list,routines_trigger_set,catalog_send_inquiry,routines_step_add,grib_send_request
```

Session: spruce-birch-dune, 2026-08-16 late evening AZT.
