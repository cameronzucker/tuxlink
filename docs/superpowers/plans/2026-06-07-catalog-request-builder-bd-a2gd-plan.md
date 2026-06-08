# Catalog Request Builder (bd-tuxlink-a2gd) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a location-aware Winlink Catalog request builder: a direct HTTPS station-list poll with a polite cache, a distance-sorted results UI, an in-band message-request path for info categories (reusing existing rails), and parse-with-fallback reply rendering (area-weather first).

**Architecture:** Additive layer on the already-built `tuxlink-ddiq` catalog inquiry subsystem. ONE new Rust capability — `catalog_fetch_stations` (HTTPS GET to `cms.winlink.org:444/listings/<Mode>Listing.aspx` → parse text rows → cache) — plus `catalog_parse_reply` (area-weather parser, degrade-to-raw). Frontend adds a sibling builder panel (form column + distance-sorted results), a local distance helper, and a structured reply view. Reuses `catalog_send_inquiry`/`catalog_list`/`useCatalog`, the `forms/updater.rs` polite-GET recipe, and the `grib/*` panel template UNCHANGED.

**Tech Stack:** Rust (Tauri commands, `reqwest 0.12`, `tokio`, `once_cell`, `serde`); React + TypeScript (Vite, plain hooks — NO TanStack/Redux in catalog files); Vitest + Testing Library (frontend), `cargo test` `#[cfg(test)]` (Rust).

**Grounding source of truth:** `dev/scratch/canyon-catalog-grounding-map.md` + `dev/scratch/canyon-catalog-grounding-LIVE-update.md` (the LIVE update WINS on any disagreement). Locked design: `docs/design/2026-06-07-catalog-request-builder-design.md`.

---

## Confirmed grounding facts (do not re-derive)

- **Endpoint (GET, HTTP 200, cert validates on :444):** `https://cms.winlink.org:444/listings/<file>?serviceCodes=PUBLIC[&historyhours=168]`. Per-mode files CONFIRMED live:
  | Mode key | file | historyhours |
  |---|---|---|
  | `vara-hf` | `RmsVaraListing.aspx` | 168 |
  | `packet` | `RmsPacketListing.aspx` | **omitted** (quirk) |
  | `ardop-hf` | `RmsArdopListing.aspx` | 168 |
  | `pactor` | `RmsPactorListing.aspx` | 168 |
  | `robust-packet` | `RmsRobustPacketListing.aspx` | 168 |
  - `vara-fm` → `RmsVaraFmListing.aspx` is **HTTP 404**; v1 omits it (bd follow-up to discover the real endpoint).
- **Listing body = plain text, per-station multi-line block** (frequencies in **kHz**):
  ```
  8P6BWS.WINLINK, -/8P6BWS, [GK03ED: BRIDGESTOWN, -], (Sat, 06 Jun 2026 08:10:00 GMT)
     E  ishmael.cadogan@barbados.gov.bb
     H  -
     A  BRIDGESTOWN, -
     -  3647.0 7092.0 10147.5 14118.0 18098.1
  ```
  Header line: `<CHANNEL>, <SysopName>/<Callsign>, [<GRID>: <City>, <State>], (<last-update>)`. Sub-lines keyed by 1-char code: `E`=email, `H`=homepage, `A`=additional info, `-`=frequency list (kHz). `-` as a value means "unknown".
- **Do NOT** copy Pat's `gateway/status.json` DTO (Hz, AccessKey, 48h clamp — different endpoint).
- **In-band request (reuse unchanged):** `To: INQUIRY@winlink.org`, `Subject: REQUEST`, body = one FILENAME/line, **≤10 filenames/message**. Reply: `From: SERVICE`, `Subject: INQUIRY - <url>`.
- **Polite-client reuse:** `reqwest 0.12` is a dep; `forms/updater.rs:106,113,162-176,189-194` has the UA-const / timeout / `classify_transport(https-only)` / client-builder recipe. Hand-roll the TTL cache (no new crate).
- **Home grid for distance:** `invoke<ConfigViewDto>('config_read').grid` (full 6-char) — NOT the precision-reduced status-bar grid.
- **Distance helper coordination:** CF agent owns the shared `haversineKm`; THIS plan adds a LOCAL `distanceKm` inside `src/catalog/` marked `// TODO: replace with CF's shared haversineKm`. Do NOT add a public helper to `src/forms/position/maidenhead.ts` (may import its `gridToLatLon`).
- **Favorites ★:** forward-hook only; gate the `favorite_upsert` call (CF-owned, unbuilt). Add bd dep edge.

## Test fixtures (staged, gitignored `dev/scratch/catalog-fixtures/`)

Direct-poll: `listing-{vara-hf,packet,ardop-hf,pactor,robust-packet}.txt`, `listing-vara-fm.txt` (404 HTML — negative fixture). Reply: `reply-area-weather-nws-fpus65.b2f` (v1 parser target), `reply-satellite-keps-celestrak.b2f`, `reply-service-advice.b2f`, `reply-aurora-image.b2f`. **Task 0 curates a trimmed committed subset into `src-tauri/tests/fixtures/catalog/`** (real PUBLIC callsign/sysop data — safe to commit).

## File Structure

**Create (Rust):** `src-tauri/src/catalog/stations.rs` (DTOs + `parse_listing`), `src-tauri/src/catalog/stations_cache.rs` (cache + `Clock`), `src-tauri/src/catalog/reply.rs` (category match + area-weather parser), `src-tauri/tests/fixtures/catalog/*` (committed fixtures).
**Modify (Rust):** `src-tauri/src/catalog/mod.rs` (declare modules + re-exports), `src-tauri/src/catalog/commands.rs` (add `catalog_fetch_stations`, `catalog_parse_reply`), `src-tauri/src/lib.rs` (register 2 commands).
**Create (TS):** `src/catalog/stationTypes.ts`, `src/catalog/useStations.ts`, `src/catalog/distance.ts`, `src/catalog/CatalogBuilderPanel.tsx` (+`.css`, +`.test.tsx`), `src/catalog/CatalogReplyView.tsx` (+`.css`, +`.test.tsx`).
**Modify (TS):** `src/catalog/useCatalog.ts` (add `fetchStations`/`parseReply` wrappers), `src/shell/AppShell.tsx` (lazy overlay gate — sibling to CatalogRequestPanel, NOT main-content switch), `src/shell/chrome/menuModel.ts` + `dispatchMenuAction.ts` (entry point), `src/shell/AppShell.test.tsx` (mount test).

---

## Task 0: Commit captured fixtures + scaffold modules

**Files:**
- Create: `src-tauri/tests/fixtures/catalog/listing-ardop-hf.txt` (+ vara-hf, packet, pactor, robust-packet, vara-fm-404.html)
- Create: `src-tauri/tests/fixtures/catalog/reply-area-weather-nws.txt`, `reply-service-advice.txt`
- Modify: `src-tauri/src/catalog/mod.rs`

- [ ] **Step 1: Curate + copy fixtures (trim listings to ~15 stations to keep fixtures small but representative; keep the full header+legend+blocks intact).**

```bash
WT=/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-a2gd-catalog-builder
SRC=/home/administrator/Code/tuxlink/dev/scratch/catalog-fixtures
DST="$WT/src-tauri/tests/fixtures/catalog"; mkdir -p "$DST"
# Listings: keep header/legend + first ~15 station blocks (a station block ends at a blank line).
for m in ardop-hf vara-hf packet pactor robust-packet; do
  awk 'BEGIN{b=0} /^[A-Z0-9]/{} {print} /^$/{b++} b>18{exit}' "$SRC/listing-$m.txt" > "$DST/listing-$m.txt"
done
cp "$SRC/listing-vara-fm.txt" "$DST/listing-vara-fm-404.html"
# Reply fixtures: strip the tuxlink message headers (keep from the blank line after headers).
awk 'f{print} /^$/{f=1}' "$SRC/reply-area-weather-nws-fpus65.b2f" > "$DST/reply-area-weather-nws.txt"
awk 'f{print} /^$/{f=1}' "$SRC/reply-service-advice.b2f" > "$DST/reply-service-advice.txt"
ls -la "$DST"
```

- [ ] **Step 2: Declare the new modules in `mod.rs`.**

Modify `src-tauri/src/catalog/mod.rs` to add (after the existing `pub mod parser;`):

```rust
pub mod reply;
pub mod stations;
pub mod stations_cache;

pub use stations::{parse_listing, Gateway, ListingMode, StationListing};
pub use reply::{parse_reply, ReplyView};
```

- [ ] **Step 3: Commit.**

```bash
git add src-tauri/tests/fixtures/catalog src-tauri/src/catalog/mod.rs
git commit -m "test(catalog): commit grounded listing+reply fixtures; scaffold station modules

Agent: canyon-poplar-tamarack
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```
(Note: `mod.rs` won't compile until Tasks 1-2 add the referenced items — that's fine; the next task's `cargo test` is the first green gate. If a pre-commit hook runs `cargo check`, do Step 2's edits as part of Task 1 instead.)

---

## Task 1: `ListingMode` enum + per-mode endpoint mapping

**Files:**
- Create: `src-tauri/src/catalog/stations.rs`
- Test: inline `#[cfg(test)] mod tests` in the same file

- [ ] **Step 1: Write the failing test.**

In `src-tauri/src/catalog/stations.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_listing_paths_match_confirmed_endpoints() {
        assert_eq!(ListingMode::VaraHf.listing_file(), "RmsVaraListing.aspx");
        assert_eq!(ListingMode::Packet.listing_file(), "RmsPacketListing.aspx");
        assert_eq!(ListingMode::ArdopHf.listing_file(), "RmsArdopListing.aspx");
        assert_eq!(ListingMode::Pactor.listing_file(), "RmsPactorListing.aspx");
        assert_eq!(ListingMode::RobustPacket.listing_file(), "RmsRobustPacketListing.aspx");
    }

    #[test]
    fn packet_omits_history_hours_others_include_it() {
        // Confirmed quirk: RmsPacketListing has no historyhours param.
        assert!(!ListingMode::Packet.uses_history_hours());
        assert!(ListingMode::VaraHf.uses_history_hours());
    }

    #[test]
    fn listing_url_is_well_formed() {
        let url = ListingMode::ArdopHf.listing_url("PUBLIC", 168);
        assert_eq!(
            url,
            "https://cms.winlink.org:444/listings/RmsArdopListing.aspx?serviceCodes=PUBLIC&historyhours=168"
        );
        let pkt = ListingMode::Packet.listing_url("PUBLIC", 168);
        assert_eq!(pkt, "https://cms.winlink.org:444/listings/RmsPacketListing.aspx?serviceCodes=PUBLIC");
    }
}
```

- [ ] **Step 2: Run to verify it fails.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib catalog::stations`
Expected: FAIL — `ListingMode` not found.

- [ ] **Step 3: Write minimal implementation (top of `stations.rs`).**

```rust
//! Direct station-list poll: DTOs + per-mode endpoint mapping + the text-listing parser.
//! Endpoint + row format grounded live 2026-06-07; see dev/scratch/canyon-catalog-grounding-LIVE-update.md.
use serde::{Deserialize, Serialize};

const LISTINGS_BASE: &str = "https://cms.winlink.org:444/listings";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ListingMode {
    VaraHf,
    Packet,
    ArdopHf,
    Pactor,
    RobustPacket,
}

impl ListingMode {
    /// The 5 modes with a CONFIRMED direct-poll endpoint (VARA FM deferred — bd follow-up).
    pub const ALL: [ListingMode; 5] = [
        ListingMode::VaraHf, ListingMode::Packet, ListingMode::ArdopHf,
        ListingMode::Pactor, ListingMode::RobustPacket,
    ];

    pub fn listing_file(self) -> &'static str {
        match self {
            ListingMode::VaraHf => "RmsVaraListing.aspx",
            ListingMode::Packet => "RmsPacketListing.aspx",
            ListingMode::ArdopHf => "RmsArdopListing.aspx",
            ListingMode::Pactor => "RmsPactorListing.aspx",
            ListingMode::RobustPacket => "RmsRobustPacketListing.aspx",
        }
    }

    /// Confirmed quirk: RmsPacketListing.aspx is served WITHOUT a historyhours param.
    pub fn uses_history_hours(self) -> bool {
        !matches!(self, ListingMode::Packet)
    }

    pub fn label(self) -> &'static str {
        match self {
            ListingMode::VaraHf => "VARA HF",
            ListingMode::Packet => "Packet",
            ListingMode::ArdopHf => "ARDOP HF",
            ListingMode::Pactor => "Pactor",
            ListingMode::RobustPacket => "Robust Packet",
        }
    }

    pub fn listing_url(self, service_codes: &str, history_hours: u32) -> String {
        let base = format!("{LISTINGS_BASE}/{}?serviceCodes={service_codes}", self.listing_file());
        if self.uses_history_hours() {
            format!("{base}&historyhours={history_hours}")
        } else {
            base
        }
    }
}
```

- [ ] **Step 4: Run to verify it passes.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib catalog::stations`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit.**

```bash
git add src-tauri/src/catalog/stations.rs
git commit -m "feat(catalog): ListingMode enum + grounded per-mode endpoint URLs

Agent: canyon-poplar-tamarack
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: `Gateway`/`StationListing` DTOs + `parse_listing` (the load-bearing parser)

**Files:**
- Modify: `src-tauri/src/catalog/stations.rs`
- Test: inline tests + `src-tauri/tests/catalog_listing_parse.rs` (fixture-driven)

- [ ] **Step 1: Add DTOs + write failing unit tests (append to `stations.rs` above the `#[cfg(test)]` mod, then add tests inside it).**

DTOs:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gateway {
    pub channel: String,               // "8P6BWS.WINLINK"
    pub callsign: String,              // "8P6BWS" (channel before first '.')
    pub sysop_name: Option<String>,    // None when "-"
    pub grid: Option<String>,          // "GK03ED"
    pub location: Option<String>,      // "BRIDGESTOWN, -"
    pub frequencies_khz: Vec<f64>,     // [3647.0, 7092.0, ...]
    pub last_update: Option<String>,   // raw "Sat, 06 Jun 2026 08:10:00 GMT"
    pub email: Option<String>,
    pub homepage: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationListing {
    pub mode: ListingMode,
    pub title: Option<String>,         // "WINLINK ARDOP CHANNEL LISTING - (...)"
    pub gateways: Vec<Gateway>,
    pub raw: String,                   // ALWAYS the full response (degrade-to-raw safety net)
    pub parsed_ok: bool,               // false if zero gateways parsed from a non-empty body
}
```

Unit tests (inside `mod tests`):

```rust
const ONE_STATION: &str = "WINLINK ARDOP CHANNEL LISTING - (Monday, June 8, 2026 03:42 UTC)\r\n\
~~~~~\r\n\
Channel, Sysop Name / Callsign, [Grid], (last update)\r\n\
------------------------------------------------------------------\r\n\
\r\n\
AI4Y.WINLINK, Richard Creasey/AI4Y, [FM07CC: Wirtz, VA], (Sat, 06 Jun 2026 08:47:00 GMT)\r\n\
   E  creas002@gmail.com\r\n\
   A  Wirtz, VA\r\n\
   -  3589.0 7101.6 10146.4 14096.4\r\n\
\r\n";

#[test]
fn parses_single_station_block() {
    let listing = parse_listing(ONE_STATION, ListingMode::ArdopHf);
    assert!(listing.parsed_ok);
    assert_eq!(listing.gateways.len(), 1);
    let g = &listing.gateways[0];
    assert_eq!(g.channel, "AI4Y.WINLINK");
    assert_eq!(g.callsign, "AI4Y");
    assert_eq!(g.sysop_name.as_deref(), Some("Richard Creasey"));
    assert_eq!(g.grid.as_deref(), Some("FM07CC"));
    assert_eq!(g.location.as_deref(), Some("Wirtz, VA"));
    assert_eq!(g.email.as_deref(), Some("creas002@gmail.com"));
    assert_eq!(g.frequencies_khz, vec![3589.0, 7101.6, 10146.4, 14096.4]);
    assert_eq!(g.last_update.as_deref(), Some("Sat, 06 Jun 2026 08:47:00 GMT"));
    // raw is always retained
    assert!(listing.raw.contains("AI4Y.WINLINK"));
}

#[test]
fn unknown_sysop_name_dash_becomes_none() {
    let txt = ONE_STATION.replace("Richard Creasey/AI4Y", "-/AI4Y");
    let listing = parse_listing(&txt, ListingMode::ArdopHf);
    assert_eq!(listing.gateways[0].sysop_name, None);
}

#[test]
fn degrades_to_raw_on_unparseable_body() {
    // HTML error page (the VARA-FM 404 shape) → parsed_ok=false, raw retained, no panic.
    let html = "<!DOCTYPE html><html><body>404</body></html>";
    let listing = parse_listing(html, ListingMode::Packet);
    assert!(!listing.parsed_ok);
    assert!(listing.gateways.is_empty());
    assert_eq!(listing.raw, html);
}

#[test]
fn empty_body_is_parsed_ok_false_not_panic() {
    let listing = parse_listing("", ListingMode::VaraHf);
    assert!(!listing.parsed_ok);
    assert!(listing.gateways.is_empty());
}
```

- [ ] **Step 2: Run to verify it fails.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib catalog::stations`
Expected: FAIL — `parse_listing` not found.

- [ ] **Step 3: Implement `parse_listing` + helpers (append to `stations.rs`).**

```rust
/// Parse a `/listings/<Mode>Listing.aspx` text body into structured gateways.
/// DEGRADES TO RAW: any deviation yields `parsed_ok=false` with `gateways` empty and
/// `raw` retained — never an error, never a panic (design §Reply rendering / §Error handling).
pub fn parse_listing(body: &str, mode: ListingMode) -> StationListing {
    let raw = body.to_string();
    let title = body.lines().next().map(|l| l.trim_end().to_string())
        .filter(|l| l.contains("CHANNEL LISTING"));
    let gateways = parse_station_blocks(body);
    let parsed_ok = !gateways.is_empty();
    StationListing { mode, title, gateways, raw, parsed_ok }
}

fn parse_station_blocks(body: &str) -> Vec<Gateway> {
    let mut out = Vec::new();
    let mut current: Option<Gateway> = None;
    for raw_line in body.lines() {
        let line = raw_line.trim_end_matches(['\r', '\n']);
        if let Some(g) = parse_header_line(line) {
            if let Some(prev) = current.take() {
                out.push(prev);
            }
            current = Some(g);
        } else if let Some(g) = current.as_mut() {
            apply_subline(g, line);
        }
    }
    if let Some(g) = current.take() {
        out.push(g);
    }
    out
}

/// Header: `<CHANNEL>, <SysopName>/<Callsign>, [<GRID>: <City, State>], (<last-update>)`
/// `<CHANNEL>` looks like `CALL.WINLINK` / `CALL-SSID`. Returns None for legend/separator lines.
fn parse_header_line(line: &str) -> Option<Gateway> {
    // Must start at column 0 with a token then ", " (sublines start with whitespace).
    if line.is_empty() || line.starts_with(' ') || line.starts_with('~') || line.starts_with('-') {
        return None;
    }
    let grid_open = line.find('[')?;
    let grid_close = line[grid_open..].find(']').map(|i| grid_open + i)?;
    let pre = &line[..grid_open]; // "CHANNEL, SysopName/Callsign, "
    let mut head = pre.splitn(2, ',');
    let channel = head.next()?.trim().to_string();
    if channel.is_empty() || !channel.contains('.') && !channel.contains('-') {
        return None;
    }
    let sysop_seg = head.next().unwrap_or("").trim().trim_end_matches(',').trim();
    let sysop_name = sysop_seg.split('/').next().map(str::trim).map(str::to_string)
        .filter(|s| !s.is_empty() && s != "-");
    let grid_field = &line[grid_open + 1..grid_close]; // "GRID: City, State"
    let grid = grid_field.split(':').next().map(str::trim).map(str::to_string)
        .filter(|s| !s.is_empty() && s != "-");
    let location = grid_field.splitn(2, ':').nth(1).map(str::trim).map(str::to_string)
        .filter(|s| !s.is_empty() && s != "-");
    let last_update = line[grid_close..].find('(')
        .and_then(|i| {
            let start = grid_close + i + 1;
            line[start..].find(')').map(|j| line[start..start + j].trim().to_string())
        })
        .filter(|s| !s.is_empty());
    let callsign = channel.split(['.', '-']).next().unwrap_or(&channel).to_string();
    Some(Gateway {
        channel, callsign, sysop_name, grid, location,
        frequencies_khz: Vec::new(), last_update, email: None, homepage: None,
    })
}

fn apply_subline(g: &mut Gateway, line: &str) {
    let t = line.trim_start();
    let (code, rest) = match t.split_once(char::is_whitespace) {
        Some((c, r)) => (c, r.trim()),
        None => return,
    };
    match code {
        "E" => g.email = Some(rest.to_string()).filter(|s| s != "-"),
        "H" => g.homepage = Some(rest.to_string()).filter(|s| s != "-"),
        "A" => { if g.location.is_none() { g.location = Some(rest.to_string()).filter(|s| s != "-"); } }
        "-" => g.frequencies_khz = rest.split_whitespace().filter_map(|f| f.parse::<f64>().ok()).collect(),
        _ => {}
    }
}
```

- [ ] **Step 4: Run to verify it passes.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib catalog::stations`
Expected: PASS.

- [ ] **Step 5: Add a fixture-driven integration test.**

Create `src-tauri/tests/catalog_listing_parse.rs`:

```rust
use tuxlink_lib::catalog::{parse_listing, ListingMode};

#[test]
fn parses_real_ardop_listing_fixture() {
    let body = include_str!("fixtures/catalog/listing-ardop-hf.txt");
    let listing = parse_listing(body, ListingMode::ArdopHf);
    assert!(listing.parsed_ok, "real fixture should parse at least one gateway");
    assert!(listing.gateways.len() >= 5, "expected several gateways, got {}", listing.gateways.len());
    // Every parsed gateway has a non-empty channel + callsign.
    for g in &listing.gateways {
        assert!(!g.channel.is_empty());
        assert!(!g.callsign.is_empty());
    }
    // At least one gateway carries frequencies in a plausible kHz range (1.8 MHz – 30 MHz).
    let has_hf = listing.gateways.iter().flat_map(|g| &g.frequencies_khz)
        .any(|&f| (1800.0..=30_000.0).contains(&f));
    assert!(has_hf, "ARDOP HF gateways should have HF-band kHz frequencies");
}

#[test]
fn vara_fm_404_html_degrades_to_raw() {
    let body = include_str!("fixtures/catalog/listing-vara-fm-404.html");
    let listing = parse_listing(body, ListingMode::Packet);
    assert!(!listing.parsed_ok);
    assert!(listing.gateways.is_empty());
    assert_eq!(listing.raw, body);
}
```

- [ ] **Step 6: Run + commit.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test catalog_listing_parse`
Expected: PASS.

```bash
git add src-tauri/src/catalog/stations.rs src-tauri/tests/catalog_listing_parse.rs
git commit -m "feat(catalog): parse_listing — grounded text-row parser with degrade-to-raw

Agent: canyon-poplar-tamarack
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Polite station cache (TTL + coalesce + min-refetch) with injectable clock

**Files:**
- Create: `src-tauri/src/catalog/stations_cache.rs`
- Test: inline `#[cfg(test)] mod tests`

- [ ] **Step 1: Write failing tests.**

In `src-tauri/src/catalog/stations_cache.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use std::sync::Arc;

    fn listing(n: usize) -> StationListing {
        StationListing { mode: ListingMode::VaraHf, title: None, gateways: vec![], raw: format!("v{n}"), parsed_ok: true }
    }

    #[tokio::test]
    async fn second_call_within_ttl_does_not_refetch() {
        let clock = Arc::new(MockClock::new(0));
        let cache = StationsCache::new(60_000, clock.clone()); // 60s TTL
        let calls = Arc::new(AtomicUsize::new(0));
        let key = CacheKey { mode: ListingMode::VaraHf, service_codes: "PUBLIC".into(), history_hours: 168 };
        let mk = || { let c = calls.clone(); async move { c.fetch_add(1, Ordering::SeqCst); Ok::<_, String>(listing(1)) } };
        cache.get_or_fetch(key.clone(), mk()).await.unwrap();
        clock.advance(30_000); // still within TTL
        cache.get_or_fetch(key.clone(), mk()).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "second call within TTL must serve cache");
    }

    #[tokio::test]
    async fn call_after_ttl_refetches() {
        let clock = Arc::new(MockClock::new(0));
        let cache = StationsCache::new(60_000, clock.clone());
        let calls = Arc::new(AtomicUsize::new(0));
        let key = CacheKey { mode: ListingMode::VaraHf, service_codes: "PUBLIC".into(), history_hours: 168 };
        let mk = || { let c = calls.clone(); async move { c.fetch_add(1, Ordering::SeqCst); Ok::<_, String>(listing(1)) } };
        cache.get_or_fetch(key.clone(), mk()).await.unwrap();
        clock.advance(60_001); // past TTL
        cache.get_or_fetch(key.clone(), mk()).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    pub struct MockClock(AtomicU64);
    impl MockClock { pub fn new(t: u64) -> Self { Self(AtomicU64::new(t)) } pub fn advance(&self, d: u64) { self.0.fetch_add(d, Ordering::SeqCst); } }
    impl Clock for MockClock { fn now_millis(&self) -> u64 { self.0.load(Ordering::SeqCst) } }
}
```

- [ ] **Step 2: Run to verify it fails.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib catalog::stations_cache`
Expected: FAIL — `StationsCache`/`Clock`/`CacheKey` not found.

- [ ] **Step 3: Implement the cache (top of `stations_cache.rs`).**

```rust
//! Polite-client cache for the station-list poll: TTL + min-refetch + coalescing, no new crate.
//! Lock-held-across-fetch (tokio::sync::Mutex) gives single-flight coalescing for free.
use crate::catalog::stations::{ListingMode, StationListing};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

pub trait Clock: Send + Sync {
    fn now_millis(&self) -> u64;
}

pub struct SystemClock;
impl Clock for SystemClock {
    fn now_millis(&self) -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub mode: ListingMode,
    pub service_codes: String,
    pub history_hours: u32,
}

struct Entry { listing: StationListing, fetched_at_ms: u64 }

pub struct StationsCache {
    ttl_ms: u64,
    clock: Arc<dyn Clock>,
    inner: Mutex<HashMap<CacheKey, Entry>>,
}

impl StationsCache {
    pub fn new(ttl_ms: u64, clock: Arc<dyn Clock>) -> Self {
        Self { ttl_ms, clock, inner: Mutex::new(HashMap::new()) }
    }

    /// Serve from cache if fresh; otherwise await `fetch` UNDER THE LOCK (coalesces concurrent
    /// callers onto one fetch + shared result). On fetch error, serve stale cache if present.
    pub async fn get_or_fetch<F, E>(&self, key: CacheKey, fetch: F) -> Result<StationListing, E>
    where
        F: Future<Output = Result<StationListing, E>>,
    {
        let mut guard = self.inner.lock().await;
        let now = self.clock.now_millis();
        if let Some(e) = guard.get(&key) {
            if now.saturating_sub(e.fetched_at_ms) < self.ttl_ms {
                return Ok(e.listing.clone());
            }
        }
        match fetch.await {
            Ok(listing) => {
                guard.insert(key, Entry { listing: listing.clone(), fetched_at_ms: now });
                Ok(listing)
            }
            Err(e) => {
                if let Some(stale) = guard.get(&key) {
                    return Ok(stale.listing.clone()); // offline/stale fallback (design §Error handling)
                }
                Err(e)
            }
        }
    }
}

impl ListingMode {
    // Hash needs Eq; ListingMode already derives Eq/Hash via Copy? add derive in stations.rs:
}
```

(Note: add `Hash` to `ListingMode`'s derive list in `stations.rs`: `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]`.)

- [ ] **Step 4: Run to verify it passes.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib catalog::stations_cache`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit.**

```bash
git add src-tauri/src/catalog/stations_cache.rs src-tauri/src/catalog/stations.rs
git commit -m "feat(catalog): polite station cache (TTL+coalesce+min-refetch, injectable clock)

Agent: canyon-poplar-tamarack
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: `catalog_fetch_stations` command (HTTP + cache + parse)

**Files:**
- Modify: `src-tauri/src/catalog/commands.rs`, `src-tauri/src/catalog/mod.rs`, `src-tauri/src/lib.rs`
- Test: inline tests (URL building, error mapping) + a `mockito` HTTP test

- [ ] **Step 1: Write the failing test (mockito-backed fetch; reuse `forms/updater.rs` loopback-http exemption).**

Append to `src-tauri/src/catalog/commands.rs` `#[cfg(test)] mod tests`:

```rust
#[tokio::test]
async fn fetch_parses_listing_from_http() {
    let mut server = mockito::Server::new_async().await;
    let body = include_str!("../../tests/fixtures/catalog/listing-ardop-hf.txt");
    let _m = server.mock("GET", mockito::Matcher::Any).with_status(200)
        .with_header("content-type", "application/text").with_body(body).create_async().await;
    // fetch_listing_from_url is the testable seam (no Tauri State); base url injected.
    let listing = super::fetch_listing_from_url(&format!("{}/x", server.url()), ListingMode::ArdopHf).await.unwrap();
    assert!(listing.parsed_ok);
    assert!(!listing.gateways.is_empty());
}
```

- [ ] **Step 2: Run to verify it fails.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib catalog::commands`
Expected: FAIL — `fetch_listing_from_url` not found.

- [ ] **Step 3: Implement the fetch seam + the command (append to `commands.rs`).**

```rust
use crate::catalog::stations::{parse_listing, ListingMode, StationListing};

const CATALOG_USER_AGENT: &str = concat!("Tuxlink/", env!("CARGO_PKG_VERSION"), " (catalog-station-poll)");
const CATALOG_HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Testable HTTP seam: GET `url`, parse as `mode`'s listing. https-only except loopback (mockito).
pub(crate) async fn fetch_listing_from_url(url: &str, mode: ListingMode) -> Result<StationListing, UiError> {
    let is_loopback = url.contains("127.0.0.1") || url.contains("localhost");
    let client = reqwest::Client::builder()
        .user_agent(CATALOG_USER_AGENT)
        .timeout(CATALOG_HTTP_TIMEOUT)
        .https_only(!is_loopback)
        .build()
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    let resp = client.get(url).send().await.map_err(|e| UiError::Transport { detail: e.to_string() })?;
    if !resp.status().is_success() {
        return Err(UiError::Unavailable { detail: format!("listing endpoint returned {}", resp.status()) });
    }
    let text = resp.text().await.map_err(|e| UiError::Transport { detail: e.to_string() })?;
    Ok(parse_listing(&text, mode)) // degrade-to-raw handled inside parse_listing
}

/// Fetch station lists for the given modes, via the polite cache (TTL/coalesce/min-refetch).
#[tauri::command]
pub async fn catalog_fetch_stations(
    modes: Vec<ListingMode>,
    service_codes: Option<String>,
    history_hours: Option<u32>,
    cache: tauri::State<'_, std::sync::Arc<crate::catalog::stations_cache::StationsCache>>,
) -> Result<Vec<StationListing>, UiError> {
    let service_codes = service_codes.unwrap_or_else(|| "PUBLIC".to_string());
    let history_hours = history_hours.unwrap_or(168);
    let mut out = Vec::with_capacity(modes.len());
    for mode in modes {
        let url = mode.listing_url(&service_codes, history_hours);
        let key = crate::catalog::stations_cache::CacheKey {
            mode, service_codes: service_codes.clone(), history_hours,
        };
        let listing = cache.get_or_fetch(key, fetch_listing_from_url(&url, mode)).await?;
        out.push(listing);
    }
    Ok(out)
}
```

- [ ] **Step 4: Register the cache as managed state + the command (`src-tauri/src/lib.rs`).**

In the `.manage(...)` setup block (near `lib.rs:88-159`), add:

```rust
.manage(std::sync::Arc::new(crate::catalog::stations_cache::StationsCache::new(
    30 * 60 * 1000, // 30 min TTL (proposed default — see plan §defaults)
    std::sync::Arc::new(crate::catalog::stations_cache::SystemClock),
)))
```

In the `tauri::generate_handler![...]` macro, in the catalog block (`lib.rs:423-426`), after `crate::catalog::commands::catalog_send_inquiry,`:

```rust
            // tuxlink-a2gd: location-aware station-list direct poll (cms.winlink.org:444/listings).
            crate::catalog::commands::catalog_fetch_stations,
            crate::catalog::commands::catalog_parse_reply,
```
(`catalog_parse_reply` is added in Task 5; add both lines now and stub the fn if executing strictly task-by-task, OR add this line in Task 5.)

- [ ] **Step 5: Run + commit.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib catalog && cargo check --manifest-path src-tauri/Cargo.toml`
Expected: PASS + clean check.

```bash
git add src-tauri/src/catalog/commands.rs src-tauri/src/lib.rs
git commit -m "feat(catalog): catalog_fetch_stations command (polite HTTP + cache + parse)

Agent: canyon-poplar-tamarack
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Reply parse-with-fallback (`reply.rs` + `catalog_parse_reply`)

**Files:**
- Create: `src-tauri/src/catalog/reply.rs`
- Modify: `src-tauri/src/catalog/commands.rs`
- Test: inline tests + fixture test against `reply-area-weather-nws.txt`

- [ ] **Step 1: Write failing tests in `reply.rs`.**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_weather_subject_matches_and_parses() {
        let subject = "INQUIRY - https://tgftp.nws.noaa.gov/data/raw/fp/fpus65.kpsr.sft.az.txt";
        let body = include_str!("../../tests/fixtures/catalog/reply-area-weather-nws.txt");
        let view = parse_reply(subject, body);
        match view {
            ReplyView::AreaWeather(w) => {
                assert!(w.product.contains("FPUS65"));
                assert!(w.office.to_lowercase().contains("phoenix"));
                assert!(!w.raw.is_empty());
            }
            other => panic!("expected AreaWeather, got {other:?}"),
        }
    }

    #[test]
    fn unknown_subject_renders_raw() {
        let view = parse_reply("Service Advice Message", "some unexpected body");
        assert!(matches!(view, ReplyView::Raw(ref s) if s == "some unexpected body"));
    }

    #[test]
    fn area_weather_marker_but_garbled_body_degrades_to_raw() {
        let subject = "INQUIRY - https://tgftp.nws.noaa.gov/data/raw/fp/xx.txt";
        let view = parse_reply(subject, "\u{fffd}\u{fffd} not a forecast");
        assert!(matches!(view, ReplyView::Raw(_)), "garbled weather body must degrade to raw");
    }
}
```

- [ ] **Step 2: Run to verify it fails.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib catalog::reply`
Expected: FAIL — `parse_reply`/`ReplyView` not found.

- [ ] **Step 3: Implement `reply.rs`.**

```rust
//! Catalog reply parse-with-fallback. v1 ships the area-weather parser; everything else renders raw.
//! Contract: ANY deviation degrades to ReplyView::Raw — never an error, never a blank (design §Reply rendering).
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ReplyView {
    AreaWeather(AreaWeather),
    Raw(String),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AreaWeather {
    pub product: String,   // "FPUS65 KPSR 050638"
    pub office: String,    // "National Weather Service Phoenix AZ"
    pub issued: String,    // "1138 PM MST Thu Jun 4 2026"
    pub raw: String,       // full body, always present (toggle target)
}

/// True for INQUIRY replies whose source URL is an NWS/SWPC area-weather text product.
fn is_area_weather(subject: &str) -> bool {
    let s = subject.to_ascii_lowercase();
    s.contains("inquiry -") && (s.contains("tgftp.nws.noaa.gov") || s.contains("nws.noaa.gov/data"))
}

pub fn parse_reply(subject: &str, body: &str) -> ReplyView {
    if is_area_weather(subject) {
        if let Some(w) = parse_area_weather(body) {
            return ReplyView::AreaWeather(w);
        }
    }
    ReplyView::Raw(body.to_string())
}

/// NWS text product: line 1 = product id (e.g. "FPUS65 KPSR 050638"); an office line containing
/// "National Weather Service"; a time-stamp line ending in a 4-digit year. Returns None (→ raw) if absent.
fn parse_area_weather(body: &str) -> Option<AreaWeather> {
    let lines: Vec<&str> = body.lines().map(str::trim_end).collect();
    let product = lines.iter().find(|l| {
        let u = l.trim();
        u.len() >= 6 && u.chars().take(6).all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            && u.split_whitespace().count() >= 2
    })?.trim().to_string();
    let office = lines.iter().find(|l| l.contains("National Weather Service"))?.trim().to_string();
    let issued = lines.iter().find(|l| {
        let toks: Vec<&str> = l.split_whitespace().collect();
        toks.last().map(|t| t.len() == 4 && t.chars().all(|c| c.is_ascii_digit())).unwrap_or(false)
            && (l.contains("AM ") || l.contains("PM "))
    }).map(|l| l.trim().to_string()).unwrap_or_default();
    Some(AreaWeather { product, office, issued, raw: body.to_string() })
}
```

- [ ] **Step 4: Add the `catalog_parse_reply` command (append to `commands.rs`).**

```rust
use crate::catalog::reply::{parse_reply, ReplyView};

/// Parse a received catalog reply (subject + decoded body) into a structured view or raw.
#[tauri::command]
pub fn catalog_parse_reply(subject: String, body: String) -> Result<ReplyView, UiError> {
    Ok(parse_reply(&subject, &body))
}
```

- [ ] **Step 5: Run + commit.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib catalog::reply && cargo check --manifest-path src-tauri/Cargo.toml`
Expected: PASS.

```bash
git add src-tauri/src/catalog/reply.rs src-tauri/src/catalog/commands.rs
git commit -m "feat(catalog): catalog_parse_reply — area-weather parser with degrade-to-raw

Agent: canyon-poplar-tamarack
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Frontend DTOs + invoke wrappers + distance helper

**Files:**
- Create: `src/catalog/stationTypes.ts`, `src/catalog/distance.ts`
- Modify: `src/catalog/useCatalog.ts`
- Test: `src/catalog/distance.test.ts`

- [ ] **Step 1: Write the failing distance test.**

`src/catalog/distance.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { distanceKm, distanceFromGrids } from './distance';

describe('distanceKm', () => {
  it('is ~0 for identical points', () => {
    expect(distanceKm({ lat: 33.4, lon: -112 }, { lat: 33.4, lon: -112 })).toBeCloseTo(0, 1);
  });
  it('matches a known great-circle distance (Phoenix↔LA ≈ 574 km)', () => {
    const d = distanceKm({ lat: 33.45, lon: -112.07 }, { lat: 34.05, lon: -118.24 });
    expect(d).toBeGreaterThan(560);
    expect(d).toBeLessThan(590);
  });
  it('returns null when a grid is unparseable', () => {
    expect(distanceFromGrids('NOTAGRID', 'DM43')).toBeNull();
  });
});
```

- [ ] **Step 2: Run to verify it fails.**

Run: `pnpm vitest run src/catalog/distance.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `distance.ts`.**

```ts
// Local great-circle distance for distance-sorted station results.
// TODO: replace with CF's shared haversineKm (src/forms/position/maidenhead.ts) once it lands
// (see session coordination brief — CF agent shoal-raven-gorge owns the shared export).
import { gridToLatLon, type LatLon } from '../forms/position/maidenhead';

const EARTH_RADIUS_KM = 6371;
const toRad = (d: number) => (d * Math.PI) / 180;

export function distanceKm(a: LatLon, b: LatLon): number {
  const dLat = toRad(b.lat - a.lat);
  const dLon = toRad(b.lon - a.lon);
  const lat1 = toRad(a.lat);
  const lat2 = toRad(b.lat);
  const h = Math.sin(dLat / 2) ** 2 + Math.cos(lat1) * Math.cos(lat2) * Math.sin(dLon / 2) ** 2;
  return 2 * EARTH_RADIUS_KM * Math.asin(Math.min(1, Math.sqrt(h)));
}

export function distanceFromGrids(gridA: string, gridB: string): number | null {
  const a = gridToLatLon(gridA);
  const b = gridToLatLon(gridB);
  if (!a || !b) return null;
  return distanceKm(a, b);
}

export const kmToMi = (km: number) => km * 0.621371;
```

- [ ] **Step 4: Create `stationTypes.ts` (mirror Rust serde camelCase).**

```ts
export type ListingMode = 'vara-hf' | 'packet' | 'ardop-hf' | 'pactor' | 'robust-packet';

export const LISTING_MODES: { mode: ListingMode; label: string }[] = [
  { mode: 'vara-hf', label: 'VARA HF' },
  { mode: 'packet', label: 'Packet' },
  { mode: 'ardop-hf', label: 'ARDOP HF' },
  { mode: 'pactor', label: 'Pactor' },
  { mode: 'robust-packet', label: 'Robust Packet' },
];

export interface Gateway {
  channel: string;
  callsign: string;
  sysopName: string | null;
  grid: string | null;
  location: string | null;
  frequenciesKhz: number[];
  lastUpdate: string | null;
  email: string | null;
  homepage: string | null;
}

export interface StationListing {
  mode: ListingMode;
  title: string | null;
  gateways: Gateway[];
  raw: string;
  parsedOk: boolean;
}

export type ReplyView =
  | { kind: 'area-weather'; product: string; office: string; issued: string; raw: string }
  | { kind: 'raw'; 0: string };
```

- [ ] **Step 5: Add invoke wrappers to `useCatalog.ts` (append, do not alter existing exports).**

```ts
import type { ListingMode, StationListing, ReplyView } from './stationTypes';

export async function fetchStations(
  modes: ListingMode[],
  opts?: { serviceCodes?: string; historyHours?: number },
): Promise<StationListing[]> {
  return invoke<StationListing[]>('catalog_fetch_stations', {
    modes,
    serviceCodes: opts?.serviceCodes ?? 'PUBLIC',
    historyHours: opts?.historyHours ?? 168,
  });
}

export async function parseReply(subject: string, body: string): Promise<ReplyView> {
  return invoke<ReplyView>('catalog_parse_reply', { subject, body });
}
```

- [ ] **Step 6: Run + commit.**

Run: `pnpm vitest run src/catalog/distance.test.ts && pnpm typecheck`
Expected: PASS.

```bash
git add src/catalog/distance.ts src/catalog/distance.test.ts src/catalog/stationTypes.ts src/catalog/useCatalog.ts
git commit -m "feat(catalog): station DTOs, fetch/parse wrappers, local distance helper

Agent: canyon-poplar-tamarack
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: `useStations` hook

**Files:**
- Create: `src/catalog/useStations.ts`, `src/catalog/useStations.test.ts`

- [ ] **Step 1: Write the failing test.**

`src/catalog/useStations.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { useStations } from './useStations';

beforeEach(() => vi.mocked(invoke).mockReset());

describe('useStations', () => {
  it('fetches + exposes gateways, sets loading false', async () => {
    vi.mocked(invoke).mockResolvedValue([
      { mode: 'ardop-hf', title: 't', parsedOk: true, raw: 'r', gateways: [
        { channel: 'AI4Y.WINLINK', callsign: 'AI4Y', sysopName: null, grid: 'FM07CC', location: null, frequenciesKhz: [7101.6], lastUpdate: null, email: null, homepage: null },
      ] },
    ]);
    const { result } = renderHook(() => useStations());
    act(() => { result.current.fetch(['ardop-hf']); });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.listings[0].gateways).toHaveLength(1);
    expect(result.current.error).toBeNull();
  });

  it('captures errors without throwing', async () => {
    vi.mocked(invoke).mockRejectedValue({ kind: 'Transport', detail: 'boom' });
    const { result } = renderHook(() => useStations());
    act(() => { result.current.fetch(['vara-hf']); });
    await waitFor(() => expect(result.current.error).not.toBeNull());
    expect(result.current.loading).toBe(false);
  });
});
```

- [ ] **Step 2: Run to verify it fails.** Run: `pnpm vitest run src/catalog/useStations.test.ts` → FAIL (no module).

- [ ] **Step 3: Implement `useStations.ts`.**

```ts
import { useCallback, useState } from 'react';
import { fetchStations } from './useCatalog';
import type { ListingMode, StationListing } from './stationTypes';

interface UseStations {
  listings: StationListing[];
  loading: boolean;
  error: string | null;
  fetch: (modes: ListingMode[], opts?: { serviceCodes?: string; historyHours?: number }) => void;
}

export function useStations(): UseStations {
  const [listings, setListings] = useState<StationListing[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetch = useCallback((modes: ListingMode[], opts?: { serviceCodes?: string; historyHours?: number }) => {
    setLoading(true);
    setError(null);
    fetchStations(modes, opts)
      .then((res) => setListings(res))
      .catch((e: unknown) => {
        const detail = (e as { detail?: string })?.detail ?? String(e);
        setError(detail);
      })
      .finally(() => setLoading(false));
  }, []);

  return { listings, loading, error, fetch };
}
```

- [ ] **Step 4: Run + commit.** Run: `pnpm vitest run src/catalog/useStations.test.ts && pnpm typecheck` → PASS.

```bash
git add src/catalog/useStations.ts src/catalog/useStations.test.ts
git commit -m "feat(catalog): useStations hook (fetch/loading/error)

Agent: canyon-poplar-tamarack
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: `CatalogBuilderPanel` — form column

**Files:**
- Create: `src/catalog/CatalogBuilderPanel.tsx`, `src/catalog/CatalogBuilderPanel.css`, `src/catalog/CatalogBuilderPanel.test.tsx`

- [ ] **Step 1: Write the failing test (form renders, mode toggle + Get Stations fires fetch).**

`src/catalog/CatalogBuilderPanel.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { CatalogBuilderPanel } from './CatalogBuilderPanel';

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === 'config_read') return { grid: 'DM43bp' };
    if (cmd === 'catalog_fetch_stations') return [];
    return undefined;
  });
});

describe('CatalogBuilderPanel form', () => {
  it('renders the form column with location, modes, radius', async () => {
    render(<CatalogBuilderPanel onClose={() => {}} />);
    expect(await screen.findByLabelText(/your location/i)).toBeTruthy();
    expect(screen.getByLabelText(/VARA HF/i)).toBeTruthy();
    expect(screen.getByLabelText(/within/i)).toBeTruthy();
  });

  it('calls catalog_fetch_stations with checked modes on Get Stations', async () => {
    render(<CatalogBuilderPanel onClose={() => {}} />);
    fireEvent.click(await screen.findByLabelText(/VARA HF/i));
    fireEvent.click(screen.getByRole('button', { name: /get stations/i }));
    await waitFor(() =>
      expect(vi.mocked(invoke)).toHaveBeenCalledWith('catalog_fetch_stations', expect.objectContaining({ modes: ['vara-hf'] })),
    );
  });
});
```

- [ ] **Step 2: Run to verify it fails.** Run: `pnpm vitest run src/catalog/CatalogBuilderPanel.test.tsx` → FAIL.

- [ ] **Step 3: Implement the panel skeleton + form column (`CatalogBuilderPanel.tsx`).**

```tsx
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { LISTING_MODES, type ListingMode } from './stationTypes';
import { useStations } from './useStations';
import { StationResults } from './StationResults';
import './CatalogBuilderPanel.css';

export interface CatalogBuilderPanelProps { onClose: () => void; }

const DEFAULT_RADIUS_MI = 300; // proposed default — plan §defaults

export function CatalogBuilderPanel({ onClose }: CatalogBuilderPanelProps) {
  const [grid, setGrid] = useState('');
  const [modes, setModes] = useState<Set<ListingMode>>(new Set());
  const [radiusMi, setRadiusMi] = useState(DEFAULT_RADIUS_MI);
  const stations = useStations();

  useEffect(() => {
    invoke<{ grid: string | null }>('config_read')
      .then((c) => { if (c.grid) setGrid(c.grid); })
      .catch(() => {});
  }, []);

  const toggleMode = (m: ListingMode) =>
    setModes((prev) => { const next = new Set(prev); next.has(m) ? next.delete(m) : next.add(m); return next; });

  const onGetStations = () => stations.fetch([...modes]);

  return (
    <div className="catalog-builder-overlay" role="dialog" aria-label="Catalog Request Builder">
      <div className="catalog-builder">
        <header className="catalog-builder__header">
          <h2>Find a Gateway</h2>
          <button className="catalog-builder__close" onClick={onClose} aria-label="Close">×</button>
        </header>
        <div className="catalog-builder__body">
          <form className="catalog-builder__form" onSubmit={(e) => { e.preventDefault(); onGetStations(); }}>
            <label className="catalog-field">
              <span>Your location</span>
              <input aria-label="Your location" value={grid} onChange={(e) => setGrid(e.target.value)} placeholder="Set your location" />
            </label>
            <fieldset className="catalog-field">
              <legend>Station modes</legend>
              {LISTING_MODES.map(({ mode, label }) => (
                <label key={mode} className="catalog-check">
                  <input type="checkbox" aria-label={label} checked={modes.has(mode)} onChange={() => toggleMode(mode)} />
                  {label}
                </label>
              ))}
            </fieldset>
            <label className="catalog-field">
              <span>Within</span>
              <input aria-label="within (miles)" type="range" min={50} max={3000} step={50}
                value={radiusMi} onChange={(e) => setRadiusMi(Number(e.target.value))} />
              <output>{radiusMi} mi</output>
            </label>
            <button type="submit" className="catalog-builder__go" disabled={modes.size === 0 || stations.loading}>
              {stations.loading ? 'Fetching…' : 'Get stations →'}
            </button>
          </form>
          <StationResults
            listings={stations.listings}
            error={stations.error}
            originGrid={grid}
            radiusMi={radiusMi}
          />
        </div>
      </div>
    </div>
  );
}
```

(`StationResults` is built in Task 9; create a 1-line stub `export function StationResults() { return null; }` in `src/catalog/StationResults.tsx` so this compiles, then flesh it out next task.)

- [ ] **Step 4: Add CSS following the flat `catalog-*` convention + design tokens.**

`CatalogBuilderPanel.css` (form column ~286px per design; constrain results to a realistic reading-pane width — `feedback_no_stretched_full_width_ui`):

```css
.catalog-builder-overlay { position: fixed; inset: 0; background: rgba(0,0,0,.5); display: grid; place-items: center; z-index: 50; }
.catalog-builder { background: var(--surface); color: var(--text); border: 1px solid var(--border); border-radius: 8px; width: min(900px, 92vw); max-height: 88vh; display: flex; flex-direction: column; }
.catalog-builder__header { display: flex; justify-content: space-between; align-items: center; padding: 12px 16px; border-bottom: 1px solid var(--border); }
.catalog-builder__close { background: none; border: none; color: var(--text-dim); font-size: 20px; cursor: pointer; }
.catalog-builder__body { display: grid; grid-template-columns: 286px 1fr; gap: 16px; padding: 16px; overflow: auto; }
.catalog-field { display: flex; flex-direction: column; gap: 6px; margin-bottom: 14px; }
.catalog-check { display: flex; gap: 8px; align-items: center; font-size: 14px; }
.catalog-builder__go { background: var(--accent); color: #fff; border: none; border-radius: 6px; padding: 8px 12px; cursor: pointer; }
.catalog-builder__go:disabled { opacity: .5; cursor: default; }
```

- [ ] **Step 5: Run + commit.** Run: `pnpm vitest run src/catalog/CatalogBuilderPanel.test.tsx && pnpm typecheck` → PASS.

```bash
git add src/catalog/CatalogBuilderPanel.tsx src/catalog/CatalogBuilderPanel.css src/catalog/CatalogBuilderPanel.test.tsx src/catalog/StationResults.tsx
git commit -m "feat(catalog): CatalogBuilderPanel form column (location/modes/radius)

Agent: canyon-poplar-tamarack
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 9: `StationResults` — distance-sorted rows, dim-beyond-radius, ★ forward hook

**Files:**
- Modify: `src/catalog/StationResults.tsx` (replace stub)
- Create: `src/catalog/StationResults.test.tsx`

- [ ] **Step 1: Write the failing test.**

`src/catalog/StationResults.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { StationResults } from './StationResults';
import type { StationListing } from './stationTypes';

const listing: StationListing = {
  mode: 'ardop-hf', title: 't', parsedOk: true, raw: 'r',
  gateways: [
    { channel: 'FAR.WINLINK', callsign: 'FAR', sysopName: null, grid: 'JN49', location: null, frequenciesKhz: [7101], lastUpdate: null, email: null, homepage: null },
    { channel: 'NEAR.WINLINK', callsign: 'NEAR', sysopName: null, grid: 'DM43', location: null, frequenciesKhz: [7102], lastUpdate: null, email: null, homepage: null },
  ],
};

describe('StationResults', () => {
  it('sorts by distance from origin (nearer first)', () => {
    render(<StationResults listings={[listing]} error={null} originGrid="DM43bp" radiusMi={300} />);
    const rows = screen.getAllByTestId('gateway-row');
    expect(rows[0]).toHaveTextContent('NEAR');
  });
  it('dims rows beyond the radius rather than hiding them', () => {
    render(<StationResults listings={[listing]} error={null} originGrid="DM43bp" radiusMi={50} />);
    expect(screen.getByText(/FAR/)).toBeTruthy(); // still present
    expect(screen.getAllByTestId('gateway-row').some((r) => r.className.includes('is-dim'))).toBe(true);
  });
  it('shows the message-request fallback offer on error', () => {
    render(<StationResults listings={[]} error="couldn't reach listing service" originGrid="" radiusMi={300} />);
    expect(screen.getByText(/request by message instead/i)).toBeTruthy();
  });
});
```

- [ ] **Step 2: Run to verify it fails.** Run: `pnpm vitest run src/catalog/StationResults.test.tsx` → FAIL.

- [ ] **Step 3: Implement `StationResults.tsx`.**

```tsx
import { useMemo } from 'react';
import { distanceFromGrids, kmToMi } from './distance';
import type { Gateway, StationListing } from './stationTypes';

interface Props {
  listings: StationListing[];
  error: string | null;
  originGrid: string;
  radiusMi: number;
  onAddFavorite?: (g: Gateway, mode: string) => void; // forward hook — CF-owned consumer
}

interface Row { g: Gateway; mode: string; distMi: number | null; }

export function StationResults({ listings, error, originGrid, radiusMi, onAddFavorite }: Props) {
  const rows = useMemo<Row[]>(() => {
    const all: Row[] = listings.flatMap((l) =>
      l.gateways.map((g) => {
        const km = g.grid ? distanceFromGrids(originGrid, g.grid) : null;
        return { g, mode: l.mode, distMi: km == null ? null : kmToMi(km) };
      }),
    );
    return all.sort((a, b) => (a.distMi ?? Infinity) - (b.distMi ?? Infinity));
  }, [listings, originGrid]);

  if (error) {
    return (
      <div className="catalog-results catalog-results--error">
        <p>{error}</p>
        <p className="catalog-results__fallback">Couldn't reach the listing service — request by message instead?</p>
      </div>
    );
  }

  return (
    <ul className="catalog-results">
      {rows.map(({ g, mode, distMi }) => {
        const dim = distMi != null && distMi > radiusMi;
        return (
          <li key={`${mode}:${g.channel}`} data-testid="gateway-row" className={`catalog-row${dim ? ' is-dim' : ''}`}>
            <span className="catalog-row__badge">{mode}</span>
            <span className="catalog-row__call">{g.callsign}</span>
            <span className="catalog-row__freq">{g.frequenciesKhz.map((f) => (f / 1000).toFixed(3)).join(', ')} MHz</span>
            <span className="catalog-row__grid">{g.grid ?? '—'}</span>
            <span className="catalog-row__dist">{distMi == null ? '—' : `${Math.round(distMi)} mi`}</span>
            <button className="catalog-row__star" aria-label={`Add ${g.callsign} to ${mode} favorites`}
              disabled={!onAddFavorite} onClick={() => onAddFavorite?.(g, mode)}>★</button>
          </li>
        );
      })}
    </ul>
  );
}
```

(The ★ is `disabled` until CF's `favorite_upsert` lands and is threaded via `onAddFavorite` — forward hook only.)

- [ ] **Step 4: Add the row CSS** (append to `CatalogBuilderPanel.css`):

```css
.catalog-results { list-style: none; margin: 0; padding: 0; overflow: auto; }
.catalog-row { display: grid; grid-template-columns: auto 1fr auto auto auto auto; gap: 10px; align-items: center; padding: 6px 8px; border-bottom: 1px solid var(--border); font-size: 13px; }
.catalog-row.is-dim { opacity: .4; }
.catalog-row__badge { background: var(--bg); color: var(--text-dim); border: 1px solid var(--border); border-radius: 4px; padding: 1px 6px; font-size: 11px; }
.catalog-row__star { background: none; border: none; color: var(--accent); cursor: pointer; }
.catalog-row__star:disabled { color: var(--text-dim); opacity: .5; cursor: default; }
.catalog-results--error { color: var(--danger); }
```

- [ ] **Step 5: Run + commit.** Run: `pnpm vitest run src/catalog/StationResults.test.tsx && pnpm typecheck` → PASS.

```bash
git add src/catalog/StationResults.tsx src/catalog/StationResults.test.tsx src/catalog/CatalogBuilderPanel.css
git commit -m "feat(catalog): distance-sorted station results, dim-beyond-radius, star forward hook

Agent: canyon-poplar-tamarack
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 10: Builder footer — queue info-category requests (reuse `catalog_send_inquiry`, ≤10 cap)

**Files:**
- Modify: `src/catalog/CatalogBuilderPanel.tsx`
- Modify: `src/catalog/CatalogBuilderPanel.test.tsx`

- [ ] **Step 1: Add a failing test for the queue action.**

Append to `CatalogBuilderPanel.test.tsx`:

```tsx
it('queues info-category requests via catalog_send_inquiry and confirms', async () => {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === 'config_read') return { grid: 'DM43bp' };
    if (cmd === 'catalog_send_inquiry') return 'MID123';
    return [];
  });
  render(<CatalogBuilderPanel onClose={() => {}} />);
  fireEvent.click(await screen.findByLabelText(/area weather/i));
  fireEvent.click(screen.getByRole('button', { name: /queue 1 request/i }));
  await waitFor(() =>
    expect(vi.mocked(invoke)).toHaveBeenCalledWith('catalog_send_inquiry', { filenames: expect.any(Array) }),
  );
  expect(await screen.findByText(/arrive in your inbox after the next connect/i)).toBeTruthy();
});
```

- [ ] **Step 2: Run to verify it fails.** Run: `pnpm vitest run src/catalog/CatalogBuilderPanel.test.tsx -t "queues info"` → FAIL.

- [ ] **Step 3: Add the info-category section + footer to `CatalogBuilderPanel.tsx`.**

Add near the top:
```tsx
import { sendCatalogInquiry } from './useCatalog';

// v1 info categories that the listing endpoint can't serve (filenames from winlink-queries.txt).
const INFO_CATEGORIES: { id: string; label: string; filename: string }[] = [
  { id: 'area-weather', label: 'Area weather', filename: 'US.ALL' },
  { id: 'propagation', label: 'Propagation', filename: 'AUR_TONIGHT' },
  { id: 'bulletins', label: 'Bulletins', filename: 'WL2K_HELP' },
];
const MAX_INQUIRY_FILENAMES = 10; // confirmed WLE cap
```
Add state + handler inside the component:
```tsx
const [infoCats, setInfoCats] = useState<Set<string>>(new Set());
const [queueState, setQueueState] = useState<{ kind: 'idle' } | { kind: 'sending' } | { kind: 'done'; count: number } | { kind: 'error'; message: string }>({ kind: 'idle' });
const toggleCat = (id: string) =>
  setInfoCats((prev) => { const n = new Set(prev); n.has(id) ? n.delete(id) : n.add(id); return n; });
const onQueue = async () => {
  const filenames = INFO_CATEGORIES.filter((c) => infoCats.has(c.id)).map((c) => c.filename).slice(0, MAX_INQUIRY_FILENAMES);
  if (filenames.length === 0) return;
  setQueueState({ kind: 'sending' });
  try { await sendCatalogInquiry(filenames); setQueueState({ kind: 'done', count: filenames.length }); }
  catch (e) { setQueueState({ kind: 'error', message: (e as { detail?: string })?.detail ?? String(e) }); }
};
```
Add the markup (inside the form, after the modes fieldset, plus a footer below):
```tsx
<fieldset className="catalog-field">
  <legend>Also request (by message)</legend>
  {INFO_CATEGORIES.map((c) => (
    <label key={c.id} className="catalog-check">
      <input type="checkbox" aria-label={c.label} checked={infoCats.has(c.id)} onChange={() => toggleCat(c.id)} />
      {c.label}
    </label>
  ))}
</fieldset>
{infoCats.size > 0 && (
  <footer className="catalog-builder__footer">
    <button type="button" onClick={onQueue} disabled={queueState.kind === 'sending'}>
      Queue {infoCats.size} request{infoCats.size > 1 ? 's' : ''}
    </button>
    {queueState.kind === 'done' && <p>Queued — they'll arrive in your Inbox after the next connect.</p>}
    {queueState.kind === 'error' && <p className="catalog-results--error">{queueState.message}</p>}
  </footer>
)}
```

- [ ] **Step 4: Run + commit.** Run: `pnpm vitest run src/catalog/CatalogBuilderPanel.test.tsx && pnpm typecheck` → PASS.

```bash
git add src/catalog/CatalogBuilderPanel.tsx src/catalog/CatalogBuilderPanel.test.tsx
git commit -m "feat(catalog): builder footer queues info-category requests via existing inquiry rails

Agent: canyon-poplar-tamarack
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 11: `CatalogReplyView` — structured area-weather + raw toggle

**Files:**
- Create: `src/catalog/CatalogReplyView.tsx`, `src/catalog/CatalogReplyView.test.tsx`

- [ ] **Step 1: Write the failing test.**

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { CatalogReplyView } from './CatalogReplyView';

beforeEach(() => vi.mocked(invoke).mockReset());

describe('CatalogReplyView', () => {
  it('renders a structured area-weather view + raw toggle', async () => {
    vi.mocked(invoke).mockResolvedValue({ kind: 'area-weather', product: 'FPUS65 KPSR', office: 'NWS Phoenix AZ', issued: '1138 PM', raw: 'RAWBODY' });
    render(<CatalogReplyView subject="INQUIRY - https://tgftp.nws.noaa.gov/x" body="b" />);
    expect(await screen.findByText(/NWS Phoenix AZ/)).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: /show raw/i }));
    expect(screen.getByText('RAWBODY')).toBeTruthy();
  });
  it('renders raw when parse returns raw', async () => {
    vi.mocked(invoke).mockResolvedValue({ kind: 'raw', 0: 'just text' });
    render(<CatalogReplyView subject="Service Advice Message" body="just text" />);
    await waitFor(() => expect(screen.getByText('just text')).toBeTruthy());
  });
});
```

- [ ] **Step 2: Run to verify it fails.** Run: `pnpm vitest run src/catalog/CatalogReplyView.test.tsx` → FAIL.

- [ ] **Step 3: Implement `CatalogReplyView.tsx`.**

```tsx
import { useEffect, useState } from 'react';
import { parseReply } from './useCatalog';
import type { ReplyView } from './stationTypes';

export function CatalogReplyView({ subject, body }: { subject: string; body: string }) {
  const [view, setView] = useState<ReplyView | null>(null);
  const [showRaw, setShowRaw] = useState(false);

  useEffect(() => {
    let live = true;
    parseReply(subject, body)
      .then((v) => { if (live) setView(v); })
      .catch(() => { if (live) setView({ kind: 'raw', 0: body }); }); // fallback on invoke failure
    return () => { live = false; };
  }, [subject, body]);

  if (!view) return <pre className="catalog-reply__raw">{body}</pre>;
  if (view.kind === 'raw') return <pre className="catalog-reply__raw">{(view as { 0: string })[0]}</pre>;

  return (
    <div className="catalog-reply">
      <dl className="catalog-reply__structured">
        <dt>Office</dt><dd>{view.office}</dd>
        <dt>Product</dt><dd>{view.product}</dd>
        <dt>Issued</dt><dd>{view.issued}</dd>
      </dl>
      <button type="button" onClick={() => setShowRaw((s) => !s)}>{showRaw ? 'Hide raw' : 'Show raw'}</button>
      {showRaw && <pre className="catalog-reply__raw">{view.raw}</pre>}
    </div>
  );
}
```

- [ ] **Step 4: Run + commit.** Run: `pnpm vitest run src/catalog/CatalogReplyView.test.tsx && pnpm typecheck` → PASS.

```bash
git add src/catalog/CatalogReplyView.tsx src/catalog/CatalogReplyView.test.tsx
git commit -m "feat(catalog): CatalogReplyView — structured area-weather + raw toggle/auto-fallback

Agent: canyon-poplar-tamarack
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 12: Mount the builder (lazy overlay) + entry points + App-level test

**Files:**
- Modify: `src/shell/AppShell.tsx` (lazy import + open-flag state + handler + render gate — sibling to CatalogRequestPanel, NOT the main-content switch)
- Modify: `src/shell/chrome/menuModel.ts`, `src/shell/chrome/dispatchMenuAction.ts`
- Modify: `src/shell/AppShell.test.tsx`

- [ ] **Step 1: Add the App-level mount test (production path).**

Append to `src/shell/AppShell.test.tsx`:

```tsx
it('opens the Catalog Builder from the menu (production mount path)', async () => {
  // catalog_fetch_stations + config_read are mocked via the existing invoke mock in this file.
  renderShell();
  // open via the menu affordance used by other panels in this suite:
  await openMenuItem('menu:message:catalog_builder'); // helper already in this file's harness
  expect(await screen.findByRole('dialog', { name: /catalog request builder/i }, { timeout: 10000 })).toBeTruthy();
});
```
(If the suite has no `openMenuItem` helper, drive the same affordance the existing `catalog_request` test uses; mirror that test exactly.)

- [ ] **Step 2: Run to verify it fails.** Run: `pnpm vitest run src/shell/AppShell.test.tsx -t "Catalog Builder"` → FAIL.

- [ ] **Step 3: Wire the 4 touch points in `AppShell.tsx`** (mirror the `CatalogRequestPanel` pattern at `:57-59,:255,:666,:1035-1038`):

```tsx
// (1) lazy import, beside the CatalogRequestPanel lazy import (~:57-59)
const CatalogBuilderPanel = lazy(() =>
  import('../catalog/CatalogBuilderPanel').then((m) => ({ default: m.CatalogBuilderPanel })));

// (2) open-flag state (~:255, beside catalogRequestOpen)
const [catalogBuilderOpen, setCatalogBuilderOpen] = useState(false);

// (3) handler entry in the handlers useMemo (~:666)
'menu:message:catalog_builder': () => setCatalogBuilderOpen(true),

// (4) render gate (~:1035-1038, beside the CatalogRequestPanel gate)
{catalogBuilderOpen && (
  <Suspense fallback={null}>
    <CatalogBuilderPanel onClose={() => setCatalogBuilderOpen(false)} />
  </Suspense>
)}
```

- [ ] **Step 4: Add the menu entries** — `menuModel.ts` (beside `:47`):

```ts
{ id: 'menu:message:catalog_builder', label: 'Find a Gateway…' },
```
and a dispatch case in `dispatchMenuAction.ts` mirroring the existing `catalog_request` case.

- [ ] **Step 5: Run + commit.** Run: `pnpm vitest run src/shell/AppShell.test.tsx && pnpm typecheck` → PASS.

```bash
git add src/shell/AppShell.tsx src/shell/AppShell.test.tsx src/shell/chrome/menuModel.ts src/shell/chrome/dispatchMenuAction.ts
git commit -m "feat(catalog): mount Catalog Builder via lazy overlay + Find a Gateway menu entry

Agent: canyon-poplar-tamarack
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 13: VARA-FM follow-up + full verification + adrev handoff

**Files:** (no source — bookkeeping + verification)

- [ ] **Step 1: File the VARA-FM endpoint-discovery follow-up.**

```bash
bd create --title="Discover VARA FM station-listing endpoint (RmsVaraFmListing.aspx is 404)" \
  --description="catalog_fetch_stations ships 5 confirmed modes (vara-hf/packet/ardop-hf/pactor/robust-packet). RmsVaraFmListing.aspx returns 404. Discover the real VARA-FM listing endpoint (likely folded into Packet/VHF listing or a different filename) and add the mode. UI/parser are mode-agnostic — adding it is config-only in ListingMode + LISTING_MODES." \
  --type=task --priority=3
bd dep add <this-id> tuxlink-a2gd  # follow-up depends on a2gd landing
```

- [ ] **Step 2: Full local verification (record provenance: worktree + branch + SHA).**

```bash
WT=/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-a2gd-catalog-builder
cargo test --manifest-path "$WT/src-tauri/Cargo.toml" --lib catalog
cargo test --manifest-path "$WT/src-tauri/Cargo.toml" --test catalog_listing_parse
pnpm -C "$WT" vitest run src/catalog src/shell/AppShell.test.tsx
pnpm -C "$WT" typecheck
pkill -9 -f vitest 2>/dev/null; pgrep -f vitest || echo "no vitest zombies"
```
Expected: all green; no vitest zombies.

- [ ] **Step 3: Codex cross-provider adversarial review** (build-robust-features mandate). Attack angles: (a) parser panics/DoS on malformed listing bodies; (b) cache lock-across-await deadlock / coalescing correctness / stale-on-error; (c) https-only bypass / SSRF via mode→URL; (d) reply parse-with-fallback never errors/blanks; (e) frequency unit (kHz vs Hz) + distance math; (f) ★ forward-hook gating. Converge the 4 proposed defaults (TTL 30m, min-refetch 15m, UA string, radius 300mi/mi). Use the CLAUDE.md stdin `review -` pattern; tee to `dev/adversarial/`.

- [ ] **Step 4:** Address adrev findings (new commits), re-verify, then proceed to the PR.

---

## Self-Review (run after writing — done)

**Spec coverage:** direct-poll (Tasks 1-4) ✓; polite cache TTL/coalesce/min-refetch (Task 3) ✓; in-band message request reuse + ≤10 cap (Task 10) ✓; parse-with-fallback area-weather + raw (Tasks 5, 11) ✓; builder UX form+results, distance-sort, dim-beyond-radius, ★ hook (Tasks 8-9) ✓; error handling direct-poll→message fallback + stale "as of" (Task 9 + cache stale-on-error) ✓; entry points + mount + App-level test (Task 12) ✓; out-of-scope VARA-FM + private listings respected (Task 13 follow-up; PUBLIC-only) ✓; 4 open defaults proposed (header + Tasks 4/8) ✓.

**Type consistency:** `ListingMode` kebab serde ↔ TS union ✓; `Gateway`/`StationListing` camelCase serde ↔ TS interfaces ✓; `ReplyView` tagged enum ↔ TS union ✓; `fetchStations`/`parseReply`/`sendCatalogInquiry` wrapper names stable ✓; `catalog_fetch_stations`/`catalog_parse_reply` command names stable across Rust + TS + lib.rs ✓.

**Placeholder scan:** no TBD/TODO-implementation; every code step shows code. (One intentional `// TODO: replace with CF's shared haversineKm` is a coordination marker, not a plan gap.)

## Notes for the executor
- "as of <time>" stale-cache stamp: `StationListing` has no timestamp field; if the design's stale-stamp is wanted in v1, add `fetched_at_ms: Option<u64>` to `StationListing` set by the cache, surfaced in `StationResults`. Flagged for adrev (kept out of the core tasks to avoid threading time through the parser).
- Do NOT touch CF-owned files: `useFavorites`/`useContacts`, `stations.json`/`contacts.json`, the six `favorite_*` commands, Compose/`useDraft.ts`, `FolderSidebar.tsx`, the AppShell main-content switch, `RadioPanel.tsx`, and `src/forms/position/maidenhead.ts` (import only).
- `src-tauri/src/lib.rs` invoke_handler + `.manage()` are EXPECTED textual merge points with the CF branch — clean concatenation per the coordination brief.
