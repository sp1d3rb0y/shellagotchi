//! Renders a bordered ASCII status card for a [`PetState`], plus a
//! machine-readable JSON mode.
//!
//! `render_card` is pure (no I/O, no time reads): `now` is passed in
//! explicitly by the caller (which obtains it via [`crate::clock::Clock`]),
//! never read internally via `chrono::Utc::now()` (banned project-wide by
//! `clippy.toml`).

use chrono::{DateTime, Duration, Utc};

use crate::pet::mood::derive_mood;
use crate::pet::state::PetState;
use crate::render::sprites::sprite_for;

/// Renders a bordered ASCII status card for `state` as of `now`.
///
/// `no_color` currently has no effect (this implementation emits plain
/// text only, no ANSI escapes) but is accepted now to lock in the API
/// shape: a later task can add real color output gated on this flag
/// without changing the function signature.
pub fn render_card(state: &PetState, now: DateTime<Utc>, _no_color: bool) -> String {
    let mood = derive_mood(state);
    let sprite = sprite_for(state.species, state.activity, mood);
    let age = format_duration(now - state.born_at);

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("{} ({:?})", state.name, state.species));
    lines.push(format!(
        "{age} old  |  {:?}  |  mood: {mood:?}",
        state.activity
    ));
    lines.push(String::new());
    for sprite_line in sprite.lines() {
        lines.push(sprite_line.to_string());
    }
    lines.push(String::new());
    lines.push(format!("satiety:   {}", bar(state.satiety.get())));
    lines.push(format!("happiness: {}", bar(state.happiness.get())));
    lines.push(format!("energy:    {}", bar(state.energy.get())));
    lines.push(format!("hygiene:   {}", bar(state.hygiene.get())));
    lines.push(format!("health:    {}", bar(state.health.get())));
    lines.push(format!("boredom:   {}", bar(state.boredom.get())));
    lines.push(format!("poops: {}", state.poops.len()));

    let width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let border = format!("+{}+", "-".repeat(width + 2));

    let mut out = String::new();
    out.push_str(&border);
    out.push('\n');
    for line in &lines {
        out.push_str(&format!("| {line:<width$} |\n"));
    }
    out.push_str(&border);
    out
}

/// Renders `state` as pretty-printed JSON. `PetState` already derives
/// `Serialize`, so this is a thin wrapper for the `--json` CLI flag.
pub fn render_json(state: &PetState) -> String {
    serde_json::to_string_pretty(state).expect("PetState serialization is infallible")
}

/// Renders a 10-character text bar plus percentage, e.g. `[########..] 82%`.
/// `value` is a 0..=100 stat; the number of filled `#` cells is
/// `round(value / 10)`.
fn bar(value: u8) -> String {
    let filled = ((value as f64 / 10.0).round() as usize).min(10);
    let empty = 10 - filled;
    format!("[{}{}] {value}%", "#".repeat(filled), ".".repeat(empty))
}

/// Formats a duration as a short human string like `2h 15m old`-style
/// (without the trailing "old"; callers append that themselves).
/// Falls back to `<1m` for durations under a minute, and includes days
/// when the duration spans 24h or more.
fn format_duration(d: Duration) -> String {
    let total_minutes = d.num_minutes().max(0);
    let days = total_minutes / (24 * 60);
    let hours = (total_minutes % (24 * 60)) / 60;
    let minutes = total_minutes % 60;

    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m")
    } else {
        "<1m".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pet::state::Species;
    use chrono::TimeZone;

    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()
    }

    #[test]
    fn render_card_contains_key_fields() {
        let start = fixed_now();
        let state = PetState::newborn("Rusty".into(), Species::Blob, start);
        let card = render_card(&state, start + Duration::hours(2), false);
        assert!(
            card.contains("Rusty"),
            "card should contain the pet's name:\n{card}"
        );
        assert!(
            card.contains("70"),
            "card should contain a stat value (70):\n{card}"
        );
    }

    #[test]
    fn render_json_is_valid_json_with_expected_fields() {
        let start = fixed_now();
        let state = PetState::newborn("Rusty".into(), Species::Blob, start);
        let json = render_json(&state);
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("render_json output must be valid JSON");
        assert_eq!(value["schema_version"], 1);
    }

    #[test]
    fn format_duration_covers_units() {
        assert_eq!(format_duration(Duration::seconds(30)), "<1m");
        assert_eq!(format_duration(Duration::minutes(15)), "15m");
        assert_eq!(
            format_duration(Duration::hours(2) + Duration::minutes(15)),
            "2h 15m"
        );
        assert_eq!(
            format_duration(Duration::days(1) + Duration::hours(3)),
            "1d 3h 0m"
        );
    }
}
