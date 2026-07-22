//! Deterministic affordance-catalog builder for Elmer's workflow (Routine CI
//! slice 1a, Task 4) — the Feasibility phase's "what can the catalog actually
//! do" step.
//!
//! [`build_affordance_catalog`] takes the SAME
//! `crate::routines::commands::ActionInfo` list `list_actions` reports (no
//! second, hand-maintained action list) and a caller-chosen set of allowed
//! "families," and projects the kept actions down to
//! [`super::artifacts::AffordanceAction`]. It is deliberately dumb: no model
//! call, no heuristics beyond a name-prefix filter — the phase that decides
//! WHICH families are relevant to an intent is a later task's job.
//!
//! ## Family scheme
//!
//! A family is the prefix of `ActionInfo.name` before its first `.`
//! (`"radio.connect"` -> `"radio"`, `"config.set_ardop"` -> `"config"`).
//! Verified against the real action registry
//! (`src-tauri/src/routines/actions/*.rs` construct names like
//! `"radio.connect"`, `"local.log"`, `"data.web_lookup"`, `"config.set_ardop"`,
//! `"data.read"`) rather than assumed: every observed action name is
//! `<family>.<verb>`, so the brief's dotted-prefix scheme matches the
//! registry as built — no adjustment needed.
//!
//! ## Fail-loud guard
//!
//! An empty filtered result returns [`CatalogError::Empty`] rather than an
//! empty [`Affordances`]. This is deliberate: a silently-empty catalog would
//! let a later phase report "the catalog has nothing for this intent" when
//! the real cause is a caller bug (an unmatched family name, a typo'd
//! filter) — the same false "everything missing" failure mode Task 1's
//! design note flags for `Affordances::missing_primitives`.

use crate::routines::commands::ActionInfo;

use super::artifacts::{AffordanceAction, Affordances};

/// Family-filtered lookups against the action catalog can come back empty
/// either because the registry genuinely has nothing for the requested
/// families, or because the caller passed a family that matches nothing
/// (typo, renamed family). Either way, an empty result is refused rather
/// than silently returned — see the module-level "Fail-loud guard" note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogError {
    Empty,
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CatalogError::Empty => {
                write!(f, "affordance catalog is empty for the requested families")
            }
        }
    }
}

impl std::error::Error for CatalogError {}

/// Filters `actions` to those whose family (the `ActionInfo.name` prefix
/// before the first `.`) appears in `families`, then projects each kept
/// action to an [`AffordanceAction`]. Returns [`CatalogError::Empty`] when
/// the filtered set is empty — never an [`Affordances`] with an empty
/// `actions` vec.
pub fn build_affordance_catalog(
    actions: &[ActionInfo],
    families: &[String],
) -> Result<Affordances, CatalogError> {
    let kept: Vec<AffordanceAction> = actions
        .iter()
        .filter(|action| {
            let family = action.name.split('.').next().unwrap_or(action.name.as_str());
            families.iter().any(|f| f == family)
        })
        .map(|action| AffordanceAction {
            name: action.name.clone(),
            transmits: action.transmits,
            needs_radio: action.needs_radio,
            writes_config: action.writes_config,
            params: action.params.iter().map(|p| p.key.clone()).collect(),
            outputs: action.outputs.iter().map(|o| o.key.clone()).collect(),
        })
        .collect();

    if kept.is_empty() {
        return Err(CatalogError::Empty);
    }

    Ok(Affordances {
        actions: kept,
        missing_primitives: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routines::commands::{OutputSpecView, ParamSpecView};

    /// Minimal `ActionInfo` builder for these tests: only `name` and
    /// `transmits` vary across cases, everything else is a fixed, plausible
    /// filler value.
    fn action_info(name: &str, transmits: bool) -> ActionInfo {
        ActionInfo {
            name: name.to_string(),
            label: String::new(),
            description: String::new(),
            needs_radio: transmits,
            transmits,
            needs_internet: false,
            writes_config: false,
            example_params: None,
            params: vec![ParamSpecView {
                key: "band".to_string(),
                value_type: "string".to_string(),
                required: false,
                description: String::new(),
                allowed: None,
                example: serde_json::Value::Null,
            }],
            outputs: vec![OutputSpecView {
                key: "grid".to_string(),
                value_type: "string".to_string(),
                description: String::new(),
                nullable: false,
            }],
        }
    }

    #[test]
    fn catalog_filters_by_family_and_fails_loud_on_empty() {
        let actions = vec![action_info("radio.connect", true), action_info("config.set", false)];
        let cat = build_affordance_catalog(&actions, &["radio".into()]).unwrap();
        assert_eq!(cat.actions.len(), 1);
        assert_eq!(cat.actions[0].name, "radio.connect");
        assert!(cat.actions[0].transmits);
        assert_eq!(cat.actions[0].params, vec!["band".to_string()]);
        assert_eq!(cat.actions[0].outputs, vec!["grid".to_string()]);
        assert!(matches!(
            build_affordance_catalog(&actions, &["nonexistent".into()]),
            Err(CatalogError::Empty)
        ));
    }

    #[test]
    fn empty_action_list_is_also_empty_err() {
        let actions: Vec<ActionInfo> = vec![];
        assert!(matches!(
            build_affordance_catalog(&actions, &["radio".into()]),
            Err(CatalogError::Empty)
        ));
    }

    #[test]
    fn multiple_families_keep_all_matching_actions() {
        let actions = vec![
            action_info("radio.connect", true),
            action_info("data.read", false),
            action_info("config.set_ardop", false),
        ];
        let cat =
            build_affordance_catalog(&actions, &["radio".into(), "data".into()]).unwrap();
        let names: Vec<&str> = cat.actions.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["radio.connect", "data.read"]);
    }
}
