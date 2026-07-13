//! Compile-time bundle of the indexed documentation corpora, used by
//! build_service to populate docs_fts (tuxlink-0gsy / spec §9.1).
//!
//! Three sources are indexed: docs/user-guide/ (also the Help sidebar),
//! docs/knowledge/ (agent-only, other Winlink clients), and docs/mcp-knowledge/
//! (playbooks; also served as MCP resources). All three are searchable via
//! docs_search and readable via docs_read.
//!
//! Adding a topic: include_str! it below + extend BUNDLED_TOPICS. The test in
//! docs_registry_test.rs FAILS if a .md exists on disk and is not registered here.
//!
//! Path resolution: include_str! is relative to THIS file. From
//! src-tauri/src/search/docs_bundle.rs, `../../../docs/...` reaches the repo root.

use crate::search::docs_index::{DocSource, DocTopic};

pub static BUNDLED_TOPICS: &[DocTopic<'static>] = &[
    DocTopic {
        slug: "01-what-is-tuxlink",
        title: "What is Tuxlink?",
        markdown: include_str!("../../../docs/user-guide/01-what-is-tuxlink.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "02-first-launch-wizard",
        title: "First-launch wizard",
        markdown: include_str!("../../../docs/user-guide/02-first-launch-wizard.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "03-sending-your-first",
        title: "Sending your first message",
        markdown: include_str!("../../../docs/user-guide/03-sending-your-first.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "04-the-winlink-ecosystem",
        title: "The Winlink ecosystem",
        markdown: include_str!("../../../docs/user-guide/04-the-winlink-ecosystem.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "05-cms-and-rms",
        title: "CMS and RMS",
        markdown: include_str!("../../../docs/user-guide/05-cms-and-rms.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "06-the-b2f-protocol",
        title: "The B2F protocol",
        markdown: include_str!("../../../docs/user-guide/06-the-b2f-protocol.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "07-mailbox-model",
        title: "Mailbox model",
        markdown: include_str!("../../../docs/user-guide/07-mailbox-model.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "08-picking-a-transport",
        title: "Picking a transport",
        markdown: include_str!("../../../docs/user-guide/08-picking-a-transport.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "09-ptt-overview",
        title: "PTT overview",
        markdown: include_str!("../../../docs/user-guide/09-ptt-overview.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "10-digirig",
        title: "Digirig",
        markdown: include_str!("../../../docs/user-guide/10-digirig.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "11-signalink-and-others",
        title: "SignaLink and others",
        markdown: include_str!("../../../docs/user-guide/11-signalink-and-others.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "12-cat-and-rigctld",
        title: "CAT and rigctld",
        markdown: include_str!("../../../docs/user-guide/12-cat-and-rigctld.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "13-radio-specific-notes",
        title: "Radio-specific notes",
        markdown: include_str!("../../../docs/user-guide/13-radio-specific-notes.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "14-packet-on-ax25",
        title: "Packet on AX.25",
        markdown: include_str!("../../../docs/user-guide/14-packet-on-ax25.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "15-ardop-deep-dive",
        title: "ARDOP deep dive",
        markdown: include_str!("../../../docs/user-guide/15-ardop-deep-dive.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "16-vara-hf-deep-dive",
        title: "VARA HF deep dive",
        markdown: include_str!("../../../docs/user-guide/16-vara-hf-deep-dive.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "17-choosing-the-right-mode",
        title: "Choosing the right mode",
        markdown: include_str!("../../../docs/user-guide/17-choosing-the-right-mode.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "18-the-mailbox",
        title: "The mailbox",
        markdown: include_str!("../../../docs/user-guide/18-the-mailbox.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "19-composing",
        title: "Composing messages",
        markdown: include_str!("../../../docs/user-guide/19-composing.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "20-html-forms",
        title: "HTML forms",
        markdown: include_str!("../../../docs/user-guide/20-html-forms.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "21-search",
        title: "Search",
        markdown: include_str!("../../../docs/user-guide/21-search.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "22-user-folders",
        title: "User folders",
        markdown: include_str!("../../../docs/user-guide/22-user-folders.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "23-catalog-requests",
        title: "Catalog requests",
        markdown: include_str!("../../../docs/user-guide/23-catalog-requests.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "24-emcomm-and-ics",
        title: "EmComm and ICS",
        markdown: include_str!("../../../docs/user-guide/24-emcomm-and-ics.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "25-net-check-ins",
        title: "Net check-ins",
        markdown: include_str!("../../../docs/user-guide/25-net-check-ins.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "26-position-and-privacy",
        title: "Position and privacy",
        markdown: include_str!("../../../docs/user-guide/26-position-and-privacy.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "27-settings",
        title: "Settings",
        markdown: include_str!("../../../docs/user-guide/27-settings.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "28-keyboard",
        title: "Keyboard shortcuts",
        markdown: include_str!("../../../docs/user-guide/28-keyboard.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "29-troubleshooting",
        title: "Troubleshooting",
        markdown: include_str!("../../../docs/user-guide/29-troubleshooting.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "30-glossary",
        title: "Glossary",
        markdown: include_str!("../../../docs/user-guide/30-glossary.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "31-credits",
        title: "Credits",
        markdown: include_str!("../../../docs/user-guide/31-credits.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "32-from-express-or-pat",
        title: "From Winlink Express or Pat",
        markdown: include_str!("../../../docs/user-guide/32-from-express-or-pat.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "33-operating-modes",
        title: "Operating modes",
        markdown: include_str!("../../../docs/user-guide/33-operating-modes.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "34-contacts-and-groups",
        title: "Contacts and groups",
        markdown: include_str!("../../../docs/user-guide/34-contacts-and-groups.md"),
        source: DocSource::UserGuide,
    },
    DocTopic {
        slug: "35-agent-mcp",
        title: "AI agent integration (MCP)",
        markdown: include_str!("../../../docs/user-guide/35-agent-mcp.md"),
        source: DocSource::UserGuide,
    },
    // Was on disk but unregistered until 2026-07-13 — the drift this file's
    // companion test now guards against.
    DocTopic {
        slug: "36-off-air-space-weather",
        title: "Off-air space weather (WWV/WWVH)",
        markdown: include_str!("../../../docs/user-guide/36-off-air-space-weather.md"),
        source: DocSource::UserGuide,
    },
    // --- docs/knowledge/ — agent-only reference on OTHER Winlink clients. Indexed
    // for docs_search/docs_read, deliberately NOT in the Help sidebar (topics.ts
    // globs docs/user-guide/ only), so Tuxlink's manual stays about Tuxlink.
    DocTopic {
        slug: "pat-winlink",
        title: "Pat Winlink (third-party client)",
        markdown: include_str!("../../../docs/knowledge/pat-winlink.md"),
        source: DocSource::Knowledge,
    },
    DocTopic {
        slug: "winlink-express",
        title: "Winlink Express (third-party client)",
        markdown: include_str!("../../../docs/knowledge/winlink-express.md"),
        source: DocSource::Knowledge,
    },
    // --- docs/mcp-knowledge/ — playbooks + reference. Until now reachable ONLY over
    // the MCP resource tier (tuxlink:// URIs), which in-app Elmer never lists or
    // reads: its runner's sole schema source is list_tools_as_specs. Indexing them
    // here is what makes them Elmer-visible at all.
    DocTopic {
        slug: "playbook-ardop-wont-connect",
        title: "Playbook: ARDOP will not connect",
        markdown: include_str!("../../../docs/mcp-knowledge/playbook-ardop-wont-connect.md"),
        source: DocSource::McpKnowledge,
    },
    DocTopic {
        slug: "playbook-cms-z-password-lag",
        title: "Playbook: new Winlink account, correct password rejected",
        markdown: include_str!("../../../docs/mcp-knowledge/playbook-cms-z-password-lag.md"),
        source: DocSource::McpKnowledge,
    },
    DocTopic {
        slug: "audio-setup",
        title: "Playbook: picking the right audio device for VARA / a digital modem",
        markdown: include_str!("../../../docs/mcp-knowledge/audio-setup.md"),
        source: DocSource::McpKnowledge,
    },
    DocTopic {
        slug: "vara-wine-setup",
        title: "Playbook: installing VARA HF under WINE",
        markdown: include_str!("../../../docs/mcp-knowledge/vara-wine-setup.md"),
        source: DocSource::McpKnowledge,
    },
    DocTopic {
        slug: "band-plan",
        title: "Band plan reference (Winlink-relevant)",
        markdown: include_str!("../../../docs/mcp-knowledge/band-plan.md"),
        source: DocSource::McpKnowledge,
    },
    DocTopic {
        slug: "modem-capability-matrix",
        title: "Modem capability matrix",
        markdown: include_str!("../../../docs/mcp-knowledge/modem-capability-matrix.md"),
        source: DocSource::McpKnowledge,
    },
    DocTopic {
        slug: "device-uv-pro",
        title: "Device setup: UV-Pro (Benshi)",
        markdown: include_str!("../../../docs/mcp-knowledge/device-uv-pro.md"),
        source: DocSource::McpKnowledge,
    },
    DocTopic {
        slug: "glossary-supplement",
        title: "Glossary supplement",
        markdown: include_str!("../../../docs/mcp-knowledge/glossary-supplement.md"),
        source: DocSource::McpKnowledge,
    },
    DocTopic {
        slug: "local-agent-deployment",
        title: "Running a Tuxlink assistant on local hardware",
        markdown: include_str!("../../../docs/mcp-knowledge/local-agent-deployment.md"),
        source: DocSource::McpKnowledge,
    },
    DocTopic {
        slug: "agents-guide",
        title: "Tuxlink agent guide",
        markdown: include_str!("../../../docs/mcp-knowledge/agents-guide.md"),
        source: DocSource::McpKnowledge,
    },
    DocTopic {
        slug: "37-ft8",
        title: "FT-8",
        markdown: include_str!("../../../docs/user-guide/37-ft8.md"),
        source: DocSource::UserGuide,
    },
];
