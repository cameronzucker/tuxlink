//! End-to-end: store N messages via the mailbox (with the Index attached),
//! then exercise every chip + free-text path. Each test owns a tempdir so
//! they run in parallel.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;
use tuxlink_lib::native_mailbox::Mailbox;
use tuxlink_lib::search::commands::SearchService;
use tuxlink_lib::search::index::Index;
use tuxlink_lib::search::saved::SavedStore;
use tuxlink_lib::search::types::{FilterKey, FilterValue, QuerySpec, ReadState};
use tuxlink_lib::winlink::compose::compose_message;
use tuxlink_lib::winlink_backend::MailboxFolder;

fn raw(from: &str, to: &[&str], subject: &str, body: &str, secs: u32) -> Vec<u8> {
    compose_message(from, to, &[], subject, body, secs as u64).to_bytes()
}

fn build(dir: &std::path::Path) -> (Mailbox, SearchService) {
    let idx = Arc::new(Mutex::new(Index::open(dir.join("search.db")).unwrap()));
    let mbox = Mailbox::new(dir.to_path_buf()).with_index(idx.clone());
    let svc = SearchService {
        index: idx,
        saved: Mutex::new(SavedStore::open(dir.join("saved.json")).unwrap()),
        now_unix: || 1_716_200_000,
    };
    (mbox, svc)
}

#[test]
fn freetext_finds_match_across_inbox_and_sent() {
    let dir = tempdir().unwrap();
    let (mbox, svc) = build(dir.path());
    mbox.store(MailboxFolder::Inbox, &raw("KX5DD", &["N7CPZ"], "DAMAGE report", "powerlines", 1_716_200_000)).unwrap();
    mbox.store(MailboxFolder::Sent,  &raw("N7CPZ", &["KX5DD"], "Re: damage", "ack", 1_716_200_100)).unwrap();
    let res = svc.run(QuerySpec { free_text: Some("damage".into()), ..QuerySpec::default() }).unwrap();
    assert_eq!(res.total_matches, 2);
}

#[test]
fn from_chip_narrows_by_sender() {
    let dir = tempdir().unwrap();
    let (mbox, svc) = build(dir.path());
    mbox.store(MailboxFolder::Inbox, &raw("KX5DD",  &["N7CPZ"], "x", "y", 1_716_200_000)).unwrap();
    mbox.store(MailboxFolder::Inbox, &raw("WX5RES", &["N7CPZ"], "x", "y", 1_716_200_100)).unwrap();
    let mut filters = BTreeMap::new();
    filters.insert(FilterKey::From, FilterValue::Addr("KX5DD".into()));
    let res = svc.run(QuerySpec { filters, ..QuerySpec::default() }).unwrap();
    assert_eq!(res.total_matches, 1);
}

#[test]
fn mark_read_propagates_to_unread_filter() {
    let dir = tempdir().unwrap();
    let (mbox, svc) = build(dir.path());
    let id = mbox.store(MailboxFolder::Inbox, &raw("KX5DD", &["N7CPZ"], "x", "y", 1_716_200_000)).unwrap();
    let mut filters = BTreeMap::new();
    filters.insert(FilterKey::ReadState, FilterValue::ReadState(ReadState::Unread));
    let before = svc.run(QuerySpec { filters: filters.clone(), ..QuerySpec::default() }).unwrap();
    assert_eq!(before.total_matches, 1);
    mbox.mark_read(MailboxFolder::Inbox, &id).unwrap();
    let after = svc.run(QuerySpec { filters, ..QuerySpec::default() }).unwrap();
    assert_eq!(after.total_matches, 0);
}

#[test]
fn rebuild_picks_up_pre_existing_mailbox() {
    let dir = tempdir().unwrap();
    // Phase 1: store without index attached
    {
        let mbox = Mailbox::new(dir.path().to_path_buf());
        mbox.store(MailboxFolder::Inbox, &raw("KX5DD", &["N7CPZ"], "DAMAGE report", "p", 1_716_200_000)).unwrap();
    }
    // Phase 2: attach index + rebuild
    let svc = SearchService {
        index: Arc::new(Mutex::new(Index::open(dir.path().join("search.db")).unwrap())),
        saved: Mutex::new(SavedStore::open(dir.path().join("saved.json")).unwrap()),
        now_unix: || 1_716_200_000,
    };
    let stats = svc.rebuild_index(dir.path().to_path_buf(), None).unwrap();
    assert_eq!(stats.messages_indexed, 1);
}
