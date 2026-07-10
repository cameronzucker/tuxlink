//! Best-effort field extraction from standard FT8 message text.
//!
//! Design contract (delta §Revised L1): hashed callsigns (`<...>` or any
//! `<...>`-bracketed token) yield None for that field — unresolvable with
//! per-slot jt9 spawn (accepted regression, surfaced downstream). Grid is
//! extracted ONLY when the trailing token is a 4-char Maidenhead locator;
//! reports (+NN/-NN/R-NN), RRR, RR73, 73 are NOT grids (delta §L4 grid
//! provenance: no grid → no map placement).

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MessageFields {
    pub from_call: Option<String>,
    pub to_call: Option<String>,
    pub grid: Option<String>,
}

fn is_grid(tok: &str) -> bool {
    if tok == "RR73" {
        return false; // FT8 sign-off token — deliberately grid-shaped, never a locator here
    }
    let b = tok.as_bytes();
    b.len() == 4
        && b[0].is_ascii_uppercase() && b[0] <= b'R'
        && b[1].is_ascii_uppercase() && b[1] <= b'R'
        && b[2].is_ascii_digit() && b[3].is_ascii_digit()
}

fn is_callsign(tok: &str) -> bool {
    (3..=11).contains(&tok.len())
        && tok.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '/')
        && tok.chars().any(|c| c.is_ascii_digit())
        && tok.chars().any(|c| c.is_ascii_uppercase())
}

/// `<...>` → None (unresolvable); `<CALL>` → Some(CALL); bare call → Some.
fn call_field(tok: &str) -> Option<String> {
    if tok == "<...>" {
        return None;
    }
    let inner = tok.strip_prefix('<').and_then(|t| t.strip_suffix('>')).unwrap_or(tok);
    is_callsign(inner).then(|| inner.to_string())
}

fn is_cq_modifier(tok: &str) -> bool {
    (1..=4).contains(&tok.len())
        && (tok.chars().all(|c| c.is_ascii_uppercase())
            || (tok.len() == 3 && tok.chars().all(|c| c.is_ascii_digit())))
        && !is_callsign(tok)
}

pub fn extract_fields(message: &str) -> MessageFields {
    let toks: Vec<&str> = message.split_whitespace().collect();
    let mut out = MessageFields::default();
    match toks.as_slice() {
        ["CQ", rest @ ..] if !rest.is_empty() => {
            let (call_idx, grid) = match rest {
                [.., last] if is_grid(last) => (rest.len().checked_sub(2), Some(last.to_string())),
                _ => (Some(rest.len() - 1), None),
            };
            // call position: last token if no grid, second-to-last if grid.
            let idx = match call_idx {
                Some(i) => i,
                None => return out, // "CQ <grid>" alone — malformed
            };
            let candidate = rest[idx];
            // Allow AT MOST one leading modifier: "CQ DX K1ABC FN42" — the
            // candidate must be the callsign; the (single, optional)
            // preceding token must be a modifier. Two or more leading
            // modifier-shaped tokens are out-of-grammar → all-None.
            if idx <= 1
                && (call_field(candidate).is_some() || candidate == "<...>")
                && rest[..idx].iter().all(|t| is_cq_modifier(t))
            {
                out.from_call = call_field(candidate);
                out.grid = grid;
                if out.from_call.is_none() {
                    out.grid = None; // hashed CQ: no usable station → no grid
                }
            }
            out
        }
        [to, from, rest @ ..] if (call_field(to).is_some() || *to == "<...>")
            && (call_field(from).is_some() || *from == "<...>") => {
            out.to_call = call_field(to);
            out.from_call = call_field(from);
            if let [suffix] = rest {
                if is_grid(suffix) {
                    out.grid = Some((*suffix).to_string());
                }
            }
            out
        }
        _ => out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(m: &str) -> MessageFields { extract_fields(m) }

    #[test]
    fn cq_with_grid() {
        assert_eq!(f("CQ JE6HOG PM53"), MessageFields {
            from_call: Some("JE6HOG".into()), to_call: None, grid: Some("PM53".into()) });
    }

    #[test]
    fn cq_compound_call_no_grid() {
        // Real capture: compound/portable call, no grid (compound calls
        // cannot carry a grid in the standard message).
        assert_eq!(f("CQ W5C/H"), MessageFields {
            from_call: Some("W5C/H".into()), to_call: None, grid: None });
    }

    #[test]
    fn cq_with_modifier_dx() {
        assert_eq!(f("CQ DX K1ABC FN42"), MessageFields {
            from_call: Some("K1ABC".into()), to_call: None, grid: Some("FN42".into()) });
    }

    #[test]
    fn report_and_r_report_and_73s_are_not_grids() {
        assert_eq!(f("YB3BBF K5OJT -19"), MessageFields {
            from_call: Some("K5OJT".into()), to_call: Some("YB3BBF".into()), grid: None });
        assert_eq!(f("N6VIN JA8NRS R-06"), MessageFields {
            from_call: Some("JA8NRS".into()), to_call: Some("N6VIN".into()), grid: None });
        assert_eq!(f("VK4DAD K5KND 73"), MessageFields {
            from_call: Some("K5KND".into()), to_call: Some("VK4DAD".into()), grid: None });
        assert_eq!(f("K0BQB WD8ASA RR73"), MessageFields {
            from_call: Some("WD8ASA".into()), to_call: Some("K0BQB".into()), grid: None });
        assert_eq!(f("K0BQB WD8ASA RRR"), MessageFields {
            from_call: Some("WD8ASA".into()), to_call: Some("K0BQB".into()), grid: None });
    }

    #[test]
    fn standard_reply_with_grid() {
        assert_eq!(f("K1ABC W9XYZ EN37"), MessageFields {
            from_call: Some("W9XYZ".into()), to_call: Some("K1ABC".into()), grid: Some("EN37".into()) });
    }

    #[test]
    fn hashed_callsigns_yield_none() {
        assert_eq!(f("<...> N4AHI EM73"), MessageFields {
            from_call: Some("N4AHI".into()), to_call: None, grid: Some("EM73".into()) });
        assert_eq!(f("<KA1ABC> W9XYZ RR73"), MessageFields {
            from_call: Some("W9XYZ".into()), to_call: Some("KA1ABC".into()), grid: None });
        // From-position hash: unresolved sender is None (design contract).
        assert_eq!(f("CQ <...>"), MessageFields::default());
    }

    #[test]
    fn multiple_cq_modifiers_are_out_of_grammar() {
        assert_eq!(f("CQ TEST DX K1ABC FN42"), MessageFields::default());
        assert_eq!(f("CQ"), MessageFields::default());
    }

    #[test]
    fn free_text_and_junk_yield_default() {
        assert_eq!(f("TNX 599 GL"), MessageFields::default());
        assert_eq!(f(""), MessageFields::default());
    }
}
