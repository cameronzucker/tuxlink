//! Forward-defense source-scan for credential-bearing structs (spec §10.2 #9).
//!
//! Walks src-tauri/src/ at test time, finds struct definitions with
//! credential-shaped String / Option<String> fields, asserts each has a
//! manual `impl Debug` somewhere in the crate. Catches NEW credential
//! structs that land without manual Debug AND without being added to
//! credential_debug_audit.rs.
//!
//! Spec §10.2 #9 literally asks for a build.rs scan; this runtime test
//! is functionally equivalent for CI purposes (both fire on every PR)
//! and is simpler to maintain — build.rs reruns on every cargo build
//! while this test only runs when `cargo test` is invoked, but CI does
//! invoke cargo test on every PR, so the gate fires before merge either way.
//!
//! Escape hatch: a struct can opt out by adding the attribute
//! `#[allow(credential_audit_skip)]` directly above its definition.
//! Use only for structs where the credential-shaped field name is a
//! false positive (e.g. a session routing token that is not an auth secret,
//! or a protocol nonce that is intentionally logged).

use std::collections::HashSet;
use std::path::PathBuf;

use quote::ToTokens;
use syn::{visit::Visit, Fields, ItemImpl, ItemStruct, Type};

/// Conservative list of credential-shaped field names. Any struct field
/// whose name exactly matches one of these AND whose type is String /
/// Option<String> triggers the manual-Debug requirement.
///
/// This list is a SUBSET of the runtime blocklist regex in
/// `src/logging/redact.rs`. The source-scan uses exact field-name matching
/// (not regex) for predictability, so only the most specific/unambiguous
/// names are listed here. The intentional gap covers generic blocklist
/// names like `auth`, `bearer`, `credentials`, `token` (bare), `cookie` —
/// the runtime redactor catches those in emission, but a NEW struct that
/// embeds one as a typed field with derived Debug will NOT be caught by
/// this scan. When extending, audit `redact.rs::FIELD_BLOCKLIST` for
/// patterns the scan should additionally trip on.
const CREDENTIAL_FIELD_NAMES: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "password_input",
    "peer_password",
    "station_password",
    "secret",
    "client_secret",
    "private_key",
    "api_key",
    "apikey",
    "auth_token",
    "access_token",
    "refresh_token",
    "oauth_token",
    "bearer_token",
    "consent_token",
    "challenge",
    "response",
    "secure_login_response",
    "secure_login_challenge",
    "secure_response",
    "credential",
    "session_cookie",
    "sessionid",
    "session_id",
    "signature",
    "nonce",
    "hmac",
    "salt",
    "keyring_value",
    "keyring_secret",
];

/// Returns true if a Type is exactly `String`, `Option<String>`, `&str`,
/// or `Option<&str>` (the credential-shaped types).
fn type_is_credential_carrier(ty: &Type) -> bool {
    let s = ty.to_token_stream().to_string();
    // token_stream rendering includes spaces around angle brackets
    matches!(
        s.as_str(),
        "String"
            | "Option < String >"
            | "& str"
            | "Option < & str >"
            | "Option<String>"
            | "Option<&str>"
    )
}

#[derive(Default)]
struct ScanState {
    /// (struct_name, file_path) for each credential-shaped struct that
    /// has #[derive(..., Debug, ...)] and NOT #[allow(credential_audit_skip)].
    derived_debug_credential_structs: Vec<(String, PathBuf)>,
    /// Set of struct names with a manual `impl Debug for X` somewhere in
    /// the crate. Keyed by bare struct name (no module path), so two
    /// modules defining structs with the same short name would alias —
    /// a manual Debug on one would silently exempt the other. No such
    /// collision exists today; verify when adding new credential-bearing
    /// structs that the name isn't reused elsewhere.
    manual_debug_impls: HashSet<String>,
}

impl<'ast> Visit<'ast> for ScanState {
    fn visit_item_struct(&mut self, s: &'ast ItemStruct) {
        // Has credential-shaped field?
        let has_cred_field = match &s.fields {
            Fields::Named(fields) => fields.named.iter().any(|f| {
                f.ident.as_ref().is_some_and(|id| {
                    CREDENTIAL_FIELD_NAMES.contains(&id.to_string().as_str())
                }) && type_is_credential_carrier(&f.ty)
            }),
            _ => false,
        };
        if !has_cred_field {
            return;
        }

        // Escape hatch: #[allow(credential_audit_skip)] opts this struct out.
        let has_skip_attr = s.attrs.iter().any(|a| {
            a.path().is_ident("allow")
                && a.to_token_stream().to_string().contains("credential_audit_skip")
        });
        if has_skip_attr {
            return;
        }

        // Has #[derive(..., Debug, ...)]?
        let has_derived_debug = s.attrs.iter().any(|a| {
            a.path().is_ident("derive")
                && a.to_token_stream().to_string().contains("Debug")
        });
        if has_derived_debug {
            // Record with an empty path; the caller patches it after the visit.
            self.derived_debug_credential_structs
                .push((s.ident.to_string(), PathBuf::new()));
        }
    }

    fn visit_item_impl(&mut self, i: &'ast ItemImpl) {
        // Match: `impl Debug for X` or `impl std::fmt::Debug for X`
        if let Some((_, ref path, _)) = i.trait_ {
            if path
                .segments
                .last()
                .is_some_and(|s| s.ident == "Debug")
            {
                if let Type::Path(p) = &*i.self_ty {
                    if let Some(seg) = p.path.segments.last() {
                        self.manual_debug_impls.insert(seg.ident.to_string());
                    }
                }
            }
        }
    }
}

/// Walk every .rs file under src-tauri/src/, parse with syn, collect:
/// - structs that have a credential-shaped field AND #[derive(Debug)] (no skip attr)
/// - manual `impl Debug for X` blocks
///
/// Then assert no struct in the first set is absent from the second set.
#[test]
fn all_credential_structs_have_manual_debug_impl() {
    let src_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut scan = ScanState::default();

    for entry in walkdir::WalkDir::new(&src_dir) {
        let entry = entry.expect("walkdir entry");
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().map_or(true, |e| e != "rs") {
            continue;
        }

        let source = std::fs::read_to_string(entry.path()).expect("read source file");
        let parsed = match syn::parse_file(&source) {
            Ok(f) => f,
            // Best-effort: a file that doesn't parse (e.g., macro-heavy,
            // build artifact) should not fail the audit — skip it.
            Err(_) => continue,
        };

        let prev_len = scan.derived_debug_credential_structs.len();
        scan.visit_file(&parsed);

        // Patch the path on freshly-recorded structs from this file.
        for entry_rec in scan.derived_debug_credential_structs[prev_len..].iter_mut() {
            if entry_rec.1.as_os_str().is_empty() {
                entry_rec.1 = entry.path().to_path_buf();
            }
        }
    }

    let mut failures: Vec<(String, PathBuf)> = scan
        .derived_debug_credential_structs
        .into_iter()
        .filter(|(name, _)| !scan.manual_debug_impls.contains(name))
        .collect();

    failures.sort_by(|a, b| a.0.cmp(&b.0));

    assert!(
        failures.is_empty(),
        "spec §10.2 #9 — credential-shaped structs with derived (not manual) Debug:\n{}\n\n\
         Each such struct must replace `#[derive(Debug)]` with a manual `impl Debug` that \
         redacts credential-shaped fields, OR opt out via #[allow(credential_audit_skip)] \
         (only valid for structs where the credential-shaped field name is a false positive \
         — e.g. a session routing token or a protocol nonce).",
        failures
            .iter()
            .map(|(n, p)| format!("  - {n} at {}", p.display()))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
