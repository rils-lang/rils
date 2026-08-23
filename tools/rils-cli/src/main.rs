mod args;
mod commands;
mod repl;

use std::process::ExitCode;

fn main() -> ExitCode {
    commands::run(std::env::args().skip(1).collect())
}
