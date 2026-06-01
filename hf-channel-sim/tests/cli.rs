// SPDX-License-Identifier: AGPL-3.0-only

//! Integration test: drive the hf-channel-sim-cli binary as a subprocess
//! and verify the JSON output.

use std::io::Write;
use std::process::{Command, Stdio};

fn cli_path() -> String {
    env!("CARGO_BIN_EXE_hf-channel-sim-cli").to_string()
}

fn synthetic_iq_bytes(n: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(n * 8);
    for i in 0..n {
        let t = i as f32 * 0.01;
        let re = t.cos();
        let im = t.sin();
        bytes.extend_from_slice(&re.to_le_bytes());
        bytes.extend_from_slice(&im.to_le_bytes());
    }
    bytes
}

#[test]
fn cli_emits_json_with_citations() {
    let mut child = Command::new(cli_path())
        .args([
            "--condition", "moderate",
            "--sample-rate", "8000",
            "--channel-seed", "1",
            "--noise-seed", "2",
            "--target-snr-db", "10",
            "--fft-size", "1024",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cli");
    let stdin = child.stdin.as_mut().expect("stdin");
    stdin.write_all(&synthetic_iq_bytes(8192)).expect("write");
    drop(child.stdin.take());

    let out = child.wait_with_output().expect("wait");
    assert!(out.status.success(), "cli exited with {:?}; stderr={}", out.status, String::from_utf8_lossy(&out.stderr));

    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(stdout.contains("Watterson"));
    assert!(stdout.contains("F.520"));
    assert!(stdout.contains("mean_snr_db"));
}

#[test]
fn cli_rejects_non_power_of_two_fft_size() {
    let out = Command::new(cli_path())
        .args(["--condition", "good", "--fft-size", "1000"])
        .stdin(Stdio::null())
        .output()
        .expect("run");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("power of two"), "got: {stderr}");
}
