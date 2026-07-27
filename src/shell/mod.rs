//! Shell integration: generates hook snippets for `shellagotchi init <shell>`.
//!
//! Each shell module exposes a `hook_snippet()` function returning the
//! text to be `eval`'d (bash/zsh) or `source`'d (fish) into the user's
//! interactive shell. The critical invariant across all of them is that
//! the hook must preserve `$?`/`$status` for the user's own subsequent
//! prompt/scripts -- shellagotchi must be an invisible passenger, never
//! a mutator of shell state it doesn't own.

pub mod bash;
pub mod fish;
pub mod zsh;

/// A supported shell for `shellagotchi init <shell>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
}

impl std::str::FromStr for Shell {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "bash" => Ok(Shell::Bash),
            "zsh" => Ok(Shell::Zsh),
            "fish" => Ok(Shell::Fish),
            other => Err(format!(
                "unsupported shell: {other} (expected bash, zsh, or fish)"
            )),
        }
    }
}

/// Returns the hook snippet for `shell`.
pub fn hook_snippet(shell: Shell) -> String {
    match shell {
        Shell::Bash => bash::hook_snippet(),
        Shell::Zsh => zsh::hook_snippet(),
        Shell::Fish => fish::hook_snippet(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bash_snippet_contains_prompt_command_wiring() {
        let snippet = bash::hook_snippet();
        assert!(snippet.contains("PROMPT_COMMAND"));
        assert!(snippet.contains("shellagotchi feed"));
        assert!(snippet.contains("shellagotchi prompt"));
    }

    #[test]
    fn zsh_snippet_contains_precmd_wiring() {
        let snippet = zsh::hook_snippet();
        assert!(snippet.contains("add-zsh-hook"));
        assert!(snippet.contains("shellagotchi feed"));
        assert!(snippet.contains("shellagotchi prompt"));
    }

    #[test]
    fn shell_fromstr_rejects_unknown_shell() {
        assert!("powershell".parse::<Shell>().is_err());
    }

    #[test]
    fn shell_fromstr_accepts_known_shells() {
        assert_eq!("bash".parse::<Shell>(), Ok(Shell::Bash));
        assert_eq!("zsh".parse::<Shell>(), Ok(Shell::Zsh));
        assert_eq!("fish".parse::<Shell>(), Ok(Shell::Fish));
    }
}
