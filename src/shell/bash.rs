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
/// `eval "$(shellagotchi init bash)"` in `~/.bashrc`.
pub fn hook_snippet() -> String {
    r#"# shellagotchi
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
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_preserves_exit_code_via_return() {
        let snippet = hook_snippet();
        assert!(snippet.contains("local ec=$?"));
        assert!(snippet.contains("return $ec"));
    }

    #[test]
    fn snippet_prepends_prompt_command_rather_than_clobbering() {
        let snippet = hook_snippet();
        assert!(snippet.contains("${PROMPT_COMMAND:+;$PROMPT_COMMAND}"));
    }
}
