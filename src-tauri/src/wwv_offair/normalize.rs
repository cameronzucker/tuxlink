/// Lowercases, then rewrites spoken English number words into digit strings so
/// the tolerant `parse_wwv` substring matcher works on STT output. Deliberately
/// small: only the closed WWV vocabulary needs to survive.
pub fn normalize_spoken_numbers(transcript: &str) -> String {
    let lower = transcript.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    let mut out: Vec<String> = Vec::new();
    let mut acc: Option<u64> = None; // accumulating integer
    let mut frac: Option<String> = None; // digits after "point"
    let mut in_point = false;

    let flush = |acc: &mut Option<u64>, frac: &mut Option<String>, out: &mut Vec<String>| {
        if let Some(n) = acc.take() {
            let mut s = n.to_string();
            if let Some(f) = frac.take() {
                s.push('.');
                s.push_str(&f);
            }
            out.push(s);
        } else if let Some(f) = frac.take() {
            out.push(format!("0.{f}"));
        }
    };

    for w in words {
        let unit = word_to_unit(w); // Some(0..=9) etc.
        match w {
            "point" => {
                in_point = true;
                if acc.is_none() {
                    acc = Some(0);
                }
            }
            "hundred" => {
                if let Some(a) = acc {
                    acc = Some(if a == 0 { 100 } else { a * 100 });
                }
            }
            "thousand" => {
                if let Some(a) = acc {
                    acc = Some(a * 1000);
                }
            }
            // Any word that decodes to a number (unit/teen/ten). Branching on
            // `if let Some(d) = unit` here (rather than a `_ if unit.is_some()`
            // guard + `unit.unwrap()`) avoids clippy's `unnecessary_unwrap`
            // under `-D warnings`.
            _ => {
                if let Some(d) = unit {
                    if in_point {
                        frac.get_or_insert_with(String::new).push_str(&d.to_string());
                    } else if let Some(ten) = word_to_ten(w) {
                        // A tens word (twenty..ninety) only combines with a
                        // preceding value that is a clean hundreds multiple
                        // ("one hundred fifty" -> 150). A bare units value before
                        // a tens word is malformed shorthand ("one fifty"): flush
                        // it and restart, so parse_wwv sees a broken number and
                        // reports no-copy rather than fabricating a wrong SFI
                        // (e.g. "one fifty" -> 51, which would pass the [50,500]
                        // bound as a bogus quiet-sun reading).
                        match acc {
                            Some(a) if a >= 100 && a % 100 == 0 => acc = Some(a + ten),
                            Some(_) => {
                                flush(&mut acc, &mut frac, &mut out);
                                acc = Some(ten);
                            }
                            None => acc = Some(ten),
                        }
                    } else {
                        acc = Some(acc.unwrap_or(0) + d);
                    }
                } else {
                    flush(&mut acc, &mut frac, &mut out);
                    in_point = false;
                    out.push(w.to_string());
                }
            }
        }
    }
    flush(&mut acc, &mut frac, &mut out);
    out.join(" ")
}

fn word_to_unit(w: &str) -> Option<u64> {
    Some(match w {
        "zero" | "oh" => 0,
        "one" => 1,
        "two" => 2,
        "three" => 3,
        "four" => 4,
        "five" => 5,
        "six" => 6,
        "seven" => 7,
        "eight" => 8,
        "nine" => 9,
        "ten" => 10,
        "eleven" => 11,
        "twelve" => 12,
        "thirteen" => 13,
        "fourteen" => 14,
        "fifteen" => 15,
        "sixteen" => 16,
        "seventeen" => 17,
        "eighteen" => 18,
        "nineteen" => 19,
        "twenty" | "thirty" | "forty" | "fifty" | "sixty" | "seventy" | "eighty" | "ninety" => {
            return word_to_ten(w)
        }
        _ => return None,
    })
}
fn word_to_ten(w: &str) -> Option<u64> {
    Some(match w {
        "twenty" => 20,
        "thirty" => 30,
        "forty" => 40,
        "fifty" => 50,
        "sixty" => 60,
        "seventy" => 70,
        "eighty" => 80,
        "ninety" => 90,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_numbers_to_digits() {
        let t = normalize_spoken_numbers(
            "Solar flux one hundred seventeen and estimated planetary A index six",
        );
        assert!(t.contains("solar flux 117"));
        assert!(t.contains("a index 6") || t.contains("a-index 6"));
    }
    #[test]
    fn decimal_k_index() {
        let t = normalize_spoken_numbers(
            "the estimated planetary k index at twelve hundred UTC was one point three three",
        );
        assert!(t.contains("1.33"));
    }
    #[test]
    fn passthrough_existing_digits() {
        assert!(normalize_spoken_numbers("Solar flux 142 reported").contains("142"));
    }

    #[test]
    fn full_hundreds_form_combines() {
        // WWV's actual phrasing: "one hundred fifty" -> 150.
        let t = normalize_spoken_numbers("solar flux one hundred fifty");
        assert!(t.contains("solar flux 150"), "got: {t}");
        // and with a trailing unit: "one hundred twenty three" -> 123.
        assert!(normalize_spoken_numbers("one hundred twenty three").contains("123"));
    }

    #[test]
    fn shorthand_tens_does_not_fabricate_sfi() {
        // "one fifty" (no "hundred") is malformed shorthand. It must NOT become
        // 51 (which would pass the [50,500] SFI bound as a bogus reading); it
        // degrades to a broken number so parse_wwv reports no-copy.
        let t = normalize_spoken_numbers("solar flux one fifty");
        assert!(!t.contains("51"), "must not fabricate 51 from 'one fifty': {t}");
        assert!(t.contains('1') && t.contains("50"), "degrades to 1 50: {t}");
    }
}
