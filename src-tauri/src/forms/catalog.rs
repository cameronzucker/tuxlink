//! Bundled forms catalog. Per spec §8, v0.1 ships 5 forms; this file
//! enumerates them and provides id-based lookup.

use crate::forms::templates;
use crate::forms::types::FormDef;

pub const BUNDLED_FORMS: &[&FormDef] = &[
    &templates::ics213::ICS213_INITIAL,
    // ics309, position, bulletin, damage_assessment added in T9.x
];

/// Look up a bundled form by its canonical ID. Returns None if not known.
pub fn find_form(id: &str) -> Option<&'static FormDef> {
    BUNDLED_FORMS.iter().find(|f| f.id == id).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_ics213_by_id() {
        let f = find_form("ICS213_Initial").expect("ICS213_Initial bundled");
        assert_eq!(f.name, "ICS-213 General Message");
        assert!(f.fields.iter().any(|fd| fd.id == "inc_name"));
        assert!(f.fields.iter().any(|fd| fd.id == "subjectline"));
    }

    #[test]
    fn returns_none_for_unknown_form() {
        assert!(find_form("Unknown_Form").is_none());
    }

    #[test]
    fn display_form_filename_set() {
        let f = find_form("ICS213_Initial").unwrap();
        assert_eq!(f.display_form, "ICS213_Initial_Viewer.html");
        assert_eq!(f.reply_template, "ICS213_SendReply.0");
    }
}
