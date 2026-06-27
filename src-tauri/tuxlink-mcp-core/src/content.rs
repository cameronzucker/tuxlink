//! Static knowledge content served as MCP resources (phase 3.5).
//!
//! The knowledge layer exposes hand-authored operator knowledge and a curated
//! subset of the in-app user guide as MCP **resources** under `tuxlink://`
//! URIs. Content is embedded at compile time via `include_str!`, so a resource
//! read is a pure table lookup with no I/O and no app state.
//!
//! Two content families live in [`CATALOG`]:
//!
//! - **MCP-knowledge docs** (`docs/mcp-knowledge/*.md`) — authored for the
//!   agent: a glossary supplement, two diagnostic playbooks, a device setup
//!   guide, a band-plan reference, and the modem capability matrix.
//! - **Curated user-guide docs** (`docs/user-guide/NN-*.md`) — the subset of
//!   the in-app guide most useful to an agent helping an operator, reused
//!   verbatim from the same source the help window indexes.
//!
//! Path resolution mirrors `src-tauri/src/search/docs_bundle.rs`: `include_str!`
//! is relative to THIS file, so from
//! `src-tauri/tuxlink-mcp-core/src/content.rs` the prefix `../../../docs/...`
//! reaches the repo-root `docs/` directory. If the build breaks with "couldn't
//! read", check the path depth.

/// One knowledge resource: its `tuxlink://` URI, a stable machine name, a
/// human-readable title, a one-line description, and the embedded markdown
/// body. All fields are `'static` because the content is compiled in.
pub struct KnowledgeResource {
    /// The `tuxlink://...` resource URI (the lookup key for `read_resource`).
    pub uri: &'static str,
    /// Stable machine name (unique, slug-style).
    pub name: &'static str,
    /// Human-readable title for resource listings.
    pub title: &'static str,
    /// One-line description of what the resource contains.
    pub description: &'static str,
    /// The embedded markdown body (`text/markdown`).
    pub markdown: &'static str,
}

/// The full catalog of knowledge resources, in listing order. The MCP-knowledge
/// docs come first, then the curated user-guide subset.
pub static CATALOG: &[KnowledgeResource] = &[
    // ----- Agent onboarding (read first) -----
    KnowledgeResource {
        uri: "tuxlink://agents/guide",
        name: "agents-guide",
        title: "Tuxlink agent guide",
        description: "Read first: what Tuxlink is, the full MCP tool surface by tier, the arm/taint model, and where the docs are.",
        markdown: include_str!("../../../docs/mcp-knowledge/agents-guide.md"),
    },
    // ----- MCP-knowledge docs (authored for the agent) -----
    KnowledgeResource {
        uri: "tuxlink://glossary-supplement",
        name: "glossary-supplement",
        title: "Glossary supplement",
        description: "Operator-level definitions of control strip, SSID, KISS, and NVIS.",
        markdown: include_str!("../../../docs/mcp-knowledge/glossary-supplement.md"),
    },
    KnowledgeResource {
        uri: "tuxlink://playbook/cms-z-password-lag",
        name: "playbook-cms-z-password-lag",
        title: "Playbook: new-account password rejected (cms-z lag)",
        description: "Why a brand-new Winlink account's correct password is rejected, and the wait-and-retry fix.",
        markdown: include_str!("../../../docs/mcp-knowledge/playbook-cms-z-password-lag.md"),
    },
    KnowledgeResource {
        uri: "tuxlink://playbook/ardop-wont-connect",
        name: "playbook-ardop-wont-connect",
        title: "Playbook: ARDOP will not connect",
        description: "Ordered diagnostic checklist for an ARDOP session that fails to establish.",
        markdown: include_str!("../../../docs/mcp-knowledge/playbook-ardop-wont-connect.md"),
    },
    KnowledgeResource {
        uri: "tuxlink://device/uv-pro",
        name: "device-uv-pro",
        title: "Device setup: UV-Pro (Benshi)",
        description: "UV-Pro APRS + Winlink dual-mode setup, KISS-vs-frequency, and Bluetooth pairing notes.",
        markdown: include_str!("../../../docs/mcp-knowledge/device-uv-pro.md"),
    },
    KnowledgeResource {
        uri: "tuxlink://reference/band-plan",
        name: "reference-band-plan",
        title: "Band plan reference (Winlink-relevant)",
        description: "Concise Winlink HF/VHF dial-frequency reference; a starting reference, verify against current band plans.",
        markdown: include_str!("../../../docs/mcp-knowledge/band-plan.md"),
    },
    KnowledgeResource {
        uri: "tuxlink://reference/modem-capability-matrix",
        name: "reference-modem-capability-matrix",
        title: "Modem capability matrix",
        description: "Comparison of ARDOP, VARA HF, and packet/AX.25: bandwidth, speed, robustness, license/cost, and when to use each.",
        markdown: include_str!("../../../docs/mcp-knowledge/modem-capability-matrix.md"),
    },
    // ----- Curated user-guide docs (reused from the in-app guide) -----
    KnowledgeResource {
        uri: "tuxlink://glossary",
        name: "guide-glossary",
        title: "Glossary",
        description: "The in-app user-guide glossary of Winlink and amateur-radio terms.",
        markdown: include_str!("../../../docs/user-guide/30-glossary.md"),
    },
    KnowledgeResource {
        uri: "tuxlink://guide/what-is-tuxlink",
        name: "guide-what-is-tuxlink",
        title: "What is Tuxlink?",
        description: "Getting started: what Tuxlink is and the problem it solves.",
        markdown: include_str!("../../../docs/user-guide/01-what-is-tuxlink.md"),
    },
    KnowledgeResource {
        uri: "tuxlink://guide/first-launch-wizard",
        name: "guide-first-launch-wizard",
        title: "First-launch wizard",
        description: "Getting started: the first-run setup wizard.",
        markdown: include_str!("../../../docs/user-guide/02-first-launch-wizard.md"),
    },
    KnowledgeResource {
        uri: "tuxlink://guide/sending-your-first",
        name: "guide-sending-your-first",
        title: "Sending your first message",
        description: "Getting started: sending a first Winlink message end to end.",
        markdown: include_str!("../../../docs/user-guide/03-sending-your-first.md"),
    },
    KnowledgeResource {
        uri: "tuxlink://guide/picking-a-transport",
        name: "guide-picking-a-transport",
        title: "Picking a transport",
        description: "How to choose between Telnet, Packet, ARDOP, and VARA for a session.",
        markdown: include_str!("../../../docs/user-guide/08-picking-a-transport.md"),
    },
    KnowledgeResource {
        uri: "tuxlink://guide/ptt",
        name: "guide-ptt",
        title: "PTT overview",
        description: "Push-to-talk methods: hardware lines, serial RTS/DTR, CAT, GPIO, CM108.",
        markdown: include_str!("../../../docs/user-guide/09-ptt-overview.md"),
    },
    KnowledgeResource {
        uri: "tuxlink://guide/packet",
        name: "guide-packet",
        title: "Packet on AX.25",
        description: "Winlink Packet over AX.25, KISS, and the Dire Wolf software TNC.",
        markdown: include_str!("../../../docs/user-guide/14-packet-on-ax25.md"),
    },
    KnowledgeResource {
        uri: "tuxlink://guide/ardop",
        name: "guide-ardop",
        title: "ARDOP deep dive",
        description: "The open HF data mode: bandwidth choices, ardopcf wiring, audio calibration.",
        markdown: include_str!("../../../docs/user-guide/15-ardop-deep-dive.md"),
    },
    KnowledgeResource {
        uri: "tuxlink://guide/vara",
        name: "guide-vara",
        title: "VARA HF deep dive",
        description: "The proprietary HF data mode: tiers, Wine on Linux, TCP wiring, VARA FM.",
        markdown: include_str!("../../../docs/user-guide/16-vara-hf-deep-dive.md"),
    },
    KnowledgeResource {
        uri: "tuxlink://guide/emcomm-ics",
        name: "guide-emcomm-ics",
        title: "EmComm and ICS",
        description: "Emergency communications context and the ICS form family (ICS-213 and others).",
        markdown: include_str!("../../../docs/user-guide/24-emcomm-and-ics.md"),
    },
    KnowledgeResource {
        uri: "tuxlink://playbook/connection-troubleshooting",
        name: "guide-connection-troubleshooting",
        title: "Troubleshooting",
        description: "The in-app connection/diagnostic troubleshooting walks.",
        markdown: include_str!("../../../docs/user-guide/29-troubleshooting.md"),
    },
    KnowledgeResource {
        uri: "tuxlink://guide/from-express-or-pat",
        name: "guide-from-express-or-pat",
        title: "From Winlink Express or Pat",
        description: "Moving to Tuxlink from Winlink Express or Pat.",
        markdown: include_str!("../../../docs/user-guide/32-from-express-or-pat.md"),
    },
];

/// Look up a resource by its `tuxlink://` URI. Returns `None` for an unknown
/// URI (the calling tool maps that onto an `invalid_request` error).
pub fn find_by_uri(uri: &str) -> Option<&'static KnowledgeResource> {
    CATALOG.iter().find(|r| r.uri == uri)
}
