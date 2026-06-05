//! Builds manifest.json for an export (spec §3.5).

use serde::Serialize;

#[derive(Serialize)]
pub struct Manifest {
    pub v: u32,
    pub exported_at: String,
    pub correlation_id: Option<String>,
    pub window: Window,
    pub build: Build,
    pub platform: Platform,
    pub runtime: Runtime,
    pub logging: LoggingMeta,
    pub compression: Compression,
    pub counts: Counts,
}

#[derive(Serialize)]
pub struct Window { pub start: String, pub end: String }

#[derive(Serialize)]
pub struct Build {
    pub version: String,
    pub git_sha: String,
    pub profile: String,
    pub rust_version: String,
    pub tauri_version: String,
}

#[derive(Serialize)]
pub struct Platform {
    pub os: String,
    pub kernel: String,
    pub distro: String,
    pub arch: String,
}

#[derive(Serialize)]
pub struct Runtime {
    pub boot_id: String,
    pub boot_at: String,
    pub log_dir: String,
}

#[derive(Serialize)]
pub struct LoggingMeta {
    pub schema_version: u32,
    pub redaction_policy_version: u32,
    pub detailed_mode: String,
    pub retention_days: u32,
    pub retention_mb_cap: u32,
}

#[derive(Serialize)]
pub struct Compression {
    pub outer_algorithm: String,
    pub outer_level: i32,
    pub inner_algorithm: String,
    pub inner_level: i32,
    pub inner_dict_version: Option<u32>,
    pub raw_events_bytes: u64,
    pub inner_compressed_bytes: u64,
    pub outer_archive_bytes: u64,
    pub inner_ratio: f64,
    pub dict_amortized_ratio: f64,
}

#[derive(Serialize, Default)]
pub struct Counts {
    pub events: u64,
    pub info: u64,
    pub warn: u64,
    pub error: u64,
}

/// Compile-time metadata baked at build time via env macros.
pub fn build_info() -> Build {
    Build {
        version: env!("CARGO_PKG_VERSION").to_string(),
        git_sha: option_env!("TUXLINK_GIT_SHA").unwrap_or("unknown").to_string(),
        profile: if cfg!(debug_assertions) { "debug".into() } else { "release".into() },
        rust_version: option_env!("TUXLINK_RUST_VERSION").unwrap_or("unknown").to_string(),
        tauri_version: "2".to_string(),
    }
}

pub fn platform_info() -> Platform {
    Platform {
        os: std::env::consts::OS.to_string(),
        kernel: kernel_release(),
        distro: distro_name(),
        arch: std::env::consts::ARCH.to_string(),
    }
}

fn kernel_release() -> String {
    std::process::Command::new("uname")
        .arg("-r")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

fn distro_name() -> String {
    std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("ID="))
                .map(|l| l.trim_start_matches("ID=").trim_matches('"').to_string())
        })
        .unwrap_or_else(|| "unknown".into())
}

/// Serialize the manifest to a JSON byte vector (pretty-printed).
pub fn render(manifest: &Manifest) -> Vec<u8> {
    serde_json::to_vec_pretty(manifest).unwrap_or_else(|_| b"{}".to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_info_returns_non_empty_strings() {
        let b = build_info();
        assert!(!b.version.is_empty());
    }

    #[test]
    fn platform_info_populates_os() {
        let p = platform_info();
        assert!(!p.os.is_empty());
    }
}
