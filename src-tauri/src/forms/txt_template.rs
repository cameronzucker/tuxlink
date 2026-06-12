//! WLE `.txt` form-template parser (tuxlink-o4p9 / Forms-push G12-A).
//!
//! A WLE form is a `.txt` template + the HTML data-entry page(s). The `.txt`
//! defines the *message* the form produces: who it goes `To:`, its `Subject:`,
//! and the human-readable `Msg:` body — each built from `<var fieldname>`
//! placeholders substituted with the operator's submitted field values. tuxlink
//! previously parsed only the `Form:` directive (for import detection) and
//! ignored the rest, so the ~131 generic-path catalog/org forms sent with a
//! generic `Form: <id>` subject, a raw key:value dump body, and the
//! operator-typed recipient — discarding the form designer's prescribed
//! recipient (often a fixed tactical/agency address like DYFI → USGS) and the
//! templated subject/body. This module parses the full directive set so the
//! send path can honor it (see `forms::serialize` + `send_webview_form`).
//!
//! ## The `.txt` grammar (observed across the 152 bundled templates)
//!
//! - CRLF line endings, Windows-1252 encoded (the caller decodes to `String`
//!   before calling [`parse_txt_template`]; this module is encoding-agnostic).
//! - Leading directives, one per line, `Name:` then a value. The colon may be
//!   followed by a space or not (`Form:USGS DYFI.html` vs
//!   `Form: Quick Message Initial.html`). Values may carry trailing tabs/spaces
//!   (`Subject: <var Subjectline>\t`) which are trimmed.
//! - Directives observed: `Form:`, `Display:`, `To:`, `Cc:`, `Subject:`,
//!   `ReplyTemplate:`, `Def:`, `SeqInc`/`SeqSet`, `Readonly:`. Unknown or blank
//!   lines before `Msg:` are ignored.
//! - `Msg:` is the LAST directive: everything after it (the rest of the `Msg:`
//!   line plus every following line) is the body template, verbatim except for
//!   leading/trailing blank-line trimming and CRLF→LF normalization. `<var X>`
//!   and host tags (`<MsgSender>` …) are left intact for the renderer.

/// A parsed WLE `.txt` form template. Every field is optional because real
/// templates omit directives freely (a broadcast form has no `To:`, a
/// fire-and-forget form has no `ReplyTemplate:`, etc.).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TxtTemplate {
    /// `Form:` input (authoring) HTML filename.
    pub input_html: Option<String>,
    /// `Form:` optional display (viewer) HTML filename (the part after the comma).
    pub display_html: Option<String>,
    /// `To:` line — a fixed address, a `<var fieldname>`, or a mix. Rendered
    /// against field values by the caller.
    pub to: Option<String>,
    /// `Cc:` line (rare).
    pub cc: Option<String>,
    /// `Subject:` line — literal text, `<var fieldname>`, or a mix.
    pub subject: Option<String>,
    /// `ReplyTemplate:` — the SendReply template this form's replies invoke
    /// (tuxlink-hhfx / G10 consumes this).
    pub reply_template: Option<String>,
    /// `Def: KEY=VALUE` default declarations, in document order.
    pub defs: Vec<(String, String)>,
    /// `Msg:` body template (LF-normalized, leading/trailing blank lines
    /// trimmed). `<var X>` + host tags preserved for the renderer.
    pub msg: Option<String>,
}

/// Directive names recognized before the `Msg:` block. Case-insensitive match.
/// A line whose prefix (up to `:`) isn't one of these — and isn't `Msg:` — is
/// ignored (covers `Readonly:`, blank lines, and any future directive we don't
/// model). `Msg` is handled separately because it consumes the remainder.
fn match_directive(line: &str) -> Option<(&'static str, &str)> {
    // Find the first colon; the name is everything before it.
    let colon = line.find(':')?;
    let (name, rest) = line.split_at(colon);
    let value = rest[1..].trim(); // skip ':' then trim spaces/tabs/CR
    let canon = match name.trim().to_ascii_lowercase().as_str() {
        "form" => "Form",
        "display" => "Display",
        "to" => "To",
        "cc" => "Cc",
        "subject" => "Subject",
        "replytemplate" => "ReplyTemplate",
        "def" => "Def",
        _ => return None,
    };
    Some((canon, value))
}

/// Parse a decoded `.txt` template into its directive set + body. Lenient: any
/// directive may be absent; unknown lines before `Msg:` are skipped.
pub fn parse_txt_template(raw: &str) -> TxtTemplate {
    let mut tpl = TxtTemplate::default();

    // Normalize line endings so `\r` never rides along on a value or in the
    // body. Split keeps empty lines (needed for body fidelity).
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = normalized.split('\n');

    // --- Directive section: walk until the `Msg:` line. ---
    let mut msg_first_line: Option<String> = None;
    for line in lines.by_ref() {
        // Detect the Msg: boundary first — it's the only multi-line directive
        // and it ends the directive section.
        let trimmed_start = line.trim_start();
        if let Some(colon) = trimmed_start.find(':') {
            if trimmed_start[..colon].trim().eq_ignore_ascii_case("msg") {
                // Any content after `Msg:` on the same line is the body's first
                // line (rare, but cheap to support). Trim the leading separator
                // whitespace after the colon; the body's own following lines
                // keep their indentation.
                let after = trimmed_start[colon + 1..].trim_start();
                if !after.is_empty() {
                    msg_first_line = Some(after.to_string());
                }
                break;
            }
        }

        let Some((name, value)) = match_directive(line) else {
            continue; // blank line, Readonly:, or anything we don't model
        };
        match name {
            "Form" => {
                // `input.html` or `input.html,display.html` (comma-separated;
                // either side may carry surrounding whitespace).
                let mut parts = value.splitn(2, ',');
                let input = parts.next().unwrap_or("").trim();
                if !input.is_empty() {
                    tpl.input_html = Some(input.to_string());
                }
                if let Some(display) = parts.next() {
                    let d = display.trim();
                    if !d.is_empty() {
                        tpl.display_html = Some(d.to_string());
                    }
                }
            }
            "Display" => {
                if !value.is_empty() {
                    tpl.display_html = Some(value.to_string());
                }
            }
            // To:/Cc:/Subject: keep their value verbatim (after trim) — it may
            // be empty (e.g. SendReply's blank `To:`), which the caller treats
            // as "no template-prescribed recipient, fall back to operator input".
            "To" => tpl.to = Some(value.to_string()),
            "Cc" => tpl.cc = Some(value.to_string()),
            "Subject" => tpl.subject = Some(value.to_string()),
            "ReplyTemplate" => {
                if !value.is_empty() {
                    tpl.reply_template = Some(value.to_string());
                }
            }
            "Def" => {
                // `KEY=VALUE`; tolerate a missing `=` (key only).
                if let Some(eq) = value.find('=') {
                    let key = value[..eq].trim().to_string();
                    let val = value[eq + 1..].to_string();
                    if !key.is_empty() {
                        tpl.defs.push((key, val));
                    }
                } else if !value.trim().is_empty() {
                    tpl.defs.push((value.trim().to_string(), String::new()));
                }
            }
            _ => {}
        }
    }

    // --- Body section: the remainder is the Msg: body template. ---
    let mut body_lines: Vec<String> = Vec::new();
    if let Some(first) = msg_first_line {
        body_lines.push(first);
    }
    for line in lines {
        body_lines.push(line.to_string());
    }
    if msg_seen(&normalized) {
        let body = trim_blank_edges(&body_lines).join("\n");
        // A Msg: directive with an entirely-blank body still counts as present
        // but yields None (nothing to render) so the caller falls back cleanly.
        tpl.msg = if body.is_empty() { None } else { Some(body) };
    }

    tpl
}

/// True if the template has a `Msg:` directive line at all (distinguishes "no
/// Msg: directive" from "Msg: present but empty body").
fn msg_seen(normalized: &str) -> bool {
    normalized.split('\n').any(|l| {
        let t = l.trim_start();
        t.find(':')
            .map(|c| t[..c].trim().eq_ignore_ascii_case("msg"))
            .unwrap_or(false)
    })
}

/// Drop leading and trailing all-whitespace lines; preserve internal blanks.
fn trim_blank_edges(lines: &[String]) -> Vec<String> {
    let start = lines.iter().position(|l| !l.trim().is_empty());
    let end = lines.iter().rposition(|l| !l.trim().is_empty());
    match (start, end) {
        (Some(s), Some(e)) => lines[s..=e].to_vec(),
        _ => Vec::new(),
    }
}

/// XML 1.0 legal `Char` test — drops C0 control characters a field value
/// might carry (copy-paste-from-PDF artifacts) so the rendered message text
/// stays well-formed, mirroring `forms::serialize`'s sanitization. Kept local
/// so this module is self-contained; the rule is a fixed Unicode-spec constant.
fn is_xml10_legal(c: char) -> bool {
    let u = c as u32;
    u == 0x9
        || u == 0xA
        || u == 0xD
        || (0x20..=0xD7FF).contains(&u)
        || (0xE000..=0xFFFD).contains(&u)
        || (0x10000..=0x10FFFF).contains(&u)
}

/// Render a `To:`/`Subject:`/`Msg:` template string into final message text.
///
/// Two substitution syntaxes, both case-insensitive:
/// - `<var fieldname>` → the operator's submitted field value (`field_values`).
///   An unknown field renders empty — matching WLE, which leaves an unfilled
///   `<var>` blank rather than literal.
/// - `<HostTag>` (a bare alphanumeric/underscore token, no space) → a
///   host-substituted value (`host_tags`): `MsgSender`, `ProgramVersion`,
///   `Callsign`, `GridSquare`, `DateTime`, `Date`, `Time`, `MsgTo`, `MsgCc`, …
///   An unrecognized `<Tag>` is left verbatim (it may be legitimate body text).
///
/// Substituted values are filtered to the XML-1.0-legal character set (the same
/// guarantee `forms::serialize` gives the XML attachment); the template's own
/// literal text is trusted and passes through unchanged. `<` characters that
/// don't open a recognized tag are emitted literally.
pub fn render_template(
    template: &str,
    field_values: &std::collections::HashMap<String, String>,
    host_tags: &std::collections::HashMap<String, String>,
) -> String {
    let fields_lc: std::collections::HashMap<String, &str> = field_values
        .iter()
        .map(|(k, v)| (k.to_ascii_lowercase(), v.as_str()))
        .collect();
    let tags_lc: std::collections::HashMap<String, &str> = host_tags
        .iter()
        .map(|(k, v)| (k.to_ascii_lowercase(), v.as_str()))
        .collect();

    let push_sanitized = |out: &mut String, v: &str| {
        out.extend(v.chars().filter(|&c| is_xml10_legal(c)));
    };

    let mut out = String::with_capacity(template.len() + 128);
    let mut rest = template;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        let after = &rest[lt..];

        // <var fieldname>
        if after.len() >= 5 && after.as_bytes()[..5].eq_ignore_ascii_case(b"<var ") {
            if let Some(end) = after.find('>') {
                let field = after[5..end].trim().to_ascii_lowercase();
                if let Some(v) = fields_lc.get(&field) {
                    push_sanitized(&mut out, v);
                }
                rest = &after[end + 1..];
                continue;
            }
        }

        // <HostTag> — bare alnum/underscore token
        if let Some(end) = after.find('>') {
            let inner = &after[1..end];
            if !inner.is_empty()
                && inner.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                if let Some(v) = tags_lc.get(&inner.to_ascii_lowercase()) {
                    push_sanitized(&mut out, v);
                    rest = &after[end + 1..];
                    continue;
                }
            }
        }

        // Not a recognized tag: emit '<' literally and advance past it.
        out.push('<');
        rest = &after[1..];
    }
    out.push_str(rest);
    out
}

/// Map one Windows-1252 byte to its Unicode `char`. 0x00–0x7F and 0xA0–0xFF are
/// identical to Unicode; only 0x80–0x9F carry cp1252's distinct punctuation
/// (smart quotes, em/en dash, ellipsis, …). The five bytes undefined in cp1252
/// (0x81/0x8D/0x8F/0x90/0x9D) fall back to their code point so decoding is total.
fn cp1252_char(b: u8) -> char {
    match b {
        0x80 => '\u{20AC}', 0x82 => '\u{201A}', 0x83 => '\u{0192}', 0x84 => '\u{201E}',
        0x85 => '\u{2026}', 0x86 => '\u{2020}', 0x87 => '\u{2021}', 0x88 => '\u{02C6}',
        0x89 => '\u{2030}', 0x8A => '\u{0160}', 0x8B => '\u{2039}', 0x8C => '\u{0152}',
        0x8E => '\u{017D}', 0x91 => '\u{2018}', 0x92 => '\u{2019}', 0x93 => '\u{201C}',
        0x94 => '\u{201D}', 0x95 => '\u{2022}', 0x96 => '\u{2013}', 0x97 => '\u{2014}',
        0x98 => '\u{02DC}', 0x99 => '\u{2122}', 0x9A => '\u{0161}', 0x9B => '\u{203A}',
        0x9C => '\u{0153}', 0x9E => '\u{017E}', 0x9F => '\u{0178}',
        other => other as char,
    }
}

/// Decode Windows-1252 bytes (the WLE bundle's encoding) to a `String`. Using
/// `String::from_utf8_lossy` instead (as `forms::import` historically does)
/// corrupts cp1252 punctuation into U+FFFD — a real defect for `Msg:` bodies
/// with curly apostrophes / em dashes.
pub fn decode_cp1252(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| cp1252_char(b)).collect()
}

/// Find the `.txt` template that governs `form_html_path`: the sibling `.txt`
/// whose `Form:` directive names this HTML file. Returns the parsed template.
///
/// The `.txt`↔`.html` link is the `Form:` directive, NOT a shared stem —
/// `ICS213_Initial.html` is governed by `ICS213 General Message.txt`. Returns
/// `None` when no governing `.txt` exists (operator-custom HTML with no
/// template, or a bundled form whose directives we don't need), in which case
/// the caller keeps its existing generic send behavior.
pub fn resolve_governing_txt(form_html_path: &std::path::Path) -> Option<TxtTemplate> {
    let folder = form_html_path.parent()?;
    let html_name = form_html_path.file_name()?.to_str()?;
    for entry in std::fs::read_dir(folder).ok()?.flatten() {
        let p = entry.path();
        let is_txt = p
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("txt"))
            .unwrap_or(false);
        if !is_txt {
            continue;
        }
        let Ok(bytes) = std::fs::read(&p) else { continue };
        let tpl = parse_txt_template(&decode_cp1252(&bytes));
        if tpl
            .input_html
            .as_deref()
            .map(|i| i.eq_ignore_ascii_case(html_name))
            .unwrap_or(false)
        {
            return Some(tpl);
        }
    }
    None
}

/// A resolved SendReply (tuxlink-hhfx / G10): the authoring HTML to serve in an
/// editable, pre-bound reply session, plus the parsed `.0` template that governs
/// the reply *message* (its `To:`/`Subject:`/`Msg:` projection + display viewer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendReply {
    /// Absolute path to the SendReply authoring HTML (`<X>_SendReply.html`) — the
    /// page the operator fills the Reply section on.
    pub html_path: std::path::PathBuf,
    /// The parsed `.0` template governing the reply message. `subject` is
    /// typically absent (SendReply `.0`s carry no `Subject:` directive — the
    /// reply subject comes from the operator's "Re: <original>" draft); `to` is
    /// typically `Some("")` (reply goes back to the operator-supplied original
    /// sender). `display_html` is the SendReply *viewer* a WLE recipient renders.
    pub template: TxtTemplate,
}

/// Resolve a form's SendReply: given the form's folder and the `ReplyTemplate:`
/// filename (e.g. `ICS213_SendReply.0`, taken from the form's governing `.txt`),
/// parse the `.0` and locate its authoring HTML.
///
/// The `.0`↔`.html` link is the `.0`'s own `Form:` directive, NOT a shared stem:
/// `HICS213_SendReply.0` declares `Form: HICS 213_SendReply.html,...` (note the
/// space the `.0` stem lacks). Resolving by stem would miss it, so the `Form:`
/// directive ([`parse_txt_template`]'s `input_html`) is the source of truth.
///
/// Returns `None` if the `.0` is missing/unreadable, declares no `Form:` input
/// HTML, or that HTML is absent on disk — the caller falls back (plain reply /
/// the legacy same-form `replyWithForm`).
pub fn resolve_sendreply(
    form_folder: &std::path::Path,
    reply_template_name: &str,
) -> Option<SendReply> {
    let txt_path = resolve_sibling_case_insensitive(form_folder, reply_template_name)?;
    let bytes = std::fs::read(&txt_path).ok()?;
    let template = parse_txt_template(&decode_cp1252(&bytes));
    let html_name = template.input_html.as_deref()?;
    let html_path = resolve_sibling_case_insensitive(form_folder, html_name)?;
    Some(SendReply {
        html_path,
        template,
    })
}

/// Find `name` inside `folder`: exact match first, then a case-insensitive scan.
/// WLE `ReplyTemplate:`/`Form:` directives usually match the on-disk name
/// exactly, but tolerate case drift so a directive like `ics213_sendreply.0`
/// still resolves on a case-sensitive filesystem.
fn resolve_sibling_case_insensitive(
    folder: &std::path::Path,
    name: &str,
) -> Option<std::path::PathBuf> {
    let exact = folder.join(name);
    if exact.exists() {
        return Some(exact);
    }
    for entry in std::fs::read_dir(folder).ok()?.flatten() {
        if entry
            .file_name()
            .to_str()
            .map(|f| f.eq_ignore_ascii_case(name))
            .unwrap_or(false)
        {
            return Some(entry.path());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn fields(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    // Real bundle sample: General Forms/Quick Message.txt (var-addressed,
    // trailing tab on Subject, multi-line Msg with a blank line + var body).
    const QUICK_MESSAGE: &str = "Form: Quick Message Initial.html\r\n\
        \r\n\
        To: <var address>\r\n\
        Subject: <var Subjectline>\t\r\n\
        Msg:\r\n\
        From <var From_Name>\r\n\
        Sent on <var time>\r\n\
        \r\n\
        <var Message>\r\n";

    // Real bundle sample: Mapping-GIS forms/USGS DYFI.txt (FIXED To:, mixed
    // literal+var Subject, Readonly: between Subject and Msg, blank-line-padded
    // body).
    const DYFI: &str = "Form:USGS DYFI.html\r\n\
        To: dyfi_reports_automated@usgs.gov\r\n\
        Subject: DYFI Automatic Entry - Winlink <var EVENTType>\r\n\
        Readonly:True\r\n\
        Msg:\r\n\
        \r\n\
        --- BEGIN json ---\r\n\
        \r\n\
        <var parseme>\r\n\
        \r\n\
        --- END json ---\r\n";

    #[test]
    fn parses_form_with_trailing_space_variant() {
        let t = parse_txt_template(QUICK_MESSAGE);
        assert_eq!(t.input_html.as_deref(), Some("Quick Message Initial.html"));
        assert_eq!(t.display_html, None);
    }

    #[test]
    fn parses_form_without_space_after_colon() {
        let t = parse_txt_template(DYFI);
        assert_eq!(t.input_html.as_deref(), Some("USGS DYFI.html"));
    }

    #[test]
    fn parses_form_with_display_companion() {
        let t = parse_txt_template(
            "Form: ICS213_Initial.html,ICS213_Initial_Viewer.html\r\nMsg:\r\nbody\r\n",
        );
        assert_eq!(t.input_html.as_deref(), Some("ICS213_Initial.html"));
        assert_eq!(t.display_html.as_deref(), Some("ICS213_Initial_Viewer.html"));
    }

    #[test]
    fn captures_var_addressed_to() {
        let t = parse_txt_template(QUICK_MESSAGE);
        assert_eq!(t.to.as_deref(), Some("<var address>"));
    }

    #[test]
    fn captures_fixed_address_to() {
        let t = parse_txt_template(DYFI);
        assert_eq!(t.to.as_deref(), Some("dyfi_reports_automated@usgs.gov"));
    }

    #[test]
    fn trims_trailing_tab_on_subject() {
        let t = parse_txt_template(QUICK_MESSAGE);
        // The real file has a trailing TAB after the closing `>`.
        assert_eq!(t.subject.as_deref(), Some("<var Subjectline>"));
    }

    #[test]
    fn keeps_mixed_literal_and_var_subject() {
        let t = parse_txt_template(DYFI);
        assert_eq!(
            t.subject.as_deref(),
            Some("DYFI Automatic Entry - Winlink <var EVENTType>")
        );
    }

    #[test]
    fn msg_body_is_everything_after_msg_line_lf_normalized() {
        let t = parse_txt_template(QUICK_MESSAGE);
        assert_eq!(
            t.msg.as_deref(),
            Some("From <var From_Name>\nSent on <var time>\n\n<var Message>")
        );
    }

    #[test]
    fn msg_body_preserves_internal_blanks_trims_edge_blanks() {
        let t = parse_txt_template(DYFI);
        // Leading blank line after `Msg:` is trimmed; the internal blank lines
        // around the json markers are preserved.
        assert_eq!(
            t.msg.as_deref(),
            Some("--- BEGIN json ---\n\n<var parseme>\n\n--- END json ---")
        );
    }

    #[test]
    fn readonly_directive_is_ignored_not_misparsed_as_body() {
        // Readonly: sits between Subject and Msg; it must not leak into the body
        // or be mistaken for a modeled directive.
        let t = parse_txt_template(DYFI);
        assert!(!t.msg.as_deref().unwrap_or("").contains("Readonly"));
        assert_eq!(t.subject.as_deref(), Some("DYFI Automatic Entry - Winlink <var EVENTType>"));
    }

    #[test]
    fn parses_reply_template_directive() {
        let t = parse_txt_template(
            "Form:ICS213_Initial.html,ICS213_Initial_Viewer.html\r\n\
             ReplyTemplate:ICS213_SendReply.0\r\nMsg:\r\nx\r\n",
        );
        assert_eq!(t.reply_template.as_deref(), Some("ICS213_SendReply.0"));
    }

    #[test]
    fn parses_def_key_value() {
        let t = parse_txt_template(
            "Form: X.html\r\nDef: MsgOriginalBody=<var MsgOriginalBody>\r\nMsg:\r\nb\r\n",
        );
        assert_eq!(
            t.defs,
            vec![("MsgOriginalBody".to_string(), "<var MsgOriginalBody>".to_string())]
        );
    }

    #[test]
    fn absent_directives_are_none() {
        // A broadcast-style form: Form: + Msg: only, no To/Subject/Reply.
        let t = parse_txt_template("Form: Bulletin Initial.html\r\nMsg:\r\nhello\r\n");
        assert_eq!(t.to, None);
        assert_eq!(t.subject, None);
        assert_eq!(t.reply_template, None);
        assert!(t.defs.is_empty());
        assert_eq!(t.msg.as_deref(), Some("hello"));
    }

    #[test]
    fn blank_to_is_some_empty_not_none() {
        // SendReply templates have a literal blank `To:` — distinct from "no To:
        // directive". The caller treats empty-To as "fall back to operator
        // recipient", so the distinction (Some("") vs None) is preserved here.
        let t = parse_txt_template("Form: ICS213_SendReply.html\r\nTo: \r\nMsg:\r\nb\r\n");
        assert_eq!(t.to.as_deref(), Some(""));
    }

    #[test]
    fn no_msg_directive_yields_none_body() {
        let t = parse_txt_template("Form: X.html\r\nTo: a@b.c\r\n");
        assert_eq!(t.msg, None);
        assert_eq!(t.to.as_deref(), Some("a@b.c"));
    }

    #[test]
    fn msg_present_but_empty_body_is_none() {
        // `Msg:` with only blank lines after it → present directive, no
        // renderable body → None (caller falls back to its default body).
        let t = parse_txt_template("Form: X.html\r\nMsg:\r\n\r\n\r\n");
        assert_eq!(t.msg, None);
    }

    #[test]
    fn handles_bare_lf_line_endings() {
        // Defensive: some operator-authored / cross-platform .txt use bare LF.
        let t = parse_txt_template("Form: X.html\nTo: <var dest>\nSubject: Hi\nMsg:\nLine1\nLine2\n");
        assert_eq!(t.input_html.as_deref(), Some("X.html"));
        assert_eq!(t.to.as_deref(), Some("<var dest>"));
        assert_eq!(t.subject.as_deref(), Some("Hi"));
        assert_eq!(t.msg.as_deref(), Some("Line1\nLine2"));
    }

    #[test]
    fn case_insensitive_directive_names() {
        let t = parse_txt_template("FORM: X.html\r\nTO: a@b.c\r\nSUBJECT: S\r\nMSG:\r\nb\r\n");
        assert_eq!(t.input_html.as_deref(), Some("X.html"));
        assert_eq!(t.to.as_deref(), Some("a@b.c"));
        assert_eq!(t.subject.as_deref(), Some("S"));
        assert_eq!(t.msg.as_deref(), Some("b"));
    }

    #[test]
    fn content_after_msg_on_same_line_is_body() {
        // Rare, but `Msg: text` (body starting on the directive line) is supported.
        let t = parse_txt_template("Form: X.html\r\nMsg: inline body\r\nmore\r\n");
        assert_eq!(t.msg.as_deref(), Some("inline body\nmore"));
    }

    // ---- render_template ----------------------------------------------

    #[test]
    fn render_substitutes_var_fields_case_insensitive() {
        let fv = fields(&[("address", "W7ABC"), ("subjectline", "Net check-in")]);
        let ht = HashMap::new();
        assert_eq!(render_template("<var address>", &fv, &ht), "W7ABC");
        // .txt casing (`<var Subjectline>`) vs field key casing (`subjectline`).
        assert_eq!(render_template("<var Subjectline>", &fv, &ht), "Net check-in");
    }

    #[test]
    fn render_unknown_var_is_empty() {
        let fv = fields(&[("a", "1")]);
        assert_eq!(render_template("x=<var missing>!", &fv, &HashMap::new()), "x=!");
    }

    #[test]
    fn render_substitutes_host_tags() {
        let fv = HashMap::new();
        let ht = fields(&[("MsgSender", "N0CALL"), ("ProgramVersion", "Tuxlink/0.53.1")]);
        assert_eq!(
            render_template("Sent by <MsgSender> using <ProgramVersion>", &fv, &ht),
            "Sent by N0CALL using Tuxlink/0.53.1"
        );
    }

    #[test]
    fn render_mixed_literal_var_and_host_tag() {
        // Mirrors USGS DYFI Subject: "DYFI Automatic Entry - Winlink <var EVENTType>".
        let fv = fields(&[("eventtype", "Exercise")]);
        let ht = fields(&[("Callsign", "W7ABC")]);
        assert_eq!(
            render_template("DYFI Automatic Entry - Winlink <var EVENTType> [<Callsign>]", &fv, &ht),
            "DYFI Automatic Entry - Winlink Exercise [W7ABC]"
        );
    }

    #[test]
    fn render_unknown_host_tag_left_verbatim() {
        // An unrecognized bare tag is preserved (could be legitimate body text);
        // we never strip content we don't understand.
        let out = render_template("a <SomethingWeDontModel> b", &HashMap::new(), &HashMap::new());
        assert_eq!(out, "a <SomethingWeDontModel> b");
    }

    #[test]
    fn render_leaves_stray_lt_literal() {
        // A `<` that doesn't open a tag (e.g. "less than") stays literal.
        let fv = fields(&[("n", "5")]);
        assert_eq!(render_template("qty <var n> < 10", &fv, &HashMap::new()), "qty 5 < 10");
    }

    #[test]
    fn render_sanitizes_illegal_control_chars_in_values() {
        // A form-feed in a field value (copy-paste-from-PDF) must not survive
        // into the rendered message text.
        let fv = fields(&[("note", "line1\u{000C}line2")]);
        let out = render_template("<var note>", &fv, &HashMap::new());
        assert_eq!(out, "line1line2");
        assert!(!out.chars().any(|c| !is_xml10_legal(c)));
    }

    #[test]
    fn render_preserves_legitimate_text_and_whitespace() {
        let fv = fields(&[("msg", "Hwy 2 washed out\nDetour via Old Cascade Rd")]);
        let out = render_template("Report:\n<var msg>\n", &fv, &HashMap::new());
        assert_eq!(out, "Report:\nHwy 2 washed out\nDetour via Old Cascade Rd\n");
    }

    #[test]
    fn render_quick_message_body_end_to_end() {
        // Parse the real Quick Message.txt body, then render it with operator
        // field values — the full parse→render path for the Msg: projection.
        let t = parse_txt_template(QUICK_MESSAGE);
        let fv = fields(&[
            ("from_name", "Jane / W7ABC"),
            ("time", "2026-06-12 14:30Z"),
            ("message", "Net moved to 7.185."),
        ]);
        let body = render_template(t.msg.as_deref().unwrap(), &fv, &HashMap::new());
        assert_eq!(body, "From Jane / W7ABC\nSent on 2026-06-12 14:30Z\n\nNet moved to 7.185.");
    }

    // ---- decode_cp1252 ------------------------------------------------

    #[test]
    fn cp1252_decodes_smart_punctuation_not_replacement_char() {
        // 0x92 = right single quote in cp1252. from_utf8_lossy would yield U+FFFD.
        let decoded = decode_cp1252(b"Don\x92t edit \x93this\x94 \x96 ok");
        assert_eq!(decoded, "Don\u{2019}t edit \u{201C}this\u{201D} \u{2013} ok");
        assert!(!decoded.contains('\u{FFFD}'));
    }

    #[test]
    fn cp1252_passes_ascii_and_latin1_through() {
        assert_eq!(decode_cp1252(b"To: dyfi_reports_automated@usgs.gov"), "To: dyfi_reports_automated@usgs.gov");
        // 0xE9 = é in both cp1252 and Latin-1.
        assert_eq!(decode_cp1252(b"caf\xe9"), "café");
    }

    // ---- resolve_governing_txt ----------------------------------------

    #[test]
    fn resolves_txt_by_form_directive_not_stem() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        // The .txt stem ("ICS213 General Message") differs from the .html stem
        // ("ICS213_Initial") — the link is the Form: directive.
        let mut txt = std::fs::File::create(dir.path().join("ICS213 General Message.txt")).unwrap();
        txt.write_all(b"Form: ICS213_Initial.html,ICS213_Initial_Viewer.html\r\nTo: <var to_name>\r\nSubject: <var subjectline>\r\nMsg:\r\nbody\r\n").unwrap();
        std::fs::write(dir.path().join("ICS213_Initial.html"), b"<html></html>").unwrap();

        let resolved = resolve_governing_txt(&dir.path().join("ICS213_Initial.html"))
            .expect("governing .txt resolved by Form: directive");
        assert_eq!(resolved.to.as_deref(), Some("<var to_name>"));
        assert_eq!(resolved.subject.as_deref(), Some("<var subjectline>"));
        assert_eq!(resolved.msg.as_deref(), Some("body"));
    }

    #[test]
    fn resolves_fixed_address_txt() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let mut txt = std::fs::File::create(dir.path().join("USGS DYFI.txt")).unwrap();
        // Real DYFI .txt: cp1252-clean, fixed To:, Readonly between Subject+Msg.
        txt.write_all(b"Form:USGS DYFI.html\r\nTo: dyfi_reports_automated@usgs.gov\r\nSubject: DYFI Automatic Entry - Winlink <var EVENTType>\r\nReadonly:True\r\nMsg:\r\n<var parseme>\r\n").unwrap();
        std::fs::write(dir.path().join("USGS DYFI.html"), b"<html></html>").unwrap();

        let resolved = resolve_governing_txt(&dir.path().join("USGS DYFI.html")).unwrap();
        assert_eq!(resolved.to.as_deref(), Some("dyfi_reports_automated@usgs.gov"));
    }

    #[test]
    fn returns_none_when_no_governing_txt() {
        let dir = tempfile::tempdir().unwrap();
        // An operator-custom HTML with no governing .txt.
        std::fs::write(dir.path().join("MyCustom.html"), b"<html></html>").unwrap();
        // A sibling .txt that governs a DIFFERENT html — must not match.
        std::fs::write(
            dir.path().join("Other.txt"),
            b"Form: SomethingElse.html\r\nMsg:\r\nx\r\n",
        )
        .unwrap();
        assert!(resolve_governing_txt(&dir.path().join("MyCustom.html")).is_none());
    }

    // ---- resolve_sendreply (G10) --------------------------------------

    #[test]
    fn resolves_sendreply_html_via_form_directive_despite_stem_drift() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        // Mirror the real HICS naming drift: the `.0` stem has NO space
        // ("HICS213_SendReply.0") but its Form: directive points to a SPACE-
        // bearing html ("HICS 213_SendReply.html"). Stem-matching would miss it.
        let mut txt = std::fs::File::create(dir.path().join("HICS213_SendReply.0")).unwrap();
        txt.write_all(
            b"Form: HICS 213_SendReply.html,HICS 213_SendReply_Viewer.html\r\n\
              Def: MsgOriginalBody=<var MsgOriginalBody>\r\nTo: \r\nMsg:\r\nReply: <var Reply>\r\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("HICS 213_SendReply.html"), b"<html></html>").unwrap();

        let resolved = resolve_sendreply(dir.path(), "HICS213_SendReply.0")
            .expect("SendReply resolved via the .0's Form: directive");
        assert_eq!(resolved.html_path, dir.path().join("HICS 213_SendReply.html"));
        assert_eq!(
            resolved.template.display_html.as_deref(),
            Some("HICS 213_SendReply_Viewer.html")
        );
        // SendReply's To: is a literal blank → Some("") (operator-recipient fallback).
        assert_eq!(resolved.template.to.as_deref(), Some(""));
        // No Subject: directive on a SendReply .0.
        assert_eq!(resolved.template.subject, None);
        assert_eq!(resolved.template.msg.as_deref(), Some("Reply: <var Reply>"));
    }

    #[test]
    fn sendreply_none_when_html_missing() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        // The .0 exists and names an html that ISN'T on disk → None (fall back).
        let mut txt = std::fs::File::create(dir.path().join("ICS213_SendReply.0")).unwrap();
        txt.write_all(b"Form: ICS213_SendReply.html\r\nMsg:\r\nx\r\n").unwrap();
        assert!(resolve_sendreply(dir.path(), "ICS213_SendReply.0").is_none());
    }

    #[test]
    fn sendreply_none_when_dot0_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(resolve_sendreply(dir.path(), "Nonexistent_SendReply.0").is_none());
    }

    #[test]
    fn resolves_sendreply_case_insensitively() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let mut txt = std::fs::File::create(dir.path().join("ICS213_SendReply.0")).unwrap();
        txt.write_all(b"Form: ICS213_SendReply.html\r\nMsg:\r\nb\r\n").unwrap();
        std::fs::write(dir.path().join("ICS213_SendReply.html"), b"<html></html>").unwrap();
        // Caller passes a differently-cased name (directive drift).
        let resolved =
            resolve_sendreply(dir.path(), "ics213_sendreply.0").expect("case-insensitive resolve");
        assert_eq!(resolved.html_path, dir.path().join("ICS213_SendReply.html"));
    }
}
