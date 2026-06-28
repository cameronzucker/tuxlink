//! CLI argument parsing for d3zwe (T8).
//!
//! Hand-rolled rather than via `clap`: `clap` is NOT in the workspace
//! `Cargo.lock`, and adding it would pull a new dependency tree and churn the
//! single lockfile this workspace shares. The arg surface is tiny, so a manual
//! parser keeps the lockfile stable and is itself trivially unit-testable.
//!
//! The endpoint default is intentionally a loopback URL so the SEC-5 gate is
//! satisfied out of the box; `--allow-remote` is the only way to reach a
//! non-loopback model.

/// Parsed command-line arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// Model endpoint (OpenAI-compatible `/v1/chat/completions` base).
    pub endpoint: String,
    /// Model name to request.
    pub model: String,
    /// MCP UDS socket path. `None` → resolve the #939 default at runtime.
    pub socket: Option<String>,
    /// One-shot prompt. `None` → interactive REPL.
    pub prompt: Option<String>,
    /// SEC-5 opt-in: permit a non-loopback (but never link-local/metadata)
    /// endpoint.
    pub allow_remote: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            // A loopback default keeps SEC-5 happy without `--allow-remote`.
            endpoint: "http://127.0.0.1:11434/v1/chat/completions".to_string(),
            model: "local".to_string(),
            socket: None,
            prompt: None,
            allow_remote: false,
        }
    }
}

/// Outcome of parsing: either ready-to-run [`Args`], or a request to print help
/// / an error message and exit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseOutcome {
    /// Run with these args.
    Run(Args),
    /// `--help` / `-h`: print usage and exit 0.
    Help,
    /// A parse error: print the message to stderr and exit non-zero.
    Error(String),
}

pub const USAGE: &str = "\
d3zwe — headless terminal frontend over the Tuxlink agent runner.

USAGE:
    d3zwe [OPTIONS]

OPTIONS:
    --endpoint <URL>   OpenAI-compatible model endpoint
                       (default: http://127.0.0.1:11434/v1/chat/completions).
                       Loopback-only unless --allow-remote (SEC-5).
    --model <NAME>     Model name to request (default: local).
    --socket <PATH>    Tuxlink MCP Unix-domain socket
                       (default: the #939 descriptor location).
    --prompt <TEXT>    Run one prompt and exit. Omit for an interactive REPL.
    --allow-remote     Permit a non-loopback endpoint (advanced; the endpoint
                       becomes a data sink). Link-local / cloud-metadata ranges
                       are ALWAYS refused regardless of this flag.
    -h, --help         Print this help.
";

/// Parse an iterator of args (EXCLUDING argv[0]). Pure — unit-testable.
pub fn parse<I, S>(args: I) -> ParseOutcome
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut out = Args::default();
    let mut iter = args.into_iter().map(Into::into).peekable();

    // Helper to take the value following a `--flag <value>` option.
    fn take_value(
        flag: &str,
        iter: &mut std::iter::Peekable<impl Iterator<Item = String>>,
    ) -> Result<String, String> {
        match iter.next() {
            Some(v) if !v.starts_with("--") => Ok(v),
            // Reject a missing value or a value that looks like the next flag —
            // `--endpoint --model x` is a user error, not an endpoint of
            // "--model".
            _ => Err(format!("option `{flag}` requires a value")),
        }
    }

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return ParseOutcome::Help,
            "--allow-remote" => out.allow_remote = true,
            "--endpoint" => match take_value("--endpoint", &mut iter) {
                Ok(v) => out.endpoint = v,
                Err(e) => return ParseOutcome::Error(e),
            },
            "--model" => match take_value("--model", &mut iter) {
                Ok(v) => out.model = v,
                Err(e) => return ParseOutcome::Error(e),
            },
            "--socket" => match take_value("--socket", &mut iter) {
                Ok(v) => out.socket = Some(v),
                Err(e) => return ParseOutcome::Error(e),
            },
            "--prompt" => match take_value("--prompt", &mut iter) {
                Ok(v) => out.prompt = Some(v),
                Err(e) => return ParseOutcome::Error(e),
            },
            other => {
                return ParseOutcome::Error(format!(
                    "unrecognized argument `{other}` (try --help)"
                ));
            }
        }
    }

    ParseOutcome::Run(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_loopback_repl() {
        let out = parse(Vec::<String>::new());
        match out {
            ParseOutcome::Run(args) => {
                assert!(args.endpoint.starts_with("http://127.0.0.1"));
                assert_eq!(args.model, "local");
                assert!(args.socket.is_none());
                assert!(args.prompt.is_none());
                assert!(!args.allow_remote);
            }
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn parses_all_options() {
        let out = parse([
            "--endpoint",
            "http://127.0.0.1:8080/v1/chat/completions",
            "--model",
            "qwen2.5",
            "--socket",
            "/run/user/1000/tuxlink/mcp.sock",
            "--prompt",
            "find a station",
            "--allow-remote",
        ]);
        let args = match out {
            ParseOutcome::Run(a) => a,
            other => panic!("expected Run, got {other:?}"),
        };
        assert_eq!(args.endpoint, "http://127.0.0.1:8080/v1/chat/completions");
        assert_eq!(args.model, "qwen2.5");
        assert_eq!(args.socket.as_deref(), Some("/run/user/1000/tuxlink/mcp.sock"));
        assert_eq!(args.prompt.as_deref(), Some("find a station"));
        assert!(args.allow_remote);
    }

    #[test]
    fn help_flag() {
        assert_eq!(parse(["--help"]), ParseOutcome::Help);
        assert_eq!(parse(["-h"]), ParseOutcome::Help);
    }

    #[test]
    fn missing_value_is_error() {
        assert!(matches!(parse(["--endpoint"]), ParseOutcome::Error(_)));
        // A following flag is not a value.
        assert!(matches!(
            parse(["--endpoint", "--model", "m"]),
            ParseOutcome::Error(_)
        ));
    }

    #[test]
    fn unknown_arg_is_error() {
        assert!(matches!(parse(["--frobnicate"]), ParseOutcome::Error(_)));
        // A bare positional is also an error (no positionals are accepted).
        assert!(matches!(parse(["hello"]), ParseOutcome::Error(_)));
    }

    #[test]
    fn prompt_with_spaces_preserved() {
        let out = parse(["--prompt", "list my inbox then summarize"]);
        match out {
            ParseOutcome::Run(args) => {
                assert_eq!(args.prompt.as_deref(), Some("list my inbox then summarize"));
            }
            other => panic!("expected Run, got {other:?}"),
        }
    }
}
