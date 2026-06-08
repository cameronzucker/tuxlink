//! Catalog reply parse-with-fallback. v1 ships the area-weather parser; everything else renders raw.
//!
//! Contract: ANY deviation degrades to `ReplyView::Raw { text }` — never an error, never a blank
//! (design §Reply rendering). The variant is a STRUCT variant on purpose: internally-tagged serde
//! (`#[serde(tag="kind")]`) cannot serialize a newtype variant wrapping a `String`
//! (it raises a runtime error), and Raw is the dominant path, so a newtype here would break the
//! IPC boundary on the most common reply. See the round-trip test below.

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
    pub product: String, // "FPUS65 KPSR 050638"
    pub office: String,  // "National Weather Service Phoenix AZ"
    pub issued: String,  // "1138 PM MST Thu Jun 4 2026" (may be empty if absent)
    pub raw: String,     // full body, always present (toggle target)
}

/// True for INQUIRY replies whose source URL is an NWS area-weather text product.
fn is_area_weather(subject: &str) -> bool {
    let s = subject.to_ascii_lowercase();
    s.contains("inquiry -") && s.contains("nws.noaa.gov")
}

/// AWIPS product line shape: `TTAAII CCCC DDHHMM` (e.g. "FPUS65 KPSR 050638").
fn is_awips_product(line: &str) -> bool {
    let mut it = line.split_whitespace();
    let (Some(ttaaii), Some(cccc), Some(ddhhmm)) = (it.next(), it.next(), it.next()) else {
        return false;
    };
    let ttaaii_ok = ttaaii.len() == 6
        && ttaaii.chars().take(4).all(|c| c.is_ascii_uppercase())
        && ttaaii.chars().skip(4).all(|c| c.is_ascii_digit());
    let cccc_ok = cccc.len() == 4 && cccc.chars().all(|c| c.is_ascii_alphanumeric());
    let ddhhmm_ok = ddhhmm.len() == 6 && ddhhmm.chars().all(|c| c.is_ascii_digit());
    ttaaii_ok && cccc_ok && ddhhmm_ok
}

pub fn parse_reply(subject: &str, body: &str) -> ReplyView {
    if is_area_weather(subject) {
        if let Some(w) = parse_area_weather(body) {
            return ReplyView::AreaWeather(w);
        }
    }
    ReplyView::Raw { text: body.to_string() }
}

/// NWS text product: an AWIPS product line, an office line ("National Weather Service ..."),
/// and an optional issued-time line. Returns `None` (→ raw) when no AWIPS product is present.
fn parse_area_weather(body: &str) -> Option<AreaWeather> {
    let product = body.lines().map(str::trim).find(|l| is_awips_product(l))?.to_string();
    let office = body
        .lines()
        .map(str::trim)
        .find(|l| l.contains("National Weather Service"))
        .unwrap_or("")
        .to_string();
    let issued = body
        .lines()
        .map(str::trim)
        .find(|l| {
            (l.contains(" AM ") || l.contains(" PM "))
                && l.rsplit(' ').next().is_some_and(|y| y.len() == 4 && y.chars().all(|c| c.is_ascii_digit()))
        })
        .unwrap_or("")
        .to_string();
    Some(AreaWeather { product, office, issued, raw: body.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_weather_subject_matches_and_parses() {
        let subject = "INQUIRY - https://tgftp.nws.noaa.gov/data/raw/fp/fpus65.kpsr.sft.az.txt";
        let body = include_str!("../../tests/fixtures/catalog/reply-area-weather-nws.txt");
        match parse_reply(subject, body) {
            ReplyView::AreaWeather(w) => {
                assert!(w.product.contains("FPUS65"), "product was {:?}", w.product);
                assert!(w.office.to_lowercase().contains("phoenix"), "office was {:?}", w.office);
                assert!(!w.raw.is_empty());
            }
            other => panic!("expected AreaWeather, got {other:?}"),
        }
    }

    #[test]
    fn unknown_subject_renders_raw() {
        let view = parse_reply("Service Advice Message", "some unexpected body");
        assert!(matches!(view, ReplyView::Raw { ref text } if text == "some unexpected body"));
    }

    #[test]
    fn weather_subject_but_no_awips_product_degrades_to_raw() {
        let subject = "INQUIRY - https://tgftp.nws.noaa.gov/data/raw/fp/xx.txt";
        let view = parse_reply(subject, "\u{fffd}\u{fffd} not a forecast at all");
        assert!(matches!(view, ReplyView::Raw { .. }), "garbled weather body must degrade to raw");
    }

    #[test]
    fn missing_issued_still_yields_area_weather() {
        let subject = "INQUIRY - https://tgftp.nws.noaa.gov/x.txt";
        let body = "FPUS65 KPSR 050638\nNational Weather Service Phoenix AZ\nROWS INCLUDE...";
        match parse_reply(subject, body) {
            ReplyView::AreaWeather(w) => assert_eq!(w.issued, ""),
            other => panic!("expected AreaWeather, got {other:?}"),
        }
    }

    #[test]
    fn raw_variant_serializes_to_tagged_object_not_a_runtime_error() {
        // Regression guard for the serde tagged-newtype trap: Raw is the dominant IPC path.
        let v = serde_json::to_value(&ReplyView::Raw { text: "hello".into() }).unwrap();
        assert_eq!(v, serde_json::json!({ "kind": "raw", "text": "hello" }));
    }

    #[test]
    fn area_weather_serializes_with_camelcase_fields() {
        let v = serde_json::to_value(&ReplyView::AreaWeather(AreaWeather {
            product: "FPUS65 KPSR 050638".into(),
            office: "National Weather Service Phoenix AZ".into(),
            issued: "".into(),
            raw: "b".into(),
        }))
        .unwrap();
        assert_eq!(v["kind"], "area-weather");
        assert_eq!(v["product"], "FPUS65 KPSR 050638");
    }
}
