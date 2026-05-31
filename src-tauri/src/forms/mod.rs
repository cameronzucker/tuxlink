//! HTML Forms support per spec docs/superpowers/specs/2026-05-30-html-forms-design.md.

pub mod catalog;
pub mod parse;
pub mod serialize;
pub mod templates;
pub mod types;
pub mod validation;

// Re-exports for ergonomic access. serialize re-exports added in T1.6+.
pub use parse::{detect_form_attachment, parse_form_xml};
pub use types::{FieldKind, FormDef, FormField, FormParameters, FormPayload};
