//! Tauri command surface for GRIB requests.
//!
//! `grib_send_request(GribRequest)` builds an `OutboundMessage` for
//! `query@saildocs.com` with the composed `send`/`sub` body, then routes
//! through the existing `backend.send_message` pipeline (same path as
//! compose-window and catalog-request sends).

use crate::grib::composer::{GribComposeError, GribRequest};
use crate::grib::composer::SAILDOCS_RECIPIENT;
use crate::ui_commands::UiError;
use crate::winlink_backend::OutboundMessage;
use tauri::State;

impl From<GribComposeError> for UiError {
    fn from(e: GribComposeError) -> Self {
        UiError::Internal { detail: e.to_string() }
    }
}

/// Compose + queue a Saildocs GRIB request in the outbox. Returns the MID
/// string on success (mirrors `message_send` contract).
#[tauri::command]
pub async fn grib_send_request(
    request: GribRequest,
    state: State<'_, crate::app_backend::BackendState>,
) -> Result<String, UiError> {
    // Build the body first so validation errors surface before we touch
    // the backend / mailbox.
    let body = crate::grib::composer::build_grib_body(&request)?;

    // Validate the subject the same way the composer does (empty-after-trim
    // is rejected by compose_grib_message). We do that check here too so
    // the OutboundMessage we hand to the backend has a non-empty subject.
    if request.subject.trim().is_empty() {
        return Err(GribComposeError::EmptySubject.into());
    }

    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;

    let date = chrono::Utc::now().to_rfc3339();
    let msg = OutboundMessage {
        to: vec![SAILDOCS_RECIPIENT.to_string()],
        cc: vec![],
        subject: request.subject.clone(),
        body,
        date,
        attachments: vec![],
    };

    let mid = backend.send_message(msg).await?;
    Ok(mid.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grib::composer::{compose_grib_message, GribDirection, GribMode, GribParameter, Latitude, Longitude};

    fn sample_request() -> GribRequest {
        GribRequest {
            mode: GribMode::Send,
            lat0: Latitude { degrees: 40, dir: GribDirection::N },
            lat1: Latitude { degrees: 60, dir: GribDirection::N },
            lon0: Longitude { degrees: 140, dir: GribDirection::W },
            lon1: Longitude { degrees: 120, dir: GribDirection::W },
            grid: (2, 2),
            times: vec![],
            params: vec![],
            sub_days: None,
            sub_time: None,
            subject: "GRIB request".to_string(),
        }
    }

    #[test]
    fn compose_grib_message_emits_canonical_minimal() {
        let req = sample_request();
        let msg = compose_grib_message("N7CPZ", &req, 1_716_200_000).unwrap();
        let tos = msg.header_all("To");
        assert_eq!(tos, vec!["SMTP:query@saildocs.com"]);
        assert_eq!(msg.header("Subject").unwrap(), "GRIB request");
        let body = std::str::from_utf8(msg.body()).unwrap();
        assert!(
            body.lines().any(|l| l == "send gfs:40N,60N,140W,120W"),
            "body should contain canonical send line, got: {body:?}"
        );
    }

    #[test]
    fn build_grib_body_with_full_options() {
        let req = GribRequest {
            mode: GribMode::Sub,
            params: vec![GribParameter::Wind, GribParameter::Waves],
            sub_days: Some(7),
            sub_time: Some("06:00".to_string()),
            ..sample_request()
        };
        let body = crate::grib::composer::build_grib_body(&req).unwrap();
        assert_eq!(body, "sub gfs:40N,60N,140W,120W|2,2||WIND,WAVES days=7 time=06:00");
    }
}
