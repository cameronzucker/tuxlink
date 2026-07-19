//! Order- and round-trip-preserving reader/writer for VARA's own `VARA.ini`.
//!
//! VARA (HF / FM / 2) persists ALL of its operator configuration to a plaintext
//! `VARA.ini` in its install directory (under the WINE prefix on Linux):
//! `[Soundcard] Input Device Name / Output Device Name / ALC Drive Level`,
//! `[PTT] Rig / PTTPort / CATPort / Baud`, `[Setup] TCP Command Port / Callsign
//! Licence / Registration Code`, and so on. Editing this file and relaunching
//! VARA is a deterministic way to configure it — no GUI automation required.
//!
//! This crate is the **pure** INI layer: parse, get, set, re-emit. It has no app
//! or Tauri dependencies so it builds and tests locally. The lifecycle contract
//! (VARA rewrites this file on exit, so edits must be **stop → edit → start**) is
//! owned by the app-crate glue, not here.
//!
//! # Guarantees
//! - **Round-trip byte-exact** for unmodified content: unknown sections, keys,
//!   blank lines, and the file's CRLF/LF line ending are all preserved. A write
//!   never destroys settings this crate doesn't understand.
//! - **Set is minimal**: updating a value rewrites only that `key=value` line;
//!   inserting a key appends within its section; inserting a section appends it.
//!
//! # Clean-room
//! These are VARA's documented, operator-facing config keys — not VARA's
//! protocol internals.
//!
//! # Redaction
//! `[Setup] Registration Code*` is a paid license key and `Password encryption`
//! is a secret; [`VaraIni`]'s `Debug` and [`VaraIni::redacted`] mask them so the
//! struct can never leak them into a log.

use std::fmt;

/// One physical line of the INI, categorized but position-preserving.
#[derive(Clone, PartialEq, Eq)]
enum Line {
    /// `[Name]` header; stores the inner name verbatim.
    Section(String),
    /// `key=value`; `key` is everything before the first `=`, `value` the rest
    /// (both verbatim — VARA keys contain spaces, e.g. `Output Device Name`).
    Entry { key: String, value: String },
    /// Blank line, comment, or anything without an `=` — preserved verbatim.
    Other(String),
}

/// A parsed `VARA.ini`, preserving line order, unknown content, and line ending.
#[derive(Clone)]
pub struct VaraIni {
    line_ending: &'static str,
    trailing_newline: bool,
    lines: Vec<Line>,
}

impl VaraIni {
    /// Parse `content`. Detects the dominant line ending (CRLF if any `\r\n` is
    /// present, else LF) and whether the file ends with a newline; both are
    /// reproduced by [`VaraIni::to_string`].
    pub fn parse(content: &str) -> Self {
        let line_ending = if content.contains("\r\n") { "\r\n" } else { "\n" };
        let trailing_newline = content.ends_with('\n');

        // Split on '\n'; strip a trailing '\r' from each segment (CRLF). The
        // final empty segment produced by a trailing newline is not a line.
        let mut segments: Vec<&str> = content.split('\n').collect();
        if trailing_newline {
            segments.pop(); // drop the artifact empty segment after the last '\n'
        }

        let lines = segments
            .into_iter()
            .map(|seg| {
                let seg = seg.strip_suffix('\r').unwrap_or(seg);
                let trimmed = seg.trim();
                if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.len() >= 2 {
                    Line::Section(trimmed[1..trimmed.len() - 1].to_string())
                } else if let Some((k, v)) = seg.split_once('=') {
                    Line::Entry { key: k.to_string(), value: v.to_string() }
                } else {
                    Line::Other(seg.to_string())
                }
            })
            .collect();

        VaraIni { line_ending, trailing_newline, lines }
    }

    /// Re-emit the INI (the full file bytes to write to disk), byte-exact with
    /// the parsed input when unmodified. NOT `Display` on purpose: this renders
    /// the UNREDACTED content (incl. the registration code), so it must be an
    /// explicit call, never reachable via `{}`. Use [`VaraIni::redacted`] /
    /// `{:?}` for logging.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (i, line) in self.lines.iter().enumerate() {
            if i > 0 {
                out.push_str(self.line_ending);
            }
            match line {
                Line::Section(name) => {
                    out.push('[');
                    out.push_str(name);
                    out.push(']');
                }
                Line::Entry { key, value } => {
                    out.push_str(key);
                    out.push('=');
                    out.push_str(value);
                }
                Line::Other(raw) => out.push_str(raw),
            }
        }
        if self.trailing_newline && !self.lines.is_empty() {
            out.push_str(self.line_ending);
        }
        out
    }

    /// Value of `key` under `[section]`, if present. Section and key match
    /// exactly (VARA writes them consistently).
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        let mut cur: Option<&str> = None;
        for line in &self.lines {
            match line {
                Line::Section(name) => cur = Some(name.as_str()),
                Line::Entry { key: k, value } if cur == Some(section) && k == key => {
                    return Some(value.as_str());
                }
                _ => {}
            }
        }
        None
    }

    /// Set `[section] key = value`. Updates the value in place if the key exists;
    /// otherwise inserts the key at the end of the section; if the section does
    /// not exist, appends the section and key. Everything else is untouched.
    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        // Pass 1: update in place if present.
        let mut cur: Option<String> = None;
        for line in &mut self.lines {
            match line {
                Line::Section(name) => cur = Some(name.clone()),
                Line::Entry { key: k, value: v }
                    if cur.as_deref() == Some(section) && k == key =>
                {
                    *v = value.to_string();
                    return;
                }
                _ => {}
            }
        }

        // Pass 2: find the section; insert after its last line.
        let mut section_start: Option<usize> = None;
        let mut insert_at: Option<usize> = None;
        let mut in_section = false;
        for (i, line) in self.lines.iter().enumerate() {
            if let Line::Section(name) = line {
                if in_section {
                    // Left the target section; insert before this next header.
                    insert_at = Some(i);
                    break;
                }
                if name == section {
                    in_section = true;
                    section_start = Some(i);
                }
            }
        }
        if let Some(start) = section_start {
            let at = insert_at.unwrap_or(self.lines.len());
            // Insert just after the last non-empty entry of the section is
            // overkill; appending at the section's end (before the next header /
            // EOF) preserves VARA's contiguous layout.
            self.lines.insert(at, Line::Entry { key: key.to_string(), value: value.to_string() });
            let _ = start;
            return;
        }

        // Section absent: append it and the entry.
        self.lines.push(Line::Section(section.to_string()));
        self.lines.push(Line::Entry { key: key.to_string(), value: value.to_string() });
        self.trailing_newline = true;
    }

    /// A [`to_string`](Self::to_string) rendering with sensitive values masked —
    /// use this for any logging / display. Masks `[Setup] Registration Code*`
    /// and `Password encryption`.
    pub fn redacted(&self) -> String {
        let mut clone = self.clone();
        for line in &mut clone.lines {
            if let Line::Entry { key, value } = line {
                if is_sensitive_key(key) && !value.is_empty() {
                    *value = "<redacted>".to_string();
                }
            }
        }
        clone.render()
    }
}

/// Keys whose values must never be logged: the paid VARA registration/license
/// key and the stored encryption password. (Callsigns are public and not masked.)
///
/// Public so the app-crate glue can apply the SAME predicate to inbound edit
/// requests (an agent may legitimately SET the registration code through the
/// stop-edit-start path; the value must be redacted in the glue's logs/Debug
/// exactly as it is in [`VaraIni::redacted`]).
pub fn is_sensitive_key(key: &str) -> bool {
    let k = key.trim();
    k.starts_with("Registration Code") || k == "Password encryption"
}

impl fmt::Debug for VaraIni {
    /// Redacted so `{:?}` can never leak the registration code into a log.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VaraIni({} lines) {{\n{}\n}}", self.lines.len(), self.redacted())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A structurally-real VARA.ini (from a live VARA under WINE) with the
    // callsign and registration code replaced by fakes. CRLF, as VARA writes.
    const SAMPLE: &str = "[Soundcard]\r\n\
Input Device Name=USB PnP Sound Device Mono\r\n\
Output Device Name=USB PnP Sound Device Analog Ste\r\n\
ALC Drive Level=-15\r\n\
Channel=0\r\n\
[PTT]\r\n\
Rig=52\r\n\
CATPort=COM1\r\n\
Baud=4800\r\n\
[Setup]\r\n\
Registration Code=FAKEFAKEFAKE1234\r\n\
Callsign Licence 0=N0CALL\r\n\
TCP Command Port=8300\r\n\
[Position]\r\n\
Top Position=3060\r\n\
Left Position=9870\r\n";

    #[test]
    fn round_trip_is_byte_exact_crlf() {
        assert_eq!(VaraIni::parse(SAMPLE).render(), SAMPLE);
    }

    #[test]
    fn round_trip_is_byte_exact_lf_and_no_trailing_newline() {
        let lf = "[Soundcard]\nOutput Device Name=Foo\n[Setup]\nTCP Command Port=8300";
        let ini = VaraIni::parse(lf);
        assert_eq!(ini.render(), lf, "LF + no-trailing-newline must round-trip");
    }

    #[test]
    fn get_reads_section_scoped_keys() {
        let ini = VaraIni::parse(SAMPLE);
        assert_eq!(ini.get("Soundcard", "Output Device Name"), Some("USB PnP Sound Device Analog Ste"));
        assert_eq!(ini.get("Soundcard", "ALC Drive Level"), Some("-15"));
        assert_eq!(ini.get("Setup", "TCP Command Port"), Some("8300"));
        // Same key name would collide without section scoping — there is none
        // here, but prove a wrong section does not match.
        assert_eq!(ini.get("PTT", "Output Device Name"), None);
        assert_eq!(ini.get("Soundcard", "Missing"), None);
    }

    #[test]
    fn set_existing_updates_only_that_line() {
        let mut ini = VaraIni::parse(SAMPLE);
        ini.set("Soundcard", "Output Device Name", "USB Audio CODEC Analog Stereo");
        assert_eq!(ini.get("Soundcard", "Output Device Name"), Some("USB Audio CODEC Analog Stereo"));
        // Everything else preserved: only the one line differs.
        let expected = SAMPLE.replace(
            "Output Device Name=USB PnP Sound Device Analog Ste",
            "Output Device Name=USB Audio CODEC Analog Stereo",
        );
        assert_eq!(ini.render(), expected);
    }

    #[test]
    fn set_missing_key_inserts_within_its_section() {
        let mut ini = VaraIni::parse(SAMPLE);
        ini.set("Soundcard", "RA-Board Device Path", "");
        assert_eq!(ini.get("Soundcard", "RA-Board Device Path"), Some(""));
        // Inserted inside [Soundcard], before [PTT] — not at EOF.
        let s = ini.render();
        let sc = s.find("[Soundcard]").unwrap();
        let ptt = s.find("[PTT]").unwrap();
        let key = s.find("RA-Board Device Path=").unwrap();
        assert!(sc < key && key < ptt, "new key must land inside [Soundcard]");
    }

    #[test]
    fn set_missing_section_appends() {
        let mut ini = VaraIni::parse(SAMPLE);
        ini.set("Monitor", "Monitor Mode", "0");
        assert_eq!(ini.get("Monitor", "Monitor Mode"), Some("0"));
        assert!(ini.render().contains("[Monitor]\r\nMonitor Mode=0"));
    }

    #[test]
    fn redaction_masks_registration_code_but_not_callsign() {
        let ini = VaraIni::parse(SAMPLE);
        let red = ini.redacted();
        assert!(!red.contains("FAKEFAKEFAKE1234"), "registration code must be masked");
        assert!(red.contains("Registration Code=<redacted>"));
        assert!(red.contains("Callsign Licence 0=N0CALL"), "public callsign not masked");
        // Debug must also be redaction-safe.
        assert!(!format!("{ini:?}").contains("FAKEFAKEFAKE1234"));
    }
}
