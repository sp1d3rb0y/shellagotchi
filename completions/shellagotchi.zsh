#compdef shellagotchi

autoload -U is-at-least

_shellagotchi() {
    typeset -A opt_args
    typeset -a _arguments_options
    local ret=1

    if is-at-least 5.2; then
        _arguments_options=(-s -S -C)
    else
        _arguments_options=(-s -C)
    fi

    local context curcontext="$curcontext" state line
    _arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
":: :_shellagotchi_commands" \
"*::: :->shellagotchi" \
&& ret=0
    case $state in
    (shellagotchi)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:shellagotchi-command-$line[1]:"
        case $line[1] in
            (feed)
_arguments "${_arguments_options[@]}" : \
'--exit=[]:EXIT:_default' \
'--duration=[]:DURATION:_default' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(daemon)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(prompt)
_arguments "${_arguments_options[@]}" : \
'--format=[]:FORMAT:_default' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(status)
_arguments "${_arguments_options[@]}" : \
'--json[]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(show)
_arguments "${_arguments_options[@]}" : \
'--json[]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(watch)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(init)
_arguments "${_arguments_options[@]}" : \
'--with-clean-alias[Also emit an explicit \`clean()\` shell function wrapper (bash/zsh only) around the real \`clean\` utility, reporting its exit code to the daemon. Purely optional sugar\: the automatic argv0-detection cleanup in \`shellagotchi feed\` already cleans the pet whenever a command named \`clean\` runs, with zero extra setup. Ignored (with a warning) for \`fish\`]' \
'-h[Print help]' \
'--help[Print help]' \
':shell:(bash zsh fish)' \
&& ret=0
;;
(doctor)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(install)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(gen-man)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(gen-completions)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
':shell:(bash elvish fish powershell zsh)' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_shellagotchi__subcmd__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:shellagotchi-help-command-$line[1]:"
        case $line[1] in
            (feed)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(daemon)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(prompt)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(status)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(show)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(watch)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(init)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(doctor)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(install)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(gen-man)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(gen-completions)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
}

(( $+functions[_shellagotchi_commands] )) ||
_shellagotchi_commands() {
    local commands; commands=(
'feed:Report a command'\''s exit status to the daemon (used by the shell hook)' \
'daemon:Run the shellagotchi daemon (the process the shell hook and CLI subcommands talk to over a Unix socket)' \
'prompt:Print a rendered prompt segment, reading ONLY the prompt cache file the daemon maintains (never the IPC socket). This makes it safe and fast enough to embed directly in a shell \`PS1\`' \
'status:Show the pet'\''s current status as a bordered ASCII card, fetched live from the daemon over IPC' \
'show:Alias for \`status\` (the plan treats \`show\` and \`status\` as the same command)' \
'watch:Launch a live, interactively-updating terminal UI showing the pet'\''s sprite, mood, and stat gauges, polling the daemon in the background. Keybinds\: q=quit, c=clean, p=pet, r=refresh' \
'init:Print the shell integration snippet for \`shell\` to stdout, meant to be evaluated/sourced directly into an rc file, e.g. \`eval "\$(shellagotchi init bash)"\` in \`~/.bashrc\`. Prints *exactly* the snippet with no extra logging, since the caller feeds stdout straight into \`eval\`/\`source\`' \
'doctor:Run diagnostic checks (config, socket, daemon, rc-file hooks) and print a human-readable report. Exits non-zero if any check fails' \
'install:Write the systemd user unit file and, if a systemd user session is available, enable + start the daemon via it' \
'gen-man:Generate the man page (troff, \`clap_mangen\`) to stdout. Hidden\: this is a packaging-time helper (\`shellagotchi gen-man > man/shellagotchi.1\`), not a user-facing feature' \
'gen-completions:Generate a shell completion script for \`shell\` (\`clap_complete\`) to stdout. Hidden\: this is a packaging-time helper (\`shellagotchi gen-completions bash > completions/shellagotchi.bash\`), not a user-facing feature' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'shellagotchi commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__daemon_commands] )) ||
_shellagotchi__subcmd__daemon_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi daemon commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__doctor_commands] )) ||
_shellagotchi__subcmd__doctor_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi doctor commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__feed_commands] )) ||
_shellagotchi__subcmd__feed_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi feed commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__gen-completions_commands] )) ||
_shellagotchi__subcmd__gen-completions_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi gen-completions commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__gen-man_commands] )) ||
_shellagotchi__subcmd__gen-man_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi gen-man commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__help_commands] )) ||
_shellagotchi__subcmd__help_commands() {
    local commands; commands=(
'feed:Report a command'\''s exit status to the daemon (used by the shell hook)' \
'daemon:Run the shellagotchi daemon (the process the shell hook and CLI subcommands talk to over a Unix socket)' \
'prompt:Print a rendered prompt segment, reading ONLY the prompt cache file the daemon maintains (never the IPC socket). This makes it safe and fast enough to embed directly in a shell \`PS1\`' \
'status:Show the pet'\''s current status as a bordered ASCII card, fetched live from the daemon over IPC' \
'show:Alias for \`status\` (the plan treats \`show\` and \`status\` as the same command)' \
'watch:Launch a live, interactively-updating terminal UI showing the pet'\''s sprite, mood, and stat gauges, polling the daemon in the background. Keybinds\: q=quit, c=clean, p=pet, r=refresh' \
'init:Print the shell integration snippet for \`shell\` to stdout, meant to be evaluated/sourced directly into an rc file, e.g. \`eval "\$(shellagotchi init bash)"\` in \`~/.bashrc\`. Prints *exactly* the snippet with no extra logging, since the caller feeds stdout straight into \`eval\`/\`source\`' \
'doctor:Run diagnostic checks (config, socket, daemon, rc-file hooks) and print a human-readable report. Exits non-zero if any check fails' \
'install:Write the systemd user unit file and, if a systemd user session is available, enable + start the daemon via it' \
'gen-man:Generate the man page (troff, \`clap_mangen\`) to stdout. Hidden\: this is a packaging-time helper (\`shellagotchi gen-man > man/shellagotchi.1\`), not a user-facing feature' \
'gen-completions:Generate a shell completion script for \`shell\` (\`clap_complete\`) to stdout. Hidden\: this is a packaging-time helper (\`shellagotchi gen-completions bash > completions/shellagotchi.bash\`), not a user-facing feature' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'shellagotchi help commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__help__subcmd__daemon_commands] )) ||
_shellagotchi__subcmd__help__subcmd__daemon_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi help daemon commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__help__subcmd__doctor_commands] )) ||
_shellagotchi__subcmd__help__subcmd__doctor_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi help doctor commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__help__subcmd__feed_commands] )) ||
_shellagotchi__subcmd__help__subcmd__feed_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi help feed commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__help__subcmd__gen-completions_commands] )) ||
_shellagotchi__subcmd__help__subcmd__gen-completions_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi help gen-completions commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__help__subcmd__gen-man_commands] )) ||
_shellagotchi__subcmd__help__subcmd__gen-man_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi help gen-man commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__help__subcmd__help_commands] )) ||
_shellagotchi__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi help help commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__help__subcmd__init_commands] )) ||
_shellagotchi__subcmd__help__subcmd__init_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi help init commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__help__subcmd__install_commands] )) ||
_shellagotchi__subcmd__help__subcmd__install_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi help install commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__help__subcmd__prompt_commands] )) ||
_shellagotchi__subcmd__help__subcmd__prompt_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi help prompt commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__help__subcmd__show_commands] )) ||
_shellagotchi__subcmd__help__subcmd__show_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi help show commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__help__subcmd__status_commands] )) ||
_shellagotchi__subcmd__help__subcmd__status_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi help status commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__help__subcmd__watch_commands] )) ||
_shellagotchi__subcmd__help__subcmd__watch_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi help watch commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__init_commands] )) ||
_shellagotchi__subcmd__init_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi init commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__install_commands] )) ||
_shellagotchi__subcmd__install_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi install commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__prompt_commands] )) ||
_shellagotchi__subcmd__prompt_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi prompt commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__show_commands] )) ||
_shellagotchi__subcmd__show_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi show commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__status_commands] )) ||
_shellagotchi__subcmd__status_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi status commands' commands "$@"
}
(( $+functions[_shellagotchi__subcmd__watch_commands] )) ||
_shellagotchi__subcmd__watch_commands() {
    local commands; commands=()
    _describe -t commands 'shellagotchi watch commands' commands "$@"
}

if [ "$funcstack[1]" = "_shellagotchi" ]; then
    _shellagotchi "$@"
else
    compdef _shellagotchi shellagotchi
fi
