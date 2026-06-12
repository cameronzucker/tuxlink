//! HTML Forms support per spec docs/superpowers/specs/2026-05-30-html-forms-design.md
//! and the full-parity revision in docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md.

pub mod catalog;
pub mod draft_library; // tuxlink-hnkn P2 Task 4 — save/reuse named slots
pub mod http_server;
pub mod import; // tuxlink-z0le/fwob — in-app form import (G5+G6)
pub mod multipart;
pub mod parse;
pub mod pdf_export; // tuxlink-cumx / G8 — on-demand faithful PDF export of a rendered form
pub mod serialize;
pub mod skin;
pub mod templates;
pub mod txt_template; // tuxlink-o4p9 / G12-A — WLE .txt form-template parser (To:/Subject:/Msg:)
pub mod types;
pub mod updater; // tuxlink-xipa Phase 3 — winlink.org Standard Forms refresh (backend layer; IPC + UI in follow-up PRs)
pub mod validation;
pub mod wle_templates;

// Re-exports for ergonomic access.
pub use parse::{detect_form_attachment, parse_form_xml};
pub use serialize::{render_body_template, serialize_catalog_form_xml, serialize_form_xml};
pub use types::{FieldKind, FormDef, FormField, FormParameters, FormPayload};
