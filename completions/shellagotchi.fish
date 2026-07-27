# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_shellagotchi_global_optspecs
    string join \n h/help
end

function __fish_shellagotchi_needs_command
    # Figure out if the current invocation already has a command.
    set -l cmd (commandline -opc)
    set -e cmd[1]
    argparse -s (__fish_shellagotchi_global_optspecs) -- $cmd 2>/dev/null
    or return
    if set -q argv[1]
        # Also print the command, so this can be used to figure out what it is.
        echo $argv[1]
        return 1
    end
    return 0
end

function __fish_shellagotchi_using_subcommand
    set -l cmd (__fish_shellagotchi_needs_command)
    test -z "$cmd"
    and return 1
    contains -- $cmd[1] $argv
end

complete -c shellagotchi -n "__fish_shellagotchi_needs_command" -s h -l help -d 'Print help'
complete -c shellagotchi -n "__fish_shellagotchi_needs_command" -f -a "feed" -d 'Report a command\'s exit status to the daemon (used by the shell hook)'
complete -c shellagotchi -n "__fish_shellagotchi_needs_command" -f -a "daemon" -d 'Run the shellagotchi daemon (the process the shell hook and CLI subcommands talk to over a Unix socket)'
complete -c shellagotchi -n "__fish_shellagotchi_needs_command" -f -a "prompt" -d 'Print a rendered prompt segment, reading ONLY the prompt cache file the daemon maintains (never the IPC socket). This makes it safe and fast enough to embed directly in a shell `PS1`'
complete -c shellagotchi -n "__fish_shellagotchi_needs_command" -f -a "status" -d 'Show the pet\'s current status as a bordered ASCII card, fetched live from the daemon over IPC'
complete -c shellagotchi -n "__fish_shellagotchi_needs_command" -f -a "show" -d 'Alias for `status` (the plan treats `show` and `status` as the same command)'
complete -c shellagotchi -n "__fish_shellagotchi_needs_command" -f -a "watch" -d 'Launch a live, interactively-updating terminal UI showing the pet\'s sprite, mood, and stat gauges, polling the daemon in the background. Keybinds: q=quit, c=clean, p=pet, r=refresh'
complete -c shellagotchi -n "__fish_shellagotchi_needs_command" -f -a "init" -d 'Print the shell integration snippet for `shell` to stdout, meant to be evaluated/sourced directly into an rc file, e.g. `eval "$(shellagotchi init bash)"` in `~/.bashrc`. Prints *exactly* the snippet with no extra logging, since the caller feeds stdout straight into `eval`/`source`'
complete -c shellagotchi -n "__fish_shellagotchi_needs_command" -f -a "doctor" -d 'Run diagnostic checks (config, socket, daemon, rc-file hooks) and print a human-readable report. Exits non-zero if any check fails'
complete -c shellagotchi -n "__fish_shellagotchi_needs_command" -f -a "install" -d 'Write the systemd user unit file and, if a systemd user session is available, enable + start the daemon via it'
complete -c shellagotchi -n "__fish_shellagotchi_needs_command" -f -a "gen-man" -d 'Generate the man page (troff, `clap_mangen`) to stdout. Hidden: this is a packaging-time helper (`shellagotchi gen-man > man/shellagotchi.1`), not a user-facing feature'
complete -c shellagotchi -n "__fish_shellagotchi_needs_command" -f -a "gen-completions" -d 'Generate a shell completion script for `shell` (`clap_complete`) to stdout. Hidden: this is a packaging-time helper (`shellagotchi gen-completions bash > completions/shellagotchi.bash`), not a user-facing feature'
complete -c shellagotchi -n "__fish_shellagotchi_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand feed" -l exit -r
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand feed" -l duration -r
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand feed" -s h -l help -d 'Print help'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand daemon" -s h -l help -d 'Print help'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand prompt" -l format -r
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand prompt" -s h -l help -d 'Print help'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand status" -l json
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand status" -s h -l help -d 'Print help'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand show" -l json
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand show" -s h -l help -d 'Print help'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand watch" -s h -l help -d 'Print help'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand init" -l with-clean-alias -d 'Also emit an explicit `clean()` shell function wrapper (bash/zsh only) around the real `clean` utility, reporting its exit code to the daemon. Purely optional sugar: the automatic argv0-detection cleanup in `shellagotchi feed` already cleans the pet whenever a command named `clean` runs, with zero extra setup. Ignored (with a warning) for `fish`'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand init" -s h -l help -d 'Print help'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand doctor" -s h -l help -d 'Print help'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand install" -s h -l help -d 'Print help'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand gen-man" -s h -l help -d 'Print help'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand gen-completions" -s h -l help -d 'Print help'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand help; and not __fish_seen_subcommand_from feed daemon prompt status show watch init doctor install gen-man gen-completions help" -f -a "feed" -d 'Report a command\'s exit status to the daemon (used by the shell hook)'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand help; and not __fish_seen_subcommand_from feed daemon prompt status show watch init doctor install gen-man gen-completions help" -f -a "daemon" -d 'Run the shellagotchi daemon (the process the shell hook and CLI subcommands talk to over a Unix socket)'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand help; and not __fish_seen_subcommand_from feed daemon prompt status show watch init doctor install gen-man gen-completions help" -f -a "prompt" -d 'Print a rendered prompt segment, reading ONLY the prompt cache file the daemon maintains (never the IPC socket). This makes it safe and fast enough to embed directly in a shell `PS1`'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand help; and not __fish_seen_subcommand_from feed daemon prompt status show watch init doctor install gen-man gen-completions help" -f -a "status" -d 'Show the pet\'s current status as a bordered ASCII card, fetched live from the daemon over IPC'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand help; and not __fish_seen_subcommand_from feed daemon prompt status show watch init doctor install gen-man gen-completions help" -f -a "show" -d 'Alias for `status` (the plan treats `show` and `status` as the same command)'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand help; and not __fish_seen_subcommand_from feed daemon prompt status show watch init doctor install gen-man gen-completions help" -f -a "watch" -d 'Launch a live, interactively-updating terminal UI showing the pet\'s sprite, mood, and stat gauges, polling the daemon in the background. Keybinds: q=quit, c=clean, p=pet, r=refresh'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand help; and not __fish_seen_subcommand_from feed daemon prompt status show watch init doctor install gen-man gen-completions help" -f -a "init" -d 'Print the shell integration snippet for `shell` to stdout, meant to be evaluated/sourced directly into an rc file, e.g. `eval "$(shellagotchi init bash)"` in `~/.bashrc`. Prints *exactly* the snippet with no extra logging, since the caller feeds stdout straight into `eval`/`source`'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand help; and not __fish_seen_subcommand_from feed daemon prompt status show watch init doctor install gen-man gen-completions help" -f -a "doctor" -d 'Run diagnostic checks (config, socket, daemon, rc-file hooks) and print a human-readable report. Exits non-zero if any check fails'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand help; and not __fish_seen_subcommand_from feed daemon prompt status show watch init doctor install gen-man gen-completions help" -f -a "install" -d 'Write the systemd user unit file and, if a systemd user session is available, enable + start the daemon via it'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand help; and not __fish_seen_subcommand_from feed daemon prompt status show watch init doctor install gen-man gen-completions help" -f -a "gen-man" -d 'Generate the man page (troff, `clap_mangen`) to stdout. Hidden: this is a packaging-time helper (`shellagotchi gen-man > man/shellagotchi.1`), not a user-facing feature'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand help; and not __fish_seen_subcommand_from feed daemon prompt status show watch init doctor install gen-man gen-completions help" -f -a "gen-completions" -d 'Generate a shell completion script for `shell` (`clap_complete`) to stdout. Hidden: this is a packaging-time helper (`shellagotchi gen-completions bash > completions/shellagotchi.bash`), not a user-facing feature'
complete -c shellagotchi -n "__fish_shellagotchi_using_subcommand help; and not __fish_seen_subcommand_from feed daemon prompt status show watch init doctor install gen-man gen-completions help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
