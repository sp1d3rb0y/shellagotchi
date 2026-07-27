# shellagotchi

A Linux userland "terminal pet" daemon. Your virtual pet is fed by the exit
codes of your shell commands: `0` is a good meal, anything else is a risky
one. Visualise it with ASCII art, check on it from the CLI, and see it live
in your shell prompt.

Status: **early scaffold** — implementation in progress, following the plan
at `.hermes/plans/2026-07-27_200126-shellagotchi.md` (Hermes-authored design
doc, kept for reference; not required to build/run the crate).

## Development

```bash
cargo build
cargo clippy -- -D warnings
cargo test
```

## Design

See the implementation plan for full domain model, IPC protocol, systemd
unit, and the 24-task TDD build order.
