//! Bash shell integration hook.
//!
//! Uses a `DEBUG` trap to capture the command about to run (so we know
//! its argv0 for junk-food classification) and `PROMPT_COMMAND` to
//! report its exit status after it finishes. `PROMPT_COMMAND` runs
//! *before* the next prompt is drawn but *after* the previous command's
//! exit code has been captured into `$?` -- our reporter function must
//! capture `$?` as the very first statement and `return` it at the end
//! so `$?` is unchanged from the shell's perspective by the time the
//! rest of any user-defined `PROMPT_COMMAND` chain (or the user's own
//! scripts) observe it.
//!
//! `shellagotchi feed` is called synchronously (not backgrounded): its
//! own IPC client has a hard 100ms timeout (see
//! `daemon::ipc::client::CLIENT_TIMEOUT`) and typically completes in
//! low-single-digit milliseconds, so backgrounding it would only add
//! job-control noise (`[1] Done ...` messages) to interactive bash for
//! no real latency benefit.

/// Returns the bash hook snippet, meant to be `eval`'d via
/// `eval "$(shellagotchi init bash)"` in `~/.bashrc`. When
/// `with_clean_alias` is `true`, an additional `clean()` shell function
/// wrapper (see [`clean_alias_snippet`]) is appended, giving users an
/// explicit opt-in alias on top of the automatic argv0-detection that
/// `shellagotchi feed` already performs unconditionally.
pub fn hook_snippet(with_clean_alias: bool) -> String {
    let base = r#"# shellagotchi
__shellagotchi_preexec() { __SHELLAGOTCHI_CMD="$BASH_COMMAND"; }
__shellagotchi_report() {
  local ec=$?
  local argv0="${__SHELLAGOTCHI_CMD%% *}"
  SHELLAGOTCHI_ARGV0="$argv0" shellagotchi feed --exit "$ec" >/dev/null 2>&1
  return $ec
}
trap '__shellagotchi_preexec' DEBUG
PROMPT_COMMAND="__shellagotchi_report${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
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
/// the `SHELLAGOTCHI_ARGV0` env var -- the same mechanism the main hook
/// uses -- so `apply_clean`'s exit-code-gated happiness bonus behaves
/// identically whether `clean` was run directly or through this alias.
///
/// This is entirely optional sugar: the automatic argv0 detection in
/// `shellagotchi feed` already cleans the pet whenever a command named
/// `clean` runs, with zero setup. This wrapper exists only for users who
/// want the happiness-bonus-relevant exit code to reflect their real
/// `clean` utility's outcome explicitly and predictably.
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
    fn snippet_prepends_prompt_command_rather_than_clobbering() {
        let snippet = hook_snippet(false);
        assert!(snippet.contains("${PROMPT_COMMAND:+;$PROMPT_COMMAND}"));
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
