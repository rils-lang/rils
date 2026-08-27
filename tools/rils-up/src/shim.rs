use std::{ffi::OsString, path::PathBuf, process::Command};

use crate::{config, install, platform};

pub(crate) fn invoked_proxy() -> Option<&'static str> {
    let executable = std::env::current_exe().ok()?;
    match executable
        .file_stem()?
        .to_string_lossy()
        .to_ascii_lowercase()
        .as_str()
    {
        "rils" => Some("rils"),
        "rils-analyzer" => Some("rils-analyzer"),
        _ => None,
    }
}

pub(crate) fn run(command: &'static str) -> Result<u8, String> {
    let home = config::rils_home()?;
    let mut arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    let explicit = take_explicit_toolchain(&mut arguments)?;
    let version = config::selected_toolchain(&home, explicit.as_deref())?;
    let toolchain = install::require_installed(&home, &version)?;
    let executable = toolchain
        .join("bin")
        .join(platform::executable_name(command));
    if !executable.is_file() {
        return Err(format!(
            "Rils toolchain {version} does not contain {}",
            executable.display()
        ));
    }
    execute(executable, arguments)
}

pub(crate) fn resolved_command(
    home: &std::path::Path,
    command: &str,
    explicit: Option<&str>,
) -> Result<(String, PathBuf), String> {
    if !matches!(command, "rils" | "rils-analyzer") {
        return Err(format!("Unsupported managed command: {command}"));
    }
    let version = config::selected_toolchain(home, explicit)?;
    let toolchain = install::require_installed(home, &version)?;
    Ok((
        version,
        toolchain
            .join("bin")
            .join(platform::executable_name(command)),
    ))
}

fn take_explicit_toolchain(arguments: &mut Vec<OsString>) -> Result<Option<String>, String> {
    let Some(first) = arguments.first().and_then(|argument| argument.to_str()) else {
        return Ok(None);
    };
    let Some(version) = first.strip_prefix('+') else {
        return Ok(None);
    };
    if version.is_empty() {
        return Ok(None);
    }
    let version = config::validate_toolchain(version)?;
    arguments.remove(0);
    Ok(Some(version))
}

#[cfg(unix)]
fn execute(executable: PathBuf, arguments: Vec<OsString>) -> Result<u8, String> {
    use std::os::unix::process::CommandExt;

    let error = Command::new(&executable).args(arguments).exec();
    Err(format!(
        "Could not execute {}: {error}",
        executable.display()
    ))
}

#[cfg(not(unix))]
fn execute(executable: PathBuf, arguments: Vec<OsString>) -> Result<u8, String> {
    let status = Command::new(&executable)
        .args(arguments)
        .status()
        .map_err(|error| format!("Could not execute {}: {error}", executable.display()))?;
    Ok(status.code().unwrap_or(1).clamp(0, u8::MAX as i32) as u8)
}

#[cfg(test)]
#[path = "../tests/unit/shim.rs"]
mod tests;
