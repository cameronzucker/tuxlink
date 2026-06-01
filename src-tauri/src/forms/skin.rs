//! tuxlink CSS skin for webview-rendered HTML Forms.
//!
//! Injected by `forms::http_server` (P1 Task 6) at `/skin.css` and linked
//! into every served form via a prepended `<link rel="stylesheet"
//! href="/skin.css">`. Uses `:where()` selectors so the skin has zero CSS
//! specificity — inline styles in the WLE template still win where they're
//! explicit; tuxlink's overrides apply only where the template has no
//! competing rule.
//!
//! Design reference: §5.5 (skin scope), §10 step 5 (CSP constraint).
//! Plan: docs/superpowers/plans/2026-06-01-html-forms-p1-webview-infra.md
//!       Task 4.

const SKIN_CSS: &str = r#"
/* tuxlink form skin — :where() for zero specificity. */
:where(body) {
  background: #0c0e12;
  color: #d6d8dc;
  font-family: -apple-system, BlinkMacSystemFont, "Inter", "Segoe UI", sans-serif;
  font-size: 14px;
  line-height: 1.5;
  margin: 0;
  padding: 1.5em;
}
:where(input, textarea, select) {
  background: #16181d;
  color: #e6e8ec;
  border: 1px solid #2a2e36;
  border-radius: 4px;
  padding: 0.45em 0.6em;
  font: inherit;
}
:where(input:focus, textarea:focus, select:focus) {
  outline: none;
  border-color: #d97706;
  box-shadow: 0 0 0 2px rgba(217, 119, 6, 0.18);
}
:where(button) {
  background: #d97706;
  color: #0c0e12;
  border: 1px solid #d97706;
  border-radius: 4px;
  padding: 0.5em 1em;
  font-weight: 600;
  cursor: pointer;
}
:where(button[type="submit"]) {
  background: #d97706;
}
:where(button[type="reset"], button[type="button"]) {
  background: transparent;
  color: #d6d8dc;
  border-color: #2a2e36;
}
:where(table) {
  border-collapse: collapse;
  margin: 1em 0;
  width: 100%;
}
:where(table th, table td) {
  border: 1px solid #2a2e36;
  padding: 0.4em 0.6em;
  text-align: left;
}
:where(table th) {
  background: #16181d;
}
:where(a, a:visited) {
  color: #f5a524;
  text-decoration: none;
}
:where(a:hover) {
  text-decoration: underline;
}
"#;

/// Return the static skin CSS. Stable across the binary's lifetime.
pub fn generate() -> &'static str {
    SKIN_CSS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skin_uses_where_selectors_for_zero_specificity() {
        let css = generate();
        assert!(
            css.contains(":where(body)"),
            "skin must use :where(body) so inline template styles still win"
        );
        assert!(
            css.contains(":where(input"),
            "skin must use :where() for input"
        );
        assert!(
            css.contains(":where(button)"),
            "skin must use :where() for button"
        );
    }

    #[test]
    fn skin_overrides_body_background_and_text() {
        let css = generate();
        // The two hex colors used in the dark theme — operator-visible
        // assertion that the skin actually picks them.
        assert!(css.contains("#0c0e12"), "body background hex missing");
        assert!(css.contains("#d6d8dc"), "body text color hex missing");
    }

    #[test]
    fn skin_styles_inputs_textareas_and_selects() {
        let css = generate();
        // The bundle of WLE form widgets we know we'll encounter.
        assert!(css.contains(":where(input, textarea, select)"));
    }

    #[test]
    fn skin_styles_submit_buttons() {
        let css = generate();
        // WLE forms use `<input type="submit">` AND `<button type="submit">`
        // depending on the template. Cover the button case at minimum.
        assert!(
            css.contains("button[type=\"submit\"]"),
            "skin must style native submit buttons"
        );
    }

    #[test]
    fn skin_styles_tables() {
        let css = generate();
        // ICS-309 viewers + several state forms render data tables; the
        // skin gives them a tuxlink-consistent look.
        assert!(css.contains(":where(table)"));
        assert!(css.contains(":where(table th, table td)"));
    }
}
