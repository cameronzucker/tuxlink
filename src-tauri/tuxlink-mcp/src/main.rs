//! tuxlink-mcp stdio→UDS shim (stub).
//!
//! The real implementation (MCP phase 3.1 Task 5) connects a `UnixStream` to
//! the app's MCP socket and pumps bytes between it and stdio. This stub keeps
//! the workspace member compiling until then.
fn main() {}
