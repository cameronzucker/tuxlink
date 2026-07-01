//! `tuxlink-agent-frontend` — shared model-adapter and MCP-client helpers for
//! the Elmer assistant spine.
//!
//! Crates that supply a concrete `Provider` + `ToolInvoker` to the bounded
//! agent loop ([`tuxlink_agent_runner::run`]) share three pieces:
//!
//! * [`endpoint`] — SEC-5 loopback/metadata enforcement for the model URL.
//! * [`egress`] — the shared socket-layer SSRF / DNS-rebind guard: the single
//!   `build_vetted_client` chokepoint that resolves, vets every resolved IP, and
//!   pins the connection so a named host cannot rebind to a forbidden IP.
//! * [`provider`] — `OpenAiProvider`: the OpenAI-compatible chat-completions
//!   adapter that talks to a local llama.cpp / Ollama shim (or, behind
//!   `--allow-remote`, an actual cloud endpoint).
//! * [`mcp_client`] — transport-agnostic rmcp helpers that convert between
//!   rmcp's wire types and the runner's value types, classify call errors as
//!   denials vs. operational failures, and collect the tool surface.
//!
//! The d3zwe binary (UDS transport) depends on this crate for all three
//! modules. The Elmer pane in the Tauri monolith will depend on it for the
//! same three, supplying its own transport.

pub mod anthropic_provider;
pub mod egress;
pub mod endpoint;
pub mod mcp_client;
pub mod provider;

// Convenience re-exports for crate consumers.
pub use provider::ApiKey;
