#![warn(clippy::disallowed_methods)]

#[allow(dead_code)]
mod clock;
mod config;
mod daemon;
mod paths;
mod pet;

use clap::{Parser, Subcommand};

use crate::clock::{Clock, SystemClock};
use crate::daemon::ipc::client::send_request;
use crate::daemon::ipc::protocol::{Request, RequestOp};

#[derive(Parser)]
#[command(name = "shellagotchi")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Report a command's exit status to the daemon (used by the shell hook).
    Feed {
        #[arg(long)]
        exit: i32,
        #[arg(long, default_value_t = 0)]
        duration: u64,
    },
    /// Run the shellagotchi daemon (the process the shell hook and CLI
    /// subcommands talk to over a Unix socket).
    Daemon,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Feed { exit, duration } => {
            feed(exit, duration).await;
        }
        Commands::Daemon => {
            tracing_subscriber::fmt::init();
            if let Err(err) = crate::daemon::run::run().await {
                tracing::error!("daemon exited with error: {err}");
                std::process::exit(1);
            }
        }
    }
}

/// Reports a command's exit status to the daemon. This is invoked from a
/// shell hook (`PROMPT_COMMAND`) after every command the user runs, so it
/// has an absolute hard requirement: it must NEVER slow down or break the
/// user's shell, even if the daemon is completely absent. Consequently
/// this function swallows every possible error silently and always
/// returns without printing anything to stdout/stderr; `main` always
/// exits 0 for this subcommand.
async fn feed(exit_code: i32, duration_ms: u64) {
    let argv0 = std::env::var("SHELLAGOTCHI_ARGV0").unwrap_or_default();
    let ts = SystemClock.now().timestamp();

    let req = Request::new(RequestOp::Feed {
        exit_code,
        duration_ms,
        argv0,
        ts,
    });

    let socket_path = crate::paths::socket_path();
    // Any error here (connection refused, timeout, malformed response,
    // missing socket, etc) is intentionally swallowed: `feed` must be
    // silent and always succeed from the shell's point of view.
    let _ = send_request(&socket_path, &req).await;
}
