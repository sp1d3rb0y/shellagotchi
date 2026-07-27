//! Fish shell integration hook.
//!
//! Fish has no exact equivalent of bash/zsh's ability to *prepend* a
//! string onto `PS1`/precmd chain: prompts in fish are defined by
//! overriding the `fish_prompt` function wholesale. This means the
//! snippet below is destructive: it REPLACES the user's existing
//! `fish_prompt` entirely rather than wrapping it, unlike the
//! bash/zsh integrations which compose cleanly with whatever the user
//! already had.
//!
//! This is a known, documented limitation of this initial version. A
//! more sophisticated fish integration (saving off the existing
//! `fish_prompt` function definition and calling it from within our
//! replacement) is future work, intentionally out of scope here.
//!
//! Exit-status reporting is handled via fish's `fish_postexec` event,
//! which fires after a command finishes and receives `$status` already
//! captured for us (fish preserves `$status` naturally here since we
//! never re-run a command that would clobber it -- there is no `return`
//! trick needed the way there is in bash/zsh, because `fish_postexec`
//! is an event handler, not something inserted into the exit-code
//! propagation path itself).

/// Returns the fish hook snippet, meant to be sourced via
/// `shellagotchi init fish | source` in `~/.config/fish/config.fish`.
///
/// `with_clean_alias` is accepted for signature parity with the
/// bash/zsh variants but is currently ignored: fish's function-definition
/// and `command`-builtin semantics differ enough from bash/zsh that a
/// faithful `clean()` wrapper is out of scope for this task. Fish users
/// still get the automatic argv0-detection cleanup in `shellagotchi feed`
/// with zero extra setup -- they just don't get this optional explicit
/// alias flavor. If `with_clean_alias` is requested for fish, callers
/// should warn the user rather than silently no-op (see `main.rs`'s
/// `Init` handler).
pub fn hook_snippet(_with_clean_alias: bool) -> String {
    r#"# shellagotchi
function __shellagotchi_postexec --on-event fish_postexec
    set -l ec $status
    set -l argv0 (string split ' ' -- $argv[1])[1]
    env SHELLAGOTCHI_ARGV0="$argv0" shellagotchi feed --exit $ec >/dev/null 2>&1
end
function fish_prompt
    # NOTE: this naive override REPLACES the user's existing fish_prompt
    # entirely, which is destructive -- fish doesn't have as clean a
    # prepend mechanism as bash/zsh PS1 string concatenation. A more
    # sophisticated fish integration (wrapping the EXISTING fish_prompt
    # function rather than replacing it) is future work, out of scope
    # for this task's initial version.
    printf '%s ' (shellagotchi prompt)
end
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_wires_postexec_and_prompt() {
        let snippet = hook_snippet(false);
        assert!(snippet.contains("fish_postexec"));
        assert!(snippet.contains("shellagotchi feed"));
        assert!(snippet.contains("shellagotchi prompt"));
    }
}
