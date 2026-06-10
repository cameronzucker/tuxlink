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

    let forecast = parse_tabular(&lines)
        .map(Forecast::Tabular)
        .or_else(|| parse_zone(&lines).map(Forecast::Zone))
        .unwrap_or(Forecast::None);

    Some(AreaWeather { product, office, issued, title, forecast, raw: body.to_string() })
}

/// An issued-time line: has " AM "/" PM " and ends with a 4-digit year.
fn is_issued_line(l: &str) -> bool {
    (l.contains(" AM ") || l.contains(" PM "))
        && l.split_whitespace()
            .next_back()
            .is_some_and(|y| y.len() == 4 && y.chars().all(|c| c.is_ascii_digit()))
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

fn is_value_cell(s: &str) -> bool {
    s == "MM" || s == "-" || (!s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
}

fn split2(tok: &str) -> (&str, &str) {
    let mut p = tok.splitn(2, '/');
    (p.next().unwrap_or(""), p.next().unwrap_or(""))
}

/// Parse the SFT grid. Anchored on the temp data row (the only line that is N
/// slash-pairs preceded by a condition row); the name is the line above the
/// conditions, the precip row the line below the temps.
fn parse_tabular(lines: &[&str]) -> Option<TabularForecast> {
    let days = parse_day_columns(lines)?;
    let n = days.len();

    let mut regions: Vec<ForecastRegion> = Vec::new();
    let mut cur_region: Option<ForecastRegion> = None;
    let raw_tokens: Vec<Vec<&str>> = lines.iter().map(|l| l.split_whitespace().collect()).collect();

    let mut i = 0usize;
    while i < lines.len() {
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

        // Try to anchor a location block at i:
        // [name][conditions:N alpha][temps:N slash][precip:N slash]
        let cond = raw_tokens.get(i + 1);
        let temps = raw_tokens.get(i + 2);
        let precip = raw_tokens.get(i + 3);
        let name_ok = !lines[i].trim().is_empty();
        let cond_ok = cond
            .is_some_and(|c| c.len() == n && c.iter().all(|t| t.chars().all(|ch| ch.is_ascii_alphabetic())));
        let temps_ok = temps.is_some_and(|t| t.len() == n && is_slash_row(t));
        let precip_ok = precip.is_some_and(|p| p.len() == n && is_slash_row(p));

        if name_ok && cond_ok && temps_ok && precip_ok {
            let cond = cond.unwrap();
            let temps = temps.unwrap();
            let precip = precip.unwrap();
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
            let loc = ForecastLocation { name: lines[i].trim().to_string(), cells };
            cur_region
                .get_or_insert_with(|| ForecastRegion { name: String::new(), locations: Vec::new() })
                .locations
                .push(loc);
            i += 4;
            continue;
        }
        i += 1;
    }
    if let Some(r) = cur_region.take() {
        if !r.locations.is_empty() {
            regions.push(r);
        }
    }

    if regions.is_empty() {
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

/// A period line: ".REST OF TONIGHT...Mostly clear. Lows 43 to 53."
fn parse_period_line(l: &str) -> Option<(String, String)> {
    let t = l.trim();
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
