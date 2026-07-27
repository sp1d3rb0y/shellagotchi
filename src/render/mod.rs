//! Rendering logic for user-facing textual output (currently: shell
//! prompt segments). Kept separate from `pet`/`daemon` since it's pure
//! presentation with no I/O or state mutation.

pub mod prompt;
