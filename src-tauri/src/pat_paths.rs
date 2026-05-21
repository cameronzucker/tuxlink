//! pat_paths — XDG-aware resolution of the filesystem locations the Pat
//! sidecar uses (its `config.json`, mailbox dir, pid file).
//!
//! Extracted (tuxlink-pqg) so the app-start path and the wizard test-send
//! resolve *identical* Pat locations. The wizard test-send must spawn its own
//! ephemeral Pat from these dirs rather than assume a Pat already running on a
//! hardcoded port (the prior `http://127.0.0.1:8080` bug). The pure
//! `*_from(...)` helpers take the env values as inputs so they are
//! deterministic + parallel-safe to unit-test without mutating process env.

use std::path::PathBuf;

/// `<XDG_CONFIG_HOME | HOME/.config>/pat/config.json` — where `PatProcess`
/// writes the rendered Pat config before exec.
pub fn pat_config_path() -> Result<PathBuf, String> {
    pat_config_path_from(
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
        std::env::var_os("HOME").map(PathBuf::from),
    )
}

/// `<XDG_DATA_HOME | HOME/.local/share>/tuxlink` — base for the Pat mailbox.
pub fn data_dir() -> Result<PathBuf, String> {
    data_dir_from(
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
        std::env::var_os("HOME").map(PathBuf::from),
    )
}

/// `<XDG_STATE_HOME | HOME/.local/state>/tuxlink` — base for the Pat pid file.
pub fn state_dir() -> Result<PathBuf, String> {
    state_dir_from(
        std::env::var_os("XDG_STATE_HOME").map(PathBuf::from),
        std::env::var_os("HOME").map(PathBuf::from),
    )
}

fn pat_config_path_from(
    xdg_config_home: Option<PathBuf>,
    home: Option<PathBuf>,
) -> Result<PathBuf, String> {
    let base = xdg_config_home
        .or_else(|| home.map(|h| h.join(".config")))
        .ok_or_else(|| "neither XDG_CONFIG_HOME nor HOME is set".to_string())?;
    Ok(base.join("pat").join("config.json"))
}

fn data_dir_from(xdg_data_home: Option<PathBuf>, home: Option<PathBuf>) -> Result<PathBuf, String> {
    let base = xdg_data_home
        .or_else(|| home.map(|h| h.join(".local").join("share")))
        .ok_or_else(|| "neither XDG_DATA_HOME nor HOME is set".to_string())?;
    Ok(base.join("tuxlink"))
}

fn state_dir_from(xdg_state_home: Option<PathBuf>, home: Option<PathBuf>) -> Result<PathBuf, String> {
    let base = xdg_state_home
        .or_else(|| home.map(|h| h.join(".local").join("state")))
        .ok_or_else(|| "neither XDG_STATE_HOME nor HOME is set".to_string())?;
    Ok(base.join("tuxlink"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pat_config_path_honors_xdg_config_home() {
        let got = pat_config_path_from(Some(PathBuf::from("/xdg/cfg")), Some(PathBuf::from("/home/op")));
        assert_eq!(got, Ok(PathBuf::from("/xdg/cfg/pat/config.json")));
    }

    #[test]
    fn pat_config_path_falls_back_to_home_dotconfig() {
        let got = pat_config_path_from(None, Some(PathBuf::from("/home/op")));
        assert_eq!(got, Ok(PathBuf::from("/home/op/.config/pat/config.json")));
    }

    #[test]
    fn pat_config_path_errors_when_neither_set() {
        assert!(pat_config_path_from(None, None).is_err());
    }

    #[test]
    fn data_dir_honors_xdg_data_home() {
        let got = data_dir_from(Some(PathBuf::from("/xdg/data")), Some(PathBuf::from("/home/op")));
        assert_eq!(got, Ok(PathBuf::from("/xdg/data/tuxlink")));
    }

    #[test]
    fn data_dir_falls_back_to_home_local_share() {
        let got = data_dir_from(None, Some(PathBuf::from("/home/op")));
        assert_eq!(got, Ok(PathBuf::from("/home/op/.local/share/tuxlink")));
    }

    #[test]
    fn data_dir_errors_when_neither_set() {
        assert!(data_dir_from(None, None).is_err());
    }

    #[test]
    fn state_dir_honors_xdg_state_home() {
        let got = state_dir_from(Some(PathBuf::from("/xdg/state")), Some(PathBuf::from("/home/op")));
        assert_eq!(got, Ok(PathBuf::from("/xdg/state/tuxlink")));
    }

    #[test]
    fn state_dir_falls_back_to_home_local_state() {
        let got = state_dir_from(None, Some(PathBuf::from("/home/op")));
        assert_eq!(got, Ok(PathBuf::from("/home/op/.local/state/tuxlink")));
    }

    #[test]
    fn state_dir_errors_when_neither_set() {
        assert!(state_dir_from(None, None).is_err());
    }
}
