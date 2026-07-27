#![warn(clippy::disallowed_methods)]

#[allow(dead_code)]
mod clock;
mod config;
mod daemon;
mod paths;
mod pet;
mod render;
mod shell;

use clap::{Parser, Subcommand};

use crate::clock::{Clock, SystemClock};
use crate::daemon::ipc::client::send_request;
use crate::daemon::ipc::protocol::{Request, RequestOp};
use crate::shell::Shell;

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
    /// Print a rendered prompt segment, reading ONLY the prompt cache
    /// file the daemon maintains (never the IPC socket). This makes it
    /// safe and fast enough to embed directly in a shell `PS1`.
    Prompt {
        #[arg(long, default_value = "compact")]
        format: String,
    },
    /// Show the pet's current status as a bordered ASCII card, fetched
    /// live from the daemon over IPC.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Alias for `status` (the plan treats `show` and `status` as the
    /// same command).
    Show {
        #[arg(long)]
        json: bool,
    },
    /// Launch a live, interactively-updating terminal UI showing the
    /// pet's sprite, mood, and stat gauges, polling the daemon in the
    /// background. Keybinds: q=quit, c=clean, p=pet, r=refresh.
    Watch,
    /// Print the shell integration snippet for `shell` to stdout, meant
    /// to be evaluated/sourced directly into an rc file, e.g.
    /// `eval "$(shellagotchi init bash)"` in `~/.bashrc`. Prints
    /// *exactly* the snippet with no extra logging, since the caller
    /// feeds stdout straight into `eval`/`source`.
    Init {
        #[arg(value_enum)]
        shell: Shell,
    },
    /// Run diagnostic checks (config, socket, daemon, rc-file hooks) and
    /// print a human-readable report. Exits non-zero if any check
    /// fails.
    Doctor,
    /// Write the systemd user unit file and, if a systemd user session
    /// is available, enable + start the daemon via it.
    Install,
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
        Commands::Prompt { format } => {
            print_prompt(&format);
        }
        Commands::Status { json } | Commands::Show { json } => {
            if let Err(err) = status(json).await {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        Commands::Watch => {
            if let Err(err) = crate::render::tui::run().await {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        Commands::Init { shell } => {
            // Deliberately just `println!` the raw snippet: the caller
            // does `eval "$(shellagotchi init bash)"`, so stdout must be
            // exactly the shell snippet, nothing else.
            println!("{}", crate::shell::hook_snippet(shell));
        }
        Commands::Doctor => {
            let all_ok = doctor().await;
            if !all_ok {
                std::process::exit(1);
            }
        }
        Commands::Install => {
            if let Err(err) = crate::daemon::install::install() {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
    }
}

/// Runs all diagnostic checks and prints a human-readable report to
/// stdout. Returns `true` if every check passed (or was informational
/// and skipped), `false` if any check failed.
async fn doctor() -> bool {
    let mut all_ok = true;
    let mut check = |label: &str, ok: bool, detail: Option<String>| {
        let marker = if ok { "[OK]" } else { "[FAIL]" };
        match detail {
            Some(detail) => println!("{marker} {label}: {detail}"),
            None => println!("{marker} {label}"),
        }
        if !ok {
            all_ok = false;
        }
    };

    println!("shellagotchi doctor");
    println!("--------------------");

    // 1. Binary location (informational; always knowable).
    match std::env::current_exe() {
        Ok(path) => check(
            "binary",
            true,
            Some(format!("running from {}", path.display())),
        ),
        Err(err) => check("binary", false, Some(format!("{err}"))),
    }

    // 2. Config parses.
    match crate::config::load() {
        Ok(_) => check(
            "config",
            true,
            Some(format!(
                "parsed OK from {}",
                crate::paths::config_file_path().display()
            )),
        ),
        Err(err) => check("config", false, Some(format!("{err}"))),
    }

    // 3. State dir writable.
    match crate::paths::ensure_dirs_exist() {
        Ok(()) => {
            let probe = crate::paths::state_dir().join(".doctor-probe");
            match std::fs::write(&probe, b"probe") {
                Ok(()) => {
                    let _ = std::fs::remove_file(&probe);
                    check("state dir writable", true, None);
                }
                Err(err) => check("state dir writable", false, Some(format!("{err}"))),
            }
        }
        Err(err) => check("state dir writable", false, Some(format!("{err}"))),
    }

    // 4. Socket exists.
    let socket_path = crate::paths::socket_path();
    check(
        "socket exists",
        socket_path.exists(),
        Some(format!("{}", socket_path.display())),
    );

    // 5. Daemon responds to ping.
    let req = Request::new(RequestOp::Ping);
    match send_request(&socket_path, &req).await {
        Ok(response) if response.ok => check("daemon ping", true, None),
        Ok(response) => check(
            "daemon ping",
            false,
            Some(
                response
                    .error
                    .unwrap_or_else(|| "daemon returned ok=false".to_string()),
            ),
        ),
        Err(err) => check("daemon ping", false, Some(format!("{err}"))),
    }

    // 6. Prompt cache fresh (existence + non-empty, per the simplified
    // bar this task's spec allows).
    let cache_path = crate::paths::prompt_cache_path();
    let cache_ok = std::fs::metadata(&cache_path)
        .map(|meta| meta.len() > 0)
        .unwrap_or(false);
    check(
        "prompt cache",
        cache_ok,
        Some(format!("{}", cache_path.display())),
    );

    // 7. Hook markers found in rc files.
    let home = std::env::var("HOME").unwrap_or_default();
    for (label, rc) in [("bashrc hook", ".bashrc"), ("zshrc hook", ".zshrc")] {
        let rc_path = std::path::Path::new(&home).join(rc);
        let found = std::fs::read_to_string(&rc_path)
            .map(|contents| contents.contains("shellagotchi"))
            .unwrap_or(false);
        check(label, found, Some(format!("{}", rc_path.display())));
    }

    // 8. Systemd unit: not yet implemented, informational only.
    println!("[SKIP] systemd unit (not yet implemented, see Task 22)");

    println!("--------------------");
    println!(
        "{}",
        if all_ok {
            "all checks passed"
        } else {
            "one or more checks failed"
        }
    );

    all_ok
}

/// Fetches the pet's current state from the daemon over IPC and prints
/// either a bordered ASCII status card or (with `--json`) the raw
/// `PetState` as pretty JSON.
///
/// Unlike `feed`/`prompt`, this command is explicit/interactive: if the
/// daemon is unreachable it prints a clear, actionable error to stderr
/// and returns an `Err` so the caller exits non-zero, rather than
/// failing silently.
async fn status(json: bool) -> anyhow::Result<()> {
    let socket_path = crate::paths::socket_path();
    let req = Request::new(RequestOp::Status);

    let response = send_request(&socket_path, &req).await.map_err(|err| {
        anyhow::anyhow!(
            "could not reach the shellagotchi daemon ({err}). Is it running? Try `shellagotchi daemon`."
        )
    })?;

    if !response.ok {
        let message = response
            .error
            .unwrap_or_else(|| "daemon returned an error with no message".to_string());
        anyhow::bail!("daemon reported an error: {message}");
    }

    let state = response
        .state
        .ok_or_else(|| anyhow::anyhow!("daemon's status response was missing pet state"))?;

    if json {
        println!("{}", crate::render::card::render_json(&state));
    } else {
        let now = SystemClock.now();
        let no_color = std::env::var("NO_COLOR").is_ok()
            || !std::io::IsTerminal::is_terminal(&std::io::stdout());
        println!(
            "{}",
            crate::render::card::render_card(&state, now, no_color)
        );
    }

    Ok(())
}

/// How long (in seconds) a prompt cache file may go un-refreshed before
/// it's considered stale (e.g. the daemon crashed or was never
/// started). Hardcoded rather than loaded from `Config` on purpose: this
/// is the performance-critical hot path invoked on every shell prompt
/// render, and loading/parsing the config file here would add
/// unnecessary I/O and latency to a path whose entire point is to be as
/// fast as a single file read. Overridable via
/// `SHELLAGOTCHI_STALE_THRESHOLD_SECS` for testability (so tests don't
/// need to sleep 300+ real seconds to exercise the stale path).
const STALE_THRESHOLD_SECS: u64 = 300;

/// Prints ONE line of the requested prompt `format`, reading only the
/// plain-text prompt cache file the daemon maintains. This function
/// deliberately never touches the IPC socket: doing so would reintroduce
/// exactly the connect/timeout latency this command exists to avoid.
///
/// - If the cache file is missing or unreadable (daemon never ran, or a
///   transient I/O error), prints nothing and returns -- matching the
///   silent, always-succeed resilience pattern used by `feed`.
/// - If the cache file's mtime is older than the staleness threshold,
///   prints `?` instead of the (possibly very stale) cached content.
/// - Otherwise prints the cache line matching `format` (unrecognized
///   format strings fall back to `compact`).
fn print_prompt(format: &str) {
    let path = crate::paths::prompt_cache_path();

    let contents = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(_) => return,
    };

    let stale_threshold_secs = std::env::var("SHELLAGOTCHI_STALE_THRESHOLD_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(STALE_THRESHOLD_SECS);

    let is_stale = std::fs::metadata(&path)
        .and_then(|meta| meta.modified())
        .map(
            |modified| match std::time::SystemTime::now().duration_since(modified) {
                Ok(age) => age.as_secs() >= stale_threshold_secs,
                // Clock skew (mtime in the future): treat as fresh.
                Err(_) => false,
            },
        )
        .unwrap_or(false);

    if is_stale {
        println!("?");
        return;
    }

    let lines: Vec<&str> = contents.split('\n').collect();
    let index = match format {
        "minimal" => 1,
        "verbose" => 2,
        _ => 0,
    };
    if let Some(line) = lines.get(index) {
        println!("{line}");
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
