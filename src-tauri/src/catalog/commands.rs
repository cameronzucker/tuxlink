//! Tauri command surface for catalog requests.
//!
//! - `catalog_list()` returns the bundled WLE catalog as `Vec<CatalogEntry>`.
//!   Pure / stateless — the file is bundled at compile time.
//! - `catalog_send_inquiry(filenames)` builds an `OutboundMessage` for
//!   `INQUIRY@winlink.org` with `Subject: REQUEST` and the joined filenames
//!   as the body, then routes through the existing `backend.send_message`
//!   pipeline (same path as compose-window sends).

use crate::catalog::composer::{
    build_inquiry_body, InquiryComposeError, INQUIRY_RECIPIENT, INQUIRY_SUBJECT,
};
use crate::catalog::parser::{parse_catalog, CatalogEntry, BUNDLED_CATALOG};
use crate::ui_commands::UiError;
use crate::winlink_backend::OutboundMessage;
use tauri::State;

impl From<InquiryComposeError> for UiError {
    fn from(e: InquiryComposeError) -> Self {
        UiError::Internal { detail: e.to_string() }
    }
}

/// Return the bundled WLE catalog. Called once on panel open. The
/// frontend caches the result for the session — the bundled file does not
/// change at runtime.
#[tauri::command]
pub fn catalog_list() -> Result<Vec<CatalogEntry>, UiError> {
    parse_catalog(BUNDLED_CATALOG).map_err(|e| UiError::Internal {
        detail: format!("bundled catalog parse failed: {e}"),
    })
}

/// Queue a catalog-request message in the outbox. `filenames` is the list of
/// selected catalog filenames (one inquiry per filename — CMS replies with
/// one separate Private message per item). Returns the MID string on
/// success (mirrors `message_send` contract).
#[tauri::command]
pub async fn catalog_send_inquiry(
    filenames: Vec<String>,
    state: State<'_, crate::app_backend::BackendState>,
) -> Result<String, UiError> {
    // Validate body composition up-front so a bad filename is caught before
    // we touch the backend / mailbox state.
    let filename_refs: Vec<&str> = filenames.iter().map(|s| s.as_str()).collect();
    let body = build_inquiry_body(&filename_refs)?;

    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;

    let date = chrono::Utc::now().to_rfc3339();
    let msg = OutboundMessage {
        to: vec![INQUIRY_RECIPIENT.to_string()],
        cc: vec![],
        subject: INQUIRY_SUBJECT.to_string(),
        body,
        date,
        attachments: vec![],
    };

    let mid = backend.send_message(msg).await?;
    Ok(mid.0)
}
