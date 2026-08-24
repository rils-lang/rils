use std::{fs, path::Path, process::Command, thread, time::Duration};

use semver::Version;
use serde::Deserialize;

use crate::{config, install, platform};

const MAX_METADATA_BYTES: u64 = 8 * 1024 * 1024;
const MAX_MANAGER_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct Release {
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

struct ManagerRelease<'a> {
    version: Version,
    asset: &'a ReleaseAsset,
    checksums: &'a ReleaseAsset,
}

pub(crate) enum UpdateStatus {
    Current(Version),
    Updated(Version),
    Scheduled(Version),
}

pub(crate) fn update(home: &Path) -> Result<UpdateStatus, String> {
    let current = Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| format!("Invalid embedded rils-up version: {error}"))?;
    let repository = std::env::var("RILS_RELEASE_REPOSITORY")
        .unwrap_or_else(|_| install::DEFAULT_REPOSITORY.to_owned());
    let url = format!("https://api.github.com/repos/{repository}/releases?per_page=100");
    let metadata = install::download(&url, MAX_METADATA_BYTES)?;
    let releases: Vec<Release> = serde_json::from_slice(&metadata)
        .map_err(|error| format!("GitHub returned invalid release metadata: {error}"))?;
    let platform = platform::package_platform()?;
    let suffix = format!("-{platform}{}", if cfg!(windows) { ".exe" } else { "" });
    let Some(latest) = latest_manager_release(&releases, &suffix)? else {
        return Err(format!(
            "No stable rils-up release is available for {platform}"
        ));
    };
    if latest.version <= current {
        return Ok(UpdateStatus::Current(current));
    }

    println!("Downloading rils-up {}", latest.version);
    let checksums = install::download(&latest.checksums.browser_download_url, MAX_METADATA_BYTES)?;
    let manager = install::download(&latest.asset.browser_download_url, MAX_MANAGER_BYTES)?;
    install::verify_checksum(&latest.asset.name, &manager, &checksums)?;

    fs::create_dir_all(home.join("bin"))
        .map_err(|error| format!("Could not create the Rils home: {error}"))?;
    if cfg!(windows) && running_from_managed_binary(home) {
        schedule_windows_update(home, &latest.version, &manager)?;
        Ok(UpdateStatus::Scheduled(latest.version))
    } else {
        replace_managed_binaries(home, &manager)?;
        Ok(UpdateStatus::Updated(latest.version))
    }
}

pub(crate) fn complete_scheduled_update(home: &Path) -> Result<(), String> {
    let source = std::env::current_exe()
        .map_err(|error| format!("Could not locate the update helper: {error}"))?;
    let manager = fs::read(&source)
        .map_err(|error| format!("Could not read {}: {error}", source.display()))?;
    let destination = home.join("bin").join(platform::executable_name("rils-up"));
    let mut last_error = None;
    for _ in 0..150 {
        match write_executable(&destination, &manager) {
            Ok(()) => {
                refresh_proxies_from(home, &manager);
                if source
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with(".rils-up-self-update-"))
                {
                    schedule_helper_deletion(&source);
                }
                return Ok(());
            }
            Err(error) => {
                last_error = Some(error);
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
    Err(last_error.unwrap_or_else(|| "Could not replace rils-up".to_owned()))
}

pub(crate) fn refresh_proxies_if_managed(home: &Path) {
    if !running_from_managed_binary(home) {
        return;
    }
    let Ok(source) = std::env::current_exe() else {
        return;
    };
    let Ok(manager) = fs::read(source) else {
        return;
    };
    refresh_proxies_from(home, &manager);
}

fn latest_manager_release<'a>(
    releases: &'a [Release],
    platform_suffix: &str,
) -> Result<Option<ManagerRelease<'a>>, String> {
    let mut latest: Option<ManagerRelease<'a>> = None;
    for release in releases {
        if release.draft || release.prerelease {
            continue;
        }
        let Some(checksums) = release
            .assets
            .iter()
            .find(|asset| asset.name == "SHA256SUMS")
        else {
            continue;
        };
        for asset in &release.assets {
            let Some(version) = manager_version_from_asset(&asset.name, platform_suffix) else {
                continue;
            };
            if latest
                .as_ref()
                .is_none_or(|candidate| version > candidate.version)
            {
                latest = Some(ManagerRelease {
                    version,
                    asset,
                    checksums,
                });
            }
        }
    }
    Ok(latest)
}

fn manager_version_from_asset(name: &str, suffix: &str) -> Option<Version> {
    let version = name.strip_prefix("rils-up-")?.strip_suffix(suffix)?;
    Version::parse(version).ok()
}

fn replace_managed_binaries(home: &Path, manager: &[u8]) -> Result<(), String> {
    for command in ["rils-up", "rils", "rils-analyzer"] {
        let path = home.join("bin").join(platform::executable_name(command));
        write_executable(&path, manager)?;
    }
    Ok(())
}

fn refresh_proxies_from(home: &Path, manager: &[u8]) {
    for command in ["rils", "rils-analyzer"] {
        let path = home.join("bin").join(platform::executable_name(command));
        if let Err(error) = write_executable(&path, manager) {
            eprintln!("warning: could not refresh {} yet: {error}", path.display());
        }
    }
}

fn write_executable(path: &Path, bytes: &[u8]) -> Result<(), String> {
    config::atomic_write(path, bytes)?;
    set_executable(path)
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|error| format!("Could not set permissions on {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn running_from_managed_binary(home: &Path) -> bool {
    let Ok(current) = std::env::current_exe().and_then(|path| path.canonicalize()) else {
        return false;
    };
    let managed = home.join("bin").join(platform::executable_name("rils-up"));
    managed.canonicalize().is_ok_and(|path| path == current)
}

fn schedule_windows_update(home: &Path, version: &Version, manager: &[u8]) -> Result<(), String> {
    let helper = home.join(format!(
        ".rils-up-self-update-{version}-{}.exe",
        std::process::id()
    ));
    fs::write(&helper, manager)
        .map_err(|error| format!("Could not write {}: {error}", helper.display()))?;
    Command::new(&helper)
        .arg("__complete-self-update")
        .arg("--home")
        .arg(home)
        .spawn()
        .map_err(|error| format!("Could not start {}: {error}", helper.display()))?;
    Ok(())
}

#[cfg(windows)]
fn schedule_helper_deletion(path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_DELAY_UNTIL_REBOOT, MoveFileExW};

    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        MoveFileExW(path.as_ptr(), std::ptr::null(), MOVEFILE_DELAY_UNTIL_REBOOT);
    }
}

#[cfg(not(windows))]
fn schedule_helper_deletion(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::{Release, ReleaseAsset, latest_manager_release, manager_version_from_asset};

    #[test]
    fn parses_manager_version_without_confusing_checksum_assets() {
        let suffix = "-windows-x86_64.exe";
        assert_eq!(
            manager_version_from_asset("rils-up-0.2.1-windows-x86_64.exe", suffix)
                .unwrap()
                .to_string(),
            "0.2.1"
        );
        assert!(
            manager_version_from_asset("rils-up-0.2.1-windows-x86_64.exe.sha256", suffix).is_none()
        );
    }

    #[test]
    fn selects_highest_stable_manager_asset() {
        let releases = vec![release("0.1.0", false), release("0.2.0", false)];
        let selected = latest_manager_release(&releases, "-linux-x86_64")
            .unwrap()
            .unwrap();
        assert_eq!(selected.version.to_string(), "0.2.0");
    }

    fn release(version: &str, prerelease: bool) -> Release {
        Release {
            draft: false,
            prerelease,
            assets: vec![
                ReleaseAsset {
                    name: format!("rils-up-{version}-linux-x86_64"),
                    browser_download_url: "https://example.invalid/manager".to_owned(),
                },
                ReleaseAsset {
                    name: "SHA256SUMS".to_owned(),
                    browser_download_url: "https://example.invalid/checksums".to_owned(),
                },
            ],
        }
    }
}
