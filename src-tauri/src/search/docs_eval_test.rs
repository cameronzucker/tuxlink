//! Retrieval evals: can a real operator question reach the document that answers it?
//!
//! These run against the REAL bundled corpus (not fixtures) and assert only
//! retrievability — the thing that was broken. Answer quality with a model in the
//! loop stays in dev/elmer-distill/.
//!
//! **The queries here are the operator's ACTUAL words, punctuation and all.** That is
//! deliberate and it is the point of the eval. An earlier draft used tidied keyword
//! strings ("pat ax25 digipeater connect"), which passed while the real question —
//! "What is the syntax for Pat Winlink in EmComm Tools in ax.25 to connect via a
//! digipeater?" — hard-errored inside FTS5 (syntax error near "." from "ax.25").
//! Testing a question no human would type proves nothing. If a query here is ever
//! "simplified" to make a test pass, the eval has been defeated.
//!
//! Assertions are "the expected slug is among the hits", never "is rank 1". BM25
//! ordering is not a stable contract and tests that pin it are brittle.

use crate::search::docs_bundle::BUNDLED_TOPICS;
use crate::search::index::Index;
use tempfile::{tempdir, TempDir};

fn corpus() -> (TempDir, Index) {
    let dir = tempdir().unwrap();
    let idx = Index::open(dir.path().join("search.db")).unwrap();
    idx.populate_docs(BUNDLED_TOPICS).unwrap();
    (dir, idx)
}

fn slugs_for(idx: &Index, query: &str) -> Vec<String> {
    idx.search_docs(query)
        .expect("an operator question must never surface as an FTS5 error")
        .into_iter()
        .map(|h| h.slug)
        .collect()
}

/// Eval 1 — the motivating question, verbatim from KJ4UYO via tuxlink-aib3n.
///
/// Two assertions, and both matter: the search must REACH the document, and the
/// document must actually CARRY the connect syntax. A hit whose body lacks the
/// connect string is a hit the model cannot answer from — which is precisely the
/// 12-token-snippet failure this whole feature exists to fix.
#[test]
fn eval_pat_ax25_digipeater_syntax_is_retrievable() {
    let (_dir, idx) = corpus();

    let hits = slugs_for(
        &idx,
        "What is the syntax for Pat Winlink in EmComm Tools in ax.25 to connect via a digipeater?",
    );
    assert!(
        hits.iter().any(|s| s == "pat-winlink"),
        "the motivating operator question could not reach pat-winlink; got {hits:?}"
    );

    let doc = idx
        .read_doc("pat-winlink")
        .unwrap()
        .expect("pat-winlink is indexed");
    assert!(
        doc.body.contains("ax25:///"),
        "pat-winlink does not carry the ax25:/// connect form — the model would give a \
         confident answer with no syntax in it"
    );
    // Hops are separated by '/'. tuxlink-aib3n's own description said commas. An
    // operator keys this string on the air; the wrong form must never come back.
    assert!(
        !doc.body.contains("ax25:///DIGI1,DIGI2"),
        "comma-separated digipeater path found — Pat separates hops with '/'"
    );
}

/// Eval 2 — P0 tuxlink-0mudm's original symptom: the model invented
/// "~/.config/tuxlink/tuxlink.cfg base64/mode 600" when the truth is the OS keyring.
#[test]
fn eval_credential_storage_is_retrievable() {
    let (_dir, idx) = corpus();
    let hits = slugs_for(&idx, "Where does Tuxlink store my Winlink password?");
    assert!(
        hits.iter()
            .any(|s| s == "27-settings" || s == "02-first-launch-wizard"),
        "no credential-storage doc reachable from the question that caused the P0; got {hits:?}"
    );
}

/// Eval 3 — the playbooks were invisible to in-app Elmer before this work: they live
/// in docs/mcp-knowledge/, which was served ONLY over the MCP resource tier, and
/// Elmer's runner never lists or reads resources. Indexing them is what fixes that.
///
/// Note the apostrophe: "won't" leaves a stray "t" token, which is exactly the kind
/// of noise the single-character filter in fts5_or_query exists to drop.
#[test]
fn eval_ardop_playbook_is_retrievable() {
    let (_dir, idx) = corpus();
    let hits = slugs_for(&idx, "ARDOP won't connect");
    assert!(
        hits.iter().any(|s| s == "playbook-ardop-wont-connect"),
        "ARDOP playbook not reachable; got {hits:?}"
    );
}

/// Eval 4 — the Winlink Express analogue of the Pat question. An operator helping
/// someone at the next table needs the WLE answer (Connection Type -> Digipeater,
/// two Via boxes), not Pat's.
#[test]
fn eval_winlink_express_packet_path_is_retrievable() {
    let (_dir, idx) = corpus();
    let hits = slugs_for(&idx, "How do I enter a digipeater path in Winlink Express?");
    assert!(
        hits.iter().any(|s| s == "winlink-express"),
        "winlink-express not reachable; got {hits:?}"
    );
}

/// Eval 5 — the conflation guard. "How do I operate Pat" and "I'm moving to Tuxlink
/// from Pat" are different questions with different documents. Both must be
/// reachable, or the corpus has collapsed the two and Elmer will answer one with the
/// other.
#[test]
fn eval_migration_topic_is_distinct_from_the_operational_doc() {
    let (_dir, idx) = corpus();
    let hits = slugs_for(&idx, "I'm switching to Tuxlink from Pat - what changes?");
    assert!(
        hits.iter().any(|s| s == "32-from-express-or-pat"),
        "migration topic not reachable; got {hits:?}"
    );
}

/// Every registered slug is readable. Guards the populate -> read round trip across
/// the whole real corpus, so a topic can never be searchable-but-unreadable.
#[test]
fn every_registered_slug_is_readable() {
    let (_dir, idx) = corpus();
    for t in BUNDLED_TOPICS {
        let doc = idx
            .read_doc(t.slug)
            .unwrap()
            .unwrap_or_else(|| panic!("registered slug {} is not readable", t.slug));
        assert!(!doc.body.trim().is_empty(), "{} has an empty body", t.slug);
    }
}
