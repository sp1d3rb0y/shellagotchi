//! Renders a [`PetState`] into short, PS1-embeddable text.
//!
//! This module is pure (no I/O, no time reads): given a `PetState` it
//! deterministically produces text. The daemon calls
//! [`render_all_formats`] after every state mutation and writes the
//! result verbatim to the prompt cache file (see
//! `crate::paths::prompt_cache_path`); the socket-free `prompt` CLI
//! subcommand reads that file directly and never touches this module or
//! the IPC socket, which is what keeps it fast enough for a shell PS1.
//!
//! Output is plain ASCII by default. Unicode glyphs (behind a
//! `config.unicode` gate) are deferred to a later task -- don't add
//! non-ASCII characters here without that gate, since it would silently
//! break `NO_COLOR`/non-unicode terminals.

use crate::pet::mood::{Mood, derive_mood};
use crate::pet::state::{PetState, Species};

/// Renders all three prompt formats for `state`, newline-separated, in a
/// fixed order: compact, minimal, verbose. This is the exact content
/// written to the prompt cache file -- the `prompt` CLI subcommand picks
/// one line based on the requested format without needing to re-derive
/// anything.
pub fn render_all_formats(state: &PetState) -> String {
    format!(
        "{}\n{}\n{}",
        render_compact(state),
        render_minimal(state),
        render_verbose(state)
    )
}

fn mood_glyph(mood: Mood) -> &'static str {
    match mood {
        Mood::Ecstatic => "^o^",
        Mood::Happy => "^_^",
        Mood::Content => "-_-",
        Mood::Meh => ":_:",
        Mood::Sad => ";_;",
        Mood::Miserable => "T_T",
        Mood::Sick => "x_x",
        Mood::Dead => "+_+",
    }
}

fn species_glyph(species: Species) -> &'static str {
    match species {
        Species::Blob => "o",
        Species::Cat => "=^.^=",
        Species::Dragon => "~<>~",
        Species::Ghost => "~o~",
    }
}

/// Short single-line summary: species glyph, mood glyph, happiness %.
/// Intended as the default `PS1` segment.
pub fn render_compact(state: &PetState) -> String {
    let mood = derive_mood(state);
    format!(
        "{} {} {}%",
        species_glyph(state.species),
        mood_glyph(mood),
        state.happiness.get()
    )
}

/// The absolute minimum: just the mood glyph. For users who want the
/// smallest possible PS1 footprint.
pub fn render_minimal(state: &PetState) -> String {
    mood_glyph(derive_mood(state)).to_string()
}

/// A fuller line with all core stats, for users who want more detail
/// (e.g. a right-side prompt or a status line rather than the main PS1).
pub fn render_verbose(state: &PetState) -> String {
    let mood = derive_mood(state);
    format!(
        "{} health:{} satiety:{} happy:{} energy:{} hygiene:{} mood:{:?}",
        species_glyph(state.species),
        state.health.get(),
        state.satiety.get(),
        state.happiness.get(),
        state.energy.get(),
        state.hygiene.get(),
        mood
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pet::stats::Stat;
    use chrono::{DateTime, TimeZone, Utc};

    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()
    }

    #[test]
    fn compact_verbose_minimal_all_differ() {
        let state = PetState::newborn("Rusty".into(), Species::Blob, fixed_now());

        let compact = render_compact(&state);
        let minimal = render_minimal(&state);
        let verbose = render_verbose(&state);

        assert!(!compact.is_empty());
        assert!(!minimal.is_empty());
        assert!(!verbose.is_empty());
        assert_ne!(compact, minimal);
        assert_ne!(compact, verbose);
        assert_ne!(minimal, verbose);
    }

    #[test]
    fn happy_and_sad_moods_render_different_glyphs() {
        let mut happy = PetState::newborn("Rusty".into(), Species::Blob, fixed_now());
        happy.happiness = Stat::new(95);

        let mut sad = PetState::newborn("Rusty".into(), Species::Blob, fixed_now());
        sad.happiness = Stat::new(5);

        assert_ne!(render_minimal(&happy), render_minimal(&sad));
    }

    #[test]
    fn output_is_ascii_by_default() {
        let state = PetState::newborn("Rusty".into(), Species::Dragon, fixed_now());
        assert!(render_compact(&state).is_ascii());
        assert!(render_minimal(&state).is_ascii());
        assert!(render_verbose(&state).is_ascii());
    }

    #[test]
    fn render_all_formats_produces_exactly_three_lines_in_order() {
        let state = PetState::newborn("Rusty".into(), Species::Cat, fixed_now());
        let all = render_all_formats(&state);
        let lines: Vec<&str> = all.split('\n').collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], render_compact(&state));
        assert_eq!(lines[1], render_minimal(&state));
        assert_eq!(lines[2], render_verbose(&state));
    }
}
