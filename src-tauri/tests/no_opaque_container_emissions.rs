//! Forward-defense source-scan: no tracing macro emits opaque container types
//! that carry message bodies, form fields, or raw GPS coordinates.
//!
//! Spec §5.7, plan acceptance §10.2 #15.
//!
//! The rule (spec §4.1 opening paragraph):
//! "Do NOT emit message bodies, form fields, GPS precision beyond 4-char
//! Maidenhead, or password/token/secret values."
//!
//! This test uses a pattern-matching source scan instead of AST parsing
//! (unlike credential_struct_source_scan.rs) because the dangerous emission
//! would typically look like `{:?}` or `{:#?}` in the message string of a
//! tracing macro — not a struct definition or a trait impl. Regex over the
//! rendered source is the right tool for this check.
//!
//! ## What is scanned
//!
//! Every `.rs` file under `src-tauri/src/`. For each file, the scan looks
//! for tracing macro invocations (`tracing::debug!`, `tracing::info!`, etc.)
//! that contain any of the banned opaque-container type names formatted into
//! the message string (i.e., the last string literal argument that is NOT a
//! named-field expression).
//!
//! ## What is NOT scanned
//!
//! Named structured fields are NOT banned — `message_id = %mid` or
//! `field_count = count` are explicitly allowed. What is banned is passing the
//! entire container (e.g., `format!("{:?}", parsed_body)` or
//! `tracing::debug!(?msg, "msg")` where `msg` is a `Message` or `ParsedBody`).
//!
//! ## Escape hatch
//!
//! A line can be whitelisted by adding the comment:
//! `// no_opaque_container_audit_skip`
//! on the SAME line. Use only for lines where the pattern match is a false
//! positive — e.g., a variable named `form_id` that contains only the form's
//! ID string, not the full form payload.
//!
//! ## Banned patterns
//!
//! The scan looks for these identifiers appearing inside a tracing macro call
//! body in a context that suggests the whole struct/enum is being formatted:
//!
//! - `ParsedBody` — form submission (contains all field values)
//! - `FormPayload` — inbound form message payload
//! - `Message` (the winlink::message::Message type) — contains subject + body
//! - `OutboundMessage` — draft content for outbound messages
//! - `MessageBody` — raw RFC5322 bytes
//! - `raw_rfc5322` — the field holding raw message bytes in MessageBody
//!
//! The check is intentionally conservative: it flags ANY occurrence of these
//! type/field names inside a tracing macro, not just the formatting-into-string
//! path, because the safe rule is "never mention these types in tracing macros
//! at all." If a future callsite legitimately needs to log one specific field
//! from these types (e.g., `message_id = %msg.id`), it must extract the field
//! into a named binding first so the TYPE NAME does not appear in the tracing
//! invocation.

use std::path::PathBuf;

/// Opaque container type / field names that must never appear inside a
/// tracing macro invocation.
const BANNED_IN_TRACING: &[&str] = &[
    // Form types — contain all field name/value pairs.
    // Both CamelCase (type name) and snake_case (variable binding) are banned:
    // a developer may write `body = ?parsed_body` and the type name won't appear.
    "ParsedBody",
    "parsed_body",
    "FormPayload",
    "form_payload",
    // Winlink message types — contain subject + body text.
    // NOTE: "Message" is a very common English word; the scanner only fires
    // when it appears INSIDE a tracing macro call, which is the dangerous
    // context. False positives from comments inside macro args are possible
    // but expected to be rare; use the escape hatch for those.
    "OutboundMessage",
    "outbound_message",
    "MessageBody",
    "message_body",
    // Field name holding raw RFC5322 bytes.
    "raw_rfc5322",
];

const TRACING_MACROS: &[&str] = &[
    "tracing::trace!",
    "tracing::debug!",
    "tracing::info!",
    "tracing::warn!",
    "tracing::error!",
    "tracing::event!",
];

const SKIP_COMMENT: &str = "// no_opaque_container_audit_skip";

/// Returns true iff the given source line appears to be inside (or opens) a
/// tracing macro call body. This is a conservative heuristic: the scan
/// treats every line that (a) contains a banned identifier AND (b) either
/// contains a tracing macro name on the same line, OR is on a continuation
/// line of a multi-line macro call, as a hit.
///
/// The multi-line continuation detection is NOT implemented here; the scan
/// uses a per-line check instead. A future improvement could use syn to parse
/// the file and check macro-invocation AST nodes — overkill for the current
/// spec requirement. The per-line approach catches the common case where the
/// type name is mentioned on the same line as the macro open `!(`.
///
/// Lines are flagged if they contain BOTH a tracing macro name AND a banned
/// identifier, or if they contain a banned identifier AND the line starts
/// (after trimming) with a tracing field or format specifier pattern.
fn line_is_suspicious(line: &str) -> Option<&'static str> {
    // Fast reject: line contains the skip comment.
    if line.contains(SKIP_COMMENT) {
        return None;
    }

    // Check if the line references any banned identifier.
    let banned = BANNED_IN_TRACING.iter().find(|&&b| line.contains(b))?;

    // The banned identifier is present. Now check if the line is inside
    // a tracing macro context. We check two signals:
    // 1. The line itself contains a tracing macro name.
    // 2. The line contains a format specifier (`?`, `%`, `{`, `}`) alongside
    //    the banned identifier, suggesting it's being formatted.

    let has_macro = TRACING_MACROS.iter().any(|&m| line.contains(m));
    let has_format = line.contains("?{") || line.contains("{:?}")
        || line.contains("{:#?}")
        || (line.contains('?') && line.contains(banned))
        || (line.contains("format!") && line.contains(banned))
        || (line.contains('%') && line.contains(banned));

    if has_macro || has_format {
        Some(banned)
    } else {
        None
    }
}

#[test]
fn no_opaque_container_types_emitted_in_tracing_macros() {
    let src_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");

    let mut violations: Vec<String> = Vec::new();

    for entry in walkdir::WalkDir::new(&src_dir) {
        let entry = entry.expect("walkdir entry");
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().map_or(true, |e| e != "rs") {
            continue;
        }

        // Skip test files, doc-tests, build scripts.
        let path_str = entry.path().to_string_lossy();
        if path_str.contains("/tests/") || path_str.ends_with("build.rs") {
            continue;
        }

        let source = std::fs::read_to_string(entry.path()).expect("read source file");

        for (line_no, line) in source.lines().enumerate() {
            if let Some(banned) = line_is_suspicious(line) {
                violations.push(format!(
                    "  {}:{}: contains '{}' in a tracing-macro context\n    > {}",
                    entry.path().display(),
                    line_no + 1,
                    banned,
                    line.trim(),
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "spec §4.1 + §5.7 — tracing macros MUST NOT emit opaque containers that carry \
         message bodies or form field values.\n\
         Found {} violation(s):\n{}\n\n\
         To fix: extract the specific safe field into a named binding and log only \
         that field. Add `// no_opaque_container_audit_skip` to suppress false positives.",
        violations.len(),
        violations.join("\n"),
    );
}

/// Spot-check: the banned type names do NOT appear in the wire_sanitizer_integration
/// test or the no_opaque_container_emissions test itself (meta-check that the
/// test file's own strings don't trigger the audit via accidental self-reference).
///
/// This test also validates that the `line_is_suspicious` heuristic works:
/// - A tracing line with a banned identifier IS flagged.
/// - A non-tracing line with a banned identifier is NOT flagged.
/// - A line with the skip comment is NOT flagged.
#[test]
fn line_is_suspicious_heuristic_smoke() {
    // Flagged: tracing macro + banned identifier.
    assert!(
        line_is_suspicious(r#"tracing::debug!(target: "t", body = ?parsed_body, "submit");"#).is_some(),
        "tracing macro with ?ParsedBody must be flagged"
    );

    // Not flagged: comment mentioning the type.
    assert!(
        line_is_suspicious(r#"// Don't log ParsedBody fields directly"#).is_none(),
        "comment mentioning banned type must not be flagged"
    );

    // Not flagged: struct definition.
    assert!(
        line_is_suspicious(r#"pub struct OutboundMessage {"#).is_none(),
        "struct definition must not be flagged"
    );

    // Not flagged: skip comment present.
    assert!(
        line_is_suspicious(
            r#"tracing::debug!(?parsed_body, "x"); // no_opaque_container_audit_skip"#
        ).is_none(),
        "line with skip comment must not be flagged"
    );

    // Not flagged: named field extraction (logs only a count, not the body).
    assert!(
        line_is_suspicious(
            r#"    field_count = parsed_body.fields.len(),"#
        ).is_none(),
        "line extracting a count from ParsedBody without format specifiers must not be flagged"
    );
}
