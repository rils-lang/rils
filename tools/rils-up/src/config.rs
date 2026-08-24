use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use semver::Version;
use serde::{Deserialize, Serialize};

const SETTINGS_FILE: &str = "settings.toml";
const OVERRIDE_FILE: &str = ".rils-version";

#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct Settings {
    pub(crate) default: Option<String>,
}

pub(crate) fn rils_home() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("RILS_HOME") {
        if path.is_empty() {
            return Err("RILS_HOME cannot be empty".to_owned());
        }
        return Ok(PathBuf::from(path));
    }
    let home = if cfg!(windows) {
        env::var_os("USERPROFILE").or_else(|| env::var_os("HOME"))
    } else {
        env::var_os("HOME")
    };
    home.map(|path| PathBuf::from(path).join(".rils"))
        .ok_or_else(|| "Could not determine the user home directory; set RILS_HOME".to_owned())
}

pub(crate) fn read_settings(home: &Path) -> Result<Settings, String> {
    let path = home.join(SETTINGS_FILE);
    if !path.exists() {
        return Ok(Settings::default());
    }
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    toml::from_str(&text).map_err(|error| format!("Invalid {}: {error}", path.display()))
}

pub(crate) fn write_settings(home: &Path, settings: &Settings) -> Result<(), String> {
    fs::create_dir_all(home)
        .map_err(|error| format!("Could not create {}: {error}", home.display()))?;
    let contents = toml::to_string_pretty(settings)
        .map_err(|error| format!("Could not encode settings: {error}"))?;
    atomic_write(&home.join(SETTINGS_FILE), contents.as_bytes())
}

pub(crate) fn validate_toolchain(value: &str) -> Result<String, String> {
    let value = value.strip_prefix('v').unwrap_or(value);
    Version::parse(value)
        .map(|version| version.to_string())
        .map_err(|error| format!("Invalid Rils toolchain version {value:?}: {error}"))
}

pub(crate) fn selected_toolchain(home: &Path, explicit: Option<&str>) -> Result<String, String> {
    if let Some(version) = explicit {
        return validate_toolchain(version);
    }
    if let Ok(version) = env::var("RILS_TOOLCHAIN") {
        return validate_toolchain(version.trim());
    }
    let current_directory = env::current_dir()
        .map_err(|error| format!("Could not determine the current directory: {error}"))?;
    if let Some(version) = find_override(&current_directory)? {
        return Ok(version);
    }
    read_settings(home)?.default.ok_or_else(|| {
        "No default Rils toolchain is configured; run `rils-up install stable`".to_owned()
    })
}

pub(crate) fn find_override(start: &Path) -> Result<Option<String>, String> {
    for directory in start.ancestors() {
        let path = directory.join(OVERRIDE_FILE);
        if path.is_file() {
            let value = fs::read_to_string(&path)
                .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
            return validate_toolchain(value.trim()).map(Some);
        }
    }
    Ok(None)
}

pub(crate) fn set_override(directory: &Path, version: &str) -> Result<PathBuf, String> {
    let version = validate_toolchain(version)?;
    let path = directory.join(OVERRIDE_FILE);
    atomic_write(&path, format!("{version}\n").as_bytes())?;
    Ok(path)
}

pub(crate) fn remove_override(directory: &Path) -> Result<bool, String> {
    let path = directory.join(OVERRIDE_FILE);
    if !path.exists() {
        return Ok(false);
    }
    fs::remove_file(&path)
        .map_err(|error| format!("Could not remove {}: {error}", path.display()))?;
    Ok(true)
}

pub(crate) fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("System clock is before the Unix epoch: {error}"))?
        .as_nanos();
    let temporary = parent.join(format!(".rils-up-{}-{nonce}.tmp", std::process::id()));
    fs::write(&temporary, contents)
        .map_err(|error| format!("Could not write {}: {error}", temporary.display()))?;
    let result = replace_file(&temporary, path);
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination)
        .map_err(|error| format!("Could not replace {}: {error}", destination.display()))
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let destination_display = destination.display().to_string();
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(format!(
            "Could not replace {}: {}",
            destination_display,
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::validate_toolchain;

    #[test]
    fn normalizes_semantic_versions() {
        assert_eq!(validate_toolchain("v0.4.0").unwrap(), "0.4.0");
        assert_eq!(validate_toolchain("0.4.0-rc.1").unwrap(), "0.4.0-rc.1");
    }

    #[test]
    fn rejects_channels_and_paths_as_installed_toolchains() {
        assert!(validate_toolchain("stable").is_err());
        assert!(validate_toolchain("../0.4.0").is_err());
    }
}
