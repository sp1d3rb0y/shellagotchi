#![warn(clippy::disallowed_methods)]

// Config and paths are wired in by later tasks (CLI/daemon wiring); allow dead_code
// here until then so this task's clippy run is clean without stubbing out callers.
#[allow(dead_code)]
mod clock;
#[allow(dead_code)]
mod config;
#[allow(dead_code)]
mod paths;
mod pet;

fn main() {
    println!("Hello, world!");
}
