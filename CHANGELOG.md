# Changelog

All notable changes to Tuxlink are documented here.

This project adheres to [Semantic Versioning](https://semver.org) with project-specific rules described in [VERSIONING.md](VERSIONING.md). Entries from `v0.0.2` onward are generated automatically by [`release-please`](https://github.com/googleapis/release-please) from [Conventional Commits](https://www.conventionalcommits.org).

## Unreleased

### Changed
- Request Center — visual redesign: request-first location hero, compact category chips, line-icon set, and true-window proportions.

### Added
- Request Center: a full-screen, request-first workspace that replaces the
  separate Catalog Request and GRIB File Request panels (Message → Request
  Center…). Location-aware request cards (state forecast, marine forecast,
  propagation, solar-terrestrial, aurora, public gateway lists, Winlink info)
  resolve from the operator's grid square; a three-pane category browser and a
  catalog-wide search request any of the bundled catalog items; the demoted
  GRIB form composes a Saildocs request. Selected items collect in a unified
  request basket that dispatches per rail on "Send all" — one inquiry message
  to the CMS for all catalog items, one Saildocs request per GRIB item — and
  surfaces a per-rail result. The GRIB File Request menu entry now deep-links
  into the Request Center's GRIB form.
- Alpha-logging infrastructure: structured `tracing`-based diagnostic logging,
  exported as a single `.tar.zst` archive via `Help → Logging → Export logs…`
  or auto-attached via `Help → Report Issue`. Six environment probes (keyring,
  audio, serial, modem-process, network, display) capture system state at
  startup and on errors. Detailed-mode toggle (Off / On / Bounded for N hours)
  controls per-target verbosity. Retention configurable from 1 day / 50 MB up
  to 365 days / 10 GB. Logs live at `$XDG_STATE_HOME/tuxlink/logs/`.
  Spec: `docs/superpowers/specs/2026-06-04-alpha-logging-design.md`.

## [0.47.0](https://github.com/cameronzucker/tuxlink/compare/v0.46.0...v0.47.0) (2026-06-10)


### Features

* **catalog:** decode NWS area-weather replies into forecast table + zone sections (tuxlink-qyjr) ([74ac4c3](https://github.com/cameronzucker/tuxlink/commit/74ac4c390e4d716ec987e55226d223b9e12b37e4))


### Bug Fixes

* **catalog:** harden area-weather parser per Codex review (tuxlink-qyjr) ([d51193d](https://github.com/cameronzucker/tuxlink/commit/d51193d32d9ff00d930cc51b42c49fdc21193f34))
* **mailbox:** align store() search-index unread seed with list() predicate (tuxlink-mzm4) ([3955552](https://github.com/cameronzucker/tuxlink/commit/3955552b0326347bbb6f54a280d0f109821503f9))

## [0.46.0](https://github.com/cameronzucker/tuxlink/compare/v0.45.0...v0.46.0) (2026-06-10)


### Features

* **uninstall:** expose cleanup flow in app ([a3422b8](https://github.com/cameronzucker/tuxlink/commit/a3422b859fa1c3fc6c2d77fdeb0cfde4cd16cd7b))

## [0.45.0](https://github.com/cameronzucker/tuxlink/compare/v0.44.0...v0.45.0) (2026-06-10)


### Features

* **compose:** Position expand-to-overlay picker + precision selector (tuxlink-sdbd) ([d47bfac](https://github.com/cameronzucker/tuxlink/commit/d47bfac8114b73d6656f7d05441a60501e6aab44))

## [0.44.0](https://github.com/cameronzucker/tuxlink/compare/v0.43.2...v0.44.0) (2026-06-10)


### Features

* **catalog:** pin-on-map location picker in Find a Gateway (tuxlink-3iav) ([9c86900](https://github.com/cameronzucker/tuxlink/commit/9c869005aa79d0c0835c6367f29215176e284744))
* **map:** mount LAN tile-source settings at Tools → Settings → Map tiles… (tuxlink-a1cc) ([8528f23](https://github.com/cameronzucker/tuxlink/commit/8528f23a87282ade89cc6d0097ed0672ffaebd3f))


### Bug Fixes

* **favorites:** stop ARDOP favorites list overlapping panel controls (tuxlink-sm22) ([602a77f](https://github.com/cameronzucker/tuxlink/commit/602a77f9a3a6f918c220751f326ff925a6b125ec))
* **mailbox:** suppress sent folder total badge ([30ce669](https://github.com/cameronzucker/tuxlink/commit/30ce669a5595a516e9619b18adb8c97a5ca05abd))
* **mailbox:** suppress Sent folder total badge ([1ef915e](https://github.com/cameronzucker/tuxlink/commit/1ef915e64d5c73940d26033d4b5aa26ca6f3475a))
* **radio:** preserve live log scrollback ([ce01f9a](https://github.com/cameronzucker/tuxlink/commit/ce01f9ace51854fd52d1cd7cb795bcd353c7659b))
* **radio:** preserve live log scrollback ([f3a129f](https://github.com/cameronzucker/tuxlink/commit/f3a129f6b1b3f2ec8174d7b83a01b13bab904625))
* **theme:** polish repository dark state colors ([ad5dbfb](https://github.com/cameronzucker/tuxlink/commit/ad5dbfb3c31c41ac1ec80996b2654e59ace49aad))
* **theme:** polish Repository Dark state colors ([833fb76](https://github.com/cameronzucker/tuxlink/commit/833fb76c8fd59126863b061a9b3c4ecd4f275281))

## [0.43.2](https://github.com/cameronzucker/tuxlink/compare/v0.43.1...v0.43.2) (2026-06-10)


### Bug Fixes

* **favorites:** align selector with radio panel controls ([6e9d5f2](https://github.com/cameronzucker/tuxlink/commit/6e9d5f2f918cbd52291fb16898cebb692825b86b))
* **mailbox:** gate form replies on parsed payload ([570c4f1](https://github.com/cameronzucker/tuxlink/commit/570c4f14ee3128f9484dd951b0770498008e868e))
* **mailbox:** preserve bare B2F catalog senders ([c29a101](https://github.com/cameronzucker/tuxlink/commit/c29a10137ef53b3a72f37d0352ac663d92761fe4))
* **theme:** strengthen high contrast light tokens ([12dd8ad](https://github.com/cameronzucker/tuxlink/commit/12dd8ad290ef3895b453f7ec8a6baa4070627d87))

## [0.43.1](https://github.com/cameronzucker/tuxlink/compare/v0.43.0...v0.43.1) (2026-06-10)


### Bug Fixes

* **logging:** keep local session log tail unredacted ([6e52bcb](https://github.com/cameronzucker/tuxlink/commit/6e52bcb340d7947937af6a7d179a4649e2f2a9dd))

## [0.43.0](https://github.com/cameronzucker/tuxlink/compare/v0.42.2...v0.43.0) (2026-06-10)


### Features

* **uninstall:** add user-owned cleanup flow ([4c9db54](https://github.com/cameronzucker/tuxlink/commit/4c9db54c30ab5e279cf429531eca9ebdc919fee9))
* **uninstall:** add user-owned cleanup flow (tuxlink-qbej) ([f06db36](https://github.com/cameronzucker/tuxlink/commit/f06db36747b832a47fcb3a9f51cc1ea37e7f1918))


### Bug Fixes

* **ci:** correct release-please Cargo.lock jsonpath so the lock version actually bumps (tuxlink-sglu) ([303a22b](https://github.com/cameronzucker/tuxlink/commit/303a22b39fdc843ed7a5fe803096cb7c99df6d9f))
* **uninstall:** satisfy cleanup CLI clippy lint (tuxlink-qbej) ([4ea566e](https://github.com/cameronzucker/tuxlink/commit/4ea566e9b0992e61220e757a13b2fdde1dc952f2))

## [0.42.2](https://github.com/cameronzucker/tuxlink/compare/v0.42.1...v0.42.2) (2026-06-10)


### Bug Fixes

* **help:** report-issue opens the Bug report template, not a blank issue (tuxlink-uhpn) ([ac6f854](https://github.com/cameronzucker/tuxlink/commit/ac6f8546222efba5f2164995ddec88b7b856157d))
* **release:** sync version sources to 0.42.1 + wire release-please ([41bbb40](https://github.com/cameronzucker/tuxlink/commit/41bbb404e3762da485f4d97b3f790c4e545337ad))

## [0.42.1](https://github.com/cameronzucker/tuxlink/compare/v0.42.0...v0.42.1) (2026-06-10)


### Bug Fixes

* **about:** update product status to Alpha ([3c99b6d](https://github.com/cameronzucker/tuxlink/commit/3c99b6db4bb444d280454b2bc10fd594c135c77a))

## [0.42.0](https://github.com/cameronzucker/tuxlink/compare/v0.41.1...v0.42.0) (2026-06-10)


### Features

* add practical dark theme presets ([5e57537](https://github.com/cameronzucker/tuxlink/commit/5e575374b71b9a9f9b6cb24ad0fca01f41f43d13))


### Bug Fixes

* **catalog:** make gateway results selectable and toggle favorites ([471e3d6](https://github.com/cameronzucker/tuxlink/commit/471e3d63cf1ff1b73de9dbc0f0971333e0d682f2))
* **catalog:** make gateway results selectable and toggle favorites ([8f91768](https://github.com/cameronzucker/tuxlink/commit/8f9176811e6933c1459665937446558ce7145230))

## [0.41.1](https://github.com/cameronzucker/tuxlink/compare/v0.41.0...v0.41.1) (2026-06-09)


### Bug Fixes

* **radio:** place gateway finder in panel command row ([5abe40d](https://github.com/cameronzucker/tuxlink/commit/5abe40db054582914e144984baad4ab4c18f6507))
* **shell:** stabilize ribbon connection slot ([90a6970](https://github.com/cameronzucker/tuxlink/commit/90a6970a10e4467d9c5d83f0595c934868d06927))

## [0.41.0](https://github.com/cameronzucker/tuxlink/compare/v0.40.0...v0.41.0) (2026-06-09)


### Features

* **mailbox:** selection-aware context menu + bulk Archive/Move (tuxlink-l80q) ([dd750ee](https://github.com/cameronzucker/tuxlink/commit/dd750eea2faa03947c09f4e1869105b5d5b00dde))
* **map:** BaseMap tile layer over raster; validated zoom raise (C11 widened) ([ac498cf](https://github.com/cameronzucker/tuxlink/commit/ac498cf7eeecc2f9fa17c847e698208a7a726701))
* **map:** cancel in-flight tiles on view change + partial-state tile layer ([6dd79f0](https://github.com/cameronzucker/tuxlink/commit/6dd79f05bcfd5416693352a0d2d1a5fe4673061c))
* **map:** expose validated-tile gate for 6-char precision ([922e855](https://github.com/cameronzucker/tuxlink/commit/922e855dce1441c3ba6461fa2f57098ed4d97b4a))
* **map:** re-tune Maidenhead lattice for full zoom range ([58d07a0](https://github.com/cameronzucker/tuxlink/commit/58d07a0ec87a78e337ab9874d644fe0da13da103))
* **map:** standalone tile-source provenance status pill (a1cc consumes) ([8a7599f](https://github.com/cameronzucker/tuxlink/commit/8a7599f7852bd84324f42881a49089a9f61909ea))
* **map:** tile layer bridge (stock TileLayer over tile:// scheme) ([466a3cf](https://github.com/cameronzucker/tuxlink/commit/466a3cf7b0304d5c33207bba91f2da9325bc3fae))
* **map:** tile-source TS types + invoke wrappers ([0b6a604](https://github.com/cameronzucker/tuxlink/commit/0b6a6048856dbadad717384d72efc8953e163433))
* **settings:** map tile source configuration UI ([07d4ea8](https://github.com/cameronzucker/tuxlink/commit/07d4ea8b399203170740d569a9c3a2d4d6d30330))
* **tiles:** bounded LRU tile cache + clear/purge ([ce0d307](https://github.com/cameronzucker/tuxlink/commit/ce0d3073a4dc7688e65c230a9f2ea3c9de96d923))
* **tiles:** cache only verified images via atomic temp+rename ([ae42530](https://github.com/cameronzucker/tuxlink/commit/ae42530d7e9441ce372c4dc4a13597a410b415b7))
* **tiles:** configure/test/clear/status tile commands ([52ebea7](https://github.com/cameronzucker/tuxlink/commit/52ebea7a318ee3673f9ded62472ccedbddd43840))
* **tiles:** per-source cache namespace + traversal-safe paths ([f6755a5](https://github.com/cameronzucker/tuxlink/commit/f6755a5446655adc716cdca6fe27e0687dd625c3))
* **tiles:** persist map tile source config ([6f51ded](https://github.com/cameronzucker/tuxlink/commit/6f51ded4871dd43717c347b8910ab636419b645f))
* **tiles:** serve tiles via tile:// URI scheme; +1 img-src token (tile:) ([fc6d290](https://github.com/cameronzucker/tuxlink/commit/fc6d2900f024cd05edf003d23c2330876387f7fe))
* **tiles:** single-flight tile de-duplication ([558a223](https://github.com/cameronzucker/tuxlink/commit/558a22319aa405c8971ea1bd875b6b0024339a22))
* **tiles:** source circuit-breaker + lazy zoom-raise ([effa0b7](https://github.com/cameronzucker/tuxlink/commit/effa0b7eb655314dd951b7e34bcfc39bc3270615))
* **tiles:** TileGatekeeper managed state ([5ec5478](https://github.com/cameronzucker/tuxlink/commit/5ec54786be4ca329a24711ac1bb78ecd45c23d28))


### Bug Fixes

* **catalog:** repair invoke-mock type signature in CatalogBuilderPanel test ([4a4d76e](https://github.com/cameronzucker/tuxlink/commit/4a4d76e77d14f0637b6ab84722c54a13d3ecae9d))
* **mailbox:** address Codex P2 review — self-move data-loss guard + selection cleanup (tuxlink-l80q) ([24a7cc2](https://github.com/cameronzucker/tuxlink/commit/24a7cc2f40d9ee10cedd566f6d1ba6d94baaf4c5))
* **tiles:** gate CRS probe egress, no_proxy all clients, cap probe body ([332c8ba](https://github.com/cameronzucker/tuxlink/commit/332c8ba0f8e2da81cdc6a04a7d3333e7218572b3))
* **tiles:** geodetic x-bound 2^(z+1) + refuse caching over-budget tiles ([5fca5f9](https://github.com/cameronzucker/tuxlink/commit/5fca5f908925753a14d642ff884d8ebbe5f47aed))
* **tiles:** reject-biased CROSS-FIELD CRS scan + geodetic_tile_index z-guard ([9bd153b](https://github.com/cameronzucker/tuxlink/commit/9bd153bef61e3cb36e8eac098878eea2542c8ef6))
* **tiles:** serialize per-namespace cache critical section (concurrency BLOCKER) ([5c712fa](https://github.com/cameronzucker/tuxlink/commit/5c712fabb41d0964c5fda7ad2368b5699cf482e4))

## [0.40.0](https://github.com/cameronzucker/tuxlink/compare/v0.39.3...v0.40.0) (2026-06-09)


### Features

* **mailbox:** add in-message find ([58282ba](https://github.com/cameronzucker/tuxlink/commit/58282ba9256a44c9bb554736d980dde0bd769f0b))


### Bug Fixes

* **compose:** gate FZ-M1 compact mode on a touch pointer, not viewport width alone ([fabc408](https://github.com/cameronzucker/tuxlink/commit/fabc408a940a5badf0c1e8008993e14d7bb23e8f))

## [0.39.3](https://github.com/cameronzucker/tuxlink/compare/v0.39.2...v0.39.3) (2026-06-09)


### Bug Fixes

* **mailbox:** print webview form fallback content ([a378766](https://github.com/cameronzucker/tuxlink/commit/a37876646ff649e256932d3b7a7641e60a351ebc))
* **scripts:** refuse a repo-root target/ in converge-build, ignore-rule-independent ([5356efd](https://github.com/cameronzucker/tuxlink/commit/5356efd0c86dc736b27e713908103eb0be235af6))
* **ui:** stack inbound selection above app chrome ([dd7a9d4](https://github.com/cameronzucker/tuxlink/commit/dd7a9d4db3aa013ecd3333fcc9f80519178c6d7c))

## [0.39.2](https://github.com/cameronzucker/tuxlink/compare/v0.39.1...v0.39.2) (2026-06-09)


### Bug Fixes

* **mailbox:** address 5 Codex P2 findings — search invalidation, Enter-clears-selection, bulk id filter, archived-sent read-state, mark-on-open guard reset (tuxlink-etxt) ([5333411](https://github.com/cameronzucker/tuxlink/commit/5333411470c34156a245196bb570f2076a176c67))
* **test:** type the invoke mock-call access in the Fix 5 mark-on-open test (tuxlink-kuhk) ([817e2fd](https://github.com/cameronzucker/tuxlink/commit/817e2fddbf7c5462eb03f6e2090756f6371ac553))

## [0.39.1](https://github.com/cameronzucker/tuxlink/compare/v0.39.0...v0.39.1) (2026-06-09)


### Bug Fixes

* **catalog:** stack Find a Gateway above app chrome so its controls stay reachable ([9d01ef9](https://github.com/cameronzucker/tuxlink/commit/9d01ef925a68ef689c6d88888c0f686c01cc4fd6))
* **test:** await async grid before asserting map-mount --active (C9 flake) ([4653b14](https://github.com/cameronzucker/tuxlink/commit/4653b14f48455d71808cf282379eae6834110374))

## [0.39.0](https://github.com/cameronzucker/tuxlink/compare/v0.38.1...v0.39.0) (2026-06-09)


### Features

* **catalog:** relocate Find a Gateway to the radio panels + Tools; split out info requests ([8c0c58a](https://github.com/cameronzucker/tuxlink/commit/8c0c58a3e8a0ce4ad8d54944384b30c8da41df43))
* **ui:** default inbound review ON + move the control to the dashboard ribbon ([e53cd79](https://github.com/cameronzucker/tuxlink/commit/e53cd79d4694ec700f4ff23b9cc4727507330bc0))

## [0.38.1](https://github.com/cameronzucker/tuxlink/compare/v0.38.0...v0.38.1) (2026-06-09)


### Bug Fixes

* **catalog:** add backdrop-click and Escape dismiss to Find a Gateway ([ff0f024](https://github.com/cameronzucker/tuxlink/commit/ff0f024970c1dd48906406fc638493250342f9a7))
* **favorites:** make Telnet Manual-only and align active-tab to modem accent ([3b6759b](https://github.com/cameronzucker/tuxlink/commit/3b6759b01239ba8594dcaeb6ec7edd7895d3e93d))
* **shell:** span the Contacts surface across the list + reader tracks ([eb34224](https://github.com/cameronzucker/tuxlink/commit/eb34224d2f267c1e6e7613235698b5c264583ddc))

## [0.38.0](https://github.com/cameronzucker/tuxlink/compare/v0.37.1...v0.38.0) (2026-06-09)


### ⚠ BREAKING CHANGES

* **compose:** Position map now uses a bundled offline world map instead of online OpenStreetMap tiles.

### Features

* **grib:** map-based region selection (item 21, tuxlink-mxmx) ([448b0cf](https://github.com/cameronzucker/tuxlink/commit/448b0cfe9a279c1047a053ab042b0bb6efeb53e7))
* **map:** BaseMap offline EPSG4326 substrate + shared leaflet icon fix + canonical test mock ([59f2e88](https://github.com/cameronzucker/tuxlink/commit/59f2e8845e229f4a78e445b104dd7b46124e55b9))
* **map:** GridMapPicker pin + box-drag modes ([e1fd645](https://github.com/cameronzucker/tuxlink/commit/e1fd645dfd12348007b2effe6a54d84fa9b3b5d8))
* **map:** pure EPSG4326 projection helpers ([e2c4615](https://github.com/cameronzucker/tuxlink/commit/e2c46158703bfef482c007d33e37bdb801d32a9f))
* **map:** pure maidenhead overlay geometry ([f86b06c](https://github.com/cameronzucker/tuxlink/commit/f86b06c1e1973892377d23137d8e50ddbef9fd73))
* **map:** pure signed-bbox→GRIB region normalizer ([a8d13a7](https://github.com/cameronzucker/tuxlink/commit/a8d13a7dd003020d78f50c50561cca6c3de3f6fd))
* **map:** toggleable maidenhead grid overlay ([99d940a](https://github.com/cameronzucker/tuxlink/commit/99d940afdecdcaaeab0d7da3823b55af6f3c2795))
* **map:** vendor public-domain equirectangular world map asset ([0df33c4](https://github.com/cameronzucker/tuxlink/commit/0df33c465b24f83341f1dc2900df551bcaec5f21))


### Bug Fixes

* **compose:** remove public-OSM tiles; use bundled offline map (tuxlink-714t) ([2a6c004](https://github.com/cameronzucker/tuxlink/commit/2a6c004909cd26e83d18cd7d5b5ad99918c8b995))
* **map:** address Codex adversarial review (4 P2 correctness/UX defects) ([c75c81f](https://github.com/cameronzucker/tuxlink/commit/c75c81f260ece2f841b6472c5c23b005746dfcb2))
* **winlink:** gate inbound-review prompt on fresh disk preference, not stale live_config ([de32878](https://github.com/cameronzucker/tuxlink/commit/de328785ef746c0d8a3973f8563ab3223d63aa9c))

## [0.37.1](https://github.com/cameronzucker/tuxlink/compare/v0.37.0...v0.37.1) (2026-06-08)


### Bug Fixes

* **shell:** align compact ribbon GridEdit source cluster (tuxlink-813d) ([5d1d5d3](https://github.com/cameronzucker/tuxlink/commit/5d1d5d32a78953d5258c39c4246f2c58e7725923))
* **shell:** align ribbon values across SSID-picker/segment + text cells (tuxlink-813d) ([958678b](https://github.com/cameronzucker/tuxlink/commit/958678b3f6843f7a3976eb06c25a5d7caad7dda2))
* **shell:** compact ribbon grows to fit its 44px touch controls (tuxlink-813d) ([0ff6001](https://github.com/cameronzucker/tuxlink/commit/0ff6001e35e1622f9f63a2f31da7a02a64e923dc))
* **shell:** FZ-M1 compact drawer auto-open, grip tab, ribbon alignment (tuxlink-813d) ([1315af2](https://github.com/cameronzucker/tuxlink/commit/1315af2bb49c7cd18fbecbb7f4ee946a030f1ec1))
* **shell:** gate compact rail behind isCompact; restore desktop sidebar (tuxlink-813d) ([6ac89e1](https://github.com/cameronzucker/tuxlink/commit/6ac89e1838f94831e19f774d6ad1f9d371f041e9))

## [0.37.0](https://github.com/cameronzucker/tuxlink/compare/v0.36.0...v0.37.0) (2026-06-08)


### ⚠ BREAKING CHANGES

* **winlink:** credentials stored only under the legacy "tuxlink-pat" keyring service are no longer auto-migrated; re-enter the CMS password if prompted.

### Bug Fixes

* **winlink:** remove tuxlink-pat legacy keyring service; read canonical tuxlink ([841ff62](https://github.com/cameronzucker/tuxlink/commit/841ff62932b8b0edd5a6e61b68ff4af189028c7c))

## [0.36.0](https://github.com/cameronzucker/tuxlink/compare/v0.35.13...v0.36.0) (2026-06-08)


### Features

* **catalog:** location-aware builder UI + reply view (frontend) ([7b6489e](https://github.com/cameronzucker/tuxlink/commit/7b6489ec51a42bf5df99d2d59384f56d47fafd51))
* **catalog:** mount builder via Find a Gateway menu + route catalog replies in reader ([c51b765](https://github.com/cameronzucker/tuxlink/commit/c51b765e0e3fb4cf0090f089215bb8f849c89731))
* **catalog:** station-list direct poll + reply parse-with-fallback (Rust) ([14680a0](https://github.com/cameronzucker/tuxlink/commit/14680a0df9de5eb6bd863faf0b14b754a4d7ba0d))
* **mailbox,radio:** compact icon rail + radio interior touch/floors (tuxlink-h7q7) ([645d1cd](https://github.com/cameronzucker/tuxlink/commit/645d1cdcf8c113d440236c1c29f790b09dd3cad6))
* **shell:** compact mode core — useViewport, push radio drawer, rail, chrome (tuxlink-h7q7) ([4f782d5](https://github.com/cameronzucker/tuxlink/commit/4f782d5ae67458f18c77a8cab68e930054a5d122))
* **shell:** useViewport compact-mode hook + shared breakpoint constant (tuxlink-h7q7) ([61a893f](https://github.com/cameronzucker/tuxlink/commit/61a893f4425df38547f8ebde7841c564402dbfc9))
* **ui:** FZ-M1 compact CSS for Compose, dialogs, wizard, forms (tuxlink-h7q7) ([9509b4b](https://github.com/cameronzucker/tuxlink/commit/9509b4bbedbf4fc3eb60244a0dc67d379fd53314))


### Bug Fixes

* **catalog:** address Codex post-impl diff review (4× P2) ([c60a564](https://github.com/cameronzucker/tuxlink/commit/c60a564696386d7b51114d44cba62b3de01a0449))
* **catalog:** clippy --all-targets -D warnings clean ([8f9ecda](https://github.com/cameronzucker/tuxlink/commit/8f9ecda6e647cee96b39d035f912194261103661))
* **compose:** clamp window default height to monitor work area for FZ-M1 (tuxlink-h7q7) ([b4a6496](https://github.com/cameronzucker/tuxlink/commit/b4a6496893085918122e7f6006cc7c7c120e92ca))
* **radio:** compact touch floors for small controls missed in 6b (tuxlink-h7q7) ([497ceb5](https://github.com/cameronzucker/tuxlink/commit/497ceb5b57200caeb80811d68518240999dade83))

## [0.35.13](https://github.com/cameronzucker/tuxlink/compare/v0.35.12...v0.35.13) (2026-06-08)


### Bug Fixes

* **compose:** show offline identifier in read-only From field ([2204c09](https://github.com/cameronzucker/tuxlink/commit/2204c096a7f6c4a57d70d63b0a99c7ff8c0f9b89))
* **forms:** cap standout native forms at a readable width ([9d65154](https://github.com/cameronzucker/tuxlink/commit/9d65154b8a87b1cf92d688ee006c7635fa64e91e))
* **logging:** list per-message movement on Telnet P2P exchanges ([030330d](https://github.com/cameronzucker/tuxlink/commit/030330d00690747d9555b326bb8abef635ad77f1))
* **mailbox:** decode mixed-encoding B2F bodies byte-wise ([56a3346](https://github.com/cameronzucker/tuxlink/commit/56a33462ee3d1891cfadf34624077cf57ffd4307))
* **shell:** reflect active ARDOP/VARA transport in ribbon idle label ([c12a53b](https://github.com/cameronzucker/tuxlink/commit/c12a53b7cbd4cf00316620b76a1209dfa2b47ae2))

## [0.35.12](https://github.com/cameronzucker/tuxlink/compare/v0.35.11...v0.35.12) (2026-06-07)


### Bug Fixes

* **ci:** satisfy clippy in session log progress ([b9cd3fd](https://github.com/cameronzucker/tuxlink/commit/b9cd3fd32ee60e9f71857307630d0cad4d4f56fc))
* **logging:** summarize B2F message movement ([2e98a10](https://github.com/cameronzucker/tuxlink/commit/2e98a1075babd8365bd1b8ebb277492acbbf213a))
* **mailbox:** preview image attachments ([173188b](https://github.com/cameronzucker/tuxlink/commit/173188bd1e14822e9608d9ebcf22b51b673cb7bb))

## [0.35.11](https://github.com/cameronzucker/tuxlink/compare/v0.35.10...v0.35.11) (2026-06-07)


### Bug Fixes

* **logging:** retain complete transport log history ([a9c7ddc](https://github.com/cameronzucker/tuxlink/commit/a9c7ddc39945bdfd98edfe1abb6cd9027a8691ad))
* **shell:** derive ribbon local time from grid ([4cdc371](https://github.com/cameronzucker/tuxlink/commit/4cdc37190c5b806effcff0f6d5cad13db5f8b9e7))

## [0.35.10](https://github.com/cameronzucker/tuxlink/compare/v0.35.9...v0.35.10) (2026-06-07)


### Bug Fixes

* **about:** replace placeholder product copy ([d2ab4c4](https://github.com/cameronzucker/tuxlink/commit/d2ab4c437df729a6301fe1059c71dab4b5911c86))
* **compose:** show configured From identity ([51d78f7](https://github.com/cameronzucker/tuxlink/commit/51d78f774be2214bd477b98966daccf2f04cd4e3))
* **menu:** move Print to File menu ([239197e](https://github.com/cameronzucker/tuxlink/commit/239197ea25abf621ff68807cbd8f8d9152d36017))
* **radio:** clarify AX.25 serial baud control ([2a6b550](https://github.com/cameronzucker/tuxlink/commit/2a6b5505321eab3c98bf4948f4bce9710319297f))

## [0.35.9](https://github.com/cameronzucker/tuxlink/compare/v0.35.8...v0.35.9) (2026-06-07)


### Bug Fixes

* **shell:** commit grid edit on blur ([3a62ad1](https://github.com/cameronzucker/tuxlink/commit/3a62ad18d63bbe10989bd6a4b6d5d3066b226512))

## [0.35.8](https://github.com/cameronzucker/tuxlink/compare/v0.35.7...v0.35.8) (2026-06-07)


### Bug Fixes

* **logging:** repair logging window chrome and clear history ([9da09d6](https://github.com/cameronzucker/tuxlink/commit/9da09d60d906d4801a851af104f3fa7547f58509))

## [0.35.7](https://github.com/cameronzucker/tuxlink/compare/v0.35.6...v0.35.7) (2026-06-07)


### Bug Fixes

* restore logging session transcript boundary ([2f78e65](https://github.com/cameronzucker/tuxlink/commit/2f78e651ae57eda4599200fa865eb636c4c9922f))

## [0.35.6](https://github.com/cameronzucker/tuxlink/compare/v0.35.5...v0.35.6) (2026-06-07)


### Bug Fixes

* **cargo:** keep xtask out of root workspace ([1327fbb](https://github.com/cameronzucker/tuxlink/commit/1327fbb3d49a0b2dd5efa30e9862cfd0b4b6bcb5))

## [0.35.5](https://github.com/cameronzucker/tuxlink/compare/v0.35.4...v0.35.5) (2026-06-07)


### Bug Fixes

* **logging:** keep startup diagnostics out of connection log ([d9d71f4](https://github.com/cameronzucker/tuxlink/commit/d9d71f4fecfa43b9ae86e7bba8c72459669db637))
* make tracing-to-session-log opt-in via session_log=true and restrict bootstrap synthetic backend session-log lines to warn/error. Update the alpha-logging spec to preserve that surface boundary. ([d9d71f4](https://github.com/cameronzucker/tuxlink/commit/d9d71f4fecfa43b9ae86e7bba8c72459669db637))

## [0.35.4](https://github.com/cameronzucker/tuxlink/compare/v0.35.3...v0.35.4) (2026-06-07)


### Bug Fixes

* **logging:** spawn startup workers on tauri runtime ([1014bad](https://github.com/cameronzucker/tuxlink/commit/1014bad67a57d92a92e9d9865b37644de9b61798))

## [0.35.3](https://github.com/cameronzucker/tuxlink/compare/v0.35.2...v0.35.3) (2026-06-06)


### Bug Fixes

* **about:** show GPL license metadata ([6a6a648](https://github.com/cameronzucker/tuxlink/commit/6a6a64877a8a7d2a5289ed76efb5f09a1acc0d4e))
* **about:** show GPL license metadata ([5bb85a1](https://github.com/cameronzucker/tuxlink/commit/5bb85a1bfd0aaf03a7fce935eb9b555e53436150))
* **ci:** restore alpha logging pipeline ([edc0a15](https://github.com/cameronzucker/tuxlink/commit/edc0a15e0a9639dd6051be8b10b0155b14189e1e))
* **ci:** workspace target paths + menuModel expected actions list ([42ecbcf](https://github.com/cameronzucker/tuxlink/commit/42ecbcf6dfb436711dbf65c9012b5569f69ba0f5))
* **mailbox:** refresh folders on native mailbox changes ([3570c0c](https://github.com/cameronzucker/tuxlink/commit/3570c0cab8247b88cedf62cd6d06dec6b6b23f3b))
* **mailbox:** refresh folders on native mailbox changes ([46bea41](https://github.com/cameronzucker/tuxlink/commit/46bea41037c92f49dafcf0019bc9d1f76c7b5395))
* **shell:** honor selected grid precision ([08f14be](https://github.com/cameronzucker/tuxlink/commit/08f14be9dfcd326b7a9f3007884980544adcddd2))
* **shell:** honor selected grid precision ([9b922ce](https://github.com/cameronzucker/tuxlink/commit/9b922ce9e9a9d7c7fd48a6974f49188f35df4b2e))
* **shell:** preserve active transport intent ([9b34938](https://github.com/cameronzucker/tuxlink/commit/9b34938c70713a5599f91c545e101ee37c3e3b1d))
* **shell:** preserve active transport intent ([b73ecc9](https://github.com/cameronzucker/tuxlink/commit/b73ecc98aa3e0131fdd3d2ab6a3cc0bf8b1eb403))

## [0.35.2](https://github.com/cameronzucker/tuxlink/compare/v0.35.1...v0.35.2) (2026-06-05)


### Bug Fixes

* **ui:** dispatch B2F messages to project parser instead of mail_parser (tuxlink-2hyf) ([5f0c14a](https://github.com/cameronzucker/tuxlink/commit/5f0c14aefa43c13d0345df980211a7fd4e1f35af))

## [0.35.1](https://github.com/cameronzucker/tuxlink/compare/v0.35.0...v0.35.1) (2026-06-05)


### Bug Fixes

* **ci:** clippy — unused PathBuf at lib + bool_assert_comparison in tests ([556b78e](https://github.com/cameronzucker/tuxlink/commit/556b78e307e8389917c33d907a1abaec5bd0236b))
* **shell:** preserve selectedConnection across folder switch (tuxlink-u4ky) ([b715b73](https://github.com/cameronzucker/tuxlink/commit/b715b7373550fe05f3dd96313d2842b01b5a362c))

## [0.35.0](https://github.com/cameronzucker/tuxlink/compare/v0.34.2...v0.35.0) (2026-06-05)


### Features

* **mailbox:** surface Winlink message ID in MessageView header (tuxlink-gtno) ([459abb4](https://github.com/cameronzucker/tuxlink/commit/459abb4cfb857048ab0e0e3b27ef78f347b6ca41))


### Bug Fixes

* **search:** empty-star color from --border-strong to --text-dim (tuxlink-ojr7) ([0384784](https://github.com/cameronzucker/tuxlink/commit/038478405783671bdfd82da1d134075483dc8530))
* **shell:** SSID picker min-width 72px → 60px (tuxlink-yn58) ([aa24018](https://github.com/cameronzucker/tuxlink/commit/aa24018e26993489f95a16376607812fa601bd03))

## [0.34.2](https://github.com/cameronzucker/tuxlink/compare/v0.34.1...v0.34.2) (2026-06-05)


### Bug Fixes

* **mailbox:** emit placeholder for binary message bodies (tuxlink-9ylw) ([5a9945d](https://github.com/cameronzucker/tuxlink/commit/5a9945da509821e637eec30b6e89d09662c8b1e0))

## [0.34.1](https://github.com/cameronzucker/tuxlink/compare/v0.34.0...v0.34.1) (2026-06-05)


### Bug Fixes

* **ci:** resolve clippy + example compile errors surfaced by main merge ([990ef24](https://github.com/cameronzucker/tuxlink/commit/990ef24416abb470113d061e96cce28edb375bbb))

## [0.34.0](https://github.com/cameronzucker/tuxlink/compare/v0.33.0...v0.34.0) (2026-06-05)


### Features

* **forms:** CheckInForm — WLE-aligned schema + OSM tile CSP allowlist (closes tuxlink-4ai0, tuxlink-bt2q) ([0918948](https://github.com/cameronzucker/tuxlink/commit/0918948e1de209571e4ae2f3274cc6556bb08e5e))


### Bug Fixes

* **forms:** apply Codex P1+P2 findings — CSP bare-host + WLE metadata + location-required + slot-leak ([3542e77](https://github.com/cameronzucker/tuxlink/commit/3542e7789be933131e609bd3e4fecccf011a051d))

## [0.33.0](https://github.com/cameronzucker/tuxlink/compare/v0.32.1...v0.33.0) (2026-06-05)


### Features

* **auth-taxonomy:** classify CMS payloads + transport errors per §3/§6.4 ([c4aa1ef](https://github.com/cameronzucker/tuxlink/commit/c4aa1ef5def1a6e1903b5f44516290eb1b4687a6))
* **b2f-events:** B2fEvent enum + B2fEventSink trait + serde-lockdown test ([109c8f5](https://github.com/cameronzucker/tuxlink/commit/109c8f5b867889ff256fce6af9bc7c3ce111a0db))
* **b2f-events:** scaffold AttemptId + FailureMode + TransportFailureKind types ([fd4a5fd](https://github.com/cameronzucker/tuxlink/commit/fd4a5fde1451caccbebe8f43d78efa53252e8acd))
* **banner:** AuthDiagnosticBanner component with 6 failure modes + 5 affordances + a11y ([4454d7c](https://github.com/cameronzucker/tuxlink/commit/4454d7c0891c030201d363bdde4b2268e810b369))
* **capabilities:** scope shell:open allowlist to winlink.org + tuxlink repo (R2 [#9](https://github.com/cameronzucker/tuxlink/issues/9)) ([016edac](https://github.com/cameronzucker/tuxlink/commit/016edac811c385abf5b960cc78c2ef5b88baa075))
* **copy:** banner headline + body mapping per spec §3/§4 (R5 revisions) ([1664b1b](https://github.com/cameronzucker/tuxlink/commit/1664b1b93e34919e07d0a6467f332c2280f6e400))
* **css:** AuthDiagnosticBanner styles matching RadioPanel palette + reduced-motion variant ([87b54b7](https://github.com/cameronzucker/tuxlink/commit/87b54b7d74fd8cee4d1de65b0ba4e1ae413a8eb4))
* **forms:** checkin template + catalog registration (Rust) ([0363de2](https://github.com/cameronzucker/tuxlink/commit/0363de2cfecfb36e15ebe81395133a27d772ddcd))
* **forms:** CheckInForm — native Winlink Check-In with PositionArbiter + slot library ([80c5ff5](https://github.com/cameronzucker/tuxlink/commit/80c5ff52510f5fcca00ddf5bdf97d791bd084d86))
* **forms:** FormDraftLibrary backend + TS wrapper for save/reuse slots ([2f71dab](https://github.com/cameronzucker/tuxlink/commit/2f71dab6ba8a4b4906fbfa7ee241233e51bb4e7d))
* **forms:** Ics309FormV2 — native ICS-309 comms log with messages_meta aggregation + PDF ([e38caad](https://github.com/cameronzucker/tuxlink/commit/e38caadef913cfc49d2e21b5025121260ed57f59))
* **forms:** messages_meta_query_for_log + render_ics309_pdf Tauri commands ([561c342](https://github.com/cameronzucker/tuxlink/commit/561c342861c0ce01cda4b183e116654cd7697e22))
* **forms:** position_current_fix Tauri command for PositionFormV2 ([583f90e](https://github.com/cameronzucker/tuxlink/commit/583f90e4d1218cebecc466c2ef06b9937eba6929))
* **forms:** PositionFormV2 — native Position Report with PositionArbiter pull ([452cd53](https://github.com/cameronzucker/tuxlink/commit/452cd5322b6f6d07831b9d92212b8c5a8768a7f0))
* **forms:** PositionMapWidget — Leaflet map for PositionFormV2 grid override ([717b76f](https://github.com/cameronzucker/tuxlink/commit/717b76f33d4dcc83af38c7c2bb838be15320995d))
* **forms:** wire FormDraftLibrary into Ics213Form and BulletinForm (slot save/load) ([d75bac6](https://github.com/cameronzucker/tuxlink/commit/d75bac6531630f05fe0d0b4bc4e20bf4d213c0a3))
* **forms:** wire FormDraftLibrary into PositionFormV2 (slot save/load) ([53a76ae](https://github.com/cameronzucker/tuxlink/commit/53a76aeb3079d67738a561ccc9c113b39f06a8c1))
* **handshake:** surface *** lines via HandshakeError::RemoteError (R3 [#3](https://github.com/cameronzucker/tuxlink/issues/3)) ([392496d](https://github.com/cameronzucker/tuxlink/commit/392496d386f86a282c47f4b82eb4d4348d8e5650))
* **hook:** useAuthDiagnostic subscribes to b2f-event + AttemptId correlation + retry counter + rate-limit ([e2922e4](https://github.com/cameronzucker/tuxlink/commit/e2922e46e991c4c8d55dccb55d34b52c9917a29a))
* **panel:** insert AuthDiagnosticBanner above SessionLogSection (spec §4.1) ([56e0ffd](https://github.com/cameronzucker/tuxlink/commit/56e0ffdd1ded4669966fa0c28354c97214a60052))
* **redaction:** add ;PQ symmetric + freeform scrubber for embedded tokens ([0c4527d](https://github.com/cameronzucker/tuxlink/commit/0c4527d360d5d1ea27ff5fa8d4102ba73b5b76f1))
* **redaction:** scaffold credential-equivalent redaction module + canonical ;PR test ([d09a92d](https://github.com/cameronzucker/tuxlink/commit/d09a92d61092e6308c3f7a4b943dd24573a693ea))
* **session:** additive run_exchange_with_events for auth diagnostics (§6.3) ([5ca19b4](https://github.com/cameronzucker/tuxlink/commit/5ca19b42396513455c45c47db098422e1b639a9a))
* **types:** add B2fEvent + FailureMode TS shapes mirroring Rust serde ([b559242](https://github.com/cameronzucker/tuxlink/commit/b5592429712187fdf8eb8e490d1cd98b707f42a5))
* **ui-commands:** add cms_connect_test (single-flight + auth-only contract) ([a5dc21d](https://github.com/cameronzucker/tuxlink/commit/a5dc21daa738d3505b727a09ba4557dbc5c3254d))
* **ui-commands:** add credentials_write_password + wizard_reopen + auth_diagnostic_clear ([bffe460](https://github.com/cameronzucker/tuxlink/commit/bffe4601dcbd80279dd363282522ffda50923926))
* **ui-commands:** add TauriEventSink + scaffold cms_connect event channel ([38dd4eb](https://github.com/cameronzucker/tuxlink/commit/38dd4eb0340d8b10dd367cf725a23218ad9906ea))
* **ui-commands:** classify cms_connect result + emit AuthClassified event ([8696abc](https://github.com/cameronzucker/tuxlink/commit/8696abcc012a261ca2827a8fd11478814f712a15))
* **urls:** add hardcoded winlink.org + tuxlink-repo URL constants (R2 [#9](https://github.com/cameronzucker/tuxlink/issues/9)) ([8ee7573](https://github.com/cameronzucker/tuxlink/commit/8ee75738859316268161c46ff1c73299fda552a1))


### Bug Fixes

* **ci:** address 3 clippy errors blocking PR [#391](https://github.com/cameronzucker/tuxlink/issues/391) build ([14cbfce](https://github.com/cameronzucker/tuxlink/commit/14cbfced73a00d629b50a32622537d44aafe2fd8))
* **forms:** CheckInForm review nits — useId for radio group + inline payload ([c2ade5f](https://github.com/cameronzucker/tuxlink/commit/c2ade5fe3b6d0e1aa292477983ac69e6231269d5))
* **forms:** Codex full-diff adrev P1+P2 findings — catalog id alias + Check-In schema deferral ([b22c533](https://github.com/cameronzucker/tuxlink/commit/b22c533c3dc28a6d66c8074b1b1c44aa8a0aac0f))
* **forms:** FormDraftLibrary review nits — camelCase IPC args + comment + test polish ([d4c6962](https://github.com/cameronzucker/tuxlink/commit/d4c696250bebdf7ea308f4731c9507a153755ba9))
* **forms:** PositionFormV2 — onChange in event handlers + inline grid error ([c1b122f](https://github.com/cameronzucker/tuxlink/commit/c1b122f6d09b6902c8fc46f955d912145077b783))
* **forms:** PositionFormV2 — wire-format payload + draft restore + no-fix UX ([bd35559](https://github.com/cameronzucker/tuxlink/commit/bd3555966110fc26663d5c2611897120e7e79530))
* **forms:** Task 4b review nits — CSS token + always-create intent comments ([58485a0](https://github.com/cameronzucker/tuxlink/commit/58485a04221d52ba18cfa4610324dde648eeee5f))
* **hook:** auto-clear test-creds circuit breaker after timeout (Codex MAJOR [#5](https://github.com/cameronzucker/tuxlink/issues/5)) ([09c4d43](https://github.com/cameronzucker/tuxlink/commit/09c4d4333954ed086166fee296047fe0ab17d167))
* **redaction:** handle embedded + lowercase + whitespace token variants (Codex BLOCKER [#1](https://github.com/cameronzucker/tuxlink/issues/1)) ([14d9c4f](https://github.com/cameronzucker/tuxlink/commit/14d9c4fd6fa6779762dcea29732dc38dd09bb5c3))
* **session:** thread caller-supplied AttemptId through run_exchange_with_events (Codex MAJOR [#2](https://github.com/cameronzucker/tuxlink/issues/2)) ([af6f1b9](https://github.com/cameronzucker/tuxlink/commit/af6f1b9f3d95365527d2c83dc8f91b051aa89363))
* **telnet:** patch shipped ;PR leak in WireTap → wire_log path (R2 [#1](https://github.com/cameronzucker/tuxlink/issues/1) BLOCKER) ([321e384](https://github.com/cameronzucker/tuxlink/commit/321e3841b3a6b979e7e7344ab9579de4517e52ca))
* **tests:** CI typecheck — type vi.fn mocks + drop unused STOPPED import ([5c56fa6](https://github.com/cameronzucker/tuxlink/commit/5c56fa6a90233d23183e98cfe5ea1dc13cdc2e9c))


### Refactors

* **credentials:** extract public write_password (R2 [#4](https://github.com/cameronzucker/tuxlink/issues/4)) ([2d9a001](https://github.com/cameronzucker/tuxlink/commit/2d9a00128a8d44b8c6c7f90ca87667a43c7fc692))

## [0.32.1](https://github.com/cameronzucker/tuxlink/compare/v0.32.0...v0.32.1) (2026-06-04)


### Bug Fixes

* **docs:** topic 06 mermaid sequence diagrams parse — remove `;` from message and note bodies ([a8918a1](https://github.com/cameronzucker/tuxlink/commit/a8918a1433922b4fe92c5ae39f55213736c19e16))

## [0.32.0](https://github.com/cameronzucker/tuxlink/compare/v0.31.0...v0.32.0) (2026-06-04)


### Features

* **tux-rig-watchdog:** PR_SET_PDEATHSIG belt-and-suspenders parent-death detection (tuxlink-a2z0) ([2f2030b](https://github.com/cameronzucker/tuxlink/commit/2f2030b313d1828fa3a29c939610958070714982))
* **tuxmodem-tx:** --watchdog flag spawns tux-rig-watchdog for SIGKILL-safe TX (tuxlink-8xfa, Phase 1.5 slice 2) ([62af099](https://github.com/cameronzucker/tuxlink/commit/62af09981053e2cd39b097d9df04a1847cf8ba3c))


### Bug Fixes

* **help:** mermaid theming wins ID-scoped specificity via !important ([a11a7ae](https://github.com/cameronzucker/tuxlink/commit/a11a7aec4659dd715125679ac3841e58465403c0))

## [0.31.0](https://github.com/cameronzucker/tuxlink/compare/v0.30.0...v0.31.0) (2026-06-04)


### Features

* **tux-rig-rts:** tux-rig-watchdog SIGKILL-safe PTT daemon (tuxlink-23ps, Phase 1.5) ([09f8702](https://github.com/cameronzucker/tuxlink/commit/09f87020168023fe9e218e62e8afa9f0333ca57d))

## [0.30.0](https://github.com/cameronzucker/tuxlink/compare/v0.29.0...v0.30.0) (2026-06-04)


### Features

* **tuxmodem-phy:** multi-symbol + preamble composition (tuxlink-k2xv, Phase 10 slice 2) ([5ce564e](https://github.com/cameronzucker/tuxlink/commit/5ce564e636131e01691f983e2aa7cea62cbcac95))
* **tuxmodem-phy:** multi-symbol framing primitive (tuxlink-cwjp, Phase 10 slice 1) ([23cfc93](https://github.com/cameronzucker/tuxlink/commit/23cfc93f7d2a805aeb7570a1e67d6608dad9496c))

## [0.29.0](https://github.com/cameronzucker/tuxlink/compare/v0.28.0...v0.29.0) (2026-06-04)


### Features

* **tuxmodem-phy:** preamble round-trip primitive (tuxlink-iyl9, Phase 12 slice 1) ([a2579e9](https://github.com/cameronzucker/tuxlink/commit/a2579e9fdf5266499a43fa331f98f0a9d02f4499))
* **tuxmodem-tx:** --write-wav PATH (encode to file, no device/PTT) — tuxlink-4dv9 ([8d43671](https://github.com/cameronzucker/tuxlink/commit/8d43671536d887a8b47c92c0a8060833ccb9a7df))

## [0.28.0](https://github.com/cameronzucker/tuxlink/compare/v0.27.0...v0.28.0) (2026-06-04)


### Features

* **tuxmodem-phy:** audio_device module + tuxmodem-audio-play bench CLI (tuxlink-h8pp) ([05391b7](https://github.com/cameronzucker/tuxlink/commit/05391b79c9e9ee5b64f500b8fbf83b206dbc32b3))

## [0.27.0](https://github.com/cameronzucker/tuxlink/compare/v0.26.2...v0.27.0) (2026-06-04)


### Features

* **tux-rig-cm108:** CM108-HID PTT primitive + CLI (tuxlink-u1js) ([9714c3d](https://github.com/cameronzucker/tuxlink/commit/9714c3d4e0160cd7c099b82570b7b54617f82354))


### Bug Fixes

* **help:** surgical mermaid CSS revert — invisible-diagram regression ([57aee1c](https://github.com/cameronzucker/tuxlink/commit/57aee1c7e090daf4fc8ff64bf375debc5c11fe07))
* **packaging:** drop reverse-DNS .desktop overlay (tuxlink-mpds) ([fcc4926](https://github.com/cameronzucker/tuxlink/commit/fcc49267be17d2519fe021b9fa0ece82d0061327))

## [0.26.2](https://github.com/cameronzucker/tuxlink/compare/v0.26.1...v0.26.2) (2026-06-04)


### Bug Fixes

* **help:** Mermaid contrast + search-result stale-slug navigation (tuxlink-b5oa) ([ebbef4c](https://github.com/cameronzucker/tuxlink/commit/ebbef4c813551ee493dd3be0ef12f646bde77605))

## [0.26.1](https://github.com/cameronzucker/tuxlink/compare/v0.26.0...v0.26.1) (2026-06-04)


### Bug Fixes

* **types:** shim marked-extended-tables (no types shipped) ([07b1fdb](https://github.com/cameronzucker/tuxlink/commit/07b1fdbfcfa684dff0ff2d20ff0d1e400a5b02d3))

## [0.26.0](https://github.com/cameronzucker/tuxlink/compare/v0.25.0...v0.26.0) (2026-06-04)


### Features

* **winlink/ardop:** ARDOP listener end-to-end (tuxlink-61yg) ([36936b8](https://github.com/cameronzucker/tuxlink/commit/36936b8cf70f42b514aba767bdd028d0bbc1871b))
* **winlink/telnet:** Telnet listener inbound mailbox symmetry (tuxlink-61yg) ([ea52c38](https://github.com/cameronzucker/tuxlink/commit/ea52c387d63152141bb93d6e412c996ca7c2d6d5))


### Bug Fixes

* **listener:** address Codex review findings on end-to-end PR (tuxlink-61yg) ([73df469](https://github.com/cameronzucker/tuxlink/commit/73df469b5cb93d39b11c485f672678b0307ad47a))
* **radio:** defend ModemLinkSection against undefined invoke responses (tuxlink-61yg) ([a897613](https://github.com/cameronzucker/tuxlink/commit/a8976133c69e96677f1365481c8cbf751510c0e5))
* **winlink/ardop:** clippy borrowed_box on arq_disconnect_via_cmd_writer (tuxlink-61yg) ([d156a42](https://github.com/cameronzucker/tuxlink/commit/d156a423e37209698d72ec9493e0177fddfa77d3))

## [0.25.0](https://github.com/cameronzucker/tuxlink/compare/v0.24.3...v0.25.0) (2026-06-03)


### Features

* **help:** widen reading column — Wide 980→1280 px, default to Wide (tuxlink-d7a7) ([8711908](https://github.com/cameronzucker/tuxlink/commit/87119088af67ab23a23bb747e90380ad918c8e34))

## [0.24.3](https://github.com/cameronzucker/tuxlink/compare/v0.24.2...v0.24.3) (2026-06-03)


### Bug Fixes

* **examples:** add missing initial_listen field to ardop_connect ([fd8cecd](https://github.com/cameronzucker/tuxlink/commit/fd8cecd82f78094fab23a8536bff011c8df0904e))
* **perf:** break infinite theme-broadcast loop pegging WebKit + Rust at idle (tuxlink-och6) [P0] ([a4329a5](https://github.com/cameronzucker/tuxlink/commit/a4329a58b2f61ecc42ca4d6bb81a0fc6c1859575))
* **tests:** add missing intent field to ExchangeConfig (3 sites) ([fc7de9e](https://github.com/cameronzucker/tuxlink/commit/fc7de9eadd38e7c50bb67e2ee54d52dcc90be55d))


### Refactors

* address 15 clippy lints surfaced by new CI gate ([62d8797](https://github.com/cameronzucker/tuxlink/commit/62d879725f32125ec075a6168b3422b5ea5b73b0))

## [0.24.2](https://github.com/cameronzucker/tuxlink/compare/v0.24.1...v0.24.2) (2026-06-03)


### Performance

* **help:** GPU-composite the reading pane + bottom-pad to 160 px (tuxlink-q5td) ([36f186c](https://github.com/cameronzucker/tuxlink/commit/36f186c4f86dc2f01f4795952a0e7a66256e2afd))

## [0.24.1](https://github.com/cameronzucker/tuxlink/compare/v0.24.0...v0.24.1) (2026-06-03)


### Bug Fixes

* **shell:** wrap QueryClientProvider around all App.tsx routing branches (tuxlink-n4hz) ([2ac208b](https://github.com/cameronzucker/tuxlink/commit/2ac208b368e355e3f575999415f0cf54647abfee))
* **ui:** radio panel takes its 400px from the reader only, not the message list (tuxlink-40u8) ([f7a8daa](https://github.com/cameronzucker/tuxlink/commit/f7a8daa0d55b1da7844a939eb628c61b7990e1b2))

## [0.24.0](https://github.com/cameronzucker/tuxlink/compare/v0.23.4...v0.24.0) (2026-06-03)


### Features

* **analysis:** per-sub-carrier SNR estimator + serde output ([630675d](https://github.com/cameronzucker/tuxlink/commit/630675d5c790e946952f763e9cbb02efba8ae41d))
* **catalog:** WLE catalog-request framework (tuxlink-ddiq) ([1204838](https://github.com/cameronzucker/tuxlink/commit/120483843c5f7fbd023af4bd9805f0ff3ffffd91))
* **channel:** two-tap Watterson WattersonChannel core ([db750e7](https://github.com/cameronzucker/tuxlink/commit/db750e7639f8f5e1fd29c10dbe52a737d2e4f698))
* **ci:** Linux packaging pipeline (deb/rpm/AppImage) ([4bffcfd](https://github.com/cameronzucker/tuxlink/commit/4bffcfd9d89074522cc2296b1f37e57919576b7d))
* **ci:** Linux packaging pipeline (deb/rpm/AppImage) (tuxlink-qybc, supersedes cs7) [jay-condor-shoal] ([e775575](https://github.com/cameronzucker/tuxlink/commit/e77557519c8839631f1ce72ec776059dc1d3455b))
* **ci:** nightly branch-lifecycle audit workflow + audit script + tests ([408ad21](https://github.com/cameronzucker/tuxlink/commit/408ad2130b50acab941f57dd68a0977bdab4bc81))
* **cli:** pipe-friendly hf-channel-sim-cli for AI-agent harnesses ([9954bba](https://github.com/cameronzucker/tuxlink/commit/9954bbaaf3f04468e2cdf06ba5d7624b7eac9f7b))
* **compose:** enable the Cc field end-to-end (tuxlink-h1km) ([4198aa6](https://github.com/cameronzucker/tuxlink/commit/4198aa629909a424a20a83b35385d8c20d49ca7e))
* **compose:** Phase 6 form integration per spec §7.1/§7.3 (tuxlink-v1p) ([608f3ff](https://github.com/cameronzucker/tuxlink/commit/608f3ff2f9f1e80c4253437957b38ac05bb850ed))
* **connections:** mark p2p+telnet built in session-type matrix (tuxlink-0pnb) ([8045abe](https://github.com/cameronzucker/tuxlink/commit/8045abee7bf43f4c34e411b464e00845403e062f))
* **connections:** restore port input on TelnetP2pRadioPanel (tuxlink-0pnb) ([5e13453](https://github.com/cameronzucker/tuxlink/commit/5e13453020eb71ce05299443aa285621ec8cf530))
* **connections:** TelnetP2pRadioPanel — Dial-mode UI for P2P Telnet (tuxlink-0pnb) ([d38e284](https://github.com/cameronzucker/tuxlink/commit/d38e284220365259d0722efe21f9f5ecc4083cef))
* **connections:** wire P2P-VARA HF/FM in the sidebar (tuxlink-kb3s) ([2935590](https://github.com/cameronzucker/tuxlink/commit/2935590541ab0bae014d8aa12ff516e61b2e3b2e))
* **fading:** spectrum-shaped complex-Gaussian Watterson tap process ([abec1e4](https://github.com/cameronzucker/tuxlink/commit/abec1e4a65eba308f99b32c6ccf725d8e25a3295))
* **forms-ts:** FormPicker modal per spec §7.1 (tuxlink-v1p) ([1ba4d50](https://github.com/cameronzucker/tuxlink/commit/1ba4d50320e94707e6328bf29062ae8378f47dbb))
* **forms-ts:** Ics213Form per spec §7.1 (tuxlink-v1p) ([b70dfb9](https://github.com/cameronzucker/tuxlink/commit/b70dfb926b5ef8bf3ffe84130b14b1eb5398def1))
* **forms-ts:** Ics213View per spec §7.2 (tuxlink-v1p) ([3d8e764](https://github.com/cameronzucker/tuxlink/commit/3d8e7646dcb529faf4cc8a99fbb1dffe527dd53c))
* **forms-ts:** KeyValueView fallback for unknown forms (tuxlink-v1p) ([cce1512](https://github.com/cameronzucker/tuxlink/commit/cce1512c17205320140c9e0ba5250bc83ceba242))
* **forms-ts:** register ICS-213 in form registry (tuxlink-v1p) ([4101706](https://github.com/cameronzucker/tuxlink/commit/4101706fa26a51d0317f1a491aabeed2b7667562))
* **forms-ts:** registry contract per spec §5.2 (tuxlink-v1p) ([a1e37cc](https://github.com/cameronzucker/tuxlink/commit/a1e37ccc91c6b0005b5e6a2c183ca5e3e6d19e0d))
* **forms-ts:** TS types mirror per spec §6.1 (tuxlink-v1p) ([4b1f4b9](https://github.com/cameronzucker/tuxlink/commit/4b1f4b9a93ad212b66ba86a0f1603539bf9ce0b0))
* **forms:** bundle 4 additional Phase 9 forms per spec §8 (tuxlink-v1p) ([13a5c3a](https://github.com/cameronzucker/tuxlink/commit/13a5c3a9c22a832c3ff9d3f02484348b9d58e77a))
* **forms:** forms-webview Tauri capability (P1 Task 7) ([03e4f7c](https://github.com/cameronzucker/tuxlink/commit/03e4f7c14021c47289603f6dd62f0eb3136e8027))
* **forms:** http_server module — lazy loopback for HTML Forms (P1 Task 6) ([ee74d85](https://github.com/cameronzucker/tuxlink/commit/ee74d85288d094a541c85d1097a770f103667dac))
* **forms:** multipart module — urlencoded + multipart body parser (P1 Task 5) ([a7f63c4](https://github.com/cameronzucker/tuxlink/commit/a7f63c4954117f66d2d77e805755275a02771442))
* **forms:** skin module — tuxlink CSS asset for webview forms (P1 Task 4) ([b2ff63f](https://github.com/cameronzucker/tuxlink/commit/b2ff63fa79409847ac55d14c32122d11eb818499))
* **forms:** wle_templates module — bundled + custom enumeration (P1 Task 3) ([4821d7e](https://github.com/cameronzucker/tuxlink/commit/4821d7e38f9535c488c7d72d10d90225402804e8))
* **githooks:** branch lifecycle state machine + pre-commit/pre-push hooks ([15f5723](https://github.com/cameronzucker/tuxlink/commit/15f57232453c6c60693b9c6adf6648c92e126146))
* **grib:** Saildocs GRIB-request framework (tuxlink-vrpk) ([e5f049a](https://github.com/cameronzucker/tuxlink/commit/e5f049aa1f9b41b90bf04feb5c70269a7a420b2e))
* **help:** help_window Rust module + invoke_handler registration (tuxlink-0gsy) ([912c365](https://github.com/cameronzucker/tuxlink/commit/912c365ec3fb8298541cefbf886e19e96124342b))
* **help:** React route + HelpView skeleton (tuxlink-0gsy) ([0c89722](https://github.com/cameronzucker/tuxlink/commit/0c897228d4cf318bb791b1e470528f34a4940e9b))
* **help:** sidebar + reading pane + topic registry (Variant A) (tuxlink-0gsy) ([38071e4](https://github.com/cameronzucker/tuxlink/commit/38071e4fe3448ed7d6c4de04449ae7618e007060))
* **help:** sidebar search UI + hit highlighting (tuxlink-0gsy) ([39e43fc](https://github.com/cameronzucker/tuxlink/commit/39e43fc37114ef5cd1a84993ea63d32cb0a2a6a2))
* **help:** text-size dropdown + Ctrl shortcuts + persistence (tuxlink-0gsy) ([ccf19a5](https://github.com/cameronzucker/tuxlink/commit/ccf19a51ae81f84036df576420b311ff7cf2bcec))
* **help:** theme inheritance + live updates (tuxlink-0gsy) ([0e70916](https://github.com/cameronzucker/tuxlink/commit/0e709169479e0f6f432ca088c8dd1981e75e4ff4))
* **hf-channel-sim:** initial AGPLv3 crate scaffolding ([a4dcb82](https://github.com/cameronzucker/tuxlink/commit/a4dcb82669d7233dc041f5cf856a90146f357b40))
* **linux:** install Tuxlink taskbar icon via .desktop entry + XDG icon paths (tuxlink-mj7i) ([bbc4465](https://github.com/cameronzucker/tuxlink/commit/bbc4465a6417ff1c7ec37d9b6c78b8b35bc85040))
* **mailbox:** display-side attachment filename sanitization (tuxlink-v1p) ([91ff113](https://github.com/cameronzucker/tuxlink/commit/91ff113c596be839f8d784bab35b4bfe750cb033))
* **mailbox:** enable Outbox folder in FolderSidebar (tuxlink-su2h) ([463892a](https://github.com/cameronzucker/tuxlink/commit/463892a1a18f29a21a3daa34730d6c6e8104f62e))
* **mailbox:** form-render dispatch in MessageView per spec §6.2 (tuxlink-v1p) ([7390316](https://github.com/cameronzucker/tuxlink/commit/7390316036bb0e74f27003b287d79c47c85c4b8e))
* **mailbox:** MessageList sort UI — operator-selectable sort with persistence ([3dc193a](https://github.com/cameronzucker/tuxlink/commit/3dc193aef0d2ba55997527d3de842963d36e273e))
* **mailbox:** MessageView attachment Save As (tuxlink-0fyj) ([f6b7171](https://github.com/cameronzucker/tuxlink/commit/f6b71714cdce17f886a1c4857013d0e18c6ad9cc))
* **mailbox:** Reply-with-form button per spec §7.4 (Codex P2 [#6](https://github.com/cameronzucker/tuxlink/issues/6)) (tuxlink-v1p) ([5350809](https://github.com/cameronzucker/tuxlink/commit/53508095fc41f1c85b284698846ea28cf02ac668))
* **mailbox:** sort UI Phase 2.1 — add Size+Recipient, swap native select for Radix popup ([18f0c48](https://github.com/cameronzucker/tuxlink/commit/18f0c48442c19ed07f7974e1ea6c25bbba636cdc))
* **mailbox:** user-folder mechanism — Phase 2 MVP (tuxlink-f62f) ([c2dd4be](https://github.com/cameronzucker/tuxlink/commit/c2dd4beaf86d3c7c5e08c58db31b58262860a839))
* **mailbox:** user-folders Phase 3 — right-click, drag-drop, rename, delete ([44ba157](https://github.com/cameronzucker/tuxlink/commit/44ba1575a6a5ef89367932dee9c3b6a7babbebc0))
* **mailbox:** wire up Archive folder (user-folders Phase 1, tuxlink-ca5x) ([e4a90fe](https://github.com/cameronzucker/tuxlink/commit/e4a90fe0a627ed1af2a19d1573a3016f84b35d51))
* **menu:** wire menu:message:print (Ctrl+P) — tuxlink-j0m3 ([d6a47ae](https://github.com/cameronzucker/tuxlink/commit/d6a47aea376a59894058f576578b61d71356d6fa))
* **modem/vara:** TCP transport codec + smoke probe ([8690701](https://github.com/cameronzucker/tuxlink/commit/869070168e33d74c6519d66e02a8cef0901e6c71))
* **modem/vara:** wire VARA TCP transport into UI (Phase 2 — tuxlink-dfmf) ([1f6c3ef](https://github.com/cameronzucker/tuxlink/commit/1f6c3ef1dcc7cc6f03dc24e6caf93550069d35c1))
* **modem:** parse PINGACK + PING events for Quality score (radio-panel-ardop P4.3; closes tuxlink-1637) ([ae90839](https://github.com/cameronzucker/tuxlink/commit/ae9083929051019134327f85c2e2cab7f147110f))
* **noise:** AWGN generator decoupled from channel ([4caa603](https://github.com/cameronzucker/tuxlink/commit/4caa6037f2bacfb4e5269d0d8cd7071f50ab9ca2))
* **params:** ITU-R F.520 + F.1487 channel condition vocabulary ([3aaec08](https://github.com/cameronzucker/tuxlink/commit/3aaec08cad657a7fed830e3556c1acf10a9e721b))
* **position:** add effective_ui_locator + ui_grid DTO field + gps_ready Off gating (tuxlink-va1i) ([6dfb48c](https://github.com/cameronzucker/tuxlink/commit/6dfb48c4974ef7dc7f679d8f6fdf78e56941f31c))
* **radio:** add cmd_port + binary inline-edit rows to ARDOP Radio section (tuxlink-jmfm) ([9b73157](https://github.com/cameronzucker/tuxlink/commit/9b7315795d05211a13ac0f2e06ae3ff95e2dffa0))
* **radio:** ArdopRadioPanel — replaces ArdopDock + ArdopHfStub (radio-panel-ardop P4.5) ([6c084c1](https://github.com/cameronzucker/tuxlink/commit/6c084c12c5a461eb1d1fb2ef4be7caf75f921ed1))
* **radio:** FrameRibbon — recent ARQ frame-type strip for Signal section (radio-panel-ardop P4.2) ([55dd337](https://github.com/cameronzucker/tuxlink/commit/55dd3379983feaa965ec46a58cd0a884baf618fe))
* **radio:** ModemLinkSection — shared TCP/USB/BT picker for TNC-mediated modes (radio-panel P3 task 3.1) ([e2ca267](https://github.com/cameronzucker/tuxlink/commit/e2ca267c7705e14813a443507df7c4fa85a34060))
* **radio:** PacketRadioPanel — replaces PacketConnectionPanel for right-panel mount (radio-panel P3 task 3.2) ([4876ec2](https://github.com/cameronzucker/tuxlink/commit/4876ec29d24cbf1ded0534fb2ecbcce9db19da69))
* **radio:** SignalSection — Quality + S/N trend + recent frames (radio-panel-ardop P4.4) ([069245d](https://github.com/cameronzucker/tuxlink/commit/069245d42f5cb8327543d388fb0ab8706a3e5dc6))
* **radio:** Sparkline — 60-sample rolling chart for Live + Signal sections (radio-panel-ardop P4.1) ([171c981](https://github.com/cameronzucker/tuxlink/commit/171c981ca3bf8395a874c7db53d79c8a6113cdf3))
* **reply:** reply-to-form + reply-with-form per spec §7.4 (tuxlink-v1p) ([2386144](https://github.com/cameronzucker/tuxlink/commit/2386144492d05d285947ccb40babc04d5f6b8442))
* **report:** end-to-end characterization report + JSON ([ff1b1f1](https://github.com/cameronzucker/tuxlink/commit/ff1b1f1e4d88f8a2f2a0371ee4553a28c3ae1227))
* **rng:** seeded Xoshiro256++ + complex Gaussian draws ([bea1a5b](https://github.com/cameronzucker/tuxlink/commit/bea1a5bb3778c064cc5e225dc5eadde2b0fca700))
* **scripts:** host-level dev-server lease + CLI wrapper ([72fd00d](https://github.com/cameronzucker/tuxlink/commit/72fd00d6292c06aeba0bc1cf204d0a0d7b4f172d))
* **scripts:** v1 converge-build.sh + pnpm dev:converged wrapper ([25547ed](https://github.com/cameronzucker/tuxlink/commit/25547ed2fccc17f19674fde6cbfb16f87b51fc45))
* **search:** docs_fts virtual table + extractor + docs_search command (tuxlink-0gsy) ([633ad44](https://github.com/cameronzucker/tuxlink/commit/633ad447173524b9272e2efbca7a6eb73c1d1ab8))
* **shell:** light theme presets + custom theme designer (tuxlink-c22r + tuxlink-vgth) ([327d623](https://github.com/cameronzucker/tuxlink/commit/327d6239a3ed962c6445db09e3a75d5ad43e6a02))
* **shell:** optimistic config_read refresh after grid + source writes (tuxlink-c79g T14) ([c8ace99](https://github.com/cameronzucker/tuxlink/commit/c8ace9984fc9bfbe82665f149def1e4aba4f9ced))
* **shell:** redesign status bar as mailbox bar (tuxlink-qxqj) ([57f287f](https://github.com/cameronzucker/tuxlink/commit/57f287fe197d41aea64d0c5cac3834ac8920a18e))
* **shell:** replace source-chip toggle with [GPS|MANUAL] segmented control (tuxlink-z5pz) ([3598693](https://github.com/cameronzucker/tuxlink/commit/359869341ad5710e9b1503446c937f69febebb34))
* **shell:** route ARDOP HF to ArdopRadioPanel; remove dual-mount (radio-panel-ardop P4.6) ([82cea5a](https://github.com/cameronzucker/tuxlink/commit/82cea5a5271f8ec2f08a2f844b841c569d6f7dc9))
* **shell:** route radio panel Packet selections to PacketRadioPanel (radio-panel P3 task 3.3) ([33d1ef6](https://github.com/cameronzucker/tuxlink/commit/33d1ef66996106d19cdded41c9f9e018998b23da))
* **shell:** route Telnet selection to TelnetRadioPanel (radio-panel-telnet P2.3) ([7efe1eb](https://github.com/cameronzucker/tuxlink/commit/7efe1eb1d3fecb56457133a1d06d6ebb06beed50))
* **shell:** Set manually button + State 4 interpunct + dimmed chip (tuxlink-c79g T13) ([79f59ae](https://github.com/cameronzucker/tuxlink/commit/79f59ae9e49090e799dcc2b05029e8fe184d6c05))
* **shell:** switch ribbon liveGrid to ui_grid for LocalUiOnly-aware display (tuxlink-va1i) ([0475950](https://github.com/cameronzucker/tuxlink/commit/047595060cde80684de6d48a5614dcf9c77f103a))
* **shell:** wire Help menu + ship user-guide docs (tuxlink-35g0 + tuxlink-gq74) ([f313daf](https://github.com/cameronzucker/tuxlink/commit/f313dafb259e9d51ccb75afc577cec3a970693b4))
* **status:** event-driven backend_status — frontend sees every transition (operator smoke fix [#4](https://github.com/cameronzucker/tuxlink/issues/4)) ([9d3c2cd](https://github.com/cameronzucker/tuxlink/commit/9d3c2cd74fa9e067421018900048b2f6928074f8))
* **theme:** --modem-accent token family restores radio dock's green identity (tuxlink-2ief) ([ac51398](https://github.com/cameronzucker/tuxlink/commit/ac51398af84878722c876cd7e7e4d47c682aff50))
* **tuxmodem-fec:** block bit interleaver with burst-decorrelation gate ([138eed9](https://github.com/cameronzucker/tuxlink/commit/138eed9f62d33d5aae392919d6e2d33b3fff449e))
* **tuxmodem-fec:** CRC-32 append + verify over bit slices ([48dca26](https://github.com/cameronzucker/tuxlink/commit/48dca26c87c4d362d6f03e854c14382c343d548f))
* **tuxmodem-fec:** FecCodec impl wiring CRC + LDPC + interleaver ([ed14a84](https://github.com/cameronzucker/tuxlink/commit/ed14a843e17a020ceab1710bb5568c80c7b849b1))
* **tuxmodem-fec:** LDPC systematic encoder + WiFi-family seed iteration ([8405e78](https://github.com/cameronzucker/tuxlink/commit/8405e7863b6980ea7091dc7b05c4a6089a5fc7eb))
* **tuxmodem-fec:** parity-check matrix + floor rate-1/4 + WiFi family LDPC codes ([ea19560](https://github.com/cameronzucker/tuxlink/commit/ea19560bfb2a043a8f00370fbcb1f3134ea2c59c))
* **tuxmodem-fec:** scaffold AGPLv3 crate for clean-sheet LDPC FEC ([3afc9e5](https://github.com/cameronzucker/tuxlink/commit/3afc9e5223aa7f95c6adcab959bed705d9a142ac))
* **tuxmodem-fec:** SPA belief-propagation decoder (LLR-form) ([aa4d46d](https://github.com/cameronzucker/tuxlink/commit/aa4d46d77469f9fc631482fabe4f007493e4f311))
* **tuxmodem-phy:** 48kHz f32 audio buffer + wav round-trip helper ([0bbd9ba](https://github.com/cameronzucker/tuxlink/commit/0bbd9ba91c653f9deaa661ac26dd140981515d1c))
* **tuxmodem-phy:** BPSK / QPSK / 16-QAM / 64-QAM + max-log LLR ([a1e8e0a](https://github.com/cameronzucker/tuxlink/commit/a1e8e0a7d2ccb54b0bbc7adea41ba492fadb3f12))
* **tuxmodem-phy:** channel-sim adapter + BER sweep + ARDOP competence gate ([fd0c422](https://github.com/cameronzucker/tuxlink/commit/fd0c42223594bb67225a32c542245c4a9e9cbf78))
* **tuxmodem-phy:** crate skeleton + error taxonomy ([b710959](https://github.com/cameronzucker/tuxlink/commit/b710959e343ff671b8384a9c0539f3ef15a0ef3c))
* **tuxmodem-phy:** FEC bus contract + SNR-aware mode router + FT-818 gate ([87ee200](https://github.com/cameronzucker/tuxlink/commit/87ee200e4fa94165f5de3e7729117f3d3511d3b4))
* **tuxmodem-phy:** mode table + ModeHint/ResolvedMode/ModeFamily skeleton ([0ab38d0](https://github.com/cameronzucker/tuxlink/commit/0ab38d0b2efc1f9493e0761027879c7cfbb4d67f))
* **tuxmodem-phy:** narrow-FSK situational floor mode ([a950860](https://github.com/cameronzucker/tuxlink/commit/a950860af1504ae76a16fa04c83b548ee97ed496))
* **tuxmodem-phy:** OFDM equalizer + receiver (clean-channel round-trip) ([926de8f](https://github.com/cameronzucker/tuxlink/commit/926de8f78cb2d64a73962aec5c34cbefbd25e5b6))
* **tuxmodem-phy:** OFDM mode parameter table (Narrow/Mid/Wide) ([7531188](https://github.com/cameronzucker/tuxlink/commit/75311881738cc0a8c6327f74a886df2526609806))
* **tuxmodem-phy:** OFDM transmitter (one-symbol modulate) ([bee9f92](https://github.com/cameronzucker/tuxlink/commit/bee9f92a77c0d334ef4ccf237d84172af2d83372))
* **tuxmodem-phy:** PhyTransport API + NullPhy contract baseline ([9b8a531](https://github.com/cameronzucker/tuxlink/commit/9b8a53167db372501c868848c69cf2fa45c272d0))
* **tuxmodem-phy:** pilot-aided per-subcarrier SNR estimator (Phase 5) ([39f03be](https://github.com/cameronzucker/tuxlink/commit/39f03be5ad58d18dc97836cc00f1d714a3011fe5))
* **tuxmodem-phy:** synchronization infrastructure (Phase 4) ([28d26e8](https://github.com/cameronzucker/tuxlink/commit/28d26e81fbbc483149548f611c0f66b95b30c968))
* **tuxmodem-phy:** water-filling per-subcarrier bit-loader ([76c5c1a](https://github.com/cameronzucker/tuxlink/commit/76c5c1a1d60bd07ecbd6bc6ad2704ce913b74e4d))
* **tuxmodem-phy:** wide-band low-density OFDM floor (default robustness mode) ([262fc1f](https://github.com/cameronzucker/tuxlink/commit/262fc1f9f9218f76fa4e13f681ea4fc61e66ce3a))
* **tuxmodem:** scaffold AGPLv3 workspace for clean-sheet modem ([ed579aa](https://github.com/cameronzucker/tuxlink/commit/ed579aac452eca30ba11412028dbf1ad32061dbb))
* **ui-cmd:** Tauri commands for P2P dial + peer-password management (tuxlink-0pnb) ([8a95481](https://github.com/cameronzucker/tuxlink/commit/8a954811389e3fd43c3ea6dc2a520b90cd49e5cc))
* **winlink/ardop:** wire ARDOP listener to listener-arms foundation (tuxlink-dhbl) ([48d2846](https://github.com/cameronzucker/tuxlink/commit/48d2846f539e919f8f1f943c4f25b976d4b4a07a))
* **winlink/ax25:** wire Packet allowlist overlay to listener-arms foundation (tuxlink-inde) ([fe28f97](https://github.com/cameronzucker/tuxlink/commit/fe28f97e736b4642f5b86283fab022c56057011a))
* **winlink/listener:** shared listener-arms foundation (tuxlink-3o2o) ([ed3de34](https://github.com/cameronzucker/tuxlink/commit/ed3de34eb19d268f37a831e5595bb22611934086))
* **winlink/listener:** shared listener-arms foundation (tuxlink-3o2o) ([c26cdf2](https://github.com/cameronzucker/tuxlink/commit/c26cdf2b69d83af8455e39321cd80eadf794c6a6))
* **winlink/telnet:** ship Telnet-P2P listener with WLE-divergent allowlist+keyring (tuxlink-xehu) ([7dec787](https://github.com/cameronzucker/tuxlink/commit/7dec78700b23057649a5a5f4fd323febc091493c))
* **winlink:** dialer-side telnet-login wrapper for P2P sessions (tuxlink-0pnb) ([d07ec8f](https://github.com/cameronzucker/tuxlink/commit/d07ec8fefaf67c3530b454b6311884a65d67c5d2))
* **winlink:** per-peer keyring helpers for P2P station passwords (tuxlink-0pnb) ([ca87324](https://github.com/cameronzucker/tuxlink/commit/ca87324312c80d5cddfa3b46216c367d354a9376))
* **winlink:** RMS-Relay foundation — SessionIntent + RoutingFlag + banner parser (tuxlink-kld3) ([a311a0b](https://github.com/cameronzucker/tuxlink/commit/a311a0bd5de7180ca9ac1b8d1273ba4bd2a597bf))
* **winlink:** TCP P2P-Telnet client transport + connect_and_exchange (tuxlink-0pnb) ([689d65a](https://github.com/cameronzucker/tuxlink/commit/689d65afa720cdf5a09ec0323db31f2f2f9b2a2f))


### Bug Fixes

* backend status honesty + reading-pane/panel decoupling (operator smoke fixes [#2](https://github.com/cameronzucker/tuxlink/issues/2)) ([8617768](https://github.com/cameronzucker/tuxlink/commit/8617768019327b50a270738cfd57767bfea61743))
* **capabilities:** grant compose window the minimize/maximize/resize-drag IPCs (tuxlink-v1p) ([7ad80dd](https://github.com/cameronzucker/tuxlink/commit/7ad80dd690564d21de7ba91605704940ecfe1566))
* **ci:** branch-audit Codex P1+P2+P3 dispositions (3 P1+P2+P3 fixes; 7→10 tests) ([9535619](https://github.com/cameronzucker/tuxlink/commit/95356193bbadb82852790b2657675eb8fb57716b))
* **cms:** hold Connected status visible for 1.5s before disconnect (operator smoke [#5](https://github.com/cameronzucker/tuxlink/issues/5)) ([2a5a0af](https://github.com/cameronzucker/tuxlink/commit/2a5a0afe50797230276085443834f2348fc25804))
* **compose:** add Minimize + Maximize titlebar buttons (tuxlink-v1p) ([71c03f4](https://github.com/cameronzucker/tuxlink/commit/71c03f4b123706310d5c91c09c41753c0f559a6b))
* **compose:** bump compose-window default size to 1100x820 (tuxlink-v1p) ([a33a2fd](https://github.com/cameronzucker/tuxlink/commit/a33a2fd41a1a2aa1d5d280b8d8f97dc228102d90))
* **deps:** bump react-dom 19.2.6 → 19.2.7 to match react (tuxlink-ola6) ([ed633d7](https://github.com/cameronzucker/tuxlink/commit/ed633d71a78ad93f5922e2737bc45a200b081ef3))
* **forms-ts:** innerhtml-ban test uses import.meta.glob (no @types/node) (tuxlink-v1p) ([2d8fa1f](https://github.com/cameronzucker/tuxlink/commit/2d8fa1fd3f76a4b29223556ae3f8fbd93a795720))
* **forms,compose:** ICS-213 date+time defaults; hide irrelevant action buttons in form mode (tuxlink-v1p) ([cfa8576](https://github.com/cameronzucker/tuxlink/commit/cfa857620b1b4b0309e59372fa8254fb6eaee83f))
* **forms:** apply Codex P1 findings to http_server + capability (P1 Task 6/7) ([138ee9d](https://github.com/cameronzucker/tuxlink/commit/138ee9dcd66c292f26b740a545015079cbb0fdb6))
* **forms:** apply Codex review P1+P2 findings (tuxlink-v1p) ([dbda3d8](https://github.com/cameronzucker/tuxlink/commit/dbda3d87770248bbac0d917e45fba3a4e8cebf81))
* **forms:** apply Codex round 2 findings (tuxlink-v1p) ([fd7e373](https://github.com/cameronzucker/tuxlink/commit/fd7e3739fbcec823f150c77344e10d6aea608446))
* **forms:** author per-form CSS, scrollable body, resize handles (tuxlink-v1p, tuxlink-ydrd) ([415b7c2](https://github.com/cameronzucker/tuxlink/commit/415b7c20ef9f1a6404a80fdeed32b3e4c762f40b))
* **forms:** style FormPicker + add keyboard navigation (tuxlink-v1p) ([4451d27](https://github.com/cameronzucker/tuxlink/commit/4451d27ee08e7c38e46886f98cf50a88ce6dba84))
* **forms:** update axum wildcard path syntax for 0.8 (tuxlink-prz6) ([a5be99b](https://github.com/cameronzucker/tuxlink/commit/a5be99baa4ffd42058fc1b4e617b858ffe422eae))
* **githooks:** branch-state-machine Codex P1+P2 dispositions ([27bf968](https://github.com/cameronzucker/tuxlink/commit/27bf968d35b8904f30f2928eb331a2494eb09706))
* **linux:** install both tuxlink.desktop + com.tuxlink.app.desktop variants (tuxlink-xcay) ([536de53](https://github.com/cameronzucker/tuxlink/commit/536de539d73bca66d7db606b70b9681325a83965))
* **linux:** use Exec=/usr/bin/env tuxlink so GIO loads the .desktop file in dev (tuxlink-5e2d) ([05deba3](https://github.com/cameronzucker/tuxlink/commit/05deba360660e7ac448414b9c5951b9d87a5148c))
* **mailbox:** list_messages returns newest-first by date (tuxlink-mjc8) ([007778e](https://github.com/cameronzucker/tuxlink/commit/007778e9bea97a1f61e8e63d12b8c644ecf5f039))
* **menu:** mark unwired Message/Session items disabled+badged (tuxlink-dpf) ([d796e98](https://github.com/cameronzucker/tuxlink/commit/d796e981d7f9f4c1774972492e0482d7884b3f3d))
* **modem/vara:** drop platformBlocked from onStartClick handler — was no-op-ing Start on aarch64 (tuxlink-poh6) ([70bb12f](https://github.com/cameronzucker/tuxlink/commit/70bb12f3420b724b3611544fd3680eb04c8316c9))
* **modem/vara:** remove `loading` state from useVaraConfig — was locking the panel on Pi (tuxlink-6dzo) ([1a571f9](https://github.com/cameronzucker/tuxlink/commit/1a571f9c8510832ea8a70623c891d87e3d59c114))
* **modem/vara:** send MYCALL after TCP open + emit session_log on Start/Stop (tuxlink-rsus) ([88d956d](https://github.com/cameronzucker/tuxlink/commit/88d956d041bd9656f23918dfe47da1b78e1555e5))
* **modem/vara:** shorten platform-block banner to 1-line production fixture (tuxlink-3inw) ([7d071c7](https://github.com/cameronzucker/tuxlink/commit/7d071c71445cc14c89000bf7be63b76fec12e7d4))
* **modem/vara:** ungate panel controls on aarch64 — tuxlink can connect to remote VARA over TCP (tuxlink-ze98) ([3720952](https://github.com/cameronzucker/tuxlink/commit/37209529048e6c52595157ad9eaaf3f049dda2d7))
* **perf:** adrev follow-ups — row-date staleness + sidebar memo + lazy-MessageView fallback (tuxlink-268k) ([b03d887](https://github.com/cameronzucker/tuxlink/commit/b03d88786ee530628851938e0f3f828863897cff))
* **perf:** cold-start CSP + custom-theme correctness (tuxlink-01vd) ([2df5e33](https://github.com/cameronzucker/tuxlink/commit/2df5e33311380bacbba560ad98c4cc8626dc8201))
* **position:** extend use_gps relaxation to position_set_source command (tuxlink-c79g T3) ([f83e2ef](https://github.com/cameronzucker/tuxlink/commit/f83e2efd99a9f08477b168c3ae51710108454828))
* **position:** GPS-fresh always wins the displayed grid (tuxlink-pjih) ([b3d617c](https://github.com/cameronzucker/tuxlink/commit/b3d617c8e6d3bfc16a17207762eb8cc37c7ac2e0))
* **position:** hold arbiter mutex across config_set_grid + position_set_source critical sections (tuxlink-c79g T6) ([a93e2e9](https://github.com/cameronzucker/tuxlink/commit/a93e2e94691a7cad38e87c13deb59b522fdf9be9))
* **position:** relax arbiter.use_gps() to infallible (tuxlink-c79g T2) ([dba3d10](https://github.com/cameronzucker/tuxlink/commit/dba3d10088c0dd6208ff63b6ceea8a029dda21a0))
* **position:** remove active_source from PositionStatusDto + position_status (tuxlink-c79g T5) ([59902f6](https://github.com/cameronzucker/tuxlink/commit/59902f691cce5b8fdbf1d481b8bb4d6d799b323c))
* **position:** restore arbiter source-gating + set_manual source-pinning (tuxlink-c79g T1) ([7792fa4](https://github.com/cameronzucker/tuxlink/commit/7792fa41b2f63165833b04de78e1e028e46b2d2b))
* **position:** restore config_set_grid persistence of position_source = Manual (tuxlink-c79g T4) ([0ffcd40](https://github.com/cameronzucker/tuxlink/commit/0ffcd402859889a1111fce0aefd935d0f9013280))
* **radio:** add Clear control to session log (panel-local reset) ([5f27150](https://github.com/cameronzucker/tuxlink/commit/5f271504b3dd0aa77425500b0ab79d585f7ee333))
* **radio:** add Radio section to ARDOP panel (audio + PTT inline editor) ([4c88618](https://github.com/cameronzucker/tuxlink/commit/4c88618074845a9cdf3d201a88b051b0db816a08))
* **radio:** address T3 code-quality findings — strict parse + 65535 + binary revert (tuxlink-jmfm) ([99b8851](https://github.com/cameronzucker/tuxlink/commit/99b8851638db1f40de7d617ad0bb2817d0f8b162))
* **radio:** ARDOP Open WebGUI uses tauri-plugin-shell instead of window.open ([94bccfe](https://github.com/cameronzucker/tuxlink/commit/94bccfe869e033403f20f388d0470a8288ce5aa9))
* **radio:** ARDOP WebGUI button gates on running + adds webgui_port override ([d045c58](https://github.com/cameronzucker/tuxlink/commit/d045c58d74197685ff0a9bff8f0f3154785fd9cf))
* **radio:** AX.25 baud default 1200 + editable selector with standard ladder ([4ed69ee](https://github.com/cameronzucker/tuxlink/commit/4ed69ee850cfc3bed2df02ca29c0eada528dd843))
* **radio:** bump ARDOP UI font sizes between Signal and log pane ([f8cb08e](https://github.com/cameronzucker/tuxlink/commit/f8cb08ef34f148f03262bd2aa624fa0afef7ef8c))
* **radio:** clamp ARDOP panel content inside the 360px width ([cc82bf4](https://github.com/cameronzucker/tuxlink/commit/cc82bf4b49918b94a099f0a12f000a322180d5a5))
* **radio:** clamp grid/flex tracks so ARDOP panel content stops overflowing (tuxlink-jrf7) ([26e663f](https://github.com/cameronzucker/tuxlink/commit/26e663fca35a7245da03a98e9c1775c873870a1f))
* **radio:** Clear log drains backend buffer so cleared lines don't reappear ([507b32b](https://github.com/cameronzucker/tuxlink/commit/507b32bd2bbc23e7bb57ccbcecaac40e430ee981))
* **radio:** close snapshot/listen race in useSessionLog via seq-dedup merge (Codex R2) ([693e904](https://github.com/cameronzucker/tuxlink/commit/693e9040d3a1c12559f37462f56634cc475e0605))
* **radio:** filter ARDOP Capture/Playback dropdowns to hardware-only (tuxlink-y7nq) ([0bc5090](https://github.com/cameronzucker/tuxlink/commit/0bc509025dc8bdd1b2607b2d47720c8b3066a8db))
* **radio:** rename Packet Connect button to Start for vocab consistency ([df09be8](https://github.com/cameronzucker/tuxlink/commit/df09be8d8d422b70b0ca17c6c4e299e7eae43edd))
* **radio:** restore ARDOP Capture/Playback/PTT pickers with real ALSA + serial enumeration (tuxlink-y7x7) ([58c24d0](https://github.com/cameronzucker/tuxlink/commit/58c24d0761eb6442e8c4c77c86e4c20e622cb316))
* **radio:** restore listenDefault preference (Packet P2P) + ARQ bandwidth dropdown (ARDOP Connect) — Codex P3+P4 P1s ([20ab2b6](https://github.com/cameronzucker/tuxlink/commit/20ab2b69c6d94c2be9cc437cdd6f783a087f2065))
* **radio:** restore outlined-subtle radio chrome — filled greens were too loud (tuxlink-vxh8) ([541dc1f](https://github.com/cameronzucker/tuxlink/commit/541dc1f8d8ebd8c7dd0872f87acf88ce2b937d21))
* **radio:** restore Telnet controls + bump type scale + larger log section (operator smoke fixes) ([1ec6305](https://github.com/cameronzucker/tuxlink/commit/1ec63054d5cc948ec3320571432f4415084e2542))
* **radio:** restore USB + BT device pickers in ModemLinkSection (tuxlink-mqu3) ([0ef5261](https://github.com/cameronzucker/tuxlink/commit/0ef5261bbc7a9b92b6d87cb3b2d124518e63aea4))
* **radio:** selects use appearance-none + chevron so they don't read as disabled ([64ab42f](https://github.com/cameronzucker/tuxlink/commit/64ab42f2ea8c5d7bdc0b9fca8858e0211b7d253a))
* **radio:** session log fills remaining vertical space in radio panel ([ee9bb35](https://github.com/cameronzucker/tuxlink/commit/ee9bb35f57573a929911ba26a4a68500fa8c6b25))
* **radio:** theme-token the radio-panel chrome so light schemes don't wash out (tuxlink-he7h) ([6c40548](https://github.com/cameronzucker/tuxlink/commit/6c405481dbb1e3a9b615c20dd53a04d80b9f9a1b))
* **radio:** wire SessionLogSection to backend events + read CMS endpoint from config (radio-panel-telnet P2 Codex fixes) ([42df27a](https://github.com/cameronzucker/tuxlink/commit/42df27afe8b727d7b1fc0ca3577974432092f52b))
* **scripts:** converge-build v1 Codex P1+P3 dispositions ([33a0562](https://github.com/cameronzucker/tuxlink/commit/33a0562f737a59ebec992b243cb14da498a11110))
* **scripts:** converge-build v2 Codex P1+P2+P3 dispositions ([f6b5b57](https://github.com/cameronzucker/tuxlink/commit/f6b5b5787af9711c9659a9431cb4567ddbf43cec))
* **scripts:** dev-server-lease Codex P1+P2 dispositions ([7c4bb2f](https://github.com/cameronzucker/tuxlink/commit/7c4bb2f926bd888f348ab52c4af5f164f88f251e))
* **search:** populate subject in search results (tuxlink-g4dj) ([92626a0](https://github.com/cameronzucker/tuxlink/commit/92626a0da6368ddf49fbd18e661d0b2e1b21ef29))
* **search:** recover from SchemaDrift at build_service so SearchService installs ([2b046f7](https://github.com/cameronzucker/tuxlink/commit/2b046f7ba3676f7b7788e5a5e3562e8d727fa0dd))
* **shell:** aria-hide GPS-ready dot glyph + aria-label fresh-fix state (tuxlink-z5pz) ([325710f](https://github.com/cameronzucker/tuxlink/commit/325710fd4f78b182427ca0fd23070e2c1889ff06))
* **shell:** DashboardRibbon SSID options render bare integer (no -N prefix) ([db82383](https://github.com/cameronzucker/tuxlink/commit/db82383577b7ca4cf69eb2585c43edf8d949fe03))
* **shell:** delete ARDOP fieldset from Settings (tuxlink-jmfm) ([bcf6924](https://github.com/cameronzucker/tuxlink/commit/bcf69246f0354a6f45d2925ca49cbc4d3dbeab0d))
* **shell:** drop active_source from PositionStatusDto + read source from config (tuxlink-c79g T9) ([d6fe710](https://github.com/cameronzucker/tuxlink/commit/d6fe710673c5768ce1f31a1a99db7c84448ef69f))
* **shell:** GPS-ready hint in State 2 is passive &lt;span&gt;, not &lt;button&gt; (tuxlink-c79g T11) ([9fa0975](https://github.com/cameronzucker/tuxlink/commit/9fa09750c0320bae410378037c540dd53104ca68))
* **shell:** restore aria-pressed={false} on Manual source chip per spec §4.4 (tuxlink-c79g T12 follow-up) ([18a6594](https://github.com/cameronzucker/tuxlink/commit/18a6594f816980792299912c6bc6160651649510))
* **shell:** restore onUseGps prop on GridEdit + DashboardRibbon invocation (tuxlink-c79g T10) ([fd4cec6](https://github.com/cameronzucker/tuxlink/commit/fd4cec6f81c3dbd203e7f07f9d8f659a5f1030c0))
* **shell:** ribbon SSID is single click-to-edit callsign select ([022c09d](https://github.com/cameronzucker/tuxlink/commit/022c09d6b96beae80484a02693e6a78ecf954b36))
* **shell:** source chip is &lt;button&gt; (Manual) or &lt;span role=status&gt; (Gps) (tuxlink-c79g T12) ([c68d6dd](https://github.com/cameronzucker/tuxlink/commit/c68d6dd9068c14e1877935946b3ed340bc7cc47a))
* **shell:** SSID picker is bare -N + adjacent callsign chip (tuxlink-i63g) ([2c5593f](https://github.com/cameronzucker/tuxlink/commit/2c5593f39ceb14bf9725a77341cc681112b82114))
* **shell:** SSID propagates to ribbon callsign + inline-edit from status pane ([a82f620](https://github.com/cameronzucker/tuxlink/commit/a82f6205bc85a843b937ef767b847c632fead62a))
* **shell:** thread outbox queue depth into sidebar counts (tuxlink-gp8b) ([f447a98](https://github.com/cameronzucker/tuxlink/commit/f447a986c9b24a32a48455dd9b2f746d85c234f3))
* **shell:** widen radio-panel chrome 360 → 400 px across all modes (tuxlink-8rng) ([f8fa232](https://github.com/cameronzucker/tuxlink/commit/f8fa23202c2e30f9391d1fa99e3b2d0e3f7511c9))
* **test:** use import.meta.glob raw-CSS pattern for tuxlink-8rng tests (TEST-1) ([3c444dd](https://github.com/cameronzucker/tuxlink/commit/3c444dd7ed38aee74643b5036d3ec02c7e55ba2e))
* **ui:** pin search-zone to a fixed 560px so it doesn't reflow the dashboard ([aa0a640](https://github.com/cameronzucker/tuxlink/commit/aa0a64079ea7bbb2f36679a34ef2876c2b7c57a0))
* **ui:** radio panel takes its 400px from the reader only, not the message list (tuxlink-40u8) ([f7a8daa](https://github.com/cameronzucker/tuxlink/commit/f7a8daa0d55b1da7844a939eb628c61b7990e1b2))
* **winlink/ardop:** address Codex review findings on ARDOP listener (tuxlink-dhbl) ([3ee4750](https://github.com/cameronzucker/tuxlink/commit/3ee4750b11c37dd39ed60f9c27b75648e944eb20))
* **winlink/ax25:** address Codex review findings on Packet allowlist (tuxlink-inde) ([694ef81](https://github.com/cameronzucker/tuxlink/commit/694ef818e211ac30f7fc013e6fc1542133dc2fef))
* **winlink/listener:** address Codex review findings on listener-arms foundation ([d8030bc](https://github.com/cameronzucker/tuxlink/commit/d8030bcf2152aafd8051101d546b8f19beb3ab29))
* **winlink/telnet:** address Codex review findings on Telnet listener (tuxlink-xehu) ([183495b](https://github.com/cameronzucker/tuxlink/commit/183495b0ee80df5249808dd527354289031b0c3e))
* **winlink:** consume paired \r\n in telnet-login wrapper (tuxlink-0pnb) ([297c5e4](https://github.com/cameronzucker/tuxlink/commit/297c5e4d322fb544e25bfbe49e5d4e8636b9c338))
* **winlink:** disarm serial/Bluetooth transports at the OS layer on abort (tuxlink-0ja) ([a396eb4](https://github.com/cameronzucker/tuxlink/commit/a396eb4320581db987230e152dbe85dac8bf18c3))
* **winlink:** impl std::error::Error for ExchangeError so chain propagates (tuxlink-0pnb) ([614299e](https://github.com/cameronzucker/tuxlink/commit/614299e070daeb23b1dc44d7e9008c49ea7710c5))
* **winlink:** unblock dialer_login TCP deadlock + send CMSTelnet default password (tuxlink-0pnb) ([d872616](https://github.com/cameronzucker/tuxlink/commit/d8726162ce0b7da2f4acc91da979b99dbf0e2dbc))
* **winlink:** wire outbox + filing into telnet_p2p_connect (tuxlink-l55l) ([bb3dcbf](https://github.com/cameronzucker/tuxlink/commit/bb3dcbf5f3cda08bcc3189fc64cf8d6082346b05))


### Performance

* **mailbox:** lazy-split MessageView + React.memo FolderSidebar (tuxlink-u8z7) ([99b4a50](https://github.com/cameronzucker/tuxlink/commit/99b4a50110653103b6931250579fadcea644af5c))
* **shell:** kill 4Hz render storm + memoize message rows + scope clock tick (tuxlink-sndh) ([080e879](https://github.com/cameronzucker/tuxlink/commit/080e879b6aa242324f6e7a3d2846366616280e18))
* **shell:** lazy-load 5 radio panels + 2 search overlays (tuxlink-twym) ([f8e932b](https://github.com/cameronzucker/tuxlink/commit/f8e932b8b326536e50929520e489e3cabf7de4bd))
* **shell:** memoize useStatusData + React.memo ribbon + status bar (tuxlink-djnl) ([d218856](https://github.com/cameronzucker/tuxlink/commit/d2188566fe781266181869c6f8ff97ac114b9af1))
* **shell:** pre-paint skeleton + lazy-load panels for cold-start (tuxlink-k0q3) ([d910598](https://github.com/cameronzucker/tuxlink/commit/d9105989ff164454d171c61bcbed83059195f16e))


### Refactors

* **compose:** drop stale v0.0.1 / v0.1 version pins from UI strings ([304ee97](https://github.com/cameronzucker/tuxlink/commit/304ee97083150d9ca630e8ee6026daa849f785a7))
* **compose:** FormPicker now reads composableForms() (tuxlink-v1p) ([996e467](https://github.com/cameronzucker/tuxlink/commit/996e467244e36bef315536806d4c29fe5c27e327))
* **connections:** mirror TelnetRadioPanel structure for TelnetP2pRadioPanel + wire status pipeline (tuxlink-0pnb) ([d32540d](https://github.com/cameronzucker/tuxlink/commit/d32540d73ee175a725aaecf83d50683e7595a568))
* **forms:** make Form optional + add composableForms() helper (tuxlink-v1p) ([38f0020](https://github.com/cameronzucker/tuxlink/commit/38f0020beb3666284a4f27151f50530623fb8d4c))
* **forms:** strip Form registration from damage_assessment/index.ts (tuxlink-v1p) ([ec649ee](https://github.com/cameronzucker/tuxlink/commit/ec649ee5d19390ce96e7f4feee53114fd3d8e78a))
* **forms:** strip Form registration from ics309/index.ts (tuxlink-v1p) ([8071f34](https://github.com/cameronzucker/tuxlink/commit/8071f3449565d6b91e82f4dc8fd6a6f7c393d36b))
* **forms:** strip Form registration from position/index.ts (tuxlink-v1p) ([db7db62](https://github.com/cameronzucker/tuxlink/commit/db7db62d4129bbdc5c30d7b5d5009339719e690a))
* **help:** remove old modal HelpPanel + reroute dispatch (tuxlink-0gsy) ([55dabf9](https://github.com/cameronzucker/tuxlink/commit/55dabf9a0eff8d3ce264e6da506e39b1b12bc551))
* **mailbox:** drop stale v0.0.1 / v0.1 version pins from UI + comments ([2baacca](https://github.com/cameronzucker/tuxlink/commit/2baaccaae39a87929b71942d0ed655f852d8abe9))
* **scripts:** converge-build v2 — build from disposable worktree at origin/main ([43979ae](https://github.com/cameronzucker/tuxlink/commit/43979ae301ad41c0107093faf5265a66dc99030c))
* **shell+wizard:** drop stale v0.0.1 / v0.1 version pins from chrome + docs ([1b422c7](https://github.com/cameronzucker/tuxlink/commit/1b422c7578d28e9cbeade6064494b375a865cd0c))
* **shell:** delete legacy PacketConnectionPanel + ArdopDock + ArdopHfStub; simplify reading-pane (P3.4 + P4.7 cleanup) ([2508789](https://github.com/cameronzucker/tuxlink/commit/2508789eb1049ce6781f5d90cc976a19d25d6a94))
* **shell:** delete TelnetCmsPanel + reading-pane fallback to MessageView (radio-panel-telnet P2.4) ([7a86c1b](https://github.com/cameronzucker/tuxlink/commit/7a86c1b1f2fbb728dd693f31e286daa12d3d7b44))
* **shell:** drop sidebar conn-dot — duplicates DashboardRibbon (tuxlink-bcgj) ([120a1f9](https://github.com/cameronzucker/tuxlink/commit/120a1f94b206568bc6a10a910ee56bb1b76e0fe7))
* **status:** useStatusData via react-query so invalidate triggers refetch ([4636944](https://github.com/cameronzucker/tuxlink/commit/46369446b3431b8d6ca44da62e22dbf04bdba098))

## [0.23.4](https://github.com/cameronzucker/tuxlink/compare/v0.23.3...v0.23.4) (2026-06-03)


### Bug Fixes

* **perf:** adrev follow-ups — row-date staleness + sidebar memo + lazy-MessageView fallback (tuxlink-268k) ([b03d887](https://github.com/cameronzucker/tuxlink/commit/b03d88786ee530628851938e0f3f828863897cff))

## [0.23.3](https://github.com/cameronzucker/tuxlink/compare/v0.23.2...v0.23.3) (2026-06-03)


### Performance

* **mailbox:** lazy-split MessageView + React.memo FolderSidebar (tuxlink-u8z7) ([99b4a50](https://github.com/cameronzucker/tuxlink/commit/99b4a50110653103b6931250579fadcea644af5c))

## [0.23.2](https://github.com/cameronzucker/tuxlink/compare/v0.23.1...v0.23.2) (2026-06-03)


### Performance

* **shell:** memoize useStatusData + React.memo ribbon + status bar (tuxlink-djnl) ([d218856](https://github.com/cameronzucker/tuxlink/commit/d2188566fe781266181869c6f8ff97ac114b9af1))

## [0.23.1](https://github.com/cameronzucker/tuxlink/compare/v0.23.0...v0.23.1) (2026-06-03)


### Performance

* **shell:** lazy-load 5 radio panels + 2 search overlays (tuxlink-twym) ([f8e932b](https://github.com/cameronzucker/tuxlink/commit/f8e932b8b326536e50929520e489e3cabf7de4bd))

## [0.23.0](https://github.com/cameronzucker/tuxlink/compare/v0.22.1...v0.23.0) (2026-06-03)


### Features

* **menu:** wire menu:message:print (Ctrl+P) — tuxlink-j0m3 ([d6a47ae](https://github.com/cameronzucker/tuxlink/commit/d6a47aea376a59894058f576578b61d71356d6fa))

## [0.22.1](https://github.com/cameronzucker/tuxlink/compare/v0.22.0...v0.22.1) (2026-06-03)


### Performance

* **shell:** kill 4Hz render storm + memoize message rows + scope clock tick (tuxlink-sndh) ([080e879](https://github.com/cameronzucker/tuxlink/commit/080e879b6aa242324f6e7a3d2846366616280e18))

## [0.22.0](https://github.com/cameronzucker/tuxlink/compare/v0.21.0...v0.22.0) (2026-06-03)


### Features

* **connections:** wire P2P-VARA HF/FM in the sidebar (tuxlink-kb3s) ([2935590](https://github.com/cameronzucker/tuxlink/commit/2935590541ab0bae014d8aa12ff516e61b2e3b2e))

## [0.21.0](https://github.com/cameronzucker/tuxlink/compare/v0.20.1...v0.21.0) (2026-06-03)


### Features

* **winlink/listener:** shared listener-arms foundation (tuxlink-3o2o) ([ed3de34](https://github.com/cameronzucker/tuxlink/commit/ed3de34eb19d268f37a831e5595bb22611934086))

## [0.20.1](https://github.com/cameronzucker/tuxlink/compare/v0.20.0...v0.20.1) (2026-06-03)


### Performance

* **shell:** pre-paint skeleton + lazy-load panels for cold-start (tuxlink-k0q3) ([d910598](https://github.com/cameronzucker/tuxlink/commit/d9105989ff164454d171c61bcbed83059195f16e))

## [0.20.0](https://github.com/cameronzucker/tuxlink/compare/v0.19.0...v0.20.0) (2026-06-03)


### Features

* **mailbox:** MessageView attachment Save As (tuxlink-0fyj) ([f6b7171](https://github.com/cameronzucker/tuxlink/commit/f6b71714cdce17f886a1c4857013d0e18c6ad9cc))

## [0.19.0](https://github.com/cameronzucker/tuxlink/compare/v0.18.0...v0.19.0) (2026-06-03)


### Features

* **grib:** Saildocs GRIB-request framework (tuxlink-vrpk) ([e5f049a](https://github.com/cameronzucker/tuxlink/commit/e5f049aa1f9b41b90bf04feb5c70269a7a420b2e))


### Bug Fixes

* **forms:** update axum wildcard path syntax for 0.8 (tuxlink-prz6) ([a5be99b](https://github.com/cameronzucker/tuxlink/commit/a5be99baa4ffd42058fc1b4e617b858ffe422eae))

## [0.18.0](https://github.com/cameronzucker/tuxlink/compare/v0.17.2...v0.18.0) (2026-06-03)


### Features

* **mailbox:** user-folder mechanism — Phase 2 MVP (tuxlink-f62f) ([c2dd4be](https://github.com/cameronzucker/tuxlink/commit/c2dd4beaf86d3c7c5e08c58db31b58262860a839))

## [0.17.2](https://github.com/cameronzucker/tuxlink/compare/v0.17.1...v0.17.2) (2026-06-02)


### Bug Fixes

* **linux:** install both tuxlink.desktop + com.tuxlink.app.desktop variants (tuxlink-xcay) ([536de53](https://github.com/cameronzucker/tuxlink/commit/536de539d73bca66d7db606b70b9681325a83965))

## [0.17.1](https://github.com/cameronzucker/tuxlink/compare/v0.17.0...v0.17.1) (2026-06-02)


### Bug Fixes

* **deps:** bump react-dom 19.2.6 → 19.2.7 to match react (tuxlink-ola6) ([ed633d7](https://github.com/cameronzucker/tuxlink/commit/ed633d71a78ad93f5922e2737bc45a200b081ef3))

## [0.17.0](https://github.com/cameronzucker/tuxlink/compare/v0.16.0...v0.17.0) (2026-06-02)


### Features

* **linux:** install Tuxlink taskbar icon via .desktop entry + XDG icon paths (tuxlink-mj7i) ([bbc4465](https://github.com/cameronzucker/tuxlink/commit/bbc4465a6417ff1c7ec37d9b6c78b8b35bc85040))


### Bug Fixes

* **radio:** filter ARDOP Capture/Playback dropdowns to hardware-only (tuxlink-y7nq) ([0bc5090](https://github.com/cameronzucker/tuxlink/commit/0bc509025dc8bdd1b2607b2d47720c8b3066a8db))
* **search:** recover from SchemaDrift at build_service so SearchService installs ([2b046f7](https://github.com/cameronzucker/tuxlink/commit/2b046f7ba3676f7b7788e5a5e3562e8d727fa0dd))

## [0.16.0](https://github.com/cameronzucker/tuxlink/compare/v0.15.1...v0.16.0) (2026-06-02)


### Features

* **mailbox:** MessageList sort UI — operator-selectable sort with persistence ([3dc193a](https://github.com/cameronzucker/tuxlink/commit/3dc193aef0d2ba55997527d3de842963d36e273e))


### Bug Fixes

* **menu:** mark unwired Message/Session items disabled+badged (tuxlink-dpf) ([d796e98](https://github.com/cameronzucker/tuxlink/commit/d796e981d7f9f4c1774972492e0482d7884b3f3d))


### Refactors

* **status:** useStatusData via react-query so invalidate triggers refetch ([4636944](https://github.com/cameronzucker/tuxlink/commit/46369446b3431b8d6ca44da62e22dbf04bdba098))

## [0.15.1](https://github.com/cameronzucker/tuxlink/compare/v0.15.0...v0.15.1) (2026-06-02)


### Bug Fixes

* **modem/vara:** drop platformBlocked from onStartClick handler — was no-op-ing Start on aarch64 (tuxlink-poh6) ([70bb12f](https://github.com/cameronzucker/tuxlink/commit/70bb12f3420b724b3611544fd3680eb04c8316c9))

## [0.15.0](https://github.com/cameronzucker/tuxlink/compare/v0.14.1...v0.15.0) (2026-06-02)


### Features

* **shell:** switch ribbon liveGrid to ui_grid for LocalUiOnly-aware display (tuxlink-va1i) ([0475950](https://github.com/cameronzucker/tuxlink/commit/047595060cde80684de6d48a5614dcf9c77f103a))

## [0.14.1](https://github.com/cameronzucker/tuxlink/compare/v0.14.0...v0.14.1) (2026-06-02)


### Bug Fixes

* **shell:** thread outbox queue depth into sidebar counts (tuxlink-gp8b) ([f447a98](https://github.com/cameronzucker/tuxlink/commit/f447a986c9b24a32a48455dd9b2f746d85c234f3))

## [0.14.0](https://github.com/cameronzucker/tuxlink/compare/v0.13.1...v0.14.0) (2026-06-02)


### Features

* **modem/vara:** wire VARA TCP transport into UI (Phase 2 — tuxlink-dfmf) ([1f6c3ef](https://github.com/cameronzucker/tuxlink/commit/1f6c3ef1dcc7cc6f03dc24e6caf93550069d35c1))


### Refactors

* **shell:** drop sidebar conn-dot — duplicates DashboardRibbon (tuxlink-bcgj) ([120a1f9](https://github.com/cameronzucker/tuxlink/commit/120a1f94b206568bc6a10a910ee56bb1b76e0fe7))

## [0.13.1](https://github.com/cameronzucker/tuxlink/compare/v0.13.0...v0.13.1) (2026-06-02)


### Bug Fixes

* **radio:** theme-token the radio-panel chrome so light schemes don't wash out (tuxlink-he7h) ([6c40548](https://github.com/cameronzucker/tuxlink/commit/6c405481dbb1e3a9b615c20dd53a04d80b9f9a1b))

## [0.13.0](https://github.com/cameronzucker/tuxlink/compare/v0.12.0...v0.13.0) (2026-06-02)


### Features

* **shell:** wire Help menu + ship user-guide docs (tuxlink-35g0 + tuxlink-gq74) ([f313daf](https://github.com/cameronzucker/tuxlink/commit/f313dafb259e9d51ccb75afc577cec3a970693b4))

## [0.12.0](https://github.com/cameronzucker/tuxlink/compare/v0.11.1...v0.12.0) (2026-06-01)


### Features

* **githooks:** branch lifecycle state machine + pre-commit/pre-push hooks ([15f5723](https://github.com/cameronzucker/tuxlink/commit/15f57232453c6c60693b9c6adf6648c92e126146))
* **scripts:** v1 converge-build.sh + pnpm dev:converged wrapper ([25547ed](https://github.com/cameronzucker/tuxlink/commit/25547ed2fccc17f19674fde6cbfb16f87b51fc45))


### Bug Fixes

* **githooks:** branch-state-machine Codex P1+P2 dispositions ([27bf968](https://github.com/cameronzucker/tuxlink/commit/27bf968d35b8904f30f2928eb331a2494eb09706))
* **scripts:** converge-build v1 Codex P1+P3 dispositions ([33a0562](https://github.com/cameronzucker/tuxlink/commit/33a0562f737a59ebec992b243cb14da498a11110))

## [0.11.1](https://github.com/cameronzucker/tuxlink/compare/v0.11.0...v0.11.1) (2026-06-01)


### Bug Fixes

* **mailbox:** list_messages returns newest-first by date (tuxlink-mjc8) ([007778e](https://github.com/cameronzucker/tuxlink/commit/007778e9bea97a1f61e8e63d12b8c644ecf5f039))

## [0.11.0](https://github.com/cameronzucker/tuxlink/compare/v0.10.0...v0.11.0) (2026-06-01)


### Features

* **compose:** enable the Cc field end-to-end (tuxlink-h1km) ([4198aa6](https://github.com/cameronzucker/tuxlink/commit/4198aa629909a424a20a83b35385d8c20d49ca7e))

## [0.10.0](https://github.com/cameronzucker/tuxlink/compare/v0.9.0...v0.10.0) (2026-06-01)


### Features

* **analysis:** per-sub-carrier SNR estimator + serde output ([630675d](https://github.com/cameronzucker/tuxlink/commit/630675d5c790e946952f763e9cbb02efba8ae41d))
* **channel:** two-tap Watterson WattersonChannel core ([db750e7](https://github.com/cameronzucker/tuxlink/commit/db750e7639f8f5e1fd29c10dbe52a737d2e4f698))
* **cli:** pipe-friendly hf-channel-sim-cli for AI-agent harnesses ([9954bba](https://github.com/cameronzucker/tuxlink/commit/9954bbaaf3f04468e2cdf06ba5d7624b7eac9f7b))
* **fading:** spectrum-shaped complex-Gaussian Watterson tap process ([abec1e4](https://github.com/cameronzucker/tuxlink/commit/abec1e4a65eba308f99b32c6ccf725d8e25a3295))
* **hf-channel-sim:** initial AGPLv3 crate scaffolding ([a4dcb82](https://github.com/cameronzucker/tuxlink/commit/a4dcb82669d7233dc041f5cf856a90146f357b40))
* **noise:** AWGN generator decoupled from channel ([4caa603](https://github.com/cameronzucker/tuxlink/commit/4caa6037f2bacfb4e5269d0d8cd7071f50ab9ca2))
* **params:** ITU-R F.520 + F.1487 channel condition vocabulary ([3aaec08](https://github.com/cameronzucker/tuxlink/commit/3aaec08cad657a7fed830e3556c1acf10a9e721b))
* **report:** end-to-end characterization report + JSON ([ff1b1f1](https://github.com/cameronzucker/tuxlink/commit/ff1b1f1e4d88f8a2f2a0371ee4553a28c3ae1227))
* **rng:** seeded Xoshiro256++ + complex Gaussian draws ([bea1a5b](https://github.com/cameronzucker/tuxlink/commit/bea1a5bb3778c064cc5e225dc5eadde2b0fca700))
* **tuxmodem-phy:** 48kHz f32 audio buffer + wav round-trip helper ([0bbd9ba](https://github.com/cameronzucker/tuxlink/commit/0bbd9ba91c653f9deaa661ac26dd140981515d1c))
* **tuxmodem-phy:** BPSK / QPSK / 16-QAM / 64-QAM + max-log LLR ([a1e8e0a](https://github.com/cameronzucker/tuxlink/commit/a1e8e0a7d2ccb54b0bbc7adea41ba492fadb3f12))
* **tuxmodem-phy:** channel-sim adapter + BER sweep + ARDOP competence gate ([fd0c422](https://github.com/cameronzucker/tuxlink/commit/fd0c42223594bb67225a32c542245c4a9e9cbf78))
* **tuxmodem-phy:** crate skeleton + error taxonomy ([b710959](https://github.com/cameronzucker/tuxlink/commit/b710959e343ff671b8384a9c0539f3ef15a0ef3c))
* **tuxmodem-phy:** FEC bus contract + SNR-aware mode router + FT-818 gate ([87ee200](https://github.com/cameronzucker/tuxlink/commit/87ee200e4fa94165f5de3e7729117f3d3511d3b4))
* **tuxmodem-phy:** mode table + ModeHint/ResolvedMode/ModeFamily skeleton ([0ab38d0](https://github.com/cameronzucker/tuxlink/commit/0ab38d0b2efc1f9493e0761027879c7cfbb4d67f))
* **tuxmodem-phy:** narrow-FSK situational floor mode ([a950860](https://github.com/cameronzucker/tuxlink/commit/a950860af1504ae76a16fa04c83b548ee97ed496))
* **tuxmodem-phy:** OFDM equalizer + receiver (clean-channel round-trip) ([926de8f](https://github.com/cameronzucker/tuxlink/commit/926de8f78cb2d64a73962aec5c34cbefbd25e5b6))
* **tuxmodem-phy:** OFDM mode parameter table (Narrow/Mid/Wide) ([7531188](https://github.com/cameronzucker/tuxlink/commit/75311881738cc0a8c6327f74a886df2526609806))
* **tuxmodem-phy:** OFDM transmitter (one-symbol modulate) ([bee9f92](https://github.com/cameronzucker/tuxlink/commit/bee9f92a77c0d334ef4ccf237d84172af2d83372))
* **tuxmodem-phy:** PhyTransport API + NullPhy contract baseline ([9b8a531](https://github.com/cameronzucker/tuxlink/commit/9b8a53167db372501c868848c69cf2fa45c272d0))
* **tuxmodem-phy:** pilot-aided per-subcarrier SNR estimator (Phase 5) ([39f03be](https://github.com/cameronzucker/tuxlink/commit/39f03be5ad58d18dc97836cc00f1d714a3011fe5))
* **tuxmodem-phy:** synchronization infrastructure (Phase 4) ([28d26e8](https://github.com/cameronzucker/tuxlink/commit/28d26e81fbbc483149548f611c0f66b95b30c968))
* **tuxmodem-phy:** water-filling per-subcarrier bit-loader ([76c5c1a](https://github.com/cameronzucker/tuxlink/commit/76c5c1a1d60bd07ecbd6bc6ad2704ce913b74e4d))
* **tuxmodem-phy:** wide-band low-density OFDM floor (default robustness mode) ([262fc1f](https://github.com/cameronzucker/tuxlink/commit/262fc1f9f9218f76fa4e13f681ea4fc61e66ce3a))
* **tuxmodem:** scaffold AGPLv3 workspace for clean-sheet modem ([ed579aa](https://github.com/cameronzucker/tuxlink/commit/ed579aac452eca30ba11412028dbf1ad32061dbb))

## [0.9.0](https://github.com/cameronzucker/tuxlink/compare/v0.8.0...v0.9.0) (2026-06-01)


### Features

* **compose:** Phase 6 form integration per spec §7.1/§7.3 (tuxlink-v1p) ([608f3ff](https://github.com/cameronzucker/tuxlink/commit/608f3ff2f9f1e80c4253437957b38ac05bb850ed))
* **forms-ts:** FormPicker modal per spec §7.1 (tuxlink-v1p) ([1ba4d50](https://github.com/cameronzucker/tuxlink/commit/1ba4d50320e94707e6328bf29062ae8378f47dbb))
* **forms-ts:** Ics213Form per spec §7.1 (tuxlink-v1p) ([b70dfb9](https://github.com/cameronzucker/tuxlink/commit/b70dfb926b5ef8bf3ffe84130b14b1eb5398def1))
* **forms-ts:** Ics213View per spec §7.2 (tuxlink-v1p) ([3d8e764](https://github.com/cameronzucker/tuxlink/commit/3d8e7646dcb529faf4cc8a99fbb1dffe527dd53c))
* **forms-ts:** KeyValueView fallback for unknown forms (tuxlink-v1p) ([cce1512](https://github.com/cameronzucker/tuxlink/commit/cce1512c17205320140c9e0ba5250bc83ceba242))
* **forms-ts:** register ICS-213 in form registry (tuxlink-v1p) ([4101706](https://github.com/cameronzucker/tuxlink/commit/4101706fa26a51d0317f1a491aabeed2b7667562))
* **forms-ts:** registry contract per spec §5.2 (tuxlink-v1p) ([a1e37cc](https://github.com/cameronzucker/tuxlink/commit/a1e37ccc91c6b0005b5e6a2c183ca5e3e6d19e0d))
* **forms-ts:** TS types mirror per spec §6.1 (tuxlink-v1p) ([4b1f4b9](https://github.com/cameronzucker/tuxlink/commit/4b1f4b9a93ad212b66ba86a0f1603539bf9ce0b0))
* **forms:** bundle 4 additional Phase 9 forms per spec §8 (tuxlink-v1p) ([13a5c3a](https://github.com/cameronzucker/tuxlink/commit/13a5c3a9c22a832c3ff9d3f02484348b9d58e77a))
* **forms:** bundle ICS-213 form per spec §8 (tuxlink-v1p) ([eb78349](https://github.com/cameronzucker/tuxlink/commit/eb783493ad9d07e0c0419561cfaace0f976a80c9))
* **forms:** create module + types per spec §6.1 (tuxlink-v1p) ([ba34575](https://github.com/cameronzucker/tuxlink/commit/ba34575f43b65306e0b27a0718a73180f53c3224))
* **forms:** detect_form_attachment per spec §3 + §10 (tuxlink-v1p) ([3f99cc8](https://github.com/cameronzucker/tuxlink/commit/3f99cc83a81ef5ee866874212c4d75f85f3da1ed))
* **forms:** parse_form_xml — hardened per spec §3 + §10 (tuxlink-v1p) ([d3b6bca](https://github.com/cameronzucker/tuxlink/commit/d3b6bcaa9cb06335eaee34026887a77a7f6d2a6d))
* **forms:** serialize_form_xml + render_body_template per spec §3 (tuxlink-v1p) ([e0ee300](https://github.com/cameronzucker/tuxlink/commit/e0ee300422621c2c9a85a493e7a8375c4cbabaf2))
* **forms:** validation module — form_id regex + size caps (tuxlink-v1p) ([b9ffb7f](https://github.com/cameronzucker/tuxlink/commit/b9ffb7f1a07b78a95a8f74f163e3d1b55ffb8271))
* **ipc:** send_form Tauri command per spec rev-3 §5.1 (tuxlink-v1p) ([b9985e8](https://github.com/cameronzucker/tuxlink/commit/b9985e83459995ed6c1a4bdf62a4df93cd1d0135))
* **mailbox:** display-side attachment filename sanitization (tuxlink-v1p) ([91ff113](https://github.com/cameronzucker/tuxlink/commit/91ff113c596be839f8d784bab35b4bfe750cb033))
* **mailbox:** form-render dispatch in MessageView per spec §6.2 (tuxlink-v1p) ([7390316](https://github.com/cameronzucker/tuxlink/commit/7390316036bb0e74f27003b287d79c47c85c4b8e))
* **mailbox:** Reply-with-form button per spec §7.4 (Codex P2 [#6](https://github.com/cameronzucker/tuxlink/issues/6)) (tuxlink-v1p) ([5350809](https://github.com/cameronzucker/tuxlink/commit/53508095fc41f1c85b284698846ea28cf02ac668))
* **parse:** add form_id + form_payload to ParsedMessageDto (tuxlink-v1p) ([8f0f700](https://github.com/cameronzucker/tuxlink/commit/8f0f700ff552332bf75107d35823f1b19e55443c))
* **radio:** SessionLogSection — shared log section per spec §4.3 (radio-panel-telnet P2.1) ([125994f](https://github.com/cameronzucker/tuxlink/commit/125994f994c136211dbbe7daf79f6aad3f8bc10c))
* **radio:** TelnetRadioPanel + shared CSS primitives (radio-panel-telnet P2.2) ([5801ae0](https://github.com/cameronzucker/tuxlink/commit/5801ae00feb25b913bcd07bc8d84e3a8189e6c63))
* **reply:** reply-to-form + reply-with-form per spec §7.4 (tuxlink-v1p) ([2386144](https://github.com/cameronzucker/tuxlink/commit/2386144492d05d285947ccb40babc04d5f6b8442))
* **shell:** route Telnet selection to TelnetRadioPanel (radio-panel-telnet P2.3) ([7efe1eb](https://github.com/cameronzucker/tuxlink/commit/7efe1eb1d3fecb56457133a1d06d6ebb06beed50))
* **status:** event-driven backend_status — frontend sees every transition (operator smoke fix [#4](https://github.com/cameronzucker/tuxlink/issues/4)) ([9d3c2cd](https://github.com/cameronzucker/tuxlink/commit/9d3c2cd74fa9e067421018900048b2f6928074f8))


### Bug Fixes

* backend status honesty + reading-pane/panel decoupling (operator smoke fixes [#2](https://github.com/cameronzucker/tuxlink/issues/2)) ([8617768](https://github.com/cameronzucker/tuxlink/commit/8617768019327b50a270738cfd57767bfea61743))
* **cms:** hold Connected status visible for 1.5s before disconnect (operator smoke [#5](https://github.com/cameronzucker/tuxlink/issues/5)) ([2a5a0af](https://github.com/cameronzucker/tuxlink/commit/2a5a0afe50797230276085443834f2348fc25804))
* **forms-ts:** innerhtml-ban test uses import.meta.glob (no @types/node) (tuxlink-v1p) ([2d8fa1f](https://github.com/cameronzucker/tuxlink/commit/2d8fa1fd3f76a4b29223556ae3f8fbd93a795720))
* **forms:** apply Codex review P1+P2 findings (tuxlink-v1p) ([dbda3d8](https://github.com/cameronzucker/tuxlink/commit/dbda3d87770248bbac0d917e45fba3a4e8cebf81))
* **forms:** apply Codex round 2 findings (tuxlink-v1p) ([fd7e373](https://github.com/cameronzucker/tuxlink/commit/fd7e3739fbcec823f150c77344e10d6aea608446))
* **forms:** author per-form CSS, scrollable body, resize handles (tuxlink-v1p, tuxlink-ydrd) ([415b7c2](https://github.com/cameronzucker/tuxlink/commit/415b7c20ef9f1a6404a80fdeed32b3e4c762f40b))
* **forms:** style FormPicker + add keyboard navigation (tuxlink-v1p) ([4451d27](https://github.com/cameronzucker/tuxlink/commit/4451d27ee08e7c38e46886f98cf50a88ce6dba84))
* **parse:** detect forms via attachment name, not body XML prefix (tuxlink-v1p) ([7dc0368](https://github.com/cameronzucker/tuxlink/commit/7dc036874b705257f3e0db777c1e3fcb7b793653))
* **radio:** close snapshot/listen race in useSessionLog via seq-dedup merge (Codex R2) ([693e904](https://github.com/cameronzucker/tuxlink/commit/693e9040d3a1c12559f37462f56634cc475e0605))
* **radio:** restore Telnet controls + bump type scale + larger log section (operator smoke fixes) ([1ec6305](https://github.com/cameronzucker/tuxlink/commit/1ec63054d5cc948ec3320571432f4415084e2542))
* **radio:** session log fills remaining vertical space in radio panel ([ee9bb35](https://github.com/cameronzucker/tuxlink/commit/ee9bb35f57573a929911ba26a4a68500fa8c6b25))
* **radio:** wire SessionLogSection to backend events + read CMS endpoint from config (radio-panel-telnet P2 Codex fixes) ([42df27a](https://github.com/cameronzucker/tuxlink/commit/42df27afe8b727d7b1fc0ca3577974432092f52b))
* **search:** populate subject in search results (tuxlink-g4dj) ([92626a0](https://github.com/cameronzucker/tuxlink/commit/92626a0da6368ddf49fbd18e661d0b2e1b21ef29))


### Refactors

* **shell:** delete TelnetCmsPanel + reading-pane fallback to MessageView (radio-panel-telnet P2.4) ([7a86c1b](https://github.com/cameronzucker/tuxlink/commit/7a86c1b1f2fbb728dd693f31e286daa12d3d7b44))

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
