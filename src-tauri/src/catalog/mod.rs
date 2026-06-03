//! Winlink catalog-request (Inquiry) support — bundles the WLE catalog file
//! and exposes it to the UI as a structured tree of inquiries.
//!
//! Grounding doc: `docs/design/2026-06-02-cms-request-protocol-grounding.md`.
//!
//! On send the UI passes back the selected `filenames`; we compose a message
//! with `To: INQUIRY@winlink.org`, `Subject: REQUEST`, body = newline-joined
//! filenames. Empirical fixture from N7CPZ's outbox: `5YTNBV3JOZA8.mime` —
//! body literally `PUB_PACKET\r\nPUB_VARA`. The CMS replies with one separate
//! Private message per inquiry.

pub mod commands;
pub mod composer;
pub mod parser;

pub use parser::{parse_catalog, CatalogEntry, CatalogParseError, BUNDLED_CATALOG};
