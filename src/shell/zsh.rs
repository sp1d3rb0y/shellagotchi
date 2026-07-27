//! Zsh shell integration hook.
//!
//! Uses zsh's native `preexec`/`precmd` hook points (via the bundled
//! `add-zsh-hook` function) instead of bash's DEBUG-trap emulation --
//! zsh has first-class support for exactly this pattern, so no manual
//! trap wiring is needed. `precmd` runs after the previous command's
//! exit status is available in `$?` but before the prompt is drawn;
//! our hook function captures `$?` first and `return`s it so zsh's own
//! subsequent precmd hooks (and the user's `$?`-reading prompt
//! expansions) see the original value unchanged.

/// Returns the zsh hook snippet, meant to be `eval`'d via
/// `eval "$(shellagotchi init zsh)"` in `~/.zshrc`. When `with_clean_alias`
/// is `true`, an additional `clean()` shell function wrapper (see
/// [`clean_alias_snippet`]) is appended.
pub fn hook_snippet(with_clean_alias: bool) -> String {
    let base = r#"# shellagotchi
__shellagotchi_preexec() { __SHELLAGOTCHI_CMD="$1"; }
__shellagotchi_precmd() {
  local ec=$?
  local argv0="${__SHELLAGOTCHI_CMD%% *}"
  SHELLAGOTCHI_ARGV0="$argv0" shellagotchi feed --exit "$ec" >/dev/null 2>&1
  return $ec
}
autoload -Uz add-zsh-hook
add-zsh-hook preexec __shellagotchi_preexec
add-zsh-hook precmd __shellagotchi_precmd
PS1="\$(shellagotchi prompt) $PS1"
"#
    .to_string();

    if with_clean_alias {
        base + &clean_alias_snippet()
    } else {
        base
    }
}

/// Returns a standalone `clean()` shell function that wraps the real
/// `clean` utility (via `command clean`, bypassing this very function so
/// it can't recurse) and reports the resulting exit code to the daemon via
/// the `SHELLAGOTCHI_ARGV0` env var, the same mechanism the main hook
/// uses. See the bash equivalent (`bash::clean_alias_snippet`) for the
/// full rationale; the zsh function body is identical since both shells
/// share the same function-definition syntax.
pub fn clean_alias_snippet() -> String {
    r#"clean() {
  command clean "$@" 2>/dev/null
  local ec=$?
  SHELLAGOTCHI_ARGV0="clean" shellagotchi feed --exit "$ec" >/dev/null 2>&1
  return $ec
}
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_preserves_exit_code_via_return() {
        let snippet = hook_snippet(false);
        assert!(snippet.contains("local ec=$?"));
        assert!(snippet.contains("return $ec"));
    }

    #[test]
    fn snippet_registers_both_preexec_and_precmd_hooks() {
        let snippet = hook_snippet(false);
        assert!(snippet.contains("add-zsh-hook preexec __shellagotchi_preexec"));
        assert!(snippet.contains("add-zsh-hook precmd __shellagotchi_precmd"));
    }

    #[test]
    fn with_clean_alias_true_includes_clean_function() {
        let with_alias = hook_snippet(true);
        assert!(with_alias.contains("clean()"));
        assert!(with_alias.contains("command clean"));

        let without_alias = hook_snippet(false);
        assert!(!without_alias.contains("command clean"));
    }
}
