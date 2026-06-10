//! Compile-time bundle of docs/user-guide/*.md, used by build_service to
//! populate docs_fts at first launch (tuxlink-0gsy / spec §9.1).
//!
//! Adding a new topic: include_str! it below + extend BUNDLED_TOPICS.
//! Section grouping for the sidebar lives in src/help/topics.ts; this file
//! is search-index-only.
//!
//! Path resolution: include_str! is relative to THIS file. From
//! src-tauri/src/search/docs_bundle.rs, `../../../docs/...` reaches the repo
//! root. If the build breaks with "couldn't read", check the path is right.

use crate::search::docs_index::DocTopic;

pub static BUNDLED_TOPICS: &[DocTopic<'static>] = &[
    DocTopic {
        slug: "01-what-is-tuxlink",
        title: "What is Tuxlink?",
        markdown: include_str!("../../../docs/user-guide/01-what-is-tuxlink.md"),
    },
    DocTopic {
        slug: "02-first-launch-wizard",
        title: "First-launch wizard",
        markdown: include_str!("../../../docs/user-guide/02-first-launch-wizard.md"),
    },
    DocTopic {
        slug: "03-sending-your-first",
        title: "Sending your first message",
        markdown: include_str!("../../../docs/user-guide/03-sending-your-first.md"),
    },
    DocTopic {
        slug: "04-the-winlink-ecosystem",
        title: "The Winlink ecosystem",
        markdown: include_str!("../../../docs/user-guide/04-the-winlink-ecosystem.md"),
    },
    DocTopic {
        slug: "05-cms-and-rms",
        title: "CMS and RMS",
        markdown: include_str!("../../../docs/user-guide/05-cms-and-rms.md"),
    },
    DocTopic {
        slug: "06-the-b2f-protocol",
        title: "The B2F protocol",
        markdown: include_str!("../../../docs/user-guide/06-the-b2f-protocol.md"),
    },
    DocTopic {
        slug: "07-mailbox-model",
        title: "Mailbox model",
        markdown: include_str!("../../../docs/user-guide/07-mailbox-model.md"),
    },
    DocTopic {
        slug: "08-picking-a-transport",
        title: "Picking a transport",
        markdown: include_str!("../../../docs/user-guide/08-picking-a-transport.md"),
    },
    DocTopic {
        slug: "09-ptt-overview",
        title: "PTT overview",
        markdown: include_str!("../../../docs/user-guide/09-ptt-overview.md"),
    },
    DocTopic {
        slug: "10-digirig",
        title: "Digirig",
        markdown: include_str!("../../../docs/user-guide/10-digirig.md"),
    },
    DocTopic {
        slug: "11-signalink-and-others",
        title: "SignaLink and others",
        markdown: include_str!("../../../docs/user-guide/11-signalink-and-others.md"),
    },
    DocTopic {
        slug: "12-cat-and-rigctld",
        title: "CAT and rigctld",
        markdown: include_str!("../../../docs/user-guide/12-cat-and-rigctld.md"),
    },
    DocTopic {
        slug: "13-radio-specific-notes",
        title: "Radio-specific notes",
        markdown: include_str!("../../../docs/user-guide/13-radio-specific-notes.md"),
    },
    DocTopic {
        slug: "14-packet-on-ax25",
        title: "Packet on AX.25",
        markdown: include_str!("../../../docs/user-guide/14-packet-on-ax25.md"),
    },
    DocTopic {
        slug: "15-ardop-deep-dive",
        title: "ARDOP deep dive",
        markdown: include_str!("../../../docs/user-guide/15-ardop-deep-dive.md"),
    },
    DocTopic {
        slug: "16-vara-hf-deep-dive",
        title: "VARA HF deep dive",
        markdown: include_str!("../../../docs/user-guide/16-vara-hf-deep-dive.md"),
    },
    DocTopic {
        slug: "17-choosing-the-right-mode",
        title: "Choosing the right mode",
        markdown: include_str!("../../../docs/user-guide/17-choosing-the-right-mode.md"),
    },
    DocTopic {
        slug: "18-the-mailbox",
        title: "The mailbox",
        markdown: include_str!("../../../docs/user-guide/18-the-mailbox.md"),
    },
    DocTopic {
        slug: "19-composing",
        title: "Composing messages",
        markdown: include_str!("../../../docs/user-guide/19-composing.md"),
    },
    DocTopic {
        slug: "20-html-forms",
        title: "HTML forms",
        markdown: include_str!("../../../docs/user-guide/20-html-forms.md"),
    },
    DocTopic {
        slug: "21-search",
        title: "Search",
        markdown: include_str!("../../../docs/user-guide/21-search.md"),
    },
    DocTopic {
        slug: "22-user-folders",
        title: "User folders",
        markdown: include_str!("../../../docs/user-guide/22-user-folders.md"),
    },
    DocTopic {
        slug: "23-catalog-requests",
        title: "Catalog requests",
        markdown: include_str!("../../../docs/user-guide/23-catalog-requests.md"),
    },
    DocTopic {
        slug: "24-emcomm-and-ics",
        title: "EmComm and ICS",
        markdown: include_str!("../../../docs/user-guide/24-emcomm-and-ics.md"),
    },
    DocTopic {
        slug: "25-net-check-ins",
        title: "Net check-ins",
        markdown: include_str!("../../../docs/user-guide/25-net-check-ins.md"),
    },
    DocTopic {
        slug: "26-position-and-privacy",
        title: "Position and privacy",
        markdown: include_str!("../../../docs/user-guide/26-position-and-privacy.md"),
    },
    DocTopic {
        slug: "27-settings",
        title: "Settings",
        markdown: include_str!("../../../docs/user-guide/27-settings.md"),
    },
    DocTopic {
        slug: "28-keyboard",
        title: "Keyboard shortcuts",
        markdown: include_str!("../../../docs/user-guide/28-keyboard.md"),
    },
    DocTopic {
        slug: "29-troubleshooting",
        title: "Troubleshooting",
        markdown: include_str!("../../../docs/user-guide/29-troubleshooting.md"),
    },
    DocTopic {
        slug: "30-glossary",
        title: "Glossary",
        markdown: include_str!("../../../docs/user-guide/30-glossary.md"),
    },
    DocTopic {
        slug: "31-credits",
        title: "Credits",
        markdown: include_str!("../../../docs/user-guide/31-credits.md"),
    },
    DocTopic {
        slug: "32-from-express-or-pat",
        title: "From Winlink Express or Pat",
        markdown: include_str!("../../../docs/user-guide/32-from-express-or-pat.md"),
    },
    DocTopic {
        slug: "33-operating-modes",
        title: "Operating modes",
        markdown: include_str!("../../../docs/user-guide/33-operating-modes.md"),
    },
    DocTopic {
        slug: "34-contacts-and-groups",
        title: "Contacts and groups",
        markdown: include_str!("../../../docs/user-guide/34-contacts-and-groups.md"),
    },
];
