mod args;
mod config;
mod install;
mod platform;
mod shim;

use std::process::ExitCode;

fn main() -> ExitCode {
    let result = match shim::invoked_proxy() {
        Some(command) => shim::run(command),
        None => args::run(),
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
