//! Run-scoped variable store: routine inputs + step outputs.

use std::collections::HashMap;

use crate::error::StepError;
use crate::refs::VarPath;
use crate::types::StepId;

#[derive(Debug, Default, Clone)]
pub struct RunVars {
    inputs: HashMap<String, serde_json::Value>,
    step_outputs: HashMap<StepId, serde_json::Value>,
}

impl RunVars {
    pub fn set_input(&mut self, name: &str, value: serde_json::Value) {
        self.inputs.insert(name.to_string(), value);
    }

    pub fn set_step_output(&mut self, id: &StepId, value: serde_json::Value) {
        self.step_outputs.insert(id.clone(), value);
    }

    /// Resolve `s1.connected` (step output) or `band_plan` (routine input).
    /// An unset path is a hard error (spec §10): it never falls back to the
    /// path's own text or an empty value.
    pub fn resolve(&self, path: &str) -> Result<serde_json::Value, StepError> {
        if let Some(vp) = VarPath::parse(path) {
            if let Some(out) = self.step_outputs.get(&vp.step) {
                if let Some(v) = out.get(&vp.output) {
                    return Ok(v.clone());
                }
            }
        } else if let Some(v) = self.inputs.get(path) {
            return Ok(v.clone());
        }
        Err(StepError::UnsetVariable(path.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::StepError;
    use crate::types::StepId;
    use serde_json::json;

    #[test]
    fn resolves_step_outputs_by_path() {
        let mut vars = RunVars::default();
        vars.set_step_output(
            &StepId("s1".into()),
            json!({"connected": true, "gateway": "W7DEF-10"}),
        );
        assert_eq!(vars.resolve("s1.connected").unwrap(), json!(true));
        assert_eq!(vars.resolve("s1.gateway").unwrap(), json!("W7DEF-10"));
    }

    #[test]
    fn resolves_routine_inputs_by_name() {
        let mut vars = RunVars::default();
        vars.set_input("band_plan", json!("winter"));
        assert_eq!(vars.resolve("band_plan").unwrap(), json!("winter"));
    }

    #[test]
    fn unset_variable_is_a_verbatim_error_never_its_own_name() {
        // Spec §10: the Laserfiche disease — an empty token writing its own
        // literal name into output — must be structurally impossible.
        let vars = RunVars::default();
        let err = vars.resolve("s9.connected").unwrap_err();
        match err {
            StepError::UnsetVariable(path) => assert_eq!(path, "s9.connected"),
            other => panic!("expected UnsetVariable, got {other:?}"),
        }
    }

    #[test]
    fn set_step_but_missing_output_key_is_also_unset() {
        let mut vars = RunVars::default();
        vars.set_step_output(&StepId("s1".into()), json!({"connected": false}));
        assert!(matches!(
            vars.resolve("s1.no_such_key"),
            Err(StepError::UnsetVariable(p)) if p == "s1.no_such_key"
        ));
    }
}
