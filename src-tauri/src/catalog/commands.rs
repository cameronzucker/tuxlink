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
    tracing::info!(
        target: "tuxlink::catalog",
        mid = %mid.0,
        filename_count = filenames.len(),
        "catalog inquiry queued",
    );
    Ok(mid.0)
}

// ============================================================================
// tuxlink-a2gd: location-aware station-list direct poll + reply parse-with-fallback
// ============================================================================

use crate::catalog::reply::{parse_reply, ReplyView};
use crate::catalog::stations::{parse_listing, ListingMode, StationListing};
use crate::catalog::stations_cache::{CacheKey, StationsCache};
use std::sync::Arc;

const CATALOG_HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Descriptive, identifiable User-Agent so winlink ops can contact rather than ban.
fn catalog_user_agent() -> String {
    format!(
        "Tuxlink/{} ({}; {})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

/// Testable HTTP seam: GET `url`, parse as `mode`'s listing. Reuses the parsed-host transport
/// classifier (rejects loopback-lookalike hosts + non-http schemes); https-only off only for
/// genuine loopback (mockito tests). `:444` cert validation is reqwest-default and MUST NOT be
/// relaxed — `danger_accept_invalid_certs` is banned here; the non-standard port does not affect
/// SNI/cert validation for host `cms.winlink.org`.
pub(crate) async fn fetch_listing_from_url(
    url: &str,
    mode: ListingMode,
) -> Result<StationListing, UiError> {
    let is_loopback = crate::forms::updater::classify_transport(url)
        .map_err(|reason| UiError::Transport { reason })?;
    let client = reqwest::Client::builder()
        .user_agent(catalog_user_agent())
        .timeout(CATALOG_HTTP_TIMEOUT)
        .https_only(!is_loopback)
        .build()
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| UiError::Transport { reason: e.to_string() })?;
    if !resp.status().is_success() {
        return Err(UiError::Unavailable {
            reason: format!("listing endpoint returned {}", resp.status()),
        });
    }
    let text = resp
        .text()
        .await
        .map_err(|e| UiError::Transport { reason: e.to_string() })?;
    // parse_listing degrades to raw internally — never errors/panics on a bad body.
    Ok(parse_listing(&text, mode))
}

/// Fetch station lists for the given modes via the polite cache (TTL + per-key coalescing +
/// stale-on-error). v1 is PUBLIC-only — `serviceCodes` is fixed to `PUBLIC`, not caller-supplied.
/// Independent modes fetch concurrently (per-key cache locks don't cross-block).
#[tauri::command]
pub async fn catalog_fetch_stations(
    modes: Vec<ListingMode>,
    history_hours: Option<u32>,
    cache: State<'_, Arc<StationsCache>>,
) -> Result<Vec<StationListing>, UiError> {
    const SERVICE_CODES: &str = "PUBLIC"; // locked v1 scope — PUBLIC only
    let history_hours = history_hours.unwrap_or(168);
    let cache = cache.inner().clone();
    let futures = modes.into_iter().map(|mode| {
        let cache = cache.clone();
        async move {
            let url = mode.listing_url(SERVICE_CODES, history_hours);
            let key = CacheKey {
                mode,
                service_codes: SERVICE_CODES.to_string(),
                history_hours,
            };
            cache.get_or_fetch(key, fetch_listing_from_url(&url, mode)).await
        }
    });
    futures::future::try_join_all(futures).await
}

/// Parse a received catalog reply (subject + decoded body) into a structured view, or raw.
/// Never errors on content (parse_reply degrades to raw); the `Result` is for IPC uniformity.
#[tauri::command]
pub fn catalog_parse_reply(subject: String, body: String) -> Result<ReplyView, UiError> {
    Ok(parse_reply(&subject, &body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fetch_parses_listing_from_http() {
        let mut server = mockito::Server::new_async().await;
        let body = include_str!("../../tests/fixtures/catalog/listing-ardop-hf.txt");
        let _m = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/text")
            .with_body(body)
            .create_async()
            .await;
        let listing =
            fetch_listing_from_url(&format!("{}/listings/x", server.url()), ListingMode::ArdopHf)
                .await
                .unwrap();
        assert!(listing.parsed_ok);
        assert!(!listing.gateways.is_empty());
    }

    #[tokio::test]
    async fn fetch_maps_non_2xx_to_unavailable() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(404)
            .with_body("nope")
            .create_async()
            .await;
        let err = fetch_listing_from_url(&format!("{}/x", server.url()), ListingMode::VaraHf)
            .await
            .unwrap_err();
        assert!(matches!(err, UiError::Unavailable { .. }));
    }

    #[test]
    fn parse_reply_command_returns_raw_for_unknown_subject() {
        let v = catalog_parse_reply("Service Advice Message".into(), "hi".into()).unwrap();
        assert!(matches!(v, ReplyView::Raw { .. }));
    }
}
