pub(crate) fn package_platform() -> Result<&'static str, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Ok("windows-x86_64"),
        ("windows", "aarch64") => Ok("windows-aarch64"),
        ("linux", "x86_64") => Ok("linux-x86_64"),
        ("linux", "aarch64") => Ok("linux-aarch64"),
        ("macos", "x86_64") => Ok("macos-x86_64"),
        ("macos", "aarch64") => Ok("macos-aarch64"),
        (os, architecture) => Err(format!(
            "Rils does not publish a toolchain for {os}/{architecture}"
        )),
    }
}

pub(crate) fn executable_name(command: &str) -> String {
    if cfg!(windows) {
        format!("{command}.exe")
    } else {
        command.to_owned()
    }
}

pub(crate) fn archive_extension() -> &'static str {
    if cfg!(windows) { "zip" } else { "tar.gz" }
}
