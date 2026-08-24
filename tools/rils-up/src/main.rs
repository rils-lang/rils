mod args;
mod config;
mod install;
mod local_installer;
mod path_env;
mod platform;
mod self_update;
mod shim;

use std::process::ExitCode;

fn main() -> ExitCode {
    let result = match (local_installer::invoked_installer(), shim::invoked_proxy()) {
        (true, _) => local_installer::run(),
        (false, Some(command)) => shim::run(command),
        (false, None) => args::run(),
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
