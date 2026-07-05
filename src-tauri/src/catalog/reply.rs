//! Catalog reply parse-with-fallback for NWS area-weather products (tuxlink-qyjr).
//!
//! Contract: ANY deviation degrades gracefully — never an error, never a blank
//! (design §Reply rendering). A recognised NWS product always yields the header
//! (`product`/`office`/`issued`/`title`); the body is additionally decoded into a
//! structured `forecast` when it matches a known shape (SFT tabular grid, ZFP
//! zone forecast) and otherwise carries `Forecast::None` (header + raw only). A
//! reply that isn't an NWS product at all renders `ReplyView::Raw`.
//!
//! The `ReplyView` and `Forecast` enums use STRUCT / struct-wrapping variants on
//! purpose: internally-tagged serde (`#[serde(tag = "kind")]`) cannot serialize a
//! newtype variant wrapping a `String`/primitive, only one wrapping a struct/map.
//! `Raw { text }` is a struct variant for exactly this reason (see the round-trip
//! test); `Forecast::Tabular(..)`/`Zone(..)` wrap structs, which is fine.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ReplyView {
    AreaWeather(AreaWeather),
    Raw { text: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AreaWeather {
    pub product: String, // AWIPS id line, e.g. "FPUS65 KPSR 090626"
    pub office: String,  // "National Weather Service Phoenix AZ"
    pub issued: String,  // "1126 PM MST Mon Jun 8 2026" (may be empty)
    pub title: String,   // "Tabular State Forecast for ..." (may be empty)
    pub forecast: Forecast,
    pub raw: String, // full body, always present (Show-raw toggle target)
}

/// The decoded forecast body. `None` = recognised NWS product whose body shape we
/// don't (yet) structure — the header still renders, body falls to raw.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Forecast {
    Tabular(TabularForecast),
    Zone(ZoneForecast),
    None,
}

// ---- SFT: Tabular State Forecast (locations × days grid) --------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabularForecast {
    pub days: Vec<ForecastDay>,
    pub regions: Vec<ForecastRegion>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastDay {
    pub dow: String,  // "Tue"
    pub date: String, // "Jun 09"
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastRegion {
    pub name: String, // "SOUTH-CENTRAL ARIZONA"
    pub locations: Vec<ForecastLocation>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastLocation {
    pub name: String,             // "Phoenix"
    pub cells: Vec<ForecastCell>, // one per day
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastCell {
    pub condition: String, // "Vryhot" / "Sunny" / "Ptcldy"
    pub low: String,       // "77"  (may be "MM"/"-"/"")
    pub high: String,      // "106"
    pub pop_night: String, // "00"  (precip % 6PM-6AM)
    pub pop_day: String,   // "00"  (precip % 6AM-6PM)
}

// ---- ZFP: Zone Forecast Product (zones × named periods) ---------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneForecast {
    pub zones: Vec<ForecastZone>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastZone {
    pub name: String,   // "Western Mogollon Rim"
    pub cities: String, // "Flagstaff, Williams, and Munds Park" (prefix stripped)
    pub periods: Vec<ForecastPeriod>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastPeriod {
    pub label: String, // "REST OF TONIGHT" / "TUESDAY"
    pub text: String,  // "Mostly clear. Lows 43 to 53. ..."
}

// ---- entry -----------------------------------------------------------------

/// True for INQUIRY replies whose source URL is an NWS area-weather text product.
fn is_nws_subject(subject: &str) -> bool {
    let s = subject.to_ascii_lowercase();
    s.contains("inquiry -") && s.contains("nws.noaa.gov")
}

/// AWIPS product line shape: `TTAAII CCCC DDHHMM` (e.g. "FPUS65 KPSR 090626").
fn is_awips_product(line: &str) -> bool {
    let mut it = line.split_whitespace();
    let (Some(ttaaii), Some(cccc), Some(ddhhmm)) = (it.next(), it.next(), it.next()) else {
        return false;
    };
    if it.next().is_some() {
        return false; // exactly three tokens
    }
    let ttaaii_ok = ttaaii.len() == 6
        && ttaaii.chars().take(4).all(|c| c.is_ascii_uppercase())
        && ttaaii.chars().skip(4).all(|c| c.is_ascii_digit());
    let cccc_ok = cccc.len() == 4 && cccc.chars().all(|c| c.is_ascii_alphanumeric());
    let ddhhmm_ok = ddhhmm.len() == 6 && ddhhmm.chars().all(|c| c.is_ascii_digit());
    ttaaii_ok && cccc_ok && ddhhmm_ok
}

pub fn parse_reply(subject: &str, body: &str) -> ReplyView {
    // An NWS product is identified by the subject URL OR (robustness) by an AWIPS
    // line + office line in the body, so a reply still structures even if the
    // subject is unusual.
    let has_awips = body.lines().map(str::trim).any(is_awips_product);
    let has_office = body.lines().any(|l| l.contains("National Weather Service"));
    if is_nws_subject(subject) || (has_awips && has_office) {
        if let Some(w) = parse_area_weather(body) {
            return ReplyView::AreaWeather(w);
        }
    }
    ReplyView::Raw { text: body.to_string() }
}

/// Build the header (always) + decode the body into a structured forecast when
/// it matches a known shape. Returns `None` (→ raw) only when there is no AWIPS
/// product line at all (i.e. not an NWS text product).
fn parse_area_weather(body: &str) -> Option<AreaWeather> {
    let lines: Vec<&str> = body.lines().map(str::trim_end).collect();
    let office_idx = lines.iter().position(|l| l.contains("National Weather Service"));
    let product = lines
        .iter()
        .map(|l| l.trim())
        .find(|l| is_awips_product(l))
        .unwrap_or("")
        .to_string();
    // Must look like an NWS text product: an AWIPS id line OR an office line.
    // (A garbled body with neither falls through to raw.)
    if product.is_empty() && office_idx.is_none() {
        return None;
    }
    let office = office_idx.map(|i| lines[i].trim().to_string()).unwrap_or_default();

    // The product title is the non-empty line immediately above the office line
    // ("Tabular State Forecast for ..." / "Zone Forecast Product for ...").
    let title = office_idx
        .and_then(|i| lines[..i].iter().rev().find(|l| !l.trim().is_empty()))
        .map(|l| l.trim().to_string())
        .filter(|t| !is_awips_product(t) && t.chars().any(|c| c.is_ascii_lowercase()))
        .unwrap_or_default();

    let issued = lines
        .iter()
        .map(|l| l.trim())
        .find(|l| is_issued_line(l))
        .unwrap_or_default()
        .to_string();

    // Pick the body decoder from the product title so a malformed SFT can't fall
    // through to the ZFP parser (which would misread SFT region headers as zone
    // "periods") and vice versa. An unrecognised title tries both, tabular first.
    let tl = title.to_ascii_lowercase();
    let forecast = if tl.contains("tabular") {
        parse_tabular(&lines).map(Forecast::Tabular).unwrap_or(Forecast::None)
    } else if tl.contains("zone forecast") {
        parse_zone(&lines).map(Forecast::Zone).unwrap_or(Forecast::None)
    } else {
        parse_tabular(&lines)
            .map(Forecast::Tabular)
            .or_else(|| parse_zone(&lines).map(Forecast::Zone))
            .unwrap_or(Forecast::None)
    };

    Some(AreaWeather { product, office, issued, title, forecast, raw: body.to_string() })
}

/// An issued-time line: has " AM "/" PM " and a bare 4-digit year token anywhere.
/// (`any`, not last-token, so dual-time lines like
/// "1132 PM MST Mon Jun 8 2026 /1232 AM MDT Tue Jun 9 2026/" — whose last token is
/// "2026/" — are still recognised; otherwise the ZFP city loop swallows them.)
fn is_issued_line(l: &str) -> bool {
    (l.contains(" AM ") || l.contains(" PM "))
        && l.split_whitespace()
            .any(|y| y.len() == 4 && y.chars().all(|c| c.is_ascii_digit()))
}

// ---- SFT tabular parser ----------------------------------------------------

/// A region header line, e.g. "...SOUTH-CENTRAL ARIZONA...".
fn region_name(l: &str) -> Option<String> {
    let t = l.trim();
    if t.len() > 6 && t.starts_with("...") && t.ends_with("...") {
        Some(t.trim_matches('.').trim().to_string())
    } else {
        None
    }
}

/// True if every token is a slash pair of digit-runs / MM / dash, e.g. "77/106"
/// or "00/00" or "MM/MM". Used to anchor the temp + precip data rows.
fn is_slash_row(tokens: &[&str]) -> bool {
    !tokens.is_empty()
        && tokens.iter().all(|t| {
            let mut parts = t.splitn(2, '/');
            match (parts.next(), parts.next()) {
                (Some(a), Some(b)) => is_value_cell(a) && is_value_cell(b),
                _ => false,
            }
        })
}

/// A single temp/PoP value inside a slash pair. Accepts:
/// * `""` — MISSING (the "Today" column carries no overnight low and no
///   nighttime PoP when the product is issued in the morning, so its cells are
///   `/104` and `/00`; `ForecastCell.low`/`.pop_night` are documented as "may be ''").
/// * `"MM"` / `"-"` — NWS missing-data / below-zero markers.
/// * a run of ASCII digits.
///
/// Empty was previously rejected (`!s.is_empty()`), which made every real
/// morning-issued SFT fail-closed to `Forecast::None` → blank report (tuxlink-kfcwc).
fn is_value_cell(s: &str) -> bool {
    s.is_empty() || s == "MM" || s == "-" || s.chars().all(|c| c.is_ascii_digit())
}

fn split2(tok: &str) -> (&str, &str) {
    let mut p = tok.splitn(2, '/');
    (p.next().unwrap_or(""), p.next().unwrap_or(""))
}

/// A condition token: alphabetic, allowing a hyphen (NWS uses "T-Storm").
fn is_cond_token(t: &str) -> bool {
    !t.is_empty() && t.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
}

/// Parse the SFT grid, FAIL-CLOSED. The data region runs from the first region
/// header to the "$$" product terminator (excluding the legend before it and the
/// footer after). Inside that region every non-blank line MUST be a region header
/// or a clean 4-line location block [name][cond:N][temps:N][precip:N]; anything
/// else makes the WHOLE forecast `None` (header + raw) rather than a partial table
/// that silently drops a location (parse-with-fallback contract).
fn parse_tabular(lines: &[&str]) -> Option<TabularForecast> {
    let days = parse_day_columns(lines)?;
    let n = days.len();
    let raw_tokens: Vec<Vec<&str>> = lines.iter().map(|l| l.split_whitespace().collect()).collect();

    let start = lines.iter().position(|l| region_name(l).is_some())?;
    let end = lines[start..]
        .iter()
        .position(|l| l.trim() == "$$")
        .map(|p| start + p)
        .unwrap_or(lines.len());

    let mut regions: Vec<ForecastRegion> = Vec::new();
    let mut cur_region: Option<ForecastRegion> = None;
    let mut i = start;
    while i < end {
        if lines[i].trim().is_empty() {
            i += 1;
            continue;
        }
        if let Some(name) = region_name(lines[i]) {
            if let Some(r) = cur_region.take() {
                if !r.locations.is_empty() {
                    regions.push(r);
                }
            }
            cur_region = Some(ForecastRegion { name, locations: Vec::new() });
            i += 1;
            continue;
        }

        // Must be a location block: [name][cond:N][temps:N slash][precip:N slash].
        let cond = raw_tokens.get(i + 1);
        let temps = raw_tokens.get(i + 2);
        let precip = raw_tokens.get(i + 3);
        let ok = cond.is_some_and(|c| c.len() == n && c.iter().all(|t| is_cond_token(t)))
            && temps.is_some_and(|t| t.len() == n && is_slash_row(t))
            && precip.is_some_and(|p| p.len() == n && is_slash_row(p));
        if !ok {
            return None; // malformed grid → fail closed
        }
        let (cond, temps, precip) = (cond.unwrap(), temps.unwrap(), precip.unwrap());
        let cells = (0..n)
            .map(|d| {
                let (low, high) = split2(temps[d]);
                let (pn, pd) = split2(precip[d]);
                ForecastCell {
                    condition: cond[d].to_string(),
                    low: low.to_string(),
                    high: high.to_string(),
                    pop_night: pn.to_string(),
                    pop_day: pd.to_string(),
                }
            })
            .collect();
        cur_region
            .get_or_insert_with(|| ForecastRegion { name: String::new(), locations: Vec::new() })
            .locations
            .push(ForecastLocation { name: lines[i].trim().to_string(), cells });
        i += 4;
    }
    if let Some(r) = cur_region.take() {
        if !r.locations.is_empty() {
            regions.push(r);
        }
    }

    if regions.iter().all(|r| r.locations.is_empty()) {
        return None;
    }
    Some(TabularForecast { days, regions })
}

/// Parse the day columns from the `FCST` / day-of-week / date header rows.
fn parse_day_columns(lines: &[&str]) -> Option<Vec<ForecastDay>> {
    // FCST row: >= 2 tokens, all "FCST".
    let fcst_idx = lines.iter().position(|l| {
        let toks: Vec<&str> = l.split_whitespace().collect();
        toks.len() >= 2 && toks.iter().all(|t| *t == "FCST")
    })?;
    let n = lines[fcst_idx].split_whitespace().count();

    let dow: Vec<&str> = lines.get(fcst_idx + 1)?.split_whitespace().collect();
    let date_toks: Vec<&str> = lines.get(fcst_idx + 2)?.split_whitespace().collect();
    if dow.len() != n || date_toks.len() != 2 * n {
        return None;
    }
    let days = (0..n)
        .map(|i| ForecastDay {
            dow: dow[i].to_string(),
            date: format!("{} {}", date_toks[2 * i], date_toks[2 * i + 1]),
        })
        .collect();
    Some(days)
}

// ---- ZFP zone parser -------------------------------------------------------

/// UGC zone line: "AZZ015-091100-" or "AZZ530>563-CAZ560>570-091200-".
fn is_ugc_line(l: &str) -> bool {
    let t = l.trim();
    let b = t.as_bytes();
    t.ends_with('-')
        && b.len() >= 4
        && b[0].is_ascii_uppercase()
        && b[1].is_ascii_uppercase()
        && b[2] == b'Z'
        && b[3].is_ascii_digit()
}

/// A period line: ".REST OF TONIGHT...Mostly clear. Lows 43 to 53." A real period
/// starts with exactly ONE dot; an SFT region header ("...SOUTH-CENTRAL ARIZONA...")
/// starts with three, and must NOT be mistaken for a period.
fn parse_period_line(l: &str) -> Option<(String, String)> {
    let t = l.trim();
    if t.starts_with("..") {
        return None;
    }
    let rest = t.strip_prefix('.')?;
    let (label, text) = rest.split_once("...")?;
    if label.is_empty() {
        return None;
    }
    Some((label.trim().to_string(), text.trim().to_string()))
}

fn parse_zone(lines: &[&str]) -> Option<ZoneForecast> {
    let mut zones: Vec<ForecastZone> = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        if !is_ugc_line(lines[i]) {
            i += 1;
            continue;
        }
        i += 1;
        // Zone name: next non-empty line, trailing '-' stripped.
        while i < lines.len() && lines[i].trim().is_empty() {
            i += 1;
        }
        let name = lines
            .get(i)
            .map(|l| l.trim().trim_end_matches('-').trim().to_string())
            .unwrap_or_default();
        i += 1;

        // Cities: "Including the cities of ..." possibly across multiple lines,
        // until the issued-time line or a period/blank/next zone.
        let mut cities = String::new();
        if lines.get(i).is_some_and(|l| l.trim().starts_with("Including the cit")) {
            let mut buf: Vec<String> = Vec::new();
            while let Some(l) = lines.get(i) {
                let t = l.trim();
                if t.is_empty() || is_issued_line(t) || t.starts_with('.') || is_ugc_line(t) {
                    break;
                }
                buf.push(t.to_string());
                i += 1;
            }
            let joined = buf.join(" ");
            cities = joined
                .trim_start_matches("Including the cities of ")
                .trim_start_matches("Including the city of ")
                .trim()
                .to_string();
        }

        // Periods until "$$" or the next zone.
        let mut periods: Vec<ForecastPeriod> = Vec::new();
        while let Some(l) = lines.get(i) {
            let t = l.trim();
            if t == "$$" || is_ugc_line(t) {
                break;
            }
            if let Some((label, text)) = parse_period_line(t) {
                periods.push(ForecastPeriod { label, text });
            } else if !t.is_empty() {
                // Continuation of the current period's text.
                if let Some(last) = periods.last_mut() {
                    if !last.text.is_empty() {
                        last.text.push(' ');
                    }
                    last.text.push_str(t);
                }
            }
            i += 1;
        }

        if !periods.is_empty() {
            zones.push(ForecastZone { name, cities, periods });
        }
    }

    if zones.is_empty() {
        None
    } else {
        Some(ZoneForecast { zones })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(view: ReplyView) -> AreaWeather {
        match view {
            ReplyView::AreaWeather(w) => w,
            other => panic!("expected AreaWeather, got {other:?}"),
        }
    }

    #[test]
    fn sft_tabular_parses_days_regions_locations_and_cells() {
        let subject = "INQUIRY - https://tgftp.nws.noaa.gov/data/raw/fp/fpus65.kpsr.sft.psr.txt";
        let body = include_str!("../../tests/fixtures/catalog/reply-sft-tabular-psr.txt");
        let w = area(parse_reply(subject, body));
        assert!(w.product.starts_with("FPUS65 KPSR"), "product {:?}", w.product);
        assert!(w.office.to_lowercase().contains("phoenix"), "office {:?}", w.office);
        assert!(w.title.to_lowercase().contains("tabular"), "title {:?}", w.title);

        let t = match w.forecast {
            Forecast::Tabular(t) => t,
            other => panic!("expected Tabular, got {other:?}"),
        };
        assert_eq!(t.days.len(), 7, "7 day columns");
        assert_eq!(t.days[0].dow, "Tue");
        assert_eq!(t.days[0].date, "Jun 09");
        assert_eq!(t.days[6].dow, "Mon");

        // Phoenix is the first location of the first region.
        let phoenix = t
            .regions
            .iter()
            .flat_map(|r| &r.locations)
            .find(|l| l.name == "Phoenix")
            .expect("Phoenix location present");
        assert_eq!(phoenix.cells.len(), 7);
        assert_eq!(phoenix.cells[0].condition, "Vryhot");
        assert_eq!(phoenix.cells[0].low, "77");
        assert_eq!(phoenix.cells[0].high, "106");
        assert_eq!(phoenix.cells[0].pop_night, "00");

        // Region names captured.
        assert!(t.regions.iter().any(|r| r.name.contains("SOUTH-CENTRAL ARIZONA")));
        assert!(t.regions.iter().any(|r| r.name.contains("SOUTHEAST CALIFORNIA")));
    }

    /// tuxlink-kfcwc regression: a REAL morning-issued SFT (operator's N7CPZ
    /// inbox, KPSR 2026-07-02 0301 MST). The "Today" column has NO overnight low
    /// and NO nighttime PoP, so its cells are `/104` and `/00`. Before the fix,
    /// `is_value_cell("")` rejected the empty half, `is_slash_row` failed, and
    /// `parse_tabular` returned `Forecast::None` for EVERY location → the whole
    /// report rendered blank ("Show full text" only). This fixture parses to
    /// `Forecast::Tabular` with empty low/pop_night on the first column.
    #[test]
    fn sft_tabular_morning_issue_empty_today_low_still_parses() {
        let subject = "INQUIRY - https://tgftp.nws.noaa.gov/data/raw/fp/fpus65.kpsr.sft.az.txt";
        let body = include_str!("../../tests/fixtures/catalog/reply-sft-tabular-psr-am.txt");
        let w = area(parse_reply(subject, body));
        assert!(w.title.to_lowercase().contains("tabular"), "title {:?}", w.title);

        let t = match w.forecast {
            Forecast::Tabular(t) => t,
            // The bug manifested here: pre-fix this was Forecast::None.
            other => panic!("expected Tabular (regression: empty Today low), got {other:?}"),
        };
        assert_eq!(t.days.len(), 7, "7 day columns");
        assert_eq!(t.days[0].dow, "Today");

        // First location of the first region: the "Today" cell has an empty low
        // and empty nighttime PoP, a present high and daytime PoP.
        let first = t
            .regions
            .iter()
            .flat_map(|r| &r.locations)
            .find(|l| l.name == "Lake Havasu City Airport")
            .expect("first location present");
        assert_eq!(first.cells.len(), 7);
        assert_eq!(first.cells[0].condition, "Sunny");
        assert_eq!(first.cells[0].low, "", "Today has no overnight low");
        assert_eq!(first.cells[0].high, "104");
        assert_eq!(first.cells[0].pop_night, "", "Today has no nighttime PoP");
        assert_eq!(first.cells[0].pop_day, "00");
        // A later column carries a normal low.
        assert_eq!(first.cells[1].low, "79");
        assert_eq!(first.cells[1].high, "107");
    }

    /// tuxlink-kfcwc: a second real morning-issued SFT from a different office
    /// (KABQ, 7 regions / ~45 locations) — same empty-Today-low pattern. Guards
    /// against a second fail-closed trigger hiding in a larger product; asserts
    /// the whole grid parses rather than degrading to a blank report.
    #[test]
    fn sft_tabular_multi_region_morning_issue_parses() {
        let subject = "INQUIRY - https://tgftp.nws.noaa.gov/data/raw/fp/fpus65.kabq.sft.abq.txt";
        let body = include_str!("../../tests/fixtures/catalog/reply-sft-tabular-abq.txt");
        let w = area(parse_reply(subject, body));
        let t = match w.forecast {
            Forecast::Tabular(t) => t,
            other => panic!("expected Tabular (7-region KABQ), got {other:?}"),
        };
        assert_eq!(t.days.len(), 7, "7 day columns");
        assert!(t.regions.len() >= 5, "many regions, got {}", t.regions.len());
        assert!(
            t.regions.iter().any(|r| r.name.contains("NORTHWEST NEW MEXICO")),
            "first region captured"
        );
        // Every location fully parsed (no silent drops): the first column low is
        // empty on the morning issue; a later column is a real digit run.
        let locs: usize = t.regions.iter().map(|r| r.locations.len()).sum();
        assert!(locs >= 20, "many locations parsed, got {locs}");
    }

    #[test]
    fn zfp_zone_parses_zones_cities_and_periods() {
        let subject = "INQUIRY - https://tgftp.nws.noaa.gov/data/raw/fp/fpus55.kfgz.zfp.fgz.txt";
        let body = include_str!("../../tests/fixtures/catalog/reply-zfp-zone-fgz.txt");
        let w = area(parse_reply(subject, body));
        assert!(w.title.to_lowercase().contains("zone forecast"), "title {:?}", w.title);

        let z = match w.forecast {
            Forecast::Zone(z) => z,
            other => panic!("expected Zone, got {other:?}"),
        };
        assert!(z.zones.len() >= 15, "many zones, got {}", z.zones.len());

        let first = &z.zones[0];
        assert_eq!(first.name, "Western Mogollon Rim");
        assert!(first.cities.contains("Flagstaff"), "cities {:?}", first.cities);
        assert!(!first.cities.starts_with("Including"), "prefix stripped: {:?}", first.cities);

        let p0 = &first.periods[0];
        assert_eq!(p0.label, "REST OF TONIGHT");
        assert!(p0.text.starts_with("Mostly clear"), "text {:?}", p0.text);
        // Multi-line period text is joined.
        let tuesday = first.periods.iter().find(|p| p.label == "TUESDAY").expect("TUESDAY period");
        assert!(tuesday.text.contains("Highs 77 to 85"), "joined text {:?}", tuesday.text);
    }

    #[test]
    fn existing_az_tabular_fixture_still_structures() {
        let subject = "INQUIRY - https://tgftp.nws.noaa.gov/data/raw/fp/fpus65.kpsr.sft.az.txt";
        let body = include_str!("../../tests/fixtures/catalog/reply-area-weather-nws.txt");
        let w = area(parse_reply(subject, body));
        assert!(matches!(w.forecast, Forecast::Tabular(_)), "az fixture is tabular");
    }

    // Codex P1 #1: a malformed SFT (Tabular title) must NOT fall through to the
    // ZFP parser and render its region headers as bogus zone "periods".
    #[test]
    fn malformed_tabular_does_not_become_a_bogus_zone() {
        let subject = "INQUIRY - https://tgftp.nws.noaa.gov/data/raw/fp/fpus65.kpsr.sft.psr.txt";
        let body = "FPUS65 KPSR 090626\nTabular State Forecast for Test\nNational Weather Service Phoenix AZ\n1126 PM MST Mon Jun 8 2026\n\n...SOUTH-CENTRAL ARIZONA...\n   Phoenix\n   (no day-column header, no grid)\n$$\n";
        let w = area(parse_reply(subject, body));
        assert!(matches!(w.forecast, Forecast::None), "malformed tabular → None, not a zone; got {:?}", w.forecast);
    }

    // Codex P1 #2: a malformed location block inside the grid fails the WHOLE
    // forecast closed (header + raw), never a partial table. And "T-Storm"
    // (hyphenated NWS condition) parses cleanly.
    #[test]
    fn tabular_fails_closed_on_bad_block_and_accepts_hyphen_conditions() {
        let header = "FPUS65 KPSR 090626\nTabular State Forecast for Test\nNational Weather Service Phoenix AZ\n1126 PM MST Mon Jun 8 2026\n\n   FCST     FCST\n   Tue      Wed\n   Jun 09   Jun 10\n";
        let subject = "INQUIRY - https://tgftp.nws.noaa.gov/data/raw/fp/fpus65.kpsr.sft.psr.txt";

        // Good grid with a hyphenated condition → structured, T-Storm preserved.
        let good = format!("{header}\n...TEST REGION...\n   Phoenix\n   T-Storm  Sunny\n   70/90    71/91\n    20/10    00/00\n\n$$\n");
        let w = area(parse_reply(subject, &good));
        let t = match w.forecast {
            Forecast::Tabular(t) => t,
            other => panic!("expected Tabular, got {other:?}"),
        };
        let c0 = &t.regions[0].locations[0].cells[0];
        assert_eq!(c0.condition, "T-Storm");
        assert_eq!(c0.high, "90");
        assert_eq!(c0.pop_day, "10");

        // Second location block is broken (temps row missing) → whole thing falls
        // back, rather than silently dropping the bad location.
        let bad = format!("{header}\n...TEST REGION...\n   Phoenix\n   Sunny    Sunny\n   70/90    71/91\n    00/00    00/00\n   BadTown\n   Sunny    Sunny\n   notatemp here\n$$\n");
        let w = area(parse_reply(subject, &bad));
        assert!(matches!(w.forecast, Forecast::None), "bad block → fail closed; got {:?}", w.forecast);
    }

    // Codex P2: dual issued-time zones ("... 2026 /1232 AM MDT ... 2026/") must
    // not leak the timestamp into the zone's `cities`.
    #[test]
    fn zfp_dual_issued_time_does_not_leak_into_cities() {
        let subject = "INQUIRY - https://tgftp.nws.noaa.gov/data/raw/fp/fpus55.kfgz.zfp.fgz.txt";
        let body = include_str!("../../tests/fixtures/catalog/reply-zfp-zone-fgz.txt");
        let w = area(parse_reply(subject, body));
        let z = match w.forecast {
            Forecast::Zone(z) => z,
            other => panic!("expected Zone, got {other:?}"),
        };
        let marble = z
            .zones
            .iter()
            .find(|zo| zo.name == "Marble and Glen Canyons")
            .expect("dual-issued zone present");
        assert!(!marble.cities.contains("1232"), "issued time leaked: {:?}", marble.cities);
        assert!(!marble.cities.contains("MDT"), "issued time leaked: {:?}", marble.cities);
        assert!(marble.cities.contains("Page"), "real cities kept: {:?}", marble.cities);
    }

    #[test]
    fn unknown_subject_renders_raw() {
        let view = parse_reply("Service Advice Message", "some unexpected body");
        assert!(matches!(view, ReplyView::Raw { ref text } if text == "some unexpected body"));
    }

    #[test]
    fn nws_subject_but_garbled_body_degrades_to_raw() {
        let subject = "INQUIRY - https://tgftp.nws.noaa.gov/data/raw/fp/xx.txt";
        let view = parse_reply(subject, "\u{fffd}\u{fffd} not a forecast at all");
        assert!(matches!(view, ReplyView::Raw { .. }), "garbled body must degrade to raw");
    }

    #[test]
    fn nws_header_present_but_unstructured_body_is_area_weather_none() {
        let subject = "INQUIRY - https://tgftp.nws.noaa.gov/x.txt";
        let body = "FPUS65 KPSR 050638\nSome Product Title\nNational Weather Service Phoenix AZ\n1138 PM MST Thu Jun 4 2026\n\nfree-form text with no grid or zones";
        let w = area(parse_reply(subject, body));
        assert_eq!(w.issued, "1138 PM MST Thu Jun 4 2026");
        assert!(matches!(w.forecast, Forecast::None), "header-only → Forecast::None");
    }

    #[test]
    fn raw_variant_serializes_to_tagged_object_not_a_runtime_error() {
        let v = serde_json::to_value(ReplyView::Raw { text: "hello".into() }).unwrap();
        assert_eq!(v, serde_json::json!({ "kind": "raw", "text": "hello" }));
    }

    #[test]
    fn forecast_none_and_tabular_serialize_with_kind_tag() {
        let none = serde_json::to_value(Forecast::None).unwrap();
        assert_eq!(none, serde_json::json!({ "kind": "none" }));
        let tab = serde_json::to_value(Forecast::Tabular(TabularForecast {
            days: vec![ForecastDay { dow: "Tue".into(), date: "Jun 09".into() }],
            regions: vec![],
        }))
        .unwrap();
        assert_eq!(tab["kind"], "tabular");
        assert_eq!(tab["days"][0]["dow"], "Tue");
    }

    #[test]
    fn area_weather_serializes_with_camelcase_and_nested_forecast() {
        let v = serde_json::to_value(ReplyView::AreaWeather(AreaWeather {
            product: "FPUS65 KPSR 050638".into(),
            office: "National Weather Service Phoenix AZ".into(),
            issued: String::new(),
            title: "Tabular State Forecast".into(),
            forecast: Forecast::None,
            raw: "b".into(),
        }))
        .unwrap();
        assert_eq!(v["kind"], "area-weather");
        assert_eq!(v["product"], "FPUS65 KPSR 050638");
        assert_eq!(v["forecast"]["kind"], "none");
    }
}
