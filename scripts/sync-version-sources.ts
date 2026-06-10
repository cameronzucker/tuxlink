#!/usr/bin/env tsx
// Keep Tuxlink's static version-bearing files in lockstep with version.txt.
//
// release-please owns version.txt as the canonical release version and, after
// tuxlink-1k3x, is configured to bump these files automatically. This script is
// the local repair/check tool for branch catch-up and release maintenance.

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const CHECK_ONLY = process.argv.includes('--check');

interface Change {
  path: string;
  before: string;
  after: string;
}

const changes: Change[] = [];

function pathFor(rel: string): string {
  return resolve(REPO_ROOT, rel);
}

function read(rel: string): string {
  return readFileSync(pathFor(rel), 'utf8');
}

function write(rel: string, content: string) {
  writeFileSync(pathFor(rel), content);
}

function record(rel: string, before: string, after: string) {
  if (before === after) return;
  changes.push({ path: rel, before, after });
  if (!CHECK_ONLY) write(rel, after);
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function updateJsonScalar(rel: string, key: string, version: string) {
  const before = read(rel);
  const pattern = new RegExp(`("${escapeRegExp(key)}"\\s*:\\s*)"[^"]+"`);
  if (!pattern.test(before)) {
    throw new Error(`No JSON key ${JSON.stringify(key)} found in ${rel}`);
  }
  const after = before.replace(pattern, `$1"${version}"`);
  JSON.parse(after);
  record(rel, before, after);
}

function updateCargoToml(rel: string, version: string) {
  const before = read(rel);
  const pattern = /^(\[package\][\s\S]*?^version\s*=\s*)"[^"]+"/m;
  if (!pattern.test(before)) {
    throw new Error(`No [package] version found in ${rel}`);
  }
  const after = before.replace(pattern, `$1"${version}"`);
  record(rel, before, after);
}

function updateCargoLock(rel: string, version: string) {
  const before = read(rel);
  const pattern = /(\[\[package\]\]\nname = "tuxlink"\nversion = )"[^"]+"/;
  if (!pattern.test(before)) {
    throw new Error(`No tuxlink package entry found in ${rel}`);
  }
  const after = before.replace(pattern, `$1"${version}"`);
  record(rel, before, after);
}

const version = read('version.txt').trim();
if (!/^\d+\.\d+\.\d+$/.test(version)) {
  throw new Error(`version.txt must contain clean semver, got ${JSON.stringify(version)}`);
}

updateJsonScalar('.github/.release-please-manifest.json', '.', version);
updateJsonScalar('package.json', 'version', version);
updateJsonScalar('src-tauri/tauri.conf.json', 'version', version);
updateCargoToml('src-tauri/Cargo.toml', version);
updateCargoLock('src-tauri/Cargo.lock', version);

if (changes.length === 0) {
  console.log(`Version sources already match ${version}.`);
} else if (CHECK_ONLY) {
  console.error(`Version sources drift from version.txt (${version}):`);
  for (const change of changes) {
    console.error(`  ${change.path}`);
  }
  process.exit(1);
} else {
  console.log(`Synced ${changes.length} version source(s) to ${version}:`);
  for (const change of changes) {
    console.log(`  ${change.path}`);
  }
}
