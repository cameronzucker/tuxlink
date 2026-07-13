//! `@`-reference tokens and variable paths (spec §14 conventions).

use std::fmt;

use crate::types::StepId;

/// A named-entity reference: `@station-set:or-gateways`, `@preset:winlink-40m`.
/// These are what reference validation (plan 3) resolves.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntityRef {
    pub kind: String,
    pub name: String,
}

impl EntityRef {
    pub fn parse(s: &str) -> Option<Self> {
        let rest = s.strip_prefix('@')?;
        let (kind, name) = rest.split_once(':')?;
        if kind.is_empty() || name.is_empty() {
            return None;
        }
        Some(EntityRef {
            kind: kind.to_string(),
            name: name.to_string(),
        })
    }
}

impl fmt::Display for EntityRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}:{}", self.kind, self.name)
    }
}

/// A step-output path: `s1.connected`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarPath {
    pub step: StepId,
    pub output: String,
}

impl VarPath {
    pub fn parse(s: &str) -> Option<Self> {
        let (step, output) = s.split_once('.')?;
        if step.is_empty() || output.is_empty() {
            return None;
        }
        Some(VarPath {
            step: StepId(step.to_string()),
            output: output.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_entity_refs() {
        let r = EntityRef::parse("@station-set:or-gateways").unwrap();
        assert_eq!(r.kind, "station-set");
        assert_eq!(r.name, "or-gateways");
        assert_eq!(r.to_string(), "@station-set:or-gateways");
    }

    #[test]
    fn non_refs_are_none() {
        assert!(EntityRef::parse("plain string").is_none());
        assert!(EntityRef::parse("@missing-colon").is_none());
        assert!(EntityRef::parse("@:empty-kind").is_none());
        assert!(EntityRef::parse("@kind:").is_none());
    }

    #[test]
    fn parses_var_paths() {
        let v = VarPath::parse("s1.connected").unwrap();
        assert_eq!(v.step.0, "s1");
        assert_eq!(v.output, "connected");
        assert!(VarPath::parse("nodot").is_none());
        assert!(VarPath::parse(".leading").is_none());
        assert!(VarPath::parse("trailing.").is_none());
    }
}
