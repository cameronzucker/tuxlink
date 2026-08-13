//! Building the outbound message for a BUNDLED (native [`FormDef`]) form send.
//!
//! ## Why this is its own module
//!
//! Two callers need the identical construction and must not diverge:
//!
//! - `ui_commands::send_form` — the Compose window's native-form send.
//! - `routines::actions::local`'s form-aware compose — a scheduled routine
//!   sending the same form with no operator at the keyboard.
//!
//! Before this module existed there was exactly one construction (inline in
//! `send_form`) and the routine path had NONE: `local.compose` rendered a
//! form's `body_template` into message text and staged it with
//! `attachments: Vec::new()`. That produces a message a human reads as an
//! ICS-213 and Winlink Express does not recognise as one — no form XML, no
//! field structure, nothing the receiver's viewer can render. Nothing on the
//! sending side is malformed, so no validator we own catches it. The failure
//! lands on someone else's machine, over RF (`tuxlink-3ddk2`).
//!
//! ## The grid is derived here, never accepted from the caller
//!
//! [`FormParameters::grid_square`] and the form's own location fields go ON
//! THE AIR inside the XML attachment. The operator's position-precision and
//! `gps_state` settings therefore govern them, and the one function that
//! applies both is [`crate::position::effective_broadcast_locator`].
//!
//! `send_form` used to take `grid_square: String` as a Tauri parameter and
//! write it into the envelope verbatim; the Compose window filled it from
//! `config_read().grid`, which is stored at FULL 6-character precision and is
//! documented as unredacted. Under the default `FourCharGrid` setting that
//! transmitted a ~3.5x4.5 km box where the operator had asked for ~70x110 km
//! (`tuxlink-bekbh`). A privacy control that only holds when the frontend
//! cooperates is not a control, so the parameter is gone: this builder takes
//! the already-resolved on-air locator and both call sites resolve it the same
//! way. The same reasoning already governs `ui_core::config::redact_config_view`,
//! which forces 4-character truncation at the MCP sink regardless of caller.

use std::collections::HashMap;

use crate::forms::types::{FormDef, FormParameters};
use crate::winlink_backend::{OutboundAttachment, OutboundMessage};

/// The attachment filename convention every Winlink form send uses:
/// `RMS_Express_Form_<form id>.xml`. Shared with `send_webview_form` so
/// receivers (Winlink Express, Pat, tuxlink's own inbox renderer) detect and
/// render a form consistently regardless of which of our two send paths
/// produced it.
pub fn attachment_filename(form_id: &str) -> String {
    format!("RMS_Express_Form_{form_id}.xml")
}

/// The `rms_express_version` string identifying this client in the XML
/// envelope. WLE writes its own version here; a receiver uses it for
/// provenance, not for parsing.
pub fn client_version() -> String {
    format!("Tuxlink/{}", env!("CARGO_PKG_VERSION"))
}

/// Build the XML envelope for a bundled form.
///
/// `on_air_grid` MUST already be the precision-reduced, privacy-gated locator
/// (see the module doc). Passing a raw stored grid here reintroduces
/// `tuxlink-bekbh`.
pub fn form_parameters(
    form: &FormDef,
    senders_callsign: String,
    on_air_grid: String,
    now: chrono::DateTime<chrono::Utc>,
) -> FormParameters {
    FormParameters {
        xml_file_version: "1.0".to_string(),
        rms_express_version: client_version(),
        submission_datetime: now.format("%Y%m%d%H%M%S").to_string(),
        senders_callsign,
        grid_square: on_air_grid,
        display_form: form.display_form.to_string(),
        reply_template: form.reply_template.to_string(),
    }
}

/// Build the complete outbound message for a bundled form: rendered subject,
/// rendered plain-text body, and the XML attachment that makes it an actual
/// form rather than prose that resembles one.
///
/// `subject_override` wins over the form's own `subject_template` when
/// supplied and non-empty — the Compose window and a routine step both allow
/// an explicit subject line. An empty override is treated as absent so a
/// caller cannot accidentally send a blank subject by threading through an
/// empty string.
pub fn build_native_form_message(
    form: &FormDef,
    field_values: &HashMap<String, String>,
    to: Vec<String>,
    cc: Vec<String>,
    senders_callsign: String,
    on_air_grid: String,
    subject_override: Option<String>,
    now: chrono::DateTime<chrono::Utc>,
) -> OutboundMessage {
    let params = form_parameters(form, senders_callsign, on_air_grid, now);

    let xml_bytes = crate::forms::serialize::serialize_form_xml(form, &params, field_values);
    let body = crate::forms::serialize::render_body_template(form.body_template, field_values);
    let subject = subject_override
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            crate::forms::serialize::render_body_template(form.subject_template, field_values)
        });

    OutboundMessage {
        to,
        cc,
        subject,
        body,
        date: now.to_rfc3339(),
        attachments: vec![OutboundAttachment {
            filename: attachment_filename(form.id),
            bytes: xml_bytes,
        }],
    }
}

/// Resolve a bundled form by id and build its outbound message, deriving the
/// on-air locator from the operator's own settings.
///
/// This is the seam both real call sites go through, and the reason the
/// derivation is testable at all: it takes the `Config` and `PositionArbiter`
/// by reference rather than reading global state, so a test can hand it a
/// 6-character stored grid under the default `FourCharGrid` setting and assert
/// what actually reaches the wire.
///
/// `config` is `None` in the pre-wizard case, where no config file exists yet.
/// There is then no stored grid to broadcast and no precision setting to
/// honour, so the envelope carries an empty locator. That matches what the
/// Compose window already produced before the wizard ran, and erring toward
/// LESS position on the air is the correct direction to fail.
///
/// `Err` is the unknown-form case, returned verbatim for the caller to wrap in
/// its own error type. It is deliberately loud: a form id that is not in the
/// bundle means the caller's reference is broken, and degrading to a plain
/// text message would ship the exact interop failure described in the module
/// doc.
pub fn compose_native_form(
    form_id: &str,
    field_values: &HashMap<String, String>,
    to: Vec<String>,
    cc: Vec<String>,
    senders_callsign: String,
    config: Option<&crate::config::Config>,
    arbiter: Option<&crate::position::PositionArbiter>,
    subject_override: Option<String>,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<OutboundMessage, String> {
    let form = crate::forms::catalog::find_form(form_id)
        .ok_or_else(|| format!("unknown form: {form_id}"))?;
    let on_air_grid = config
        .map(|cfg| crate::position::effective_broadcast_locator(cfg, arbiter))
        .unwrap_or_default();
    Ok(build_native_form_message(
        form,
        field_values,
        to,
        cc,
        senders_callsign,
        on_air_grid,
        subject_override,
        now,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forms::catalog::find_form;

    fn checkin_values() -> HashMap<String, String> {
        let mut v = HashMap::new();
        v.insert("organization".into(), "Winlink Net".into());
        v.insert("newsubject".into(), "Morning check-in".into());
        v.insert("msgto".into(), "NET CONTROL".into());
        v.insert("msgsender".into(), "N0CALL".into());
        v.insert("status".into(), "EXERCISE".into());
        v
    }

    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-08-12T14:30:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    /// The whole point of the module: a form send carries the XML attachment
    /// that makes it a form. `local.compose` staged `attachments: Vec::new()`,
    /// which is the silent-interop failure this exists to prevent.
    #[test]
    fn a_form_message_carries_its_xml_attachment() {
        let form = find_form("Winlink_Check-In").expect("bundled");
        let msg = build_native_form_message(
            form,
            &checkin_values(),
            vec!["NET@winlink.org".into()],
            vec![],
            "N0CALL".into(),
            "CN87".into(),
            None,
            now(),
        );

        assert_eq!(msg.attachments.len(), 1, "a form send must attach its XML");
        assert_eq!(
            msg.attachments[0].filename,
            "RMS_Express_Form_Winlink_Check-In.xml"
        );
        let xml = String::from_utf8_lossy(&msg.attachments[0].bytes);
        assert!(xml.contains("<RMS_Express_Form>"));
        assert!(xml.contains("<msgto>NET CONTROL</msgto>"));
    }

    /// The envelope carries whatever locator it was handed and does not
    /// re-derive one. The privacy reduction happens at the call site (see the
    /// module doc); this test pins that the builder does not silently
    /// substitute a different value, so a caller that passes the reduced grid
    /// gets the reduced grid on the wire.
    #[test]
    fn the_envelope_transmits_exactly_the_locator_it_was_given() {
        let form = find_form("Winlink_Check-In").expect("bundled");
        let msg = build_native_form_message(
            form,
            &checkin_values(),
            vec!["NET@winlink.org".into()],
            vec![],
            "N0CALL".into(),
            "CN87".into(),
            None,
            now(),
        );
        let xml = String::from_utf8_lossy(&msg.attachments[0].bytes);
        assert!(
            xml.contains("<grid_square>CN87</grid_square>"),
            "envelope must carry the resolved on-air locator; got {xml}"
        );
        assert!(
            !xml.contains("CN87ux"),
            "nothing may re-expand the locator past what the caller resolved"
        );
    }

    /// A form with no stored/permitted position sends an empty locator rather
    /// than omitting the element or inventing one. Matches what the pre-wizard
    /// Compose path already did.
    #[test]
    fn an_absent_locator_sends_empty_not_missing() {
        let form = find_form("Winlink_Check-In").expect("bundled");
        let msg = build_native_form_message(
            form,
            &checkin_values(),
            vec!["NET@winlink.org".into()],
            vec![],
            "N0CALL".into(),
            String::new(),
            None,
            now(),
        );
        let xml = String::from_utf8_lossy(&msg.attachments[0].bytes);
        assert!(xml.contains("<grid_square></grid_square>"));
    }

    #[test]
    fn subject_renders_from_the_forms_own_template_when_not_overridden() {
        let form = find_form("Winlink_Check-In").expect("bundled");
        let msg = build_native_form_message(
            form,
            &checkin_values(),
            vec!["NET@winlink.org".into()],
            vec![],
            "N0CALL".into(),
            "CN87".into(),
            None,
            now(),
        );
        // Check-In's subject_template is `<var Newsubject>`.
        assert_eq!(msg.subject, "Morning check-in");
    }

    #[test]
    fn an_explicit_subject_wins_but_a_blank_one_does_not() {
        let form = find_form("Winlink_Check-In").expect("bundled");
        let explicit = build_native_form_message(
            form,
            &checkin_values(),
            vec!["NET@winlink.org".into()],
            vec![],
            "N0CALL".into(),
            "CN87".into(),
            Some("Overridden".into()),
            now(),
        );
        assert_eq!(explicit.subject, "Overridden");

        let blank = build_native_form_message(
            form,
            &checkin_values(),
            vec!["NET@winlink.org".into()],
            vec![],
            "N0CALL".into(),
            "CN87".into(),
            Some("   ".into()),
            now(),
        );
        assert_eq!(
            blank.subject, "Morning check-in",
            "a whitespace-only override must fall back, not send a blank subject"
        );
    }

    /// The body is the human-readable projection; the XML is the machine one.
    /// Both must be present — the body alone is exactly the degraded message.
    // ── The privacy regression this module was written to close ──────────
    //
    // tuxlink-bekbh: `send_form` took `grid_square` as a caller parameter and
    // the Compose window filled it from `config_read().grid`, which is stored
    // at full 6-character precision. Under the DEFAULT `FourCharGrid` setting
    // that put a ~3.5x4.5 km box on the air where the operator had chosen
    // ~70x110 km. These tests fail if the derivation is ever reverted to
    // reading `config.identity.grid` directly.

    fn config_with(
        grid: &str,
        precision: crate::config::PositionPrecision,
        gps_state: crate::config::GpsState,
    ) -> crate::config::Config {
        let mut cfg = crate::test_helpers::native_test_config();
        cfg.identity.grid = Some(grid.to_string());
        cfg.privacy.position_precision = precision;
        cfg.privacy.gps_state = gps_state;
        cfg
    }

    fn grid_square_of(msg: &OutboundMessage) -> String {
        let xml = String::from_utf8_lossy(&msg.attachments[0].bytes).into_owned();
        let start = xml.find("<grid_square>").expect("envelope has grid_square")
            + "<grid_square>".len();
        let end = xml[start..].find("</grid_square>").expect("closing tag") + start;
        xml[start..end].to_string()
    }

    #[test]
    fn a_six_char_grid_goes_out_reduced_under_the_default_precision() {
        use crate::config::{GpsState, PositionPrecision};
        let cfg = config_with(
            "CN87ux",
            PositionPrecision::FourCharGrid,
            GpsState::BroadcastAtPrecision,
        );
        let msg = compose_native_form(
            "Winlink_Check-In",
            &checkin_values(),
            vec!["NET@winlink.org".into()],
            vec![],
            "N0CALL".into(),
            Some(&cfg),
            None,
            None,
            now(),
        )
        .expect("bundled form");

        assert_eq!(
            grid_square_of(&msg),
            "CN87",
            "the operator chose 4-character precision; the full grid must not go out"
        );
    }

    #[test]
    fn the_operators_opt_in_to_six_char_precision_is_honoured_too() {
        use crate::config::{GpsState, PositionPrecision};
        let cfg = config_with(
            "CN87ux",
            PositionPrecision::SixCharGrid,
            GpsState::BroadcastAtPrecision,
        );
        let msg = compose_native_form(
            "Winlink_Check-In",
            &checkin_values(),
            vec!["NET@winlink.org".into()],
            vec![],
            "N0CALL".into(),
            Some(&cfg),
            None,
            None,
            now(),
        )
        .expect("bundled form");

        assert_eq!(
            grid_square_of(&msg),
            "CN87ux",
            "reduction must follow the setting, not clamp unconditionally"
        );
    }

    /// Pre-wizard: no config on disk. The envelope must carry no position at
    /// all rather than fall back to anything the arbiter happens to hold.
    #[test]
    fn no_config_sends_no_position() {
        let msg = compose_native_form(
            "Winlink_Check-In",
            &checkin_values(),
            vec!["NET@winlink.org".into()],
            vec![],
            "N0CALL".into(),
            None,
            // An arbiter that DOES hold a precise position, to prove the
            // absent config is what governs and not the arbiter's contents.
            Some(&crate::position::PositionArbiter::new(
                crate::config::PositionSource::Manual,
                Some("CN87ux".to_string()),
                crate::config::PositionPrecision::SixCharGrid,
            )),
            None,
            now(),
        )
        .expect("bundled form");

        assert_eq!(
            grid_square_of(&msg),
            "",
            "with no config there is no precision setting to honour, so nothing goes out"
        );
    }

    /// A form id that is not bundled fails rather than degrading to a plain
    /// text message. The degraded message is the interop bug; refusing is the
    /// only correct answer.
    #[test]
    fn an_unknown_form_refuses_instead_of_sending_prose() {
        let cfg = crate::test_helpers::native_test_config();
        let err = compose_native_form(
            "Not_A_Real_Form",
            &checkin_values(),
            vec!["NET@winlink.org".into()],
            vec![],
            "N0CALL".into(),
            Some(&cfg),
            None,
            None,
            now(),
        )
        .expect_err("an unknown form id must not produce a message");
        assert!(err.contains("Not_A_Real_Form"), "error names the id: {err}");
    }

    #[test]
    fn body_and_xml_are_both_produced_from_the_same_values() {
        let form = find_form("Winlink_Check-In").expect("bundled");
        let msg = build_native_form_message(
            form,
            &checkin_values(),
            vec!["NET@winlink.org".into()],
            vec![],
            "N0CALL".into(),
            "CN87".into(),
            None,
            now(),
        );
        assert!(msg.body.contains("NET CONTROL"), "body: {}", msg.body);
        let xml = String::from_utf8_lossy(&msg.attachments[0].bytes);
        assert!(xml.contains("<msgto>NET CONTROL</msgto>"));
    }
}
