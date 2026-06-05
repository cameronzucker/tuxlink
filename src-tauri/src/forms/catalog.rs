//! Bundled forms catalog. Per spec §8, v0.1 ships 5 forms; this file
//! enumerates them and provides id-based lookup.

use crate::forms::templates;
use crate::forms::types::FormDef;

pub const BUNDLED_FORMS: &[&FormDef] = &[
    &templates::ics213::ICS213_INITIAL,
    &templates::ics309::FORM309_INITIAL,
    &templates::position::POSITION_REPORT,
    &templates::bulletin::BULLETIN_INITIAL,
    &templates::damage_assessment::DAMAGE_ASSESSMENT_INITIAL,
    &templates::checkin::WINLINK_CHECK_IN,
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

    #[test]
    fn finds_form_309_by_id() {
        let f = find_form("Form-309_Initial").expect("Form-309_Initial bundled");
        assert_eq!(f.name, "ICS-309 Communications Log");
        assert!(f.fields.iter().any(|fd| fd.id == "opname"));
        assert!(f.fields.iter().any(|fd| fd.id == "time1"));
        assert_eq!(f.display_form, "Form-309_Viewer.html");
    }

    #[test]
    fn finds_bulletin_by_id() {
        let f = find_form("Bulletin_Initial").expect("Bulletin_Initial bundled");
        assert_eq!(f.name, "Bulletin");
        assert!(f.fields.iter().any(|fd| fd.id == "bullnr"));
        assert!(f.fields.iter().any(|fd| fd.id == "message"));
        assert_eq!(f.display_form, "Bulletin Viewer.html");
    }

    #[test]
    fn finds_position_report_by_id() {
        let f = find_form("Position_Report").expect("Position_Report bundled");
        assert_eq!(f.name, "GPS Position Report");
        assert!(f.fields.iter().any(|fd| fd.id == "lat"));
        assert!(f.fields.iter().any(|fd| fd.id == "lon"));
        assert_eq!(f.display_form, "GPS Position Report.html");
    }

    #[test]
    fn finds_damage_assessment_by_id() {
        let f = find_form("Damage_Assessment_Initial").expect("Damage_Assessment_Initial bundled");
        assert_eq!(f.name, "Damage Assessment");
        assert!(f.fields.iter().any(|fd| fd.id == "surarea"));
        assert!(f.fields.iter().any(|fd| fd.id == "dollar16"));
        assert_eq!(f.display_form, "Damage_Assessment_Viewer.html");
    }

    #[test]
    fn finds_winlink_check_in_by_id() {
        let f = find_form("Winlink_Check-In").expect("Winlink_Check-In bundled");
        assert_eq!(f.name, "Winlink Check-In");
        assert!(f.fields.iter().any(|fd| fd.id == "tactical_call"));
        assert!(f.fields.iter().any(|fd| fd.id == "status"));
        assert!(f.fields.iter().any(|fd| fd.id == "group_net"));
        assert!(f.fields.iter().any(|fd| fd.id == "grid"));
        assert_eq!(f.display_form, "Winlink_Check-In_Viewer.html");
    }
}
