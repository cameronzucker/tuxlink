//! Finding vocabulary (spec §10): the machine-readable output of
//! [`super::validate`] / [`super::validate_fleet`]. MCP and the UI both
//! consume `Finding` directly, so it stays serde-Serialize with a stable
//! `&'static str` code per finding class — never a formatted string that
//! could drift between releases.

use serde::Serialize;

use crate::types::StepId;

/// Whether a finding blocks enable/run (spec §10: "errors block enable/run,
/// never save") or is informational.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

/// One static-validation result. `code` is the machine-readable class
/// (SCREAMING_SNAKE, e.g. `"UNRESOLVED_REF"`); `message` is the
/// human-readable explanation and MUST name the offending entity verbatim
/// (spec §10) — no paraphrasing a station-set or step id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub code: &'static str,
    pub severity: Severity,
    pub routine: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<StepId>,
    pub message: String,
}

impl Finding {
    pub fn error(code: &'static str, routine: impl Into<String>, message: impl Into<String>) -> Self {
        Finding {
            code,
            severity: Severity::Error,
            routine: routine.into(),
            track: None,
            step: None,
            message: message.into(),
        }
    }

    pub fn warning(code: &'static str, routine: impl Into<String>, message: impl Into<String>) -> Self {
        Finding {
            code,
            severity: Severity::Warning,
            routine: routine.into(),
            track: None,
            step: None,
            message: message.into(),
        }
    }

    pub fn with_track(mut self, track: impl Into<String>) -> Self {
        self.track = Some(track.into());
        self
    }

    pub fn with_step(mut self, step: StepId) -> Self {
        self.step = Some(step);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_wire_shape_survives_rename_all() {
        // serde rename_all on enums renames TAGS only (project pitfall,
        // see types.rs's identical guard): assert the exact wire strings.
        assert_eq!(serde_json::to_value(Severity::Error).unwrap(), serde_json::json!("error"));
        assert_eq!(serde_json::to_value(Severity::Warning).unwrap(), serde_json::json!("warning"));
    }

    #[test]
    fn builders_set_severity_and_leave_track_step_unset() {
        let f = Finding::error("SOME_CODE", "r1", "boom");
        assert_eq!(f.code, "SOME_CODE");
        assert_eq!(f.severity, Severity::Error);
        assert_eq!(f.routine, "r1");
        assert_eq!(f.track, None);
        assert_eq!(f.step, None);
        assert_eq!(f.message, "boom");

        let w = Finding::warning("OTHER_CODE", "r2", "meh")
            .with_track("t1")
            .with_step(StepId("s1".into()));
        assert_eq!(w.severity, Severity::Warning);
        assert_eq!(w.track, Some("t1".to_string()));
        assert_eq!(w.step, Some(StepId("s1".into())));
    }

    #[test]
    fn omits_null_track_and_step_when_serialized() {
        let f = Finding::error("SOME_CODE", "r1", "boom");
        let v = serde_json::to_value(&f).unwrap();
        assert!(!v.as_object().unwrap().contains_key("track"));
        assert!(!v.as_object().unwrap().contains_key("step"));
    }
}
