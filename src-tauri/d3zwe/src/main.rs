//! d3zwe — headless terminal frontend over `tuxlink-agent-runner` (Elmer spine).
//!
//! Wires the two REAL adapters the loop crate leaves abstract — the
//! loopback-enforced [`tuxlink_agent_frontend::provider::OpenAiProvider`] (SEC-5)
//! and the rmcp-over-UDS [`uds::UdsToolInvoker`] (relays denials, never arms) —
//! and drives the bounded agent loop ([`tuxlink_agent_runner::run`]) one-shot or
//! in a REPL, printing the transcript + outcome.
//!
//! Ctrl-C cancels the in-flight run via a [`CancellationToken`]; if the run was
//! cancelled while a gated egress tool may have been in flight, a best-effort
//! ungated abort (`cms_abort` / `modem_ardop_disconnect` / `vara_stop_session`)
//! is issued so a cancel cannot leave the transmitter keyed.
//!
//! The live model + socket run is the operator's N305 trial — NOT a CI test. The
//! decision logic (URL validation, response mapping, arg parsing, denial
//! classification, transcript rendering) is all in pure, unit-tested helpers in
//! `tuxlink-agent-frontend` (shared with the Elmer pane) and the local `print`
//! module.

mod cli;
mod print;
mod uds;

use std::io::{self, BufRead, Write};
use std::process::ExitCode;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use tuxlink_agent_runner::{run, EgressStatus, Limits, RunOutcome, ToolInvoker};
use tuxlink_agent_frontend::endpoint::validate_endpoint;
use tuxlink_agent_frontend::provider::{ApiKey, OpenAiProvider};

use crate::cli::{Args, ParseOutcome};
use crate::uds::{UdsToolInvoker, ABORT_TOOLS};

#[tokio::main]
async fn main() -> ExitCode {
    // Skip argv[0].
    let args = match cli::parse(std::env::args().skip(1)) {
        ParseOutcome::Run(a) => a,
        ParseOutcome::Help => {
            print!("{}", cli::USAGE);
            return ExitCode::SUCCESS;
        }
        ParseOutcome::Error(msg) => {
            eprintln!("d3zwe: {msg}");
            return ExitCode::from(2);
        }
    };

    match real_main(args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("d3zwe: {msg}");
            ExitCode::FAILURE
        }
    }
}

async fn real_main(args: Args) -> Result<(), String> {
    // SEC-5: validate the endpoint BEFORE building any client. The endpoint comes
    // only from the CLI/config here — never from a tool result.
    let endpoint = validate_endpoint(&args.endpoint, args.allow_remote)
        .map_err(|e| e.to_string())?;

    // The model adapter. An API key, if any, is read from the environment and
    // never logged. A local llama.cpp/Ollama shim usually needs none.
    let api_key = std::env::var("D3ZWE_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .map(ApiKey::new);
    let http = reqwest::Client::builder()
        .build()
        .map_err(|e| format!("could not build HTTP client: {e}"))?;
    let provider = OpenAiProvider::new(http, endpoint, args.model.clone(), api_key);

    // The tool adapter: connect the rmcp client over the Tuxlink MCP UDS.
    let socket_path = match &args.socket {
        Some(p) => std::path::PathBuf::from(p),
        None => uds::default_socket_path(),
    };
    eprintln!("d3zwe: connecting to MCP socket {}", socket_path.display());
    let invoker = UdsToolInvoker::connect(&socket_path)
        .await
        .map_err(|e| e.to_string())?;
    let invoker = Arc::new(invoker);
    eprintln!(
        "d3zwe: connected — {} tools available",
        invoker.tools().len()
    );

    let limits = Limits::default();

    if let Some(prompt) = args.prompt.clone() {
        // One-shot.
        run_one(&provider, &invoker, &prompt, limits).await;
    } else {
        // REPL.
        repl(&provider, &invoker, limits).await;
    }

    // Release the single MCP session slot on the way out.
    if let Ok(invoker) = Arc::try_unwrap(invoker) {
        invoker.shutdown().await;
    }
    Ok(())
}

/// Run a single prompt to a terminal outcome, wiring Ctrl-C → cancel → abort.
async fn run_one(
    provider: &OpenAiProvider,
    invoker: &Arc<UdsToolInvoker>,
    prompt: &str,
    limits: Limits,
) {
    let cancel = CancellationToken::new();

    // Ctrl-C cancels the in-flight run. We use a child token so a single Ctrl-C
    // cancels THIS run; the REPL re-arms a fresh token per prompt.
    let ctrlc_cancel = cancel.clone();
    let ctrlc_task = tokio::spawn(async move {
        // If installing the handler fails, we simply never cancel via Ctrl-C —
        // the bounded-turn cap still terminates the loop.
        if tokio::signal::ctrl_c().await.is_ok() {
            eprintln!("\nd3zwe: Ctrl-C — cancelling…");
            ctrlc_cancel.cancel();
        }
    });

    // EgressStatus is observed-only by the loop; d3zwe does not read the live
    // guard (it relays denials), so a default snapshot is correct here.
    let outcome = run(
        prompt,
        provider,
        &**invoker,
        EgressStatus::default(),
        limits,
        cancel.clone(),
    )
    .await;

    // Stop the Ctrl-C watcher (whether it fired or not).
    ctrlc_task.abort();

    // If the run was cancelled, a gated egress tool may have been keying the
    // transmitter when the operator aborted. Best-effort issue every ungated
    // abort so the cancel cannot leave TX up. Aborts are NEVER gated, so this is
    // safe to call unconditionally on cancel.
    if matches!(outcome, RunOutcome::Cancelled) {
        for tool in ABORT_TOOLS {
            let ok = invoker.call_abort(tool).await;
            eprintln!(
                "d3zwe: cancel-abort {tool}: {}",
                if ok { "sent" } else { "no-op/failed" }
            );
        }
    }

    println!("{}", print::render_outcome(&outcome));
}

/// A minimal interactive REPL: read a line, run it, print the outcome, repeat.
/// Blank lines are ignored; EOF (Ctrl-D) or `:quit` exits.
async fn repl(provider: &OpenAiProvider, invoker: &Arc<UdsToolInvoker>, limits: Limits) {
    eprintln!("d3zwe REPL — type a prompt, `:quit` or Ctrl-D to exit.");
    let stdin = io::stdin();
    loop {
        print!("d3zwe> ");
        let _ = io::stdout().flush();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                eprintln!("\nd3zwe: EOF — exiting.");
                break;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("d3zwe: stdin error: {e}");
                break;
            }
        }
        let prompt = line.trim();
        if prompt.is_empty() {
            continue;
        }
        if prompt == ":quit" || prompt == ":q" {
            break;
        }

        run_one(provider, invoker, prompt, limits).await;
    }
}
