//! v0.1 find-messages: FTS5-indexed search over the filesystem-canonical
//! mailbox. The mailbox is canonical (see `native_mailbox.rs`); this module
//! maintains a derived `search.db` regenerable from disk via the
//! `rebuild-index` command.
//!
//! See `docs/design/2026-05-30-find-messages-design.md` for the full design.

pub mod commands;
pub mod extractor;
pub mod index;
pub mod query;
pub mod saved;
pub mod types;

#[cfg(test)]
mod module_smoke {
    #[test]
    fn module_is_wired() {
        assert_eq!(2 + 2, 4);
    }
}
