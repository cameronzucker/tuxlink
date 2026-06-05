//! Wizard phase state machine. Replaces the prior boolean `wizard_completed` so
//! the Location step can be a first-class persisted phase (Codex CODEX-1 fix).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WizardPhase {
    /// User has not completed the wizard.
    #[default]
    None,
    /// Callsign + Winlink account identity is persisted; location is next.
    Identity,
    /// Location is persisted; wizard is complete.
    Complete,
}

impl WizardPhase {
    /// Compatibility shim: existing `get_wizard_completed` command returns
    /// `phase == Complete`.
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_none() {
        assert_eq!(WizardPhase::default(), WizardPhase::None);
    }

    #[test]
    fn is_complete_only_for_complete_variant() {
        assert!(!WizardPhase::None.is_complete());
        assert!(!WizardPhase::Identity.is_complete());
        assert!(WizardPhase::Complete.is_complete());
    }

    #[test]
    fn serializes_snake_case() {
        assert_eq!(serde_json::to_string(&WizardPhase::Identity).unwrap(), "\"identity\"");
        assert_eq!(serde_json::to_string(&WizardPhase::Complete).unwrap(), "\"complete\"");
        assert_eq!(serde_json::to_string(&WizardPhase::None).unwrap(), "\"none\"");
    }

    #[test]
    fn deserializes_snake_case() {
        let p: WizardPhase = serde_json::from_str("\"identity\"").unwrap();
        assert_eq!(p, WizardPhase::Identity);
    }
}
