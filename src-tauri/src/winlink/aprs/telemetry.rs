//! APRS telemetry parser (RX).
//!
//! Parses the two halves of the APRS telemetry system (APRS101 §13), pinned to
//! the spec and aprslib `telemetry`:
//!
//!   1. **Data reports** — DTI `T`, form `T#sss,a1,a2,a3,a4,a5,bbbbbbbb` — five
//!      analog channels (raw counts) and eight binary bits, with a sequence id.
//!   2. **Definition messages** — ordinary addressed messages whose body begins
//!      `PARM.` / `UNIT.` / `EQNS.` / `BITS.` — the channel names, units, scaling
//!      coefficients, and bit senses for a station's telemetry.
//!
//! Raw analog counts are scaled to engineering units by the per-channel EQNS
//! coefficients: `value = a·raw² + b·raw + c` (see [`apply_eqns`]). Scaling and
//! the per-station accumulation of definitions are a stateful concern handled by
//! [`super::telemetry_store`] (per-station defs) + the engine emit + the
//! telemetry panel — this module is a pure, fully-unit-testable decoder.
//! RF-honesty: it reports only the fields present on the wire; absent analog
//! channels are `None`, never a fabricated 0.
//!
//! Engine wiring (tuxlink-2phz): `parse_telemetry_data` is called on the raw-feed
//! path for `T#` reports and `parse_telemetry_definition` on the message path for
//! `PARM/UNIT/EQNS/BITS`, both feeding `TelemetryStore` which emits the enriched
//! `aprs-telemetry:new` DTO. The telemetry-graph panel is the remaining
//! fast-follow.

/// A decoded APRS telemetry data report (`T#…`).
#[derive(Debug, Clone, PartialEq)]
pub struct TelemetryData {
    /// Sequence id. `Some(n)` for a numeric sequence (`T#005`); `None` when the
    /// sender used the non-numeric `MIC` marker or omitted a parsable number.
    pub seq: Option<u16>,
    /// Up to five analog channels (A1…A5) in wire order — raw counts, before
    /// EQNS scaling. A position holds `None` if that field was blank/unparsable.
    pub analog: Vec<Option<f64>>,
    /// Up to eight binary bits (B1…B8) in wire order.
    pub digital: Vec<bool>,
    /// Free-text comment trailing the binary field, if any.
    pub comment: String,
}

/// A decoded APRS telemetry **definition** message body.
#[derive(Debug, Clone, PartialEq)]
pub enum TelemetryDefinition {
    /// `PARM.` — channel names: up to 5 analog labels then up to 8 binary labels.
    Parm(Vec<String>),
    /// `UNIT.` — channel units/labels, positionally aligned with `Parm`.
    Unit(Vec<String>),
    /// `EQNS.` — scaling coefficients, three (`a,b,c`) per analog channel, in
    /// wire order (so `[a1,b1,c1,a2,b2,c2,…]`).
    Eqns(Vec<f64>),
    /// `BITS.` — the sense (0/1) of each of the 8 binary channels plus the
    /// optional project title that follows the first comma.
    Bits { sense: Vec<bool>, project: String },
}

/// Parse an APRS telemetry data report info field (`T#sss,a1,…,a5,bbbbbbbb`).
///
/// Returns `None` if the field is not a telemetry data report (wrong DTI). The
/// leading `T` is required; the `#` immediately after it is expected but
/// tolerated-if-absent (some encoders emit `T` then the sequence directly).
pub fn parse_telemetry_data(info: &[u8]) -> Option<TelemetryData> {
    let s = std::str::from_utf8(info).ok()?;
    let rest = s.strip_prefix('T')?;
    // The sequence is conventionally introduced by '#'; accept its absence too.
    let rest = rest.strip_prefix('#').unwrap_or(rest);

    let mut fields = rest.split(',');
    let seq_tok = fields.next()?.trim();
    // A telemetry report must carry data fields, not just a bare "T#".
    let seq = seq_tok.parse::<u16>().ok();

    let mut analog: Vec<Option<f64>> = Vec::new();
    for _ in 0..5 {
        match fields.next() {
            Some(tok) => {
                let t = tok.trim();
                analog.push(if t.is_empty() { None } else { t.parse::<f64>().ok() });
            }
            None => break,
        }
    }

    // The binary field is 8 0/1 chars; any trailing text is a comment. It may
    // arrive as its own comma-field or run straight on after the analog values.
    let (digital, comment) = match fields.next() {
        Some(tok) => parse_bits_field(tok),
        None => (Vec::new(), String::new()),
    };
    // Anything after the binary field's own commas is part of the comment.
    let trailing: Vec<&str> = fields.collect();
    let comment = if trailing.is_empty() {
        comment
    } else if comment.is_empty() {
        trailing.join(",")
    } else {
        format!("{},{}", comment, trailing.join(","))
    };

    // Require at least one analog channel OR binary data so a stray `T...` line
    // that merely starts with 'T' is not mistaken for telemetry.
    if analog.is_empty() && digital.is_empty() {
        return None;
    }
    Some(TelemetryData { seq, analog, digital, comment })
}

/// Split a binary telemetry field into its leading run of 0/1 bits (max 8) and
/// the trailing comment text.
fn parse_bits_field(tok: &str) -> (Vec<bool>, String) {
    let mut bits = Vec::new();
    let mut rest_idx = 0;
    for (i, ch) in tok.char_indices() {
        if bits.len() == 8 {
            rest_idx = i;
            break;
        }
        match ch {
            '0' => bits.push(false),
            '1' => bits.push(true),
            _ => {
                rest_idx = i;
                break;
            }
        }
        rest_idx = i + ch.len_utf8();
    }
    (bits, tok[rest_idx..].trim().to_string())
}

/// Parse an APRS telemetry **definition** message body (the text after the
/// `:ADDRESSEE:` prefix). Returns `None` if it is not a `PARM/UNIT/EQNS/BITS`
/// definition.
pub fn parse_telemetry_definition(body: &str) -> Option<TelemetryDefinition> {
    if let Some(rest) = body.strip_prefix("PARM.") {
        return Some(TelemetryDefinition::Parm(split_labels(rest)));
    }
    if let Some(rest) = body.strip_prefix("UNIT.") {
        return Some(TelemetryDefinition::Unit(split_labels(rest)));
    }
    if let Some(rest) = body.strip_prefix("EQNS.") {
        let coeffs = rest.split(',').map(|t| t.trim().parse::<f64>().unwrap_or(0.0)).collect();
        return Some(TelemetryDefinition::Eqns(coeffs));
    }
    if let Some(rest) = body.strip_prefix("BITS.") {
        // `BITS.bbbbbbbb,Project Title` — 8 sense bits then the title (which may
        // itself contain commas, so split only on the first).
        let (bits_str, project) = rest.split_once(',').unwrap_or((rest, ""));
        let sense = bits_str.chars().take(8).map(|c| c == '1').collect();
        return Some(TelemetryDefinition::Bits { sense, project: project.trim().to_string() });
    }
    None
}

/// Comma-split a `PARM.`/`UNIT.` list into trimmed labels, preserving empty
/// positions (a blank channel name is meaningful for column alignment).
fn split_labels(rest: &str) -> Vec<String> {
    rest.split(',').map(|t| t.trim().to_string()).collect()
}

/// Apply an APRS EQNS quadratic to a raw analog count: `a·raw² + b·raw + c`.
pub fn apply_eqns(raw: f64, a: f64, b: f64, c: f64) -> f64 {
    a * raw * raw + b * raw + c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_standard_data_report() {
        let t = parse_telemetry_data(b"T#005,199,000,255,073,123,01101001").unwrap();
        assert_eq!(t.seq, Some(5));
        assert_eq!(t.analog, vec![Some(199.0), Some(0.0), Some(255.0), Some(73.0), Some(123.0)]);
        assert_eq!(t.digital, vec![false, true, true, false, true, false, false, true]);
        assert_eq!(t.comment, "");
    }

    #[test]
    fn captures_a_trailing_comment_after_the_bits() {
        let t = parse_telemetry_data(b"T#231,7.4,3.2,0,0,0,00000000,solar node").unwrap();
        assert_eq!(t.seq, Some(231));
        assert_eq!(t.analog[0], Some(7.4));
        assert_eq!(t.digital.len(), 8);
        assert_eq!(t.comment, "solar node");
    }

    #[test]
    fn non_numeric_mic_sequence_yields_none_seq() {
        let t = parse_telemetry_data(b"T#MIC,1,2,3,4,5,10101010").unwrap();
        assert_eq!(t.seq, None);
        assert_eq!(t.analog, vec![Some(1.0), Some(2.0), Some(3.0), Some(4.0), Some(5.0)]);
    }

    #[test]
    fn blank_analog_channel_is_none_not_zero() {
        let t = parse_telemetry_data(b"T#010,12,,34,,56,00000000").unwrap();
        assert_eq!(t.analog, vec![Some(12.0), None, Some(34.0), None, Some(56.0)]);
    }

    #[test]
    fn tolerates_missing_hash_and_short_reports() {
        // No '#', fewer than 5 analog channels, no bits.
        let t = parse_telemetry_data(b"T100,42,43").unwrap();
        assert_eq!(t.seq, Some(100));
        assert_eq!(t.analog, vec![Some(42.0), Some(43.0)]);
        assert!(t.digital.is_empty());
    }

    #[test]
    fn rejects_non_telemetry() {
        assert!(parse_telemetry_data(b":N0CALL   :hello").is_none()); // message, not telemetry
        assert!(parse_telemetry_data(b"Test comment").is_none()); // starts with T but no fields
        assert!(parse_telemetry_data(b"T#005").is_none()); // bare seq, no data
    }

    #[test]
    fn parses_parm_unit_definitions() {
        let parm = parse_telemetry_definition("PARM.Vbat,Temp,RxCount,,,Door,Fan").unwrap();
        assert_eq!(
            parm,
            TelemetryDefinition::Parm(vec![
                "Vbat".into(), "Temp".into(), "RxCount".into(), "".into(), "".into(),
                "Door".into(), "Fan".into(),
            ])
        );
        let unit = parse_telemetry_definition("UNIT.Volts,degC,Pkts").unwrap();
        assert_eq!(
            unit,
            TelemetryDefinition::Unit(vec!["Volts".into(), "degC".into(), "Pkts".into()])
        );
    }

    #[test]
    fn parses_eqns_coefficients() {
        let eqns = parse_telemetry_definition("EQNS.0,0.075,0,0,1,-100").unwrap();
        assert_eq!(
            eqns,
            TelemetryDefinition::Eqns(vec![0.0, 0.075, 0.0, 0.0, 1.0, -100.0])
        );
    }

    #[test]
    fn parses_bits_sense_and_project_title() {
        let bits = parse_telemetry_definition("BITS.10110000,Solar Shed Telemetry").unwrap();
        match bits {
            TelemetryDefinition::Bits { sense, project } => {
                assert_eq!(sense, vec![true, false, true, true, false, false, false, false]);
                assert_eq!(project, "Solar Shed Telemetry");
            }
            _ => panic!("expected Bits"),
        }
    }

    #[test]
    fn bits_project_title_may_contain_commas() {
        let bits = parse_telemetry_definition("BITS.11111111,Site A, Rack 2").unwrap();
        match bits {
            TelemetryDefinition::Bits { project, .. } => assert_eq!(project, "Site A, Rack 2"),
            _ => panic!("expected Bits"),
        }
    }

    #[test]
    fn non_definition_bodies_return_none() {
        assert!(parse_telemetry_definition("just a normal message").is_none());
        assert!(parse_telemetry_definition("ack003").is_none());
    }

    #[test]
    fn eqns_scaling_matches_the_quadratic() {
        // value = a·raw² + b·raw + c. With a=0,b=0.075,c=0 a count of 200 → 15.0 V.
        assert!((apply_eqns(200.0, 0.0, 0.075, 0.0) - 15.0).abs() < 1e-9);
        // a=0.001, b=0, c=-2 at raw=100 → 0.001*10000 - 2 = 8.0
        assert!((apply_eqns(100.0, 0.001, 0.0, -2.0) - 8.0).abs() < 1e-9);
    }
}
