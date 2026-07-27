//! Live interactive `watch` TUI, built on `ratatui`/`crossterm`.
//!
//! [`draw`] is a pure render function: given a `Frame`, a `PetState`, and
//! an explicit `now` (never read internally via `chrono::Utc::now()`,
//! per the crate-wide no-internal-clock-read rule), it deterministically
//! paints the sprite, mood, and stat gauges. This is the function unit
//! tested below via `ratatui::backend::TestBackend`.
//!
//! [`run`] is the real terminal event loop: it sets up raw mode + the
//! alternate screen, polls the daemon over IPC on an interval, and
//! handles keybinds. Per the plan's guidance, this is not practically
//! unit-testable (animation timing, real terminal state) and is instead
//! smoke-tested manually.

use std::io::Stdout;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};
use ratatui::{Frame, Terminal};

use crate::clock::{Clock, SystemClock};
use crate::daemon::ipc::client::send_request;
use crate::daemon::ipc::protocol::{Request, RequestOp};
use crate::pet::state::PetState;

/// How often the live watch loop polls the daemon for fresh state.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Pure render function: draws the current `state` into `frame`, as of
/// `now`. Deterministic given its inputs -- this is what makes it
/// unit-testable via `TestBackend` without a real terminal.
pub fn draw(frame: &mut Frame, state: &PetState, now: chrono::DateTime<chrono::Utc>) {
    let mood = crate::pet::mood::derive_mood(state);
    let sprite = crate::render::sprites::sprite_for(state.species, state.activity, mood);
    let age = now.signed_duration_since(state.born_at);

    let area = frame.area();

    // Defensive minimums: on a tiny terminal, fixed-length chunks can
    // exceed the available area. Clamp each section's requested height
    // to what's actually left so `Layout::split` never panics or
    // silently drops widgets in a way that would confuse a real user
    // (ratatui itself won't panic on overflow, but we still want
    // sensible degradation rather than requesting more than exists).
    let header_len = area.height.min(8);
    let remaining_after_header = area.height.saturating_sub(header_len);
    let gauges_len = remaining_after_header.min(6);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_len),
            Constraint::Length(gauges_len),
            Constraint::Min(0),
        ])
        .split(area);

    let header_text = format!(
        "{sprite}\n{} -- {:?} -- mood: {mood:?} -- {}m old",
        state.name,
        state.activity,
        age.num_minutes().max(0)
    );
    let header = Paragraph::new(header_text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("shellagotchi: {}", state.name)),
    );
    frame.render_widget(header, chunks[0]);

    render_gauges(frame, state, chunks[1]);

    let footer = Paragraph::new("q: quit  c: clean  p: pet  r: refresh")
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, chunks[2]);
}

/// Renders one single-line gauge per tracked stat, stacked vertically
/// within `area`. Splits `area` into as many equal rows as there are
/// stats, so it degrades gracefully (rows just get squeezed) rather than
/// panicking on a very short `area`.
fn render_gauges(frame: &mut Frame, state: &PetState, area: Rect) {
    let stats: [(&str, u8); 5] = [
        ("satiety", state.satiety.get()),
        ("happiness", state.happiness.get()),
        ("energy", state.energy.get()),
        ("hygiene", state.hygiene.get()),
        ("health", state.health.get()),
    ];

    let constraints: Vec<Constraint> = stats.iter().map(|_| Constraint::Length(1)).collect();
    let gauge_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    for (i, (label, value)) in stats.iter().enumerate() {
        let Some(chunk) = gauge_chunks.get(i) else {
            // Area too small to fit this row at all -- nothing to draw.
            break;
        };
        let gauge = Gauge::default()
            .block(Block::default().title(*label))
            .percent(u16::from(*value))
            .label(format!("{value}%"));
        frame.render_widget(gauge, *chunk);
    }
}

/// RAII guard that restores the terminal to its normal (non-raw,
/// primary-screen) state on drop, regardless of how the enclosing scope
/// exits (normal return, `?`-propagated error, or panic during unwind).
/// This is the robust way to guarantee cleanup: an explicit cleanup call
/// at the end of `run` would be skipped by any early return.
struct TerminalGuard;

impl TerminalGuard {
    fn enter(stdout: &mut Stdout) -> anyhow::Result<Self> {
        crossterm::terminal::enable_raw_mode()?;
        execute!(stdout, EnterAlternateScreen)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Best-effort: these can only fail if the terminal state was
        // already inconsistent, and there's nothing more we can do about
        // it from a `Drop` impl (no `?` available here).
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
    }
}

/// Runs the live interactive `watch` TUI: polls the daemon over IPC every
/// [`POLL_INTERVAL`], redraws via [`draw`], and handles keybinds:
/// - `q` -- quit
/// - `c` -- send a `clean` request to the daemon
/// - `p` -- send a `pet` request to the daemon
/// - `r` -- force an immediate re-poll/redraw
///
/// Terminal state (raw mode, alternate screen) is restored on every exit
/// path via [`TerminalGuard`]'s `Drop` impl, including early returns from
/// IPC errors and panics during unwind.
pub async fn run() -> anyhow::Result<()> {
    let socket_path = crate::paths::socket_path();

    let mut stdout = std::io::stdout();
    let _guard = TerminalGuard::enter(&mut stdout)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = fetch_status(&socket_path).await?;

    loop {
        let now = SystemClock.now();
        terminal.draw(|frame| draw(frame, &state, now))?;

        let mut should_refresh = false;
        let mut should_quit = false;

        tokio::select! {
            _ = tokio::time::sleep(POLL_INTERVAL) => {
                should_refresh = true;
            }
            key = poll_key_event() => {
                match key? {
                    Some(KeyCode::Char('q')) => should_quit = true,
                    Some(KeyCode::Char('c')) => {
                        let _ = send_request(&socket_path, &Request::new(RequestOp::Clean)).await;
                        should_refresh = true;
                    }
                    Some(KeyCode::Char('p')) => {
                        let _ = send_request(&socket_path, &Request::new(RequestOp::Pet)).await;
                        should_refresh = true;
                    }
                    Some(KeyCode::Char('r')) => {
                        should_refresh = true;
                    }
                    _ => {}
                }
            }
        }

        if should_quit {
            break;
        }
        if should_refresh && let Ok(fresh) = fetch_status(&socket_path).await {
            state = fresh;
        }
    }

    Ok(())
}

/// Polls for a single crossterm key-press event without blocking the
/// async runtime for long: `event::poll` with a short timeout is used
/// inside a loop so this future yields promptly if nothing is pending,
/// letting it race cleanly against the poll-interval sleep in `run`'s
/// `tokio::select!`.
async fn poll_key_event() -> anyhow::Result<Option<KeyCode>> {
    loop {
        if event::poll(Duration::from_millis(20))? {
            if let Event::Key(key_event) = event::read()? {
                return Ok(Some(key_event.code));
            }
            // Non-key event (resize, mouse, etc.) -- keep waiting.
            continue;
        }
        // Nothing pending right now; yield back to the executor so this
        // future doesn't monopolize the task and the outer `select!`'s
        // sleep branch gets a fair chance to win.
        tokio::task::yield_now().await;
    }
}

/// Fetches the pet's current state from the daemon via a `status` IPC
/// request.
async fn fetch_status(socket_path: &std::path::Path) -> anyhow::Result<PetState> {
    let response = send_request(socket_path, &Request::new(RequestOp::Status)).await?;
    if !response.ok {
        anyhow::bail!(
            "daemon reported an error: {}",
            response.error.unwrap_or_default()
        );
    }
    response
        .state
        .ok_or_else(|| anyhow::anyhow!("daemon's status response was missing pet state"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pet::state::Species;
    use chrono::TimeZone;
    use ratatui::backend::TestBackend;

    fn fixed_now() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()
    }

    #[test]
    fn draw_produces_correct_buffer_size() {
        let now = fixed_now();
        let state = PetState::newborn("Rusty".into(), Species::Blob, now);
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|f| draw(f, &state, now)).unwrap();

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer.area, Rect::new(0, 0, 80, 24));
    }

    #[test]
    fn draw_includes_pet_name_in_output() {
        let now = fixed_now();
        let state = PetState::newborn("Rusty".into(), Species::Blob, now);
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|f| draw(f, &state, now)).unwrap();

        let buffer = terminal.backend().buffer();
        let text: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(
            text.contains("Rusty"),
            "expected buffer text to contain 'Rusty':\n{text}"
        );
    }

    #[test]
    fn draw_does_not_panic_on_tiny_terminal() {
        let now = fixed_now();
        let state = PetState::newborn("Rusty".into(), Species::Blob, now);
        let backend = TestBackend::new(10, 5);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| draw(f, &state, now))
            .expect("draw must not panic/error on a tiny terminal");
    }

    #[test]
    fn draw_reflects_different_moods_differently() {
        use crate::pet::stats::Stat;

        let now = fixed_now();
        let mut happy_state = PetState::newborn("Rusty".into(), Species::Blob, now);
        happy_state.happiness = Stat::new(95);
        let mut sad_state = PetState::newborn("Rusty".into(), Species::Blob, now);
        sad_state.happiness = Stat::new(5);

        let happy_backend = TestBackend::new(80, 24);
        let mut happy_terminal = Terminal::new(happy_backend).unwrap();
        happy_terminal.draw(|f| draw(f, &happy_state, now)).unwrap();
        let happy_text: String = happy_terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();

        let sad_backend = TestBackend::new(80, 24);
        let mut sad_terminal = Terminal::new(sad_backend).unwrap();
        sad_terminal.draw(|f| draw(f, &sad_state, now)).unwrap();
        let sad_text: String = sad_terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();

        assert_ne!(
            happy_text, sad_text,
            "expected rendered output to differ between very different moods"
        );
    }
}
