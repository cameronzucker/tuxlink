# Changelog

All notable changes to Tuxlink are documented here.

This project adheres to [Semantic Versioning](https://semver.org) with project-specific rules described in [VERSIONING.md](VERSIONING.md). Entries from `v0.0.2` onward are generated automatically by [`release-please`](https://github.com/googleapis/release-please) from [Conventional Commits](https://www.conventionalcommits.org).

## [0.8.0](https://github.com/cameronzucker/tuxlink/compare/v0.7.1...v0.8.0) (2026-05-31)


### Features

* **radio:** define RadioPanel types (radio-panel-shell P1.1) ([da21647](https://github.com/cameronzucker/tuxlink/commit/da216472156ababd7143fc49fc6f10babeaaac6d))
* **radio:** placeholder mode panel (radio-panel-shell P1.4) ([1448cfa](https://github.com/cameronzucker/tuxlink/commit/1448cfa8bc3b992e51d141799a2c180eee9a2044))
* **radio:** RadioPanel shell component (radio-panel-shell P1.3) ([6be7086](https://github.com/cameronzucker/tuxlink/commit/6be708682701c21b638526c4482de631241b330b))
* **radio:** visibility hook computes panel mount + mode (radio-panel-shell P1.2) ([c4ec053](https://github.com/cameronzucker/tuxlink/commit/c4ec053da699f63387878b6ddd211c6e2d34552b))
* **shell:** mount RadioPanel placeholder via visibility hook (radio-panel-shell P1.5) ([fa34a40](https://github.com/cameronzucker/tuxlink/commit/fa34a400863552c7305d2a880b398f9c0475f350))


### Bug Fixes

* **radio:** two Codex P1 findings on radio-panel P1 chrome ([2da4adb](https://github.com/cameronzucker/tuxlink/commit/2da4adbd36675c959f1cafe2a8c4a59edf268b55))


### Refactors

* **shell:** remove bottom session-log strip (radio-panel-shell P1.6) ([6d8858d](https://github.com/cameronzucker/tuxlink/commit/6d8858d495b27be4c7a292a274eabf1a12fabade))
* **shell:** rename View → Toggle Radio Panel (radio-panel-shell P1.7) ([364d65f](https://github.com/cameronzucker/tuxlink/commit/364d65f0d02fcde9d6a8f08d5ffdca5d3b2e07ef))

## [0.7.1](https://github.com/cameronzucker/tuxlink/compare/v0.7.0...v0.7.1) (2026-05-31)


### Bug Fixes

* **shell:** ARDOP HF dock dead-end on cold start + wire View → Toggle Radio Dock (tuxlink-mnk4) ([aa8e6ad](https://github.com/cameronzucker/tuxlink/commit/aa8e6ad8746d01aa6f948fc20c7ec4d97657affa))

## [0.7.0](https://github.com/cameronzucker/tuxlink/compare/v0.6.0...v0.7.0) (2026-05-31)


### Features

* **modem:** ARDOP bandwidth selection (200/500/1000/2000 Hz) — ARQBW wired through Settings (tuxlink-j0ij) ([85a6d90](https://github.com/cameronzucker/tuxlink/commit/85a6d907ba4318d14a4b109072a86e8f8e919375))
* **modem:** ARDOP WebGUI link — spawn ardopcf with -G + dock link to Spectrum/Waterfall (tuxlink-60wh) ([11f444d](https://github.com/cameronzucker/tuxlink/commit/11f444d7b3892746cac28b4da8202981037d38c1))
* **search:** SavedSearchesPanel modal + AppShell Manage wiring (tuxlink-1hu) ([12c05f8](https://github.com/cameronzucker/tuxlink/commit/12c05f8c1a858b66ad95c3893cb6b2783b2f8337))


### Bug Fixes

* **modem:** ARDOP ABORT during in-flight connect via cmd-socket side channel (tuxlink-o3f2) ([22cfe80](https://github.com/cameronzucker/tuxlink/commit/22cfe80dba31f374c23a3f50a33704b0d7b77193))
* **search:** apply 3 of 5 Codex adrev findings (tuxlink-1hu) ([fff6001](https://github.com/cameronzucker/tuxlink/commit/fff6001de68f5d0557d9b213309c856dcae570ac))
* **search:** wire search results into MessageList (tuxlink-c7qz) ([f38a5fa](https://github.com/cameronzucker/tuxlink/commit/f38a5fae0f10a42afdd9205ada80cfed1e733e18))

## [0.6.0](https://github.com/cameronzucker/tuxlink/compare/v0.5.0...v0.6.0) (2026-05-31)


### Features

* **modem:** ArdopDock Send/Receive button triggers modem_ardop_b2f_exchange (tuxlink-ecth) ([fc95383](https://github.com/cameronzucker/tuxlink/commit/fc95383bbf7232d531799c634828b88d0de77aae))


### Bug Fixes

* **modem:** require live backend peer as Send/Receive target (tuxlink-ecth) ([0173985](https://github.com/cameronzucker/tuxlink/commit/01739856ff9789596efd12a1bc2a097be9678169))

## [0.5.0](https://github.com/cameronzucker/tuxlink/compare/v0.4.0...v0.5.0) (2026-05-31)


### Features

* **modem:** modem_ardop_connect pre-flight identity check (tuxlink-5738) ([b6da454](https://github.com/cameronzucker/tuxlink/commit/b6da4544135f40e88ceae7dbb8f2ea249ce4d31e))

## [0.4.0](https://github.com/cameronzucker/tuxlink/compare/v0.3.1...v0.4.0) (2026-05-30)


### Features

* **backend:** add OutboundAttachment + extend OutboundMessage (tuxlink-v1p) ([3b236af](https://github.com/cameronzucker/tuxlink/commit/3b236af753d8795ede19fde99c7374a14ea56a0e))
* **backend:** config_get_ardop / config_set_ardop Tauri commands (tuxlink-4ek) ([aa32b65](https://github.com/cameronzucker/tuxlink/commit/aa32b65d3cb45a7dc808a3bf35975e0e0418c2bf))
* **backend:** modem_ardop_connect with RADIO-1 token gate + ArdopTransport spawn (tuxlink-4ek) ([4533f5c](https://github.com/cameronzucker/tuxlink/commit/4533f5ce20f20a20ce0dd6c1dc754534715162ce))
* **backend:** modem_get_status + modem_ardop_disconnect + ModemSession Tauri state (tuxlink-4ek) ([c3fa8f7](https://github.com/cameronzucker/tuxlink/commit/c3fa8f729038229d476112e360a2ce28548171c9))
* **backend:** ModemStatusBroadcaster background thread + modem:status emit (tuxlink-4ek) ([0949253](https://github.com/cameronzucker/tuxlink/commit/0949253c334636ce1664eb4336ef25f029daa6a3))
* **config:** ArdopUiConfig struct + Config.modem_ardop field (tuxlink-4ek) ([b76ba51](https://github.com/cameronzucker/tuxlink/commit/b76ba5124a20e55830aeb3f21c14dd594ab3af4c))
* **connections:** add 'ardop-hf' protocol to sessionTypes catalog (tuxlink-4ek) ([928c1ae](https://github.com/cameronzucker/tuxlink/commit/928c1ae111a6fd7a8cb38f663724b17875e28d0e))
* **modem:** ArdopDock running state — ARQ grid + meters + mono status block (tuxlink-4ek) ([57550d3](https://github.com/cameronzucker/tuxlink/commit/57550d3b5c33b584dfb887d07dd15511083c3057))
* **modem:** ArdopDock stopped-state render (Connect form) (tuxlink-4ek) ([3e852c5](https://github.com/cameronzucker/tuxlink/commit/3e852c57502cd36ce0a1fc2cfcbb1a9e2a1b211d))
* **modem:** ModemSession shared state + RADIO-1 consent token mint/check (tuxlink-4ek) ([0d15bfc](https://github.com/cameronzucker/tuxlink/commit/0d15bfcb25d6c5ca9bea78a583f0f83317b9a7c5))
* **modem:** ModemStatus struct + ModemState enum + serde wire contract (tuxlink-4ek) ([7f96535](https://github.com/cameronzucker/tuxlink/commit/7f9653574e26a4220a509369d0b3a123a79b6f41))
* **modem:** RADIO-1 consent modal + backend-minted token wire (tuxlink-4ek) ([3145bd4](https://github.com/cameronzucker/tuxlink/commit/3145bd479f54e43e3fce62eedb33052190d9a8f0))
* **modem:** TS ModemStatus type mirroring the Rust wire shape (tuxlink-4ek) ([b7c42a6](https://github.com/cameronzucker/tuxlink/commit/b7c42a63a631c25e313e374c82907b155812cd08))
* **modem:** useConsent hook owning the in-session RADIO-1 token (tuxlink-4ek) ([8441aa8](https://github.com/cameronzucker/tuxlink/commit/8441aa8996c39787821d13d83079bc6212315175))
* **modem:** useModemStatus React hook subscribing to modem:status event (tuxlink-4ek) ([d62b3f6](https://github.com/cameronzucker/tuxlink/commit/d62b3f67b1462963c59c2f82b7b34de7c0df9294))
* **settings:** ARDOP HF section — binary/capture/playback/PTT/cmd-port (tuxlink-4ek) ([f639cf7](https://github.com/cameronzucker/tuxlink/commit/f639cf79171751c0d9373c3f6ab4b880dd6ef127))
* **shell:** conditional 4-col grid + ArdopDock mount + ARDOP HF reading-pane stub (tuxlink-4ek) ([680fe24](https://github.com/cameronzucker/tuxlink/commit/680fe24fd67f0b0c7f6b54b42594e8eb1f9114b7))


### Bug Fixes

* **modem:** close RADIO-1 consent-gate bypasses found by Codex adrev (tuxlink-4ek) ([42732dd](https://github.com/cameronzucker/tuxlink/commit/42732ddb112d198c534a9e6af341acf76f20567c))
* **modem:** lock STOPPED as Readonly&lt;ModemStatus&gt; + cover lastError null (tuxlink-4ek) ([3a79a49](https://github.com/cameronzucker/tuxlink/commit/3a79a49d72d5296875ade837d361259c7ee59cc8))
* **modem:** useModemStatus — plug listener-handle leak + surface fetch errors (tuxlink-4ek) ([53b9f9b](https://github.com/cameronzucker/tuxlink/commit/53b9f9b2b135b3351802ab9b2f6f48ff28b2867a))


### Refactors

* **modem:** rename init_config_from_session → init_config_from_persisted_config (tuxlink-4ek) ([92b735c](https://github.com/cameronzucker/tuxlink/commit/92b735c7adeac72e0abab8f4a55498e9ceae8235))

## [0.3.1](https://github.com/cameronzucker/tuxlink/compare/v0.3.0...v0.3.1) (2026-05-30)


### Bug Fixes

* **ui:** parse Winlink B2F Date header so CMS messages show real dates (tuxlink-p3u) ([cdf21e1](https://github.com/cameronzucker/tuxlink/commit/cdf21e1b25a958d3a796776a923fb2959d9796e1))

## [0.3.0](https://github.com/cameronzucker/tuxlink/compare/v0.2.0...v0.3.0) (2026-05-30)


### Features

* **ax25:** opt-in RFCOMM byte trace for on-air RX diagnosis (tuxlink-4ef) + note the abort-write race (tuxlink-0ja) ([685385d](https://github.com/cameronzucker/tuxlink/commit/685385d7981f966900c5b6ad030b3fe3f8eaf359))


### Bug Fixes

* **ax25:** RADIO-1 safety bundle — bounded connect airtime, abort-before-TX, no pre-connect DISC (tuxlink-2y4) ([7673cac](https://github.com/cameronzucker/tuxlink/commit/7673cacc8413a8c827c9d32a85989cbd9f7650a3))
* **backend:** refresh live config on config_set_* so UI selections apply restart-free (tuxlink-ka7, tuxlink-p5u) ([195b6c6](https://github.com/cameronzucker/tuxlink/commit/195b6c6a7d5a019df83432a5eb4ca0e99099c142))
* **config:** degrade unknown packet.link variant to None + add TUXLINK_CONFIG_DIR (tuxlink-efo) ([4b482af](https://github.com/cameronzucker/tuxlink/commit/4b482afd95c0baa294d6e5e4d76c4db6b5bd745b))
* **scripts:** new_tuxlink_worktree default base → main (tuxlink-1k7) ([a7522e0](https://github.com/cameronzucker/tuxlink/commit/a7522e0e24f9a451bcf2926b77d78deabeddc863))

## [0.2.0](https://github.com/cameronzucker/tuxlink/compare/v0.1.0...v0.2.0) (2026-05-30)


### Features

* **ax25:** clean serial/Bluetooth Stop for a packet listen (tuxlink-nj1) ([12486ff](https://github.com/cameronzucker/tuxlink/commit/12486ffd69fc3ecbfa567469fcf664d0971d6704))
* **connections:** session-type accordion selector + per-intent panes (tuxlink-3pb) ([e916709](https://github.com/cameronzucker/tuxlink/commit/e9167099d8df02e594fbe7326fed60ff4e5f6333))
* **connections:** session-type accordion sidebar + AppShell pane dispatch (tuxlink-3pb) ([31edf19](https://github.com/cameronzucker/tuxlink/commit/31edf193c46e40891657348e2d91d0aa26c5b729))
* **connections:** session-type/protocol catalog (tuxlink-3pb) ([5369344](https://github.com/cameronzucker/tuxlink/commit/536934482bdb85619011c07b841ced6da28bcbda))
* **connections:** stub pane for not-yet-built session types (tuxlink-3pb) ([175c63e](https://github.com/cameronzucker/tuxlink/commit/175c63e76b37d8d02641c0e53ef1ecc03376dc2e))
* **connections:** Telnet-CMS connection pane (relocated CMS controls) (tuxlink-3pb) ([033169a](https://github.com/cameronzucker/tuxlink/commit/033169abb59c48f70e3ffa41126ce1c3bc977365))
* **connect:** user-switchable CMS server host + transport (tuxlink-3o0) ([c430332](https://github.com/cameronzucker/tuxlink/commit/c4303323fade43cf70090a0863a28c5de0b26501))
* **modem-ardop:** ARQ connect/disconnect + DataSocket byte stream (tuxlink-6aj) ([27965f1](https://github.com/cameronzucker/tuxlink/commit/27965f17e07ea6619041ae5f784299761cde6f66))
* **modem-ardop:** cmd-socket session + init handshake (sync, threaded) (tuxlink-6aj) ([b22ef0c](https://github.com/cameronzucker/tuxlink/commit/b22ef0c24c1b6601c24348b90d941445942187d9))
* **modem-ardop:** Phase 1 wire codec — cmd, command, frame (tuxlink-6aj) ([c6ef211](https://github.com/cameronzucker/tuxlink/commit/c6ef2113f39fca979ede123537eadb221582a618))
* **modem:** ManagedModem process supervisor (SIGINT/SIGKILL, device-release) (tuxlink-6aj) ([b112d50](https://github.com/cameronzucker/tuxlink/commit/b112d503a100fd9b75c171e13a874e68649247d3))
* **modem:** ModemTransport trait + ArdopTransport (sync, object-safe) (tuxlink-6aj) ([0fac5b0](https://github.com/cameronzucker/tuxlink/commit/0fac5b083c4cbee640dcf219c9b49d30816c1340))
* **modem:** with_managed_modem + shutdown + ardop_connect CLI (tuxlink-6aj) ([3502a65](https://github.com/cameronzucker/tuxlink/commit/3502a650b42da6dd644bac00ac13e0e7269fd9dd))
* **packet:** add "Bluetooth" link kind + btMac to the TS DTO (tuxlink-nx2) ([e123da9](https://github.com/cameronzucker/tuxlink/commit/e123da9ace0e08c0947d92532777ff6bf527a7f3))
* **packet:** in-app Bluetooth RFCOMM-socket transport (tuxlink-nx2) ([a511c69](https://github.com/cameronzucker/tuxlink/commit/a511c69a2f9a1bb663647f91c09237221d137955))
* **packet:** intent prop gates Listen for cms-gateway vs p2p (tuxlink-3pb) ([cbcdb88](https://github.com/cameronzucker/tuxlink/commit/cbcdb8800ae02db95e54bba1be246831641df723))


### Bug Fixes

* **ax25:** floor connect/retransmit T1 to an RF-realistic minimum (tuxlink-uhc) ([3c9f577](https://github.com/cameronzucker/tuxlink/commit/3c9f577be5725a8cd30fa135d5073408c88ab884))
* **connections:** auto-expand the selected session type so the selection stays visible (tuxlink-3pb review) ([90932de](https://github.com/cameronzucker/tuxlink/commit/90932deedee9602b36fbf4ecfe06a09f42b87770))
* **connections:** harden isBuilt with intent-level gate + edge tests (tuxlink-3pb review) ([a11447a](https://github.com/cameronzucker/tuxlink/commit/a11447ac84cff5ad7cbc0a2ee06153fe3b7a1c0e))
* **modem-ardop:** CmdSocket Drop joins reader thread; bound write (tuxlink-6aj) ([aff5ec5](https://github.com/cameronzucker/tuxlink/commit/aff5ec5e3ff046767f9437910d340ac594f8a701))
* **modem-ardop:** Codex adversarial findings — outbound framing (P0) + 3 more (tuxlink-6aj) ([582bbe1](https://github.com/cameronzucker/tuxlink/commit/582bbe13018558a8cb44645c237de6c5a9527ab1))
* **modem-ardop:** DataDecoder re-syncs past a malformed length (no spin) (tuxlink-6aj) ([b925885](https://github.com/cameronzucker/tuxlink/commit/b92588597f3a121498d9348c0cfb9264ba760f49))
* **modem:** shutdown verifies audio release even on stop error; clears managed (tuxlink-6aj) ([cedb259](https://github.com/cameronzucker/tuxlink/commit/cedb25951752ccad6f34d2734a159c72a90842db))


### Refactors

* **modem-ardop:** ArdopTransport::init clean partial-failure, no unwrap (tuxlink-6aj) ([8aff02e](https://github.com/cameronzucker/tuxlink/commit/8aff02ee9ec2e99af32fc7c6ede6952edd2ab5c4))
* **settings:** drop CMS fieldset — relocated to the Telnet-CMS pane (tuxlink-3pb) ([4b86327](https://github.com/cameronzucker/tuxlink/commit/4b8632731da4b901ac1ead1fe35368008e3718e0))

## [0.1.0](https://github.com/cameronzucker/tuxlink/compare/v0.0.1...v0.1.0) (2026-05-22)


### ⚠ BREAKING CHANGES

* **chrome:** remove native titlebar + menu; HTML chrome is canonical (ng3)
* **ui:** the v0.0.1 main UI is Mock B, not Mock D. The tab strip is removed; the dashboard ribbon, folder sidebar, and human session log return.

### Features

* **app:** Connect command + ribbon button — run a native CMS exchange from the UI (tuxlink-0ic) ([5a8c705](https://github.com/cameronzucker/tuxlink/commit/5a8c70513b6288ffba57600c3f41296f1933c2ae))
* **app:** cut the app over to NativeBackend; draft Winlink client registration (tuxlink-0ic) ([894ac84](https://github.com/cameronzucker/tuxlink/commit/894ac847144a867c31aa65f7ce21c5f970bbe497))
* **ax25-packet:** wire packet Listen + link-close abort end to end ([8802220](https://github.com/cameronzucker/tuxlink/commit/88022208f3e4de640ad87b7ceb5cc4c1358793ec))
* **ax25:** add ExchangeRole to run_exchange; Dial preserves slave behaviour (tuxlink-7fr) ([7d3ba20](https://github.com/cameronzucker/tuxlink/commit/7d3ba2055bad79601d53ddbd41040eb28c70b32f))
* **ax25:** address path (dest/src/&lt;=2 digis) encode/decode (tuxlink-7fr) ([2e0c864](https://github.com/cameronzucker/tuxlink/commit/2e0c864e7b29d2fbba127fa7765eaf3ed88f6798))
* **ax25:** answer() — await inbound SABM, reply UA, surface the peer (tuxlink-7fr) ([c0ca661](https://github.com/cameronzucker/tuxlink/commit/c0ca6614445fffacb87ea5195a3d101edbf47fe6))
* **ax25:** Ax25Params with 1200-baud defaults (T1/N2/PACLEN/MAXFRAME) (tuxlink-7fr) ([5d104f5](https://github.com/cameronzucker/tuxlink/commit/5d104f5565c6957ea2cd09bd9db441ad6f138bc3))
* **ax25:** Ax25Stream::read — in-order I-frame delivery + RR/REJ reply (tuxlink-7fr) ([753a476](https://github.com/cameronzucker/tuxlink/commit/753a4760be0ee370f68d2b5b5b838ef8974f253e))
* **ax25:** Ax25Stream::write — PACLEN segmentation + MAXFRAME window + RR ack (tuxlink-7fr) ([de72f3b](https://github.com/cameronzucker/tuxlink/commit/de72f3b32b1a674b8d63bf345ab254108622ac05))
* **ax25:** connect_link serial arm — USB COM + Bluetooth RFCOMM via serialport (tuxlink-7fr) ([4226299](https://github.com/cameronzucker/tuxlink/commit/4226299b136785ad91fd5e3c1488b0870098e95c))
* **ax25:** connect() — push KISS params, send SABM, await UA (T1/N2/DM) (tuxlink-7fr) ([2b52c24](https://github.com/cameronzucker/tuxlink/commit/2b52c246f144583bf7d41df6934e49719135a05f))
* **ax25:** decode AX.25 address field with round-trip (tuxlink-7fr) ([5be441a](https://github.com/cameronzucker/tuxlink/commit/5be441a63853a107544b6511789ebfbce814a87f))
* **ax25:** disconnect() + Drop — send DISC, await UA (best-effort, bounded) (tuxlink-7fr) ([643802d](https://github.com/cameronzucker/tuxlink/commit/643802d3a5d36c0b1d9f5a5e6c2a5f54dbfeba9f))
* **ax25:** encode AX.25 address field (call+SSID, cr/last bits) (tuxlink-7fr) ([d42ab66](https://github.com/cameronzucker/tuxlink/commit/d42ab66f0dedc4a3abf69c8dbb08af313153966e))
* **ax25:** full-frame decode with round-trip (tuxlink-7fr) ([717e86a](https://github.com/cameronzucker/tuxlink/commit/717e86abb0eafc71dfcb33ef958a0b3c695d89b3))
* **ax25:** full-frame encode (path+control+PID/info, no FCS) (tuxlink-7fr) ([8ce4d8b](https://github.com/cameronzucker/tuxlink/commit/8ce4d8ba08827528533a1d172d7e86a3d99d06fe))
* **ax25:** incremental KISS decoder (de-escape, split reads) (tuxlink-7fr) ([6111541](https://github.com/cameronzucker/tuxlink/commit/6111541b01fa324ce60416013ed28e9af22c5d0e))
* **ax25:** KISS data-frame encode with FEND/FESC escaping (tuxlink-7fr) ([e1f4411](https://github.com/cameronzucker/tuxlink/commit/e1f4411660273fceede31f4d4f8227c022fe40fa))
* **ax25:** KISS TNC parameter command frames (tuxlink-7fr) ([6494ab6](https://github.com/cameronzucker/tuxlink/commit/6494ab63ef5e3314ae8a94b65394ad70f9a56cfc))
* **ax25:** KissLinkConfig + ByteLink trait + connect_link TCP arm (tuxlink-7fr) ([79ca745](https://github.com/cameronzucker/tuxlink/commit/79ca7458e2d1ca021900a0279c4557a3f57b8d57))
* **ax25:** mod-8 control-field encode/decode (U/S/I frames) (tuxlink-7fr) ([bed56ea](https://github.com/cameronzucker/tuxlink/commit/bed56eace4f75b81f16b59dbe454bbd3be73cffb))
* **ax25:** native_packet_exchange + native_packet_connect — wire AX.25 into B2F session (tuxlink-7fr) ([ed02756](https://github.com/cameronzucker/tuxlink/commit/ed027562b0f0c26bd799bc28fa49b3d4c359ece4))
* **ax25:** packet_config_get/set + packet_connect/set_listen Tauri commands (tuxlink-7fr) ([8700c7a](https://github.com/cameronzucker/tuxlink/commit/8700c7a15ddb11a396b2b57b6f0153621f4b7723))
* **ax25:** scaffold P2 datalink/link/params modules + add serialport dep (tuxlink-7fr) ([b7febe8](https://github.com/cameronzucker/tuxlink/commit/b7febe8a4f87270918932515d5f8696d89fcc712))
* **ax25:** scaffold winlink/ax25 wire-codec module (tuxlink-7fr) ([0eaf112](https://github.com/cameronzucker/tuxlink/commit/0eaf112c3a33da708c4ecf64138cd44a64a4e48e))
* **ax25:** T1 timeout retransmit (capped at N2) + REJ recovery (tuxlink-7fr) ([1a0821b](https://github.com/cameronzucker/tuxlink/commit/1a0821bf8969b4995df3e4c12999720ee536437c))
* **ax25:** TransportConfig::Packet + PacketRole + resolve_packet_endpoint (tuxlink-7fr) ([43562a2](https://github.com/cameronzucker/tuxlink/commit/43562a2a99ce34f6d839d4827c49c51b19f93b2b))
* **backend:** app-start Pat bootstrap + single BackendState three-state status (tuxlink-22l) ([3677463](https://github.com/cameronzucker/tuxlink/commit/3677463fe1f21dcc00166c059c639b421e49c4a3))
* **backend:** PatBackend::spawn — bridge Pat stderr to durable log + broadcast, supervised lifecycle (tuxlink-22l) ([238e13b](https://github.com/cameronzucker/tuxlink/commit/238e13bb5fb958cf003b41689ea5aa0a779a1ac1))
* **backend:** SessionLogState ring buffer + seq (Task A, tuxlink-22l) ([165ac14](https://github.com/cameronzucker/tuxlink/commit/165ac1462a6acbe615cbc99944acaa2e7b75d1e8))
* **chrome:** add app_quit command for HTML menu Quit (ng3) ([d692dff](https://github.com/cameronzucker/tuxlink/commit/d692dff89d98616e83ff0aaebef1d13af1fb6e46))
* **chrome:** borderless-window resize handles (ng3) ([419ad0b](https://github.com/cameronzucker/tuxlink/commit/419ad0b4c8fff42f88a3b79b29bc4f222e9efb47))
* **chrome:** data-driven menu model + action-id manifest (ng3) ([4fb6183](https://github.com/cameronzucker/tuxlink/commit/4fb6183adc4f33e580d148af0126bd93b9ce0230))
* **chrome:** grant window-control capabilities for custom chrome (ng3) ([5f36882](https://github.com/cameronzucker/tuxlink/commit/5f36882d51d900d219cd0de8cda9a6327ef2a248))
* **chrome:** HTML MenuBar component (ng3) ([555973d](https://github.com/cameronzucker/tuxlink/commit/555973dd78fc40b34cc58748f5acda3dd2162453))
* **chrome:** HTML TitleBar with window controls (ng3) ([fcb88f8](https://github.com/cameronzucker/tuxlink/commit/fcb88f803d6503c4fa5809589bcf5af95c638e14))
* **chrome:** in-process menu action dispatcher (ng3) ([1bff1de](https://github.com/cameronzucker/tuxlink/commit/1bff1de8c3f4899d947dca641652ad5a90a5f59e))
* **chrome:** keyboard accelerator hook + matcher (ng3) ([754309a](https://github.com/cameronzucker/tuxlink/commit/754309a6014f043298f5a34d24105d42207ce580))
* **chrome:** minimal compose-window title bar; closes msr duplicate menu (ng3) ([071cd3f](https://github.com/cameronzucker/tuxlink/commit/071cd3fdb9895ef9d92fb9b89a411976062d85f0))
* **chrome:** remove native titlebar + menu; HTML chrome is canonical (ng3) ([19a92a9](https://github.com/cameronzucker/tuxlink/commit/19a92a9417d2491cfbc0ad02cfe45fcb3039fc49))
* **chrome:** render HTML chrome in AppShell; dispatch menu in-process (ng3) ([46bb2e4](https://github.com/cameronzucker/tuxlink/commit/46bb2e41708320e4fb0ceb587147c06f392740b6))
* **chrome:** token-driven chrome stylesheet (ng3) ([c046421](https://github.com/cameronzucker/tuxlink/commit/c04642147892d52c8d679518a3fc75fb5009d8f3))
* **chrome:** tuxlink app icon — bundle + in-app titlebar + README (tuxlink-9dg) ([08f8a1c](https://github.com/cameronzucker/tuxlink/commit/08f8a1c7e9b5e16098d1f273df78385108e9a91f))
* **compose:** Task 14 — separate-window compose + draft persistence ([1b69fa3](https://github.com/cameronzucker/tuxlink/commit/1b69fa36533a3ff822a5029c55d67cbcfebb80c7))
* **config:** additive [packet] section — sticky SSID, KISS link, AX.25 params (tuxlink-7fr) ([895a5b5](https://github.com/cameronzucker/tuxlink/commit/895a5b547169eb767224fe6981b89aea48d52f99))
* **config:** position_source field (default Gps, additive/no schema bump) (tuxlink-686) ([614d4ff](https://github.com/cameronzucker/tuxlink/commit/614d4ff00339b23ca47ee8e7689abf5dd460ddbb))
* **config:** tuxlink-4mt Phase 1 — validate_identity + describe-helper ([5f7103e](https://github.com/cameronzucker/tuxlink/commit/5f7103ef557d0e84fd2ca340e06dc98f52bcbd8d))
* **config:** tuxlink-4mt Phase 2 — nested Config types per AMD-1 + AMD-11 drift defense ([a7d10e0](https://github.com/cameronzucker/tuxlink/commit/a7d10e0bfdc6388bde8f4820f728ddb988f1e0f2))
* **config:** tuxlink-4mt Phase 3 — Config::validate + ConfigValidationError ([4a363d4](https://github.com/cameronzucker/tuxlink/commit/4a363d46e6357359bdc552aeae8dfe9947220fbc))
* **config:** tuxlink-4mt Phase 4 — read_config + ConfigReadError ([0b7bca7](https://github.com/cameronzucker/tuxlink/commit/0b7bca7e7f28f8204fc2edc708701630f13607df))
* **config:** tuxlink-4mt Phase 5 — write_config_atomic + ConfigWriteError ([93e6334](https://github.com/cameronzucker/tuxlink/commit/93e6334a45b81f7d25c7d2a90d66392b7547cfd3))
* **config:** typed tuxlink Config with schema version and validation ([#4](https://github.com/cameronzucker/tuxlink/issues/4)) ([b85da90](https://github.com/cameronzucker/tuxlink/commit/b85da90fa25cf9ea2f77b6d4103db9fc70bc8144))
* **connect:** abort control for an in-flight CMS connection (tuxlink-9z2) ([1163f04](https://github.com/cameronzucker/tuxlink/commit/1163f04629e57513ac78b4683b3b38867e8ef570))
* **hooks:** main-checkout race protection + session leases + rev-parse refactor ([05e31c3](https://github.com/cameronzucker/tuxlink/commit/05e31c321b65a97ee1a0ca9017132ed1feb030e9))
* **mailbox:** folder sidebar + virtualized message list + AppShell + routing ([5106064](https://github.com/cameronzucker/tuxlink/commit/5106064eab58bd693be37ec1abbae54ac2e5effc))
* **mailbox:** IPC foundation — UiError, AppBackend, mailbox_list, trait additions ([523bea6](https://github.com/cameronzucker/tuxlink/commit/523bea664e329fe93e649f672d24b7dd0aaf30d4))
* **mailbox:** Task 13 — message reading pane + RFC5322 parse ([b504ccf](https://github.com/cameronzucker/tuxlink/commit/b504ccfe262e6bc92e37cce90bc441be45477990))
* **mailbox:** track read/unread state in the native store (tuxlink-xgn) ([b840b4d](https://github.com/cameronzucker/tuxlink/commit/b840b4d221b8d2a61f25c2350d22a03b42773c7e))
* **native:** functional NativeBackend over the native store + transports (tuxlink-0ic) ([8c9d6a0](https://github.com/cameronzucker/tuxlink/commit/8c9d6a0614c10ec4ce6e31cdb8839fb972746776))
* **native:** Pat-independent on-disk message store (tuxlink-0ic) ([b7ab559](https://github.com/cameronzucker/tuxlink/commit/b7ab55947d9646d4825ba2d01068bb518de84c0b))
* operator-only live_cms_smoke binary + consent gate (tuxlink-nk7, Task 6) ([34f6ef5](https://github.com/cameronzucker/tuxlink/commit/34f6ef51c3bfe8ac081f3aa55c2ac04b36cefbb6))
* **packet-ui:** add P3 PacketConfigDto/command TS mirror types (tuxlink-7fr) ([2f0c153](https://github.com/cameronzucker/tuxlink/commit/2f0c153e20644cb747653cdb838d53bf4e2cd62a))
* **packet-ui:** AppShell reader slot + ribbon + status-bar packet transport indicator (Tasks 11-12) (tuxlink-7fr) ([d8e7f9e](https://github.com/cameronzucker/tuxlink/commit/d8e7f9ed6eaf79380dbe4fa55a63c1ffb21af9ad))
* **packet-ui:** PacketConnectionPanel (Tasks 3-8) — full inline panel + container (tuxlink-7fr) ([3a3573e](https://github.com/cameronzucker/tuxlink/commit/3a3573ec1ad46ca104b99904ba02490f4b71a82a))
* **packet-ui:** pin packet/AX.25 session-log projection (shaped transport lines kept; raw frames in Raw) (tuxlink-7fr) ([5d02fb6](https://github.com/cameronzucker/tuxlink/commit/5d02fb69be54ff9fef5d42f2c2a455248356f7cf))
* **packet-ui:** pure config helpers (effectiveCall, ssidOptions, immutable updaters, pathPreview) (tuxlink-7fr) ([02f14e6](https://github.com/cameronzucker/tuxlink/commit/02f14e629f56eb8c9e4dccadf1b74763937ebfe2))
* **packet-ui:** pure ribbon/status-bar packet formatters (Packet 1200 · Listening as N7CPZ-7) (tuxlink-7fr) ([96f9841](https://github.com/cameronzucker/tuxlink/commit/96f9841f17a644750e8029a2f462d6f156701b91))
* **packet-ui:** real Listen control with honest armed state ([406fe5d](https://github.com/cameronzucker/tuxlink/commit/406fe5dcf936fc5b1342d0aec07de713eecc2305))
* **packet-ui:** selectable Packet (AX.25) sidebar entry with transport-state dot (tuxlink-7fr) ([18380a6](https://github.com/cameronzucker/tuxlink/commit/18380a697f210e03bf009d1f9173e4c803325ef5))
* **packet:** live Listening/Connected status feed to ribbon + status bar (tuxlink-orj) ([0aa1002](https://github.com/cameronzucker/tuxlink/commit/0aa10020ce8cabab1c5f56a013f9297823c6498f))
* **pat-client:** blocking HTTP client for Pat's mailbox API ([8f40405](https://github.com/cameronzucker/tuxlink/commit/8f40405568797c3245941d2425acde699cc91f58))
* **pat-client:** tuxlink-z5f Phase 0 — async PatClient + read + Clone + log_sink ([7f3cdb1](https://github.com/cameronzucker/tuxlink/commit/7f3cdb1921494c7de65215231d4ccd8be2496160))
* **pat-config:** tuxlink-756 — pat_config module + 6 contract tests ([a93a7d8](https://github.com/cameronzucker/tuxlink/commit/a93a7d8136dc58c6b42060c841214a22e4fdb7d6))
* **pat-process:** tuxlink-756 — render Pat config at spawn time ([4a8f344](https://github.com/cameronzucker/tuxlink/commit/4a8f3444f01ce83d5323c666b6a4d2619dbad7b6))
* **pat:** child-process lifecycle for the bundled Pat daemon ([#5](https://github.com/cameronzucker/tuxlink/issues/5)) ([4c64252](https://github.com/cameronzucker/tuxlink/commit/4c64252a3987963105786eec5129691c439e933e))
* **position:** CMS locator sourced from the arbiter (tuxlink-686) ([cf3bb02](https://github.com/cameronzucker/tuxlink/commit/cf3bb02a02fa641b61c9cb3b52a4698745ab09c7))
* **position:** config_set_grid command + managed arbiter (tuxlink-686) ([abafcc8](https://github.com/cameronzucker/tuxlink/commit/abafcc8d1cbf90c7973be390fe40b3be79b9e82e))
* **position:** gpsd TPV -&gt; Fix parsing (tuxlink-686) ([40f56ab](https://github.com/cameronzucker/tuxlink/commit/40f56abf680e34986f251cf70c64d9bfe6f9cbd0))
* **position:** gpsd watch task with reconnect backoff (tuxlink-686) ([e117f25](https://github.com/cameronzucker/tuxlink/commit/e117f25a3fbbae9af1baa6f73d82d0f799cec64d))
* **position:** inline-edit grid + source chip in the ribbon (tuxlink-686) ([ff01e5c](https://github.com/cameronzucker/tuxlink/commit/ff01e5cf2c0b97f15310c26f7203a4477e8d2be3))
* **position:** lat/lon -&gt; Maidenhead 6-char conversion (tuxlink-686) ([0c5684c](https://github.com/cameronzucker/tuxlink/commit/0c5684c5bc02dffb71e7e3d20dd121dbfcc5d475))
* **position:** Maidenhead -&gt; lat/lon (square center) (tuxlink-686) ([db790a0](https://github.com/cameronzucker/tuxlink/commit/db790a02d2b1c5c2c9499b796f74d1c7d6755c6d))
* **position:** source-arbiter state machine (manual sticky, broadcast reduction) (tuxlink-686) ([abb207f](https://github.com/cameronzucker/tuxlink/commit/abb207f64f7b2f973bee53ab9dbb2d33dfc686b3))
* **position:** surface position_source in the status DTO (tuxlink-686) ([f197368](https://github.com/cameronzucker/tuxlink/commit/f197368454ef795db036bad96acd884462bc427e))
* **position:** use-gps switch + spawn gpsd client at startup (tuxlink-686) ([be4a992](https://github.com/cameronzucker/tuxlink/commit/be4a99297ec309f1366710f6b6f98f29aca1c958))
* **scripts:** Python moniker generator (3-word hyphenated from 100-word pool) ([e0aa0ba](https://github.com/cameronzucker/tuxlink/commit/e0aa0ba826a6fc65f551a816dccca20e2540f14e))
* **scripts:** Python worktree-creator + sessions-lister ([88e6daf](https://github.com/cameronzucker/tuxlink/commit/88e6daf1b7e4206d085d03cc02fabeacb7763f51))
* **session-log:** tee the raw B2F wire dialogue into the session log Raw view (tuxlink-nki) ([19e7e2b](https://github.com/cameronzucker/tuxlink/commit/19e7e2b39524d3c08c45f82baabccf908467b668))
* **session:** Task 15 — session log pane (Human/Raw projections) ([8a19312](https://github.com/cameronzucker/tuxlink/commit/8a1931279cac4b346374e6bb0492e02d2829ad53))
* **settings:** inline GPS privacy controls — gps_state + precision (tuxlink-39b) ([eb44a5e](https://github.com/cameronzucker/tuxlink/commit/eb44a5eb02d2b1743fe10850b75309ba187f8738))
* **shell:** integration commit — wire AppShell regions, register IPC commands, config_read/backend_status, FolderSidebar counts, compose routing ([266353a](https://github.com/cameronzucker/tuxlink/commit/266353a51d603f9e6608b018a6ed65e07c54b6fc))
* **shell:** Task 16 — DashboardRibbon + StatusBar + useStatus formatters (tuxlink-hvv) ([a93fea9](https://github.com/cameronzucker/tuxlink/commit/a93fea9bfc3e28fc8bc3dd08b5d861a9a20786a7))
* **tray:** Task 8 — system tray icon + window-close-to-tray ([ee5ca55](https://github.com/cameronzucker/tuxlink/commit/ee5ca558c42cb591b491373cafee990e99e6bfe0))
* **ui:** realize mock-d fidelity — tokens, compact rows, reply bar ([f0c5be1](https://github.com/cameronzucker/tuxlink/commit/f0c5be123022584d77f8e591e680d611cded7da5))
* **ui:** rebuild v0.0.1 main UI to Mock D (Mail.app-minimal) (tuxlink-yd4) ([cf679e5](https://github.com/cameronzucker/tuxlink/commit/cf679e5fd624df2f50b7a00ed9d9e3f503befe2b))
* **ui:** selectable color schemes — night/tactical + grayscale (tuxlink-8za) ([5af09aa](https://github.com/cameronzucker/tuxlink/commit/5af09aa14b0bff3d1f3b27b5e3828ae1dbe7c849))
* **ui:** session log dedupe on seq + subscribe-then-snapshot (tuxlink-22l) ([2c980c9](https://github.com/cameronzucker/tuxlink/commit/2c980c94c869dc27e9641d87a3929430423161f9))
* **ui:** tuxlink-6vi Task 7 — native OS menu bar with AMD-10 additions ([7fccb60](https://github.com/cameronzucker/tuxlink/commit/7fccb606b5a9ae81609a15cf2ec5baf7dd634eb0))
* **ui:** VITE_TUXLINK_LIVE escape hatch to disable dev fixture for live smoke (tuxlink-22l) ([739cfdc](https://github.com/cameronzucker/tuxlink/commit/739cfdca06124efdcfd4146d0418022077c1ae85))
* **winlink-backend:** tuxlink-z5f — WinlinkBackend trait + PatBackend + NativeBackend stub ([8489640](https://github.com/cameronzucker/tuxlink/commit/8489640f9f651715377c82c950d4fe784620d912))
* **winlink:** B2F message exchange turns (send/receive) (tuxlink-0ic) ([3f0b7e7](https://github.com/cameronzucker/tuxlink/commit/3f0b7e7a9e64004ea6de262cdf7407eec17ffc45))
* **winlink:** B2F proposal batch checksum, inbound parse, and FS answers (tuxlink-0ic) ([3c5b240](https://github.com/cameronzucker/tuxlink/commit/3c5b240f64946899282e744a0c58c7898d5930b4))
* **winlink:** B2F proposal offer line (tuxlink-0ic) ([947b01c](https://github.com/cameronzucker/tuxlink/commit/947b01c6b82fd990c5e9808f417adc1f1abbf3a3))
* **winlink:** compose an outbound Winlink message from fields (tuxlink-0ic) ([fadfcc7](https://github.com/cameronzucker/tuxlink/commit/fadfcc7ee61438c604d627803a0cdb373c44ecec))
* **winlink:** framed block transfer for message bodies (tuxlink-0ic) ([10164b5](https://github.com/cameronzucker/tuxlink/commit/10164b5b5a9361bbc3e345f33fae1d788fb2fab3))
* **winlink:** full session driver + telnet transport (tuxlink-0ic) ([09e62fd](https://github.com/cameronzucker/tuxlink/commit/09e62fd3de74166b85f8338beb829262d50774e3))
* **winlink:** handshake build/parse + shared CR-line framing (tuxlink-0ic) ([f9d39c7](https://github.com/cameronzucker/tuxlink/commit/f9d39c730aea8a6b63dcefbb2f3e6f9c6db31a2e))
* **winlink:** lzhuf compression for the FBB B2 format (tuxlink-0ic) ([815ab2e](https://github.com/cameronzucker/tuxlink/commit/815ab2e48b55b6fa45f3b120e361dd5f65936277))
* **winlink:** lzhuf decompression for the FBB B2 format (tuxlink-0ic) ([930e677](https://github.com/cameronzucker/tuxlink/commit/930e67706c43add282f4ac49566e7cd1ea96aebc))
* **winlink:** native Winlink message format — serialize + parse (tuxlink-0ic) ([95f0e51](https://github.com/cameronzucker/tuxlink/commit/95f0e51aa7b777b567785f0c6187232237401e68))
* **winlink:** secure-login response for the password challenge (tuxlink-0ic) ([f00916c](https://github.com/cameronzucker/tuxlink/commit/f00916cad08bbe14b8f8564ddb0216ff76c65c74))
* **winlink:** telnet login preamble + *** error handling; validated against live CMS (tuxlink-0ic) ([7d41244](https://github.com/cameronzucker/tuxlink/commit/7d4124449c3367551986637742a28b2a000c8e81))
* **winlink:** TLS-wrapped telnet transport (default), validated against live CMS (tuxlink-0ic) ([7a501c0](https://github.com/cameronzucker/tuxlink/commit/7a501c0c59f2f5286c95c83c9cbc2960760547fd))
* **winlink:** turn a message into a proposal + compressed body (tuxlink-0ic) ([7b560a9](https://github.com/cameronzucker/tuxlink/commit/7b560a948d28d2910691168141bafe73e83e70cb))
* **wizard:** live test-send spawns its own ephemeral Pat (tuxlink-pqg) ([3685c2d](https://github.com/cameronzucker/tuxlink/commit/3685c2dc66302c5a8cb8ce76cdbff62e30c9ec84))
* **wizard:** Phase 1 infrastructure — types + reducer + context + Rust skeleton + App routing ([0e21a94](https://github.com/cameronzucker/tuxlink/commit/0e21a949983e5305c88f56b5afba3b5fa5a7b9ce))
* **wizard:** Step 1 Welcome — connection-type routing (Task 9 / tuxlink-ko0) ([188c489](https://github.com/cameronzucker/tuxlink/commit/188c489a9c4cab81a210d66b5ee0a870cd2aed5c))
* **wizard:** Step2Credentials + capability + CSP + shell-open (Tasks 3.3+3.4 / tuxlink-1r5) ([9db8a83](https://github.com/cameronzucker/tuxlink/commit/9db8a836a9b6821c34b8b6758356bac6d25ea9ba))
* **wizard:** Task 11 — test-send 4-substate + wizard_run_test_send (MOCKED, Part-97 dedup) ([0549bbc](https://github.com/cameronzucker/tuxlink/commit/0549bbc24fa235c11753d0d462a83e30a8de6370))
* **wizard:** Task 11.5 — offline-identity path (tuxlink-d76) ([ce59f57](https://github.com/cameronzucker/tuxlink/commit/ce59f572285fb09a99e5f849eb77724d94ce43d8))
* **wizard:** validators.ts — callsign/password/grid per spec §5.9 + AMD-3 ([60fd5de](https://github.com/cameronzucker/tuxlink/commit/60fd5dee3386205b01ec1fee4ff6b45afe23cd36))
* **wizard:** wizard_persist_cms — keyring-first transactional write per spec §3.2 ([e29295a](https://github.com/cameronzucker/tuxlink/commit/e29295a753a32f4a8aea575156c52f1373963a7c))


### Bug Fixes

* **ax25-b2f:** correct master-role handshake; prove end-to-end over TCP/KISS ([8cf6811](https://github.com/cameronzucker/tuxlink/commit/8cf6811ee66f86c8dcc24ba0566e85f7ae8ec056))
* **ax25-ui:** classify USB vs Bluetooth devices in the picker (no conflation) (tuxlink-7fr) ([9e481dd](https://github.com/cameronzucker/tuxlink/commit/9e481dd8f8f29b761da4b59a530536e699b85984))
* **ax25-ui:** real USB/BT device picker + honest status + controlled modem inputs (tuxlink-7fr) ([7c30135](https://github.com/cameronzucker/tuxlink/commit/7c301356950937216aec35d52e4005c0c086517b))
* **ax25:** address Codex adversarial findings on the B2F packet path ([1ccde4c](https://github.com/cameronzucker/tuxlink/commit/1ccde4c4e643cec2083dcb2bac47a7332efa89d8))
* **ax25:** connected-mode state-machine correctness hardening (A–I) + P3/P4 contract notes (J,K,L) ([60372e7](https://github.com/cameronzucker/tuxlink/commit/60372e76be3161de499c20ac2ddd89a133a4f04c))
* **ax25:** harden wire codec per code review — panic-safe shifts, error type (tuxlink-7fr) ([ef72711](https://github.com/cameronzucker/tuxlink/commit/ef7271160e6bdae7383ceb198c3013eaac5c8a37))
* **backend:** deliver startup logs via buffer-polling drain + fix Pat process leaks/teardown (tuxlink-22l Codex R2) ([b0d7599](https://github.com/cameronzucker/tuxlink/commit/b0d7599d1aff8e57b7eb72bcdf9c5ca2e5a2765e))
* **backend:** enforce PatProcess announce timeout + null stdout + forward all stderr lines (tuxlink-22l) ([83f7430](https://github.com/cameronzucker/tuxlink/commit/83f74304d53bf4c695ee9685ee472845a637c97b))
* **backend:** start session-log drain before spawn so failed-start diagnostics emit (tuxlink-22l Codex R3 [#2](https://github.com/cameronzucker/tuxlink/issues/2)) ([d2d2c0c](https://github.com/cameronzucker/tuxlink/commit/d2d2c0c2095958ed8ba564226ef629ae50dcd368))
* **build:** set default-run = tuxlink so cargo run / tauri dev pick the app bin (tuxlink-0ic) ([b864864](https://github.com/cameronzucker/tuxlink/commit/b864864b858ce3803fc1d98b1902059d36243fb3))
* **chrome:** compose window centered (Wayland can't dock a separate window) (ng3 [#4](https://github.com/cameronzucker/tuxlink/issues/4)/[#8](https://github.com/cameronzucker/tuxlink/issues/8)) ([d801fad](https://github.com/cameronzucker/tuxlink/commit/d801fadb413c84675f9af6c5e1a7156c64449c2d))
* **chrome:** make menu hover highlight perceptible (ng3 re-smoke [#1](https://github.com/cameronzucker/tuxlink/issues/1)) ([6fa3fb1](https://github.com/cameronzucker/tuxlink/commit/6fa3fb1427a6a2a1a4669d9e885f1cf392107e50))
* **chrome:** move New Message to the Message menu; rename id (ng3 smoke [#5](https://github.com/cameronzucker/tuxlink/issues/5)) ([0282519](https://github.com/cameronzucker/tuxlink/commit/0282519428eb2bf5a955290d162224a0b01d6d13))
* **chrome:** remove unused vi import in useAccelerators.test.ts ([5718688](https://github.com/cameronzucker/tuxlink/commit/571868867c37ae22ed3ef17a1ede5d2c273e39d2))
* **chrome:** repair .layout-b grid rows for HTML chrome (ng3 re-smoke) ([e122970](https://github.com/cameronzucker/tuxlink/commit/e1229705ba7b351dd89d5fd871323c98e9fbaa64))
* **chrome:** ResizeHandles uses local string-union, not unexported ResizeDirection (ng3) ([a3897d7](https://github.com/cameronzucker/tuxlink/commit/a3897d7bdfa800f2f17748db7ab62f93b2b60874))
* **chrome:** restore menubar fidelity + larger app icon (ng3 smoke) ([887921f](https://github.com/cameronzucker/tuxlink/commit/887921f459a388c41946e35e890d10f92e188a1e))
* **chrome:** single compose title bar + dock bottom-right (ng3 smoke [#4](https://github.com/cameronzucker/tuxlink/issues/4)/[#8](https://github.com/cameronzucker/tuxlink/issues/8)) ([101ee17](https://github.com/cameronzucker/tuxlink/commit/101ee17939d460b6ada9b154c4e071926ae0cdb7))
* **chrome:** submenu no longer overlaps the parent dropdown border (ng3 re-smoke) ([6f7bbd9](https://github.com/cameronzucker/tuxlink/commit/6f7bbd95c1f98327b75002095f241433af8c78d2))
* **compose:** apply Codex P1+P2+P3+chrono findings from adrev ([87b6b15](https://github.com/cameronzucker/tuxlink/commit/87b6b15a5a005534d68c7823f0e432fa88d79037))
* **compose:** route window close through self-only command; drop window-class grants (tuxlink-h2y) ([5355542](https://github.com/cameronzucker/tuxlink/commit/535554258211d41e344dc91d34f8e80fcd5eb069))
* **compose:** validate draft_id charset/length before label+route (tuxlink-g3d) ([84a6ca3](https://github.com/cameronzucker/tuxlink/commit/84a6ca36a54717f868d51ba1538197ab9ef74038))
* **connect:** bound DNS, total connect deadline, all-address errors (tuxlink-lbg) ([c612a48](https://github.com/cameronzucker/tuxlink/commit/c612a485d81880d86b5b811c970bde3e7e8733ab))
* **connect:** harden against double-connect + abort races (Codex adrev) ([d38b92c](https://github.com/cameronzucker/tuxlink/commit/d38b92cec449fee46b4b43221cb0bcddc56a179d))
* **disposal:** D4 codex P1 remediation — archive outside worktree + inventory --ignored ([08886a6](https://github.com/cameronzucker/tuxlink/commit/08886a6abf8e8107c1430864f3a51aaaa7b66e9c))
* **disposal:** D4 codex P1 remediation — archive outside worktree + inventory --ignored ([197ab04](https://github.com/cameronzucker/tuxlink/commit/197ab0497f4ff3a0be3400337c0a19d9e07c6189))
* **docs:** cleanup-PR codex P2 remediation — ADR 0010 recipe, public-facing squash refs, ADR 0006 / BD-1 staleness ([fb017af](https://github.com/cameronzucker/tuxlink/commit/fb017af4e359b66891b1166efe41afe4bd0f464d))
* **docs:** cleanup-PR codex P2s — ADR 0010 recipe + public-facing squash refs + ADR 0006/BD-1 staleness ([b641825](https://github.com/cameronzucker/tuxlink/commit/b6418259710fdb36f69a7b29144a809d511be588))
* **hooks:** D1 codex remediation — lease location, git -C, bare stash/branch, shellcheck ([f1d4552](https://github.com/cameronzucker/tuxlink/commit/f1d45529b66d92cd61ae2ea0dcd361fb02faa4ac))
* **hooks:** D1 codex review remediation — lease location + edge cases + shellcheck ([4c6b066](https://github.com/cameronzucker/tuxlink/commit/4c6b0666bf3db6268fd2b8f8e0d05ec3dadfa516))
* **live-cms-smoke:** stream Pat stderr + non-fatal generous connect (tuxlink-22l) ([190d75c](https://github.com/cameronzucker/tuxlink/commit/190d75c2bea9b31821975868c4321991253150cf))
* **mailbox:** Task 13 reading-pane error states + body wrap ([23599c3](https://github.com/cameronzucker/tuxlink/commit/23599c3893acd259ddd35a4016e7ee53e88784cc))
* **menu:** Tools-menu coherence — drop Preferences dupe, disable+badge unwired stubs, fix Settings spacing (tuxlink-39b) ([93f0be9](https://github.com/cameronzucker/tuxlink/commit/93f0be93e7def751324e16f8ca8bc91ddda6d605))
* **menu:** uniform menu-row height — fixed min-height + centered content (tuxlink-39b) ([fc83be3](https://github.com/cameronzucker/tuxlink/commit/fc83be3af551a36080417ec2111c9b897f1c962f))
* **merge:** add packet field to tuxlink-686 position test fixtures ([08c57d1](https://github.com/cameronzucker/tuxlink/commit/08c57d196e28968103b1556764fdcb9872450b52))
* **net:** default CMS host to cms-z.winlink.org until tuxlink is registered (ng3 re-smoke) ([7c50359](https://github.com/cameronzucker/tuxlink/commit/7c50359e1c5184d27dd860cff91f0cec3b56d824))
* **packet-ui:** remove unused imports to clear tsc --noEmit warnings (tuxlink-7fr) ([3c2371c](https://github.com/cameronzucker/tuxlink/commit/3c2371c9a45d2491fbf03aa787766cdd23432908))
* **pat-client:** send() uses multipart/form-data per real Pat API contract ([d722ec3](https://github.com/cameronzucker/tuxlink/commit/d722ec34d2c262d26e32a249da17a0db5b9e84ba))
* **pat:** enforce read-side byte cap before buffering message body (tuxlink-f1a) ([6095882](https://github.com/cameronzucker/tuxlink/commit/6095882070073f2810ca21b3438dcce1aa9f8361))
* **position:** honor gps_state privacy in on-air locator + surface live broadcast grid (Codex P1) (tuxlink-686) ([48187b3](https://github.com/cameronzucker/tuxlink/commit/48187b3b68f59f3bf592b47a16bd5d912ed2629b))
* **safety-stack:** get_tuxlink_sessions.py lease-dir parity with hook (tuxlink-arv P1) ([40838d1](https://github.com/cameronzucker/tuxlink/commit/40838d10e654e3633068f43bbe10c0150f3eedc2))
* **scripts,gitignore:** D4 codex P2 remediation — bd CLI + worktrees ignored + --issue required ([3671658](https://github.com/cameronzucker/tuxlink/commit/367165851dfa7f87e9523076bc648e28d8cc13aa))
* **scripts,gitignore:** D4 codex P2 remediation — bd update --append-notes + worktrees/ ignored + --issue required ([c5c8438](https://github.com/cameronzucker/tuxlink/commit/c5c8438b886302488e6f45a6a0c73f6c89a1c1ac))
* **scripts:** B3 codex P2 — fail closed when moniker collision pre-flight fails ([85b6df8](https://github.com/cameronzucker/tuxlink/commit/85b6df82bfcf4d85d0fef374451e9660c214650d))
* **scripts:** B3 codex P2 remediation — fail closed when git log pre-flight fails ([1cc8279](https://github.com/cameronzucker/tuxlink/commit/1cc82797bc84ccb952329efd4ab6e3bbdc77bf72))
* **session-log:** resolve listener leak, snapshot/event race, new-session scroll resume, and strengthen scroll-pause tests ([c3e614e](https://github.com/cameronzucker/tuxlink/commit/c3e614e1367160b4b608a472ce67ca71c4ff66ba))
* **session-log:** summarize binary wire payloads instead of mojibake (tuxlink-nki) ([f35a8d2](https://github.com/cameronzucker/tuxlink/commit/f35a8d2f5251dc99e4b88d04263559f8587e3885))
* **settings:** menu consolidation + grid-input width/prompt + green-locked chip (tuxlink-39b) ([fb25c5a](https://github.com/cameronzucker/tuxlink/commit/fb25c5ae63db87562a64f016e734da5162341b27))
* **shell:** compose-window capability + session_log_snapshot + Disconnected transport + compose listener guard (Codex integration round) ([b6331de](https://github.com/cameronzucker/tuxlink/commit/b6331deb2ac3382c0486a96ca7f9c1cbdebde84e))
* **test:** add missing position_source to DashboardRibbon test literal ([c6c381f](https://github.com/cameronzucker/tuxlink/commit/c6c381f455240181f0cbdf761ce4ab61e3ce8bbd))
* **test:** add position_source to PrivacyConfig literals after 686 merge (tuxlink-7fr) ([2e52eeb](https://github.com/cameronzucker/tuxlink/commit/2e52eeb49c93a7740cc08ac0524008d559c72867))
* **test:** fail-closed isolation for real-keyring integration tests (tuxlink-cnd) ([7e7c01a](https://github.com/cameronzucker/tuxlink/commit/7e7c01a4b93b9991849e71187631ed6f6471c88d))
* **tray:** apply Codex findings — single tray, main-only close-to-tray, event guard, spec tests ([1518a12](https://github.com/cameronzucker/tuxlink/commit/1518a12a0f5604ac1b57a92d7af7a2d93db907fb))
* **ui:** address Codex P2 — no form XML in reply/forward, note dropped attachments ([adbbf4b](https://github.com/cameronzucker/tuxlink/commit/adbbf4b1920552275dc3e10c5e9678c547ec6003))
* **ui:** clamp connection status so a rejection can't push Connect off-screen (ng3 smoke [#6](https://github.com/cameronzucker/tuxlink/issues/6)) ([5797b2f](https://github.com/cameronzucker/tuxlink/commit/5797b2fc73017de0139b3da71374e7aec9921945))
* **ui:** concise human-readable connection error in the ribbon (ng3 re-smoke [#5](https://github.com/cameronzucker/tuxlink/issues/5)) ([7d7b77e](https://github.com/cameronzucker/tuxlink/commit/7d7b77e4877985951177017f4ca99d1b0a54793a))
* **ui:** correct Mock D fidelity to literal — drop the creative liberties (tuxlink-yd4) ([9e7f21c](https://github.com/cameronzucker/tuxlink/commit/9e7f21cfcba8c459bd96ed10f62f073312ce5d05))
* **ui:** disable webkit2gtk DMA-BUF renderer on Linux (tuxlink-wfw) ([8607d04](https://github.com/cameronzucker/tuxlink/commit/8607d04c3091f702bb6bf23153b053535b334676))
* **ui:** global CSS foundation — reset/box-sizing, central --tux-* tokens, dark base+form theming, retitle ([0c01f75](https://github.com/cameronzucker/tuxlink/commit/0c01f7504243569cd5e181292c6a42c0a1a155c2))
* **ui:** real identity by default (dev fixture opt-in) + Linux key hint, not Mac (tuxlink-0ic) ([f9485dc](https://github.com/cameronzucker/tuxlink/commit/f9485dc0f67d56709fad62090c9494a984b3701e))
* **ui:** rebuild v0.0.1 main UI to Mock B — the approved design (NOT Mock D) ([1afee84](https://github.com/cameronzucker/tuxlink/commit/1afee84e23a519c7b1e846ccbc13b37e06c80e09))
* **ui:** revert to canonical Linux Quit pattern ([888b957](https://github.com/cameronzucker/tuxlink/commit/888b9574a59372329b248ce76e63de546832e4d1))
* **ui:** ribbon shows configured/active transport, not hardcoded 'telnet ready' (tuxlink-989) ([86c25c9](https://github.com/cameronzucker/tuxlink/commit/86c25c9f50cccdbdb97759e05bdf387c2ff82859))
* **ui:** surface backend-error reason in ribbon + cap session-log buffer (tuxlink-22l Codex R2) ([6269da6](https://github.com/cameronzucker/tuxlink/commit/6269da65076cc86f50099d3b34be07c42fdef561))
* **ui:** surface CMS connect result in the session log, not beside the button (tuxlink-0ic) ([ba25328](https://github.com/cameronzucker/tuxlink/commit/ba25328726557f5d620ab4e83e51857213ff8a69))
* **ui:** switch Quit to PredefinedMenuItem::quit (replaces 40a7f1d approach) ([4a0b19a](https://github.com/cameronzucker/tuxlink/commit/4a0b19a7a2076a043f0b4fdd811de85252848838))
* **ui:** tuxlink-r21 — handle File → Quit natively ([40a7f1d](https://github.com/cameronzucker/tuxlink/commit/40a7f1da70ec094e3c01c61981ceca85ad6d1ddf))
* **ui:** wizard sending-substate watchdog so a Busy-from-other never strands the window (tuxlink-9w8) ([ea314d8](https://github.com/cameronzucker/tuxlink/commit/ea314d88e7f85e66ce3d460d56205c6ee6f42345))
* **window:** recoverable close-to-tray + larger default window (tuxlink-9zd) ([39da45e](https://github.com/cameronzucker/tuxlink/commit/39da45e95e8db9faac0c1b15fdde7f865f2ed965))
* **winlink:** fail-fast CMS connect + progress logging (tuxlink-gqo) ([0de773a](https://github.com/cameronzucker/tuxlink/commit/0de773ac8d6135ab2b8caf5bcd385f9d2ec4a4ad))
* **winlink:** reduce CMS handshake locator to broadcast precision (tuxlink-882) ([d3411ad](https://github.com/cameronzucker/tuxlink/commit/d3411add391ed895e8546d6a18e2b6330b4fac9b))
* **wizard:** add http_announce_timeout to merged PatSpawnOptions site ([45e2f6a](https://github.com/cameronzucker/tuxlink/commit/45e2f6a6963ba519d090e84d80e8de9faceb0801))
* **wizard:** address Codex pqg adrev — isolate test-send Pat, harden gate/connect/reply (tuxlink-pqg) ([8775f15](https://github.com/cameronzucker/tuxlink/commit/8775f15637c07c457513d84f1bdf22361caf7cba))
* **wizard:** Codex pqg R2 — fail-closed under CI + document reply residual (tuxlink-pqg) ([da2a06f](https://github.com/cameronzucker/tuxlink/commit/da2a06f7239e2972df44679be43073502c7ce49d))
* **wizard:** query MOCKED-mode signal on mount to fix banner race (tuxlink-fzm) ([275b866](https://github.com/cameronzucker/tuxlink/commit/275b86650bc7d393fd9b5a000c7b119faee56e77))
* **wizard:** real Secret Service keyring backend + faithful cross-process test (tuxlink-1r5) ([5f269d9](https://github.com/cameronzucker/tuxlink/commit/5f269d97da694017adde096f56608d5c142b3841))
* **wizard:** style the first-run wizard + wire completion hand-off (tuxlink-dj6, tuxlink-eh7) ([1d1c01b](https://github.com/cameronzucker/tuxlink/commit/1d1c01baea506441fdad8032a0a17d678be7214d))
* **wizard:** Task 11 consent-state correctness — reducer-routed Retry, Busy no-op, MOCKED banner; reconcile §3.8 (click-exception) ([b45f592](https://github.com/cameronzucker/tuxlink/commit/b45f592d8d40b3cff21aab72ed357a65fc1a73dd))


### Refactors

* **ui:** port foundation to Mock D cool-slate system; lock §3 supersession ([09c910c](https://github.com/cameronzucker/tuxlink/commit/09c910c6b5ee940cb707f169646e3332ddb51427))
* **wizard:** live test-send failures return structured Ok(Failed) (tuxlink-2a7) ([0ae9dd5](https://github.com/cameronzucker/tuxlink/commit/0ae9dd5111d7f8b0dc76b2184e834c60b0088d0b))

## 0.0.1 (2026-05-21)

First tagged release. Tuxlink is a Linux-native desktop Winlink client for amateur-radio
email — a proper mail application for [Winlink](https://winlink.org/), where the prior
Linux options were a Windows client under WINE or [Pat](https://getpat.io/)'s web UI. The
milestone for this release: a new operator can install Tuxlink, complete the onboarding
wizard, send a Winlink CMS message, receive a reply, and never invoke Pat directly.

Built on Tauri 2 (Rust backend) with a React 18 / TypeScript frontend, distributed as a
Linux AppImage.

### Highlights

- **Onboarding wizard** — first-run setup for CMS-connected operation (callsign + Winlink
  CMS password) or an offline / radio-only identity. The CMS password is stored in the OS
  keyring (Secret Service) and never written to a config file on disk. An optional
  test-send verifies the round-trip before entering the mailbox.
- **Native Winlink CMS client** — a from-scratch Rust implementation of the Winlink
  session: telnet and TLS-wrapped transports, secure-login challenge/response, the FBB B2
  forwarding protocol with lzhuf compression, framed block transfer, and B2F message
  exchange — validated against the live Winlink CMS, backed by a Pat-independent on-disk
  message store.
- **Connect** — a one-click CMS exchange from the dashboard ribbon, with fail-fast connect
  timeouts, live per-step progress in the session log, and an Abort control for an
  in-flight connection.
- **Mailbox** — folder sidebar (Inbox / Sent / Outbox / Archive), a virtualized message
  list, a reading pane with RFC 5322 parsing, and read/unread tracking.
- **Compose** — author new messages and replies in a separate window with draft
  persistence.
- **Live session log** — a human-readable projection of the CMS session as it happens,
  plus a raw view.
- **Desktop integration** — custom dark application chrome (titlebar + menu bar) with
  keyboard accelerators, a system tray with close-to-tray, and selectable color schemes
  (night / tactical / grayscale).
- **Bundled Pat sidecar** — Tuxlink spawns and supervises the
  [tuxlink-pat](https://github.com/cameronzucker/tuxlink-pat) fork as a managed child
  process, so operators never run Pat directly.

### Not in this release

VARA HF / VARA FM, AX.25 / packet radio, and Hamlib rig control are deferred to v0.1+. See
[VERSIONING.md](VERSIONING.md) and the README roadmap for the full scope.
