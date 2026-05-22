//! tuxlink build script: bundles the Pat sidecar from external/tuxlink-pat
//! submodule into Tauri's externalBin path. Gated to release-profile only;
//! debug + cargo test paths skip the Go build entirely. See
//! docs/superpowers/specs/2026-05-18-fork-setup-design.md §3.2 + §3.4 for
//! the design rationale; docs/development.md for build-deps notes.

use std::path::{Path, PathBuf};
use std::process::Command;

// Share parse_go_version with src/lib.rs's test discovery via #[path].
// One source file, two consumers: build.rs (here) reads via #[path];
// lib.rs reads under `#[cfg(test)] mod build_support;` (see Step 3).
#[path = "src/build_support.rs"]
mod build_support;
use build_support::parse_go_version;

fn main() {
    // Gate the Pat sidecar build to release profile only.
    // cargo test and cargo build (debug) skip the Go invocation entirely
    // — they don't need Pat for tuxlink's own test suite (per spec §3.5).
    //
    // IMPORTANT: this must run BEFORE tauri_build::build(), because
    // tauri_build validates that the externalBin sidecar file exists at the
    // configured path (sidecars/pat-<TARGET-TRIPLE>) — if missing, the
    // build fails with "resource path ... doesn't exist" regardless of
    // profile. In release we produce the real binary; in debug + test we
    // touch a 0-byte stub at the expected path so tauri_build validation
    // passes. The stub satisfies the path-exists check; the debug app will
    // not actually invoke Pat (that path is only exercised under release
    // bundling).
    let profile = std::env::var("PROFILE").unwrap_or_default();
    if profile != "release" {
        if let Err(e) = ensure_sidecar_stub() {
            panic!("build.rs: failed to create debug-profile sidecar stub: {e}");
        }
        println!("cargo:warning=build.rs: skipping Pat sidecar build (PROFILE={profile}; release-only path); stub created for tauri_build validation");
        // Standard Tauri build hook runs after stub is in place.
        tauri_build::build();
        return;
    }

    let submodule = submodule_path();
    println!("cargo:rerun-if-changed={}", submodule.display());

    if let Err(e) = check_submodule_complete(&submodule) {
        panic!("build.rs: submodule check failed: {e}");
    }

    if let Err(e) = check_go_toolchain() {
        panic!("build.rs: Go toolchain check failed: {e}");
    }

    if let Err(e) = build_pat_sidecar(&submodule) {
        panic!("build.rs: Pat sidecar build failed: {e}");
    }

    // tauri_build::build() runs LAST in release so the real sidecar
    // produced by build_pat_sidecar() is in place when validation runs.
    tauri_build::build();
}

/// Touch a 0-byte sidecar stub at sidecars/pat-<TARGET-TRIPLE> so that
/// tauri_build::build()'s externalBin path-validation passes under debug
/// + cargo test profiles, which intentionally skip the Go-build path.
///
/// The stub is overwritten by the real binary in release builds.
fn ensure_sidecar_stub() -> Result<(), String> {
    let target = std::env::var("TARGET").map_err(|e| {
        format!("Cargo did not set TARGET env var (this build script must run under cargo): {e}")
    })?;
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let sidecar_dir = manifest_dir.join("sidecars");
    std::fs::create_dir_all(&sidecar_dir).map_err(|e| {
        format!("Failed to create sidecars dir {}: {e}", sidecar_dir.display())
    })?;
    let sidecar = sidecar_dir.join(format!("pat-{target}"));
    if !sidecar.exists() {
        std::fs::File::create(&sidecar).map_err(|e| {
            format!("Failed to create stub sidecar {}: {e}", sidecar.display())
        })?;
    }
    Ok(())
}

/// Resolve the submodule path from the cargo manifest dir (src-tauri/)
/// to the repo-root external/tuxlink-pat/.
fn submodule_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("external")
        .join("tuxlink-pat")
}

/// 2-condition submodule completeness check per spec §3.4:
/// (1) .git presence (file or directory), (2) make.bash canary file.
/// The 3rd condition from spec §3.4 (parent-index SHA-match) requires the
/// git2 crate and is deferred to follow-up if SHA-mismatch states surface
/// during execution. The two conditions here catch the common partial-
/// state failures (deinit'd, --recurse-submodules=false, interrupted clone).
fn check_submodule_complete(submodule: &Path) -> Result<(), String> {
    let dot_git = submodule.join(".git");
    if !dot_git.exists() {
        return Err(format!(
            "external/tuxlink-pat submodule is not initialized.\n\
             Detected: {} does not exist.\n\
             Recover:\n  \
               git submodule deinit -f external/tuxlink-pat\n  \
               git submodule update --init --recursive",
            dot_git.display()
        ));
    }
    let make_bash = submodule.join("make.bash");
    if !make_bash.exists() {
        return Err(format!(
            "external/tuxlink-pat submodule is not in a buildable state.\n\
             Detected: {} does not exist (expected upstream Pat's make.bash).\n\
             Recover:\n  \
               git submodule deinit -f external/tuxlink-pat\n  \
               git submodule update --init --recursive",
            make_bash.display()
        ));
    }
    Ok(())
}

/// Check Go is installed AND at version 1.24+ (per Pat's go.mod).
fn check_go_toolchain() -> Result<(), String> {
    let output = Command::new("go").arg("version").output().map_err(|e| {
        format!(
            "Go toolchain required to build Pat from the tuxlink-pat submodule.\n\
             Install: apt install golang-go libax25-dev (Debian/Ubuntu) or equivalent.\n\
             Pat requires Go 1.24 or later (per external/tuxlink-pat/go.mod).\n\
             End-users: use the prebuilt AppImage instead of building from source.\n\
             See docs/development.md.\n\
             Underlying error: {e}"
        )
    })?;
    if !output.status.success() {
        return Err(format!(
            "go version command failed: stdout={:?} stderr={:?}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        ));
    }
    let version_str = String::from_utf8_lossy(&output.stdout);
    let (major, minor) = parse_go_version(&version_str)
        .ok_or_else(|| format!("Could not parse Go version from: {version_str}"))?;
    if major < 1 || (major == 1 && minor < 24) {
        return Err(format!(
            "Go 1.24 or later required (per external/tuxlink-pat/go.mod and Pat's make.bash).\n\
             Detected: go{major}.{minor}\n\
             Upgrade: see https://go.dev/doc/install"
        ));
    }
    Ok(())
}

// parse_go_version lives in src/build_support.rs (shared with cargo test
// discovery via lib.rs's `#[cfg(test)] mod build_support;` — see Step 3
// above). Imported via the #[path] mod build_support; declaration at
// top of file.

/// Invoke `SKIP_TESTS=1 bash make.bash` in the submodule + rename the
/// produced `pat` binary to `pat-<TARGET-TRIPLE>` at the stable sidecar
/// path src-tauri/sidecars/.
fn build_pat_sidecar(submodule: &Path) -> Result<(), String> {
    // TARGET is set by cargo as a RUNTIME env var to build scripts, NOT a
    // compile-time env — so std::env::var, not env!() (R1 P2 catch).
    let target = std::env::var("TARGET").map_err(|e| {
        format!("Cargo did not set TARGET env var (this build script must run under cargo): {e}")
    })?;
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let sidecar_dir = manifest_dir.join("sidecars");
    std::fs::create_dir_all(&sidecar_dir).map_err(|e| {
        format!("Failed to create sidecars dir {}: {e}", sidecar_dir.display())
    })?;

    let status = Command::new("bash")
        .arg("make.bash")
        .env("SKIP_TESTS", "1")
        .current_dir(submodule)
        .status()
        .map_err(|e| format!("Failed to invoke bash make.bash in {}: {e}", submodule.display()))?;
    if !status.success() {
        return Err(format!(
            "bash make.bash failed in {} with exit code {:?}. \
             See stderr above for Pat's build errors.",
            submodule.display(),
            status.code()
        ));
    }

    // make.bash produces ./pat in the submodule root.
    let built = submodule.join("pat");
    if !built.exists() {
        return Err(format!(
            "Expected {} to exist after bash make.bash, but it does not.",
            built.display()
        ));
    }

    // Rename to pat-<triple> at the stable sidecar path.
    let sidecar = sidecar_dir.join(format!("pat-{target}"));
    std::fs::rename(&built, &sidecar).map_err(|e| {
        format!(
            "Failed to rename {} to {}: {e}",
            built.display(),
            sidecar.display()
        )
    })?;

    println!("cargo:warning=build.rs: Pat sidecar ready at {}", sidecar.display());
    Ok(())
}
