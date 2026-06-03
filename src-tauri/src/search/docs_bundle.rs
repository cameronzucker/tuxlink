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
        slug: "01-getting-started",
        title: "Getting started",
        markdown: include_str!("../../../docs/user-guide/01-getting-started.md"),
    },
    DocTopic {
        slug: "02-connections",
        title: "Connections",
        markdown: include_str!("../../../docs/user-guide/02-connections.md"),
    },
    DocTopic {
        slug: "03-mailbox",
        title: "The mailbox",
        markdown: include_str!("../../../docs/user-guide/03-mailbox.md"),
    },
    DocTopic {
        slug: "04-composing",
        title: "Composing messages",
        markdown: include_str!("../../../docs/user-guide/04-composing.md"),
    },
    DocTopic {
        slug: "05-forms",
        title: "HTML forms",
        markdown: include_str!("../../../docs/user-guide/05-forms.md"),
    },
    DocTopic {
        slug: "06-search",
        title: "Search",
        markdown: include_str!("../../../docs/user-guide/06-search.md"),
    },
    DocTopic {
        slug: "07-settings",
        title: "Settings",
        markdown: include_str!("../../../docs/user-guide/07-settings.md"),
    },
    DocTopic {
        slug: "08-color-schemes",
        title: "Color schemes",
        markdown: include_str!("../../../docs/user-guide/08-color-schemes.md"),
    },
    DocTopic {
        slug: "09-keyboard",
        title: "Keyboard shortcuts",
        markdown: include_str!("../../../docs/user-guide/09-keyboard.md"),
    },
    DocTopic {
        slug: "10-troubleshooting",
        title: "Troubleshooting",
        markdown: include_str!("../../../docs/user-guide/10-troubleshooting.md"),
    },
];
