# Security Policy

## Reporting a vulnerability

Security issues affecting Tuxlink should be reported **privately**, not via public GitHub issues.

Two private channels are accepted:

1. **GitHub private security advisory** (preferred) — <https://github.com/cameronzucker/tuxlink/security/advisories/new>. This creates a draft advisory visible only to the reporter and the maintainer until disclosure.
2. **Email** — <cameronzucker@gmail.com> with the subject prefix `[tuxlink security]`.

Please include:

- A clear description of the issue and its impact.
- Reproduction steps (binary version, OS, callsign-redacted config snippet if relevant).
- Any proof-of-concept code, redacted of sensitive content.

A response is provided within **7 calendar days** acknowledging receipt and giving an initial assessment. Resolution timelines depend on severity but follow industry norms (90 days from initial report for non-critical, fewer for critical).

## Supported versions

Tuxlink follows [SemVer](VERSIONING.md). Security patches target the **latest released minor version**. Earlier minor versions receive patches only if a critical issue cannot be mitigated by upgrading (see VERSIONING.md §Hotfix recipe).

| Version | Supported |
|---|---|
| Latest released `0.x` | ✅ |
| All earlier `0.x` | upgrade-required, no backports unless critical and upgrade is blocking |

Pre-1.0 releases are explicitly experimental. Use in production amateur-radio operations is at the operator's risk; the licensee is responsible for transmissions per FCC Part 97.

## Scope

In-scope:

- The Tauri application binary and its bundled dependencies.
- The HTTP client communicating with the bundled Pat process.
- Configuration handling (`$XDG_CONFIG_HOME/tuxlink/config.json`).
- AppImage distribution: signing, checksum publication, supply-chain integrity.
- Live-CMS testing policy enforcement (see [docs/live-cms-testing-policy.md](docs/live-cms-testing-policy.md)). Bypassing the consent gate is a security-relevant defect.

Out of scope (report upstream):

- Vulnerabilities in [Pat](https://github.com/la5nta/pat) itself — report to la5nta/pat.
- Vulnerabilities in [Tauri](https://github.com/tauri-apps/tauri) — report to tauri-apps.
- Vulnerabilities in upstream Rust / Node dependencies — report to the respective project, then notify Tuxlink so we can pin a patched version.

## Disclosure

Coordinated disclosure is preferred. Once a fix is available and released, the advisory is published publicly with a CVE if applicable, the affected versions, and the fixed version. Reporters are credited in the advisory unless they request anonymity.
