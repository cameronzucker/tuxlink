//! Direct station-list poll: DTOs + per-mode endpoint mapping + the text-listing parser.
//!
//! Endpoint + row format grounded live 2026-06-07 (see
//! `dev/scratch/canyon-catalog-grounding-LIVE-update.md` in the main checkout).
//! `GET https://cms.winlink.org:444/listings/<Mode>Listing.aspx?serviceCodes=PUBLIC[&historyhours=168]`
//! returns plain text; each station is a multi-line block:
//!
//! ```text
//! 8P6BWS.WINLINK, -/8P6BWS, [GK03ED: BRIDGESTOWN, -], (Sat, 06 Jun 2026 08:10:00 GMT)
//!    E  ishmael.cadogan@barbados.gov.bb     <- E = sysop email
//!    H  -                                    <- H = homepage
//!    A  BRIDGESTOWN, -                       <- A = additional info (city, state)
//!    -  3647.0 7092.0 10147.5                <- "-" line = frequency list in kHz
//! ```
//!
//! The parser DEGRADES TO RAW: any deviation yields `parsed_ok=false` with empty `gateways`
//! and `raw` retained — never an error, never a panic (design §Reply rendering / §Error handling).

use serde::{Deserialize, Serialize};

const LISTINGS_BASE: &str = "https://cms.winlink.org:444/listings";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ListingMode {
    VaraHf,
    Packet,
    ArdopHf,
    Pactor,
    RobustPacket,
}

impl ListingMode {
    /// The 5 modes with a CONFIRMED direct-poll endpoint. VARA FM is deferred —
    /// `RmsVaraFmListing.aspx` is HTTP 404; the real endpoint is undiscovered (bd follow-up).
    pub const ALL: [ListingMode; 5] = [
        ListingMode::VaraHf,
        ListingMode::Packet,
        ListingMode::ArdopHf,
        ListingMode::Pactor,
        ListingMode::RobustPacket,
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

    /// Confirmed quirk: `RmsPacketListing.aspx` is served WITHOUT a `historyhours` param;
    /// every other mode includes it.
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

    /// Build the absolute listing URL. `service_codes` is the operator-configured
    /// filter (default `"PUBLIC"`; space-separated for multiple) — a sysop-assigned
    /// directory tag, not a credential. See `winlink::credentials::service_codes_read`.
    pub fn listing_url(self, service_codes: &str, history_hours: u32) -> String {
        let base = format!(
            "{LISTINGS_BASE}/{}?serviceCodes={service_codes}",
            self.listing_file()
        );
        if self.uses_history_hours() {
            format!("{base}&historyhours={history_hours}")
        } else {
            base
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gateway {
    pub channel: String,            // "8P6BWS.WINLINK"
    pub callsign: String,           // "8P6BWS" (channel before the first '.'/'-')
    pub sysop_name: Option<String>, // None when "-"
    pub grid: Option<String>,       // "GK03ED"
    pub location: Option<String>,   // "BRIDGESTOWN, -"
    pub frequencies_khz: Vec<f64>,  // [3647.0, 7092.0, ...]
    pub last_update: Option<String>, // raw "Sat, 06 Jun 2026 08:10:00 GMT"
    pub email: Option<String>,
    pub homepage: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationListing {
    pub mode: ListingMode,
    pub title: Option<String>,
    pub gateways: Vec<Gateway>,
    pub raw: String,
    pub parsed_ok: bool,
    /// Unix millis when this listing was fetched; `None` for an in-memory parse.
    /// The cache stamps it on both fresh-store and stale-return paths so the UI can
    /// show an "as of <time>" caption (design §Error handling — no silent staleness).
    pub fetched_at_ms: Option<u64>,
}

/// Parse a `/listings/<Mode>Listing.aspx` text body into structured gateways.
/// DEGRADES TO RAW: a body that is not a recognizable channel listing (e.g. an
/// IIS/ASP.NET error page) yields `parsed_ok=false`, `gateways` empty, `raw` retained.
pub fn parse_listing(body: &str, mode: ListingMode) -> StationListing {
    // Mirror parser.rs: tolerate a leading UTF-8 BOM if the endpoint ever emits one.
    let body_stripped = body.strip_prefix('\u{FEFF}').unwrap_or(body);
    let raw = body.to_string();

    let title = body_stripped
        .lines()
        .map(|l| l.trim_end())
        .find(|l| l.contains("CHANNEL LISTING"))
        .map(str::to_string);

    // Only trust parsed gateways if the body actually looks like a channel listing —
    // a title line OR the canonical column-header row. This is the degrade-to-raw gate
    // that keeps ASP.NET error pages (which contain bracketed tokens) from minting bogus
    // gateways instead of triggering the "couldn't reach the listing service" fallback.
    let has_listing_marker = title.is_some()
        || body_stripped
            .lines()
            .any(|l| l.contains("Channel,") && l.contains("Callsign"));

    let gateways = if has_listing_marker {
        parse_station_blocks(body_stripped)
    } else {
        Vec::new()
    };
    let parsed_ok = has_listing_marker && !gateways.is_empty();

    StationListing {
        mode,
        title,
        gateways,
        raw,
        parsed_ok,
        fetched_at_ms: None,
    }
}

/// Detect the mode of a received station-listing reply from its self-identifying
/// header line (`WINLINK <MODE> CHANNEL LISTING`). This is how a radio-delivered
/// `PUB_*` "Update Via Radio" reply (tuxlink-xrbw) is recognized and routed to the
/// right `parse_listing` mode without subject-line or request-ID correlation.
///
/// Returns `None` when the body is not a recognizable channel listing (an NWS
/// weather reply, ordinary mail, an error page) — the caller then leaves it as a
/// plain message. VARA FM is excluded (no confirmed listing endpoint; the only
/// VARA listing tuxlink ingests is VARA HF).
pub fn detect_listing_mode(body: &str) -> Option<ListingMode> {
    let body = body.strip_prefix('\u{FEFF}').unwrap_or(body);
    let title = body
        .lines()
        .map(str::trim)
        .find(|l| l.contains("CHANNEL LISTING"))?
        .to_uppercase();
    // Order matters: "ROBUST PACKET" must be tested before "PACKET", and the
    // VARA-FM exclusion before the bare VARA match.
    if title.contains("ROBUST PACKET") {
        Some(ListingMode::RobustPacket)
    } else if title.contains("VARA FM") {
        None
    } else if title.contains("VARA") {
        Some(ListingMode::VaraHf)
    } else if title.contains("ARDOP") {
        Some(ListingMode::ArdopHf)
    } else if title.contains("PACTOR") {
        Some(ListingMode::Pactor)
    } else if title.contains("PACKET") {
        Some(ListingMode::Packet)
    } else {
        None
    }
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

/// A channel token looks like `CALL.WINLINK` or `CALL-SSID`: ASCII-alnum runs separated by
/// `.`/`-`, starting alnum, with at least one separator. Rejects HTML/CSS/error-page tokens
/// (anything with `<`, `{`, `"`, `/`, `:`, or whitespace).
fn looks_like_channel(token: &str) -> bool {
    !token.is_empty()
        && token.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && token.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && token.contains(['.', '-'])
}

/// Header: `<CHANNEL>, <SysopName>/<Callsign>, [<GRID>: <City, State>], (<last-update>)`.
/// Returns `None` for legend/separator/error-page lines. All slicing is anchored to ASCII
/// delimiters (`[ ] ( )`) found via `str::find` (valid char boundaries) — multibyte-safe.
fn parse_header_line(line: &str) -> Option<Gateway> {
    if line.is_empty() || line.starts_with(char::is_whitespace) {
        return None;
    }
    let grid_open = line.find('[')?;
    let grid_close = line[grid_open..].find(']').map(|i| grid_open + i)?;

    let pre = &line[..grid_open]; // "CHANNEL, SysopName/Callsign, "
    let mut head = pre.splitn(2, ',');
    let channel = head.next()?.trim().to_string();
    if !looks_like_channel(&channel) {
        return None;
    }
    // Empty-callsign guard: a channel of "." / "-" would yield an empty callsign — drop it.
    let callsign = channel
        .split(['.', '-'])
        .next()
        .filter(|s| !s.is_empty())?
        .to_string();

    let sysop_seg = head.next().unwrap_or("").trim().trim_end_matches(',').trim();
    let sysop_name = sysop_seg
        .split('/')
        .next()
        .map(str::trim)
        .map(str::to_string)
        .filter(|s| !s.is_empty() && s != "-");

    let grid_field = &line[grid_open + 1..grid_close]; // "GRID: City, State"
    let grid = grid_field
        .split(':')
        .next()
        .map(str::trim)
        .map(str::to_string)
        .filter(|s| !s.is_empty() && s != "-");
    let location = grid_field
        .split_once(':')
        .map(|x| x.1)
        .map(str::trim)
        .map(str::to_string)
        .filter(|s| !s.is_empty() && s != "-");

    let last_update = line[grid_close..].find('(').and_then(|i| {
        let start = grid_close + i + 1;
        line[start..]
            .find(')')
            .map(|j| line[start..start + j].trim().to_string())
    }).filter(|s| !s.is_empty());

    Some(Gateway {
        channel,
        callsign,
        sysop_name,
        grid,
        location,
        frequencies_khz: Vec::new(),
        last_update,
        email: None,
        homepage: None,
    })
}

fn apply_subline(g: &mut Gateway, line: &str) {
    let t = line.trim_start();
    let Some((code, rest)) = t.split_once(char::is_whitespace) else {
        return;
    };
    let rest = rest.trim();
    match code {
        "E" => g.email = Some(rest.to_string()).filter(|s| s != "-"),
        "H" => g.homepage = Some(rest.to_string()).filter(|s| s != "-"),
        "A" => {
            if g.location.is_none() {
                g.location = Some(rest.to_string()).filter(|s| s != "-");
            }
        }
        "-" => {
            g.frequencies_khz = rest
                .split_whitespace()
                .filter_map(|f| f.parse::<f64>().ok())
                .filter(|f| f.is_finite() && *f > 0.0) // reject NaN/inf/negatives
                .collect();
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_station(header_sysop: &str) -> String {
        format!(
            "WINLINK ARDOP CHANNEL LISTING - (Monday, June 8, 2026 03:42 UTC)\r\n\
             ~~~~~\r\n\
             Channel, Sysop Name / Callsign, [Grid], (last update)\r\n\
             ------------------------------------------------------------------\r\n\
             \r\n\
             AI4Y.WINLINK, {header_sysop}, [FM07CC: Wirtz, VA], (Sat, 06 Jun 2026 08:47:00 GMT)\r\n   \
             E  creas002@gmail.com\r\n   \
             A  Wirtz, VA\r\n   \
             -  3589.0 7101.6 10146.4 14096.4\r\n\r\n"
        )
    }

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
        assert!(!ListingMode::Packet.uses_history_hours());
        assert!(ListingMode::VaraHf.uses_history_hours());
    }

    #[test]
    fn listing_url_is_well_formed() {
        assert_eq!(
            ListingMode::ArdopHf.listing_url("PUBLIC", 168),
            "https://cms.winlink.org:444/listings/RmsArdopListing.aspx?serviceCodes=PUBLIC&historyhours=168"
        );
        assert_eq!(
            ListingMode::Packet.listing_url("PUBLIC", 168),
            "https://cms.winlink.org:444/listings/RmsPacketListing.aspx?serviceCodes=PUBLIC"
        );
    }

    #[test]
    fn parses_single_station_block() {
        let listing = parse_listing(&one_station("Richard Creasey/AI4Y"), ListingMode::ArdopHf);
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
        assert!(listing.raw.contains("AI4Y.WINLINK"));
        assert_eq!(listing.fetched_at_ms, None);
    }

    #[test]
    fn unknown_sysop_name_dash_becomes_none() {
        let listing = parse_listing(&one_station("-/AI4Y"), ListingMode::ArdopHf);
        assert_eq!(listing.gateways[0].sysop_name, None);
    }

    #[test]
    fn degrades_to_raw_on_aspnet_error_page() {
        // IIS/ASP.NET error pages contain bracketed tokens but no CHANNEL LISTING marker.
        let html = "<!DOCTYPE html><html><body>Server Error in '/' Application. \
                    <span>HttpException [0x80004005]</span> input[type=text] <a>[link]</a></body></html>";
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

    #[test]
    fn channel_of_only_separator_is_rejected() {
        // ".WINLINK" / "-x" must not mint an empty-callsign gateway.
        assert!(parse_header_line(".WINLINK, x/y, [AA00: a, b], (t)").is_none());
        assert!(parse_header_line("-, x/y, [AA00: a, b], (t)").is_none());
    }

    #[test]
    fn frequency_line_rejects_nan_inf_and_negatives() {
        let body = one_station("Bob/AI4Y").replace(
            "-  3589.0 7101.6 10146.4 14096.4",
            "-  3589.0 NaN inf -7.0 1e400 7101.6",
        );
        let listing = parse_listing(&body, ListingMode::ArdopHf);
        assert_eq!(listing.gateways[0].frequencies_khz, vec![3589.0, 7101.6]);
    }

    #[test]
    fn multibyte_sysop_name_does_not_panic_and_parses() {
        // Real station from the packet listing: "André/PI1ZTM" (non-ASCII 'é').
        let body = "WINLINK PACKET CHANNEL LISTING - (x)\r\n\
                    Channel, Sysop Name / Callsign, [Grid], (last update)\r\n\
                    \r\n\
                    PI1ZTM-12.RMS.PACKET, André/PI1ZTM, [JO22DB: Den Haag, zh], (Mon, 08 Jun 2026 02:38:00 GMT)\r\n   \
                    E  pd2atg@gmail.com\r\n   \
                    -  144.925\r\n";
        let listing = parse_listing(body, ListingMode::Packet);
        assert!(listing.parsed_ok);
        assert_eq!(listing.gateways[0].sysop_name.as_deref(), Some("André"));
        assert_eq!(listing.gateways[0].callsign, "PI1ZTM");
        assert_eq!(listing.gateways[0].frequencies_khz, vec![144.925]);
    }

    #[test]
    fn bom_prefixed_title_is_stripped() {
        let body = format!("\u{FEFF}{}", one_station("Bob/AI4Y"));
        let listing = parse_listing(&body, ListingMode::ArdopHf);
        assert!(listing.parsed_ok);
        assert!(listing.title.as_deref().unwrap().starts_with("WINLINK"));
    }

    // ---- detect_listing_mode (tuxlink-xrbw) ----------------------------------

    fn header(mode_words: &str) -> String {
        format!("WINLINK {mode_words} CHANNEL LISTING - (x)\r\nbody\r\n")
    }

    #[test]
    fn detect_mode_from_each_header() {
        assert_eq!(detect_listing_mode(&header("VARA")), Some(ListingMode::VaraHf));
        assert_eq!(detect_listing_mode(&header("ARDOP")), Some(ListingMode::ArdopHf));
        assert_eq!(detect_listing_mode(&header("PACTOR")), Some(ListingMode::Pactor));
        assert_eq!(detect_listing_mode(&header("PACKET")), Some(ListingMode::Packet));
    }

    #[test]
    fn detect_robust_packet_before_packet() {
        // "ROBUST PACKET" contains "PACKET" — order must not misclassify it.
        assert_eq!(
            detect_listing_mode(&header("ROBUST PACKET")),
            Some(ListingMode::RobustPacket)
        );
    }

    #[test]
    fn detect_mode_from_a_real_listing_body() {
        // one_station() carries a "WINLINK ARDOP CHANNEL LISTING" header.
        assert_eq!(detect_listing_mode(&one_station("Bob/AI4Y")), Some(ListingMode::ArdopHf));
        // BOM-prefixed bodies still detect.
        let bom = format!("\u{FEFF}{}", one_station("Bob/AI4Y"));
        assert_eq!(detect_listing_mode(&bom), Some(ListingMode::ArdopHf));
    }

    #[test]
    fn detect_mode_none_for_non_listings() {
        assert_eq!(detect_listing_mode(""), None);
        assert_eq!(detect_listing_mode("Just an ordinary email body.\r\nNothing here."), None);
        // An NWS area-weather reply is not a channel listing.
        assert_eq!(
            detect_listing_mode("FPUS55 KFGZ 032234\r\nZone Forecast Product for Northern Arizona"),
            None
        );
        // VARA FM is intentionally excluded (no confirmed listing endpoint).
        assert_eq!(detect_listing_mode(&header("VARA FM")), None);
    }
}
