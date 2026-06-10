// Version-consistency guard (tuxlink-1k3x).
//
// release-please bumps `version.txt` (canonical, release-type=simple) but the
// other in-repo version sources must be kept in lockstep via the config's
// `extra-files` updaters. If any drifts, the .deb is mislabeled and the app
// announces the wrong version to Winlink CMS (CARGO_PKG_VERSION feeds the B2F
// handshake). This test fails loudly the moment a source falls out of sync —
// it is the regression guard for the 0.0.1-forever bug.
//
// `Cargo.lock` is intentionally NOT checked here: `cargo --locked` in CI already
// enforces Cargo.lock-vs-Cargo.toml consistency.

import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

function read(rel: string): string {
  return readFileSync(resolve(REPO_ROOT, rel), 'utf8');
}

/// The canonical version release-please owns directly.
const canonical = read('version.txt').trim();

/// `version` field of a JSON file.
function jsonVersion(rel: string): string {
  return JSON.parse(read(rel)).version;
}

/// `[package] version` of a Cargo.toml (first `version = "..."` inside the
/// `[package]` table, before any later section).
function cargoPackageVersion(rel: string): string {
  const m = read(rel).match(/^\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m);
  if (!m) throw new Error(`no [package] version found in ${rel}`);
  return m[1];
}

describe('version consistency (tuxlink-1k3x)', () => {
  it('canonical version.txt is a clean semver', () => {
    expect(canonical).toMatch(/^\d+\.\d+\.\d+$/);
  });

  it('package.json matches version.txt', () => {
    expect(jsonVersion('package.json')).toBe(canonical);
  });

  it('src-tauri/tauri.conf.json (the .deb/.rpm/.AppImage version) matches version.txt', () => {
    expect(jsonVersion('src-tauri/tauri.conf.json')).toBe(canonical);
  });

  it('src-tauri/Cargo.toml (CARGO_PKG_VERSION → Winlink handshake) matches version.txt', () => {
    expect(cargoPackageVersion('src-tauri/Cargo.toml')).toBe(canonical);
  });
});
