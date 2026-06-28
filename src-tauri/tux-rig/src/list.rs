//! `rigctl -l` model enumeration — the installed hamlib's supported rigs.
//!
//! The model list is queried at runtime from the installed hamlib rather than
//! maintained in tuxlink, so it is always accurate to the operator's hamlib.
//! Columns in `rigctl -l` are separated by runs of 2+ spaces; single spaces
//! appear only WITHIN a model name ("NET rigctl"), so a 2+-space column split
//! is robust. The header line's first column is "Rig #", which fails the u32
//! parse and is therefore skipped without a special case.

use std::process::{Command, Stdio};

use crate::RigError;

/// One supported rig as reported by `rigctl -l`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RigModel {
    pub id: u32,
    pub manufacturer: String,
    pub model: String,
}

/// Split a `rigctl -l` row into columns on runs of 2+ spaces. Single spaces
/// are preserved inside a column (multi-word model names). Leading indentation
/// is ignored.
fn split_columns(line: &str) -> Vec<String> {
    let mut cols: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut space_run = 0usize;
    for ch in line.chars() {
        if ch == ' ' {
            space_run += 1;
        } else {
            if space_run >= 2 && !cur.is_empty() {
                cols.push(cur.trim().to_string());
                cur = String::new();
            } else if space_run == 1 && !cur.is_empty() {
                cur.push(' ');
            }
            space_run = 0;
            cur.push(ch);
        }
    }
    if !cur.is_empty() {
        cols.push(cur.trim().to_string());
    }
    cols
}

/// Parse one row into a [`RigModel`], or `None` if it is not a data row (header,
/// blank, or malformed). The header's first column "Rig #" fails the u32 parse.
fn parse_line(line: &str) -> Option<RigModel> {
    let cols = split_columns(line);
    let id: u32 = cols.first()?.parse().ok()?;
    let manufacturer = cols.get(1)?.clone();
    let model = cols.get(2)?.clone();
    if manufacturer.is_empty() || model.is_empty() {
        return None;
    }
    Some(RigModel { id, manufacturer, model })
}

/// Parse the full stdout of `rigctl -l` into the supported-model list.
fn parse_rig_list(stdout: &str) -> Vec<RigModel> {
    stdout.lines().filter_map(parse_line).collect()
}

/// Query the installed hamlib for its supported rig models by running
/// `<rigctl_binary> -l`. Returns the parsed list. Errors (binary missing,
/// non-UTF-8 output) map to [`RigError::Spawn`]; the Tauri command layer
/// converts any error to an empty list so the picker degrades to manual entry.
pub fn list_models(rigctl_binary: &str) -> Result<Vec<RigModel>, RigError> {
    let output = Command::new(rigctl_binary)
        .arg("-l")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| RigError::Spawn(format!("failed to run {rigctl_binary} -l: {e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_rig_list(&stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
 Rig #  Mfg                    Model                       Version         Status   Macro
     1  Hamlib                 Dummy                       20231112.0      Stable   RIG_MODEL_DUMMY
     2  Hamlib                 NET rigctl                  20231112.0      Stable   RIG_MODEL_NETRIGCTL
  1049  Yaesu                  FT-710                      20240514.0      Stable   RIG_MODEL_FT710
  3073  Icom                   IC-7300                     20231112.0      Stable   RIG_MODEL_IC7300
";

    #[test]
    fn parses_id_mfg_and_multiword_model() {
        let got = parse_rig_list(SAMPLE);
        assert_eq!(
            got,
            vec![
                RigModel { id: 1, manufacturer: "Hamlib".into(), model: "Dummy".into() },
                RigModel { id: 2, manufacturer: "Hamlib".into(), model: "NET rigctl".into() },
                RigModel { id: 1049, manufacturer: "Yaesu".into(), model: "FT-710".into() },
                RigModel { id: 3073, manufacturer: "Icom".into(), model: "IC-7300".into() },
            ],
        );
    }

    #[test]
    fn skips_header_and_blank_lines() {
        // Header line ("Rig #  Mfg ...") + a blank line produce no entries.
        assert!(parse_rig_list(" Rig #  Mfg  Model\n\n").is_empty());
    }

    #[test]
    fn empty_input_is_empty() {
        assert!(parse_rig_list("").is_empty());
    }
}
